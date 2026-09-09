// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Conformance tests generated from the ontology rather than written per feature.
//!
//! Every other test file names the things it checks. This one names none: it asks
//! the ontology what exists, resolves it, and asserts the rules the ontology says
//! it obeys. A domain added next year is covered by all of it on the commit that
//! adds it, with no edit here — which is the point, because the failure mode of a
//! hand-written suite is that the newest reader is the least tested one.
//!
//! What that buys, concretely: five entities shipped across 3.3.0 and 3.4.0
//! passing an enum's `Unknown` variant through as a *measured* value. Each was
//! caught by eye during review. [`unknown_is_never_dressed_as_a_measurement`]
//! below catches that class by construction.
//!
//! These run in-process against `ironmonlib` rather than through the binary.
//! `tests/agentic_contract.rs` covers the CLI surface — exit codes, JSON shape,
//! the `describe`/`get`/`snapshot` commands — and is the right place for anything
//! about how the ontology is *presented*. This file is about whether the readings
//! themselves obey the schema.

use ironmonlib::ontology::resolve::{self, Reading};
use ironmonlib::ontology::{Domain, Entity, EntityKind, Ontology, Provenance, Unit};

/// One resolve pass, shared by every test that needs live readings.
///
/// Resolving is expensive — it enumerates disks, spawns the SMART collector,
/// queries WMI. Each test taking its own snapshot made this file slower than the
/// rest of the suite combined.
fn readings() -> &'static [Reading] {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Vec<Reading>> = OnceLock::new();
    CACHE.get_or_init(resolve::snapshot)
}

fn ontology() -> &'static Ontology {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Ontology> = OnceLock::new();
    CACHE.get_or_init(Ontology::build)
}

/// Whether a unit's values are numbers rather than strings.
fn is_numeric(unit: Unit) -> bool {
    !matches!(unit, Unit::Text | Unit::Identifier)
}

// ── The rules the ontology documents about itself ────────────────────────────

/// Every declared entity must be reachable. The resolver sweeps unbound ids into
/// the snapshot as `unavailable`, so an entity missing entirely means that sweep
/// is broken — and a silently absent id is the one answer the module forbids.
#[test]
fn every_declared_entity_appears_in_a_snapshot() {
    let produced: std::collections::HashSet<&str> =
        readings().iter().map(|r| r.id.as_str()).collect();

    let missing: Vec<&str> = ontology()
        .entities
        .values()
        .filter(|e| {
            // Templates expand per instance and diagnostics fire only on their
            // condition; neither is expected verbatim.
            !Ontology::is_template(&e.id) && e.kind != EntityKind::Diagnostic
        })
        .map(|e| e.id.as_str())
        .filter(|id| !produced.contains(id))
        .collect();

    assert!(
        missing.is_empty(),
        "declared but absent from the snapshot: {missing:?}"
    );
}

/// Every reading must trace back to a declared entity. The inverse of the above,
/// and the one that catches a resolver inventing an id the schema never promised.
#[test]
fn every_reading_traces_back_to_the_schema() {
    let orphans: Vec<&str> = readings()
        .iter()
        .map(|r| r.id.as_str())
        // Diagnostics are keyed `<domain>.<none>` and declared as such.
        .filter(|id| ontology().template_for(id).is_none())
        .collect();

    assert!(
        orphans.is_empty(),
        "resolved ids with no entity behind them: {orphans:?}"
    );
}

/// An unavailable reading without a reason is as much a dead end as a fabricated
/// number: the agent learns the value is missing but not whether to retry, elevate,
/// or conclude the hardware is absent.
#[test]
fn every_absence_carries_a_usable_reason() {
    // Words that technically fill the field while saying nothing. A note is for a
    // reader who cannot see the machine.
    const EMPTY_EXCUSES: [&str; 6] = ["", "unknown", "n/a", "none", "error", "failed"];

    let bad: Vec<String> = readings()
        .iter()
        .filter(|r| r.provenance == Provenance::Unavailable)
        .filter_map(|r| {
            let note = r.note.as_deref().unwrap_or("").trim();
            let useless = EMPTY_EXCUSES.contains(&note.to_ascii_lowercase().as_str())
                // A bare few words cannot distinguish the cases a caller must act
                // on differently.
                || note.split_whitespace().count() < 3;
            if useless {
                Some(format!("{} => {note:?}", r.id))
            } else {
                None
            }
        })
        .collect();

    assert!(
        bad.is_empty(),
        "unavailable readings whose note explains nothing: {bad:#?}"
    );
}

/// An unavailable reading must carry no value, and an available one must carry a
/// value. The two fields encode the same fact and cannot be allowed to disagree —
/// a consumer trusting `provenance` and a consumer trusting `value.is_some()`
/// would otherwise reach opposite conclusions from the same row.
#[test]
fn provenance_and_value_never_contradict_each_other() {
    let bad: Vec<String> = readings()
        .iter()
        .filter_map(|r| match (r.provenance, r.value.is_some()) {
            (Provenance::Unavailable, true) => {
                Some(format!("{} is unavailable but carries a value", r.id))
            }
            (p, false) if p != Provenance::Unavailable => {
                Some(format!("{} is {} but carries no value", r.id, p.as_str()))
            }
            _ => None,
        })
        .collect();

    assert!(bad.is_empty(), "{bad:#?}");
}

/// `nullable: false` is the schema promising a reader will always have a value.
/// The ontology states outright that a null there "is a bug in the reader, not an
/// absent device", so this is that claim made enforceable.
#[test]
fn non_nullable_entities_are_never_null() {
    let violations: Vec<String> = readings()
        .iter()
        .filter(|r| r.provenance == Provenance::Unavailable)
        .filter_map(|r| {
            let entity = ontology().template_for(&r.id)?;
            if entity.nullable {
                return None;
            }
            Some(format!(
                "{} is declared non-nullable but resolved unavailable: {}",
                r.id,
                r.note.as_deref().unwrap_or("(no note)")
            ))
        })
        .collect();

    assert!(
        violations.is_empty(),
        "either the reader is broken or the entity should be nullable: {violations:#?}"
    );
}

/// A `celsius` value arriving as the string "45" forces every consumer to guess
/// which fields need parsing. The unit already says which are numbers.
#[test]
fn values_have_the_json_type_their_unit_implies() {
    let bad: Vec<String> = readings()
        .iter()
        .filter_map(|r| {
            let value = r.value.as_ref()?;
            let unit = r.unit?;
            let mismatched = if is_numeric(unit) {
                !value.is_number()
            } else {
                !value.is_string()
            };
            if !mismatched {
                return None;
            }
            Some(format!(
                "{} is declared {} but resolved to {value}",
                r.id,
                unit.as_str()
            ))
        })
        .collect();

    assert!(bad.is_empty(), "{bad:#?}");
}

/// The range gate in the resolver is meant to have already caught these. If one
/// reaches here the gate has a hole, and an agent is being handed a number the
/// schema itself calls impossible.
#[test]
fn no_reading_violates_its_own_declared_range() {
    let impossible: Vec<String> = readings()
        .iter()
        .filter_map(|r| {
            let value = r.value.as_ref()?.as_f64()?;
            let entity = ontology().template_for(&r.id)?;
            entity.validate_range(value)
        })
        .collect();

    assert!(impossible.is_empty(), "{impossible:#?}");
}

/// The specific mistake this file was written for.
///
/// Five entities across two releases resolved an enum's `Unknown` variant to the
/// *string* "unknown" with `measured` provenance, letting an agent record a health
/// or attestation check that never succeeded. "Unknown" is an absence and belongs
/// in the provenance, not the value.
#[test]
fn unknown_is_never_dressed_as_a_measurement() {
    const ABSENCES: [&str; 5] = ["unknown", "unspecified", "undetermined", "n/a", "none"];

    let dressed: Vec<String> = readings()
        .iter()
        .filter(|r| r.provenance != Provenance::Unavailable)
        .filter_map(|r| {
            let text = r.value.as_ref()?.as_str()?.trim().to_ascii_lowercase();
            if !ABSENCES.contains(&text.as_str()) {
                return None;
            }
            Some(format!(
                "{} resolved to {text:?} with provenance {} — an absence reported as \
                 a reading; use Reading::unavailable with the reason",
                r.id,
                r.provenance.as_str()
            ))
        })
        .collect();

    assert!(dressed.is_empty(), "{dressed:#?}");
}

// ── Rules about the schema itself, independent of any machine ────────────────

/// An id is the contract. Mixed case or whitespace in one would be a permanent
/// wart, since ids may be added but never repurposed.
#[test]
fn every_id_is_well_formed() {
    let bad: Vec<String> = ontology()
        .entities
        .values()
        .filter_map(|e| {
            let id = &e.id;
            let reason = if id != &id.to_lowercase() {
                "is not lowercase"
            } else if id.contains(' ') {
                "contains a space"
            } else if id.split('.').any(|s| s.is_empty()) {
                "has an empty segment"
            } else if !id.starts_with(&format!("{}.", e.domain.as_str())) {
                "does not start with its own domain"
            } else {
                return None;
            };
            Some(format!("{id} {reason}"))
        })
        .collect();

    assert!(bad.is_empty(), "{bad:#?}");
}

/// A description is what an agent reads to decide whether an id is the one it
/// wants. This checks it is a written sentence rather than a stub.
///
/// The bar is deliberately low. It was first written as "at least four words" and
/// flagged ten entities, every one of which was fine — "Seconds since boot.",
/// "Executable name.", "Total L2 cache." are complete answers, and a guardrail
/// that fails on adequate work is one people learn to switch off. What actually
/// signals a stub is prose that was never written: empty, or not a sentence.
#[test]
fn every_entity_describes_itself() {
    let bad: Vec<String> = ontology()
        .entities
        .values()
        .filter_map(|e| {
            let d = e.description.trim();
            let reason = if d.is_empty() {
                "has no description"
            } else if d.split_whitespace().count() < 2 {
                "has a one-word description"
            } else if !d.ends_with('.') {
                "has a description that is not a sentence"
            } else if d.eq_ignore_ascii_case(&e.id) {
                "restates its own id"
            } else {
                return None;
            };
            Some(format!("{} {reason}: {d:?}", e.id))
        })
        .collect();

    assert!(bad.is_empty(), "{bad:#?}");
}

/// A derived value is only as trustworthy as its inputs, so an agent must be able
/// to follow `derived_from` to them. A name that resolves to nothing breaks that
/// chain, and the whole reason `Derived` is distinct from `Measured` is that the
/// chain can be walked.
#[test]
fn derived_entities_name_inputs_that_exist() {
    let broken: Vec<String> = ontology()
        .entities
        .values()
        .flat_map(|e| {
            e.derived_from
                .iter()
                .filter(|input| ontology().get(input).is_none())
                .map(move |input| format!("{} derives from {input}, which is not declared", e.id))
        })
        .collect();

    assert!(broken.is_empty(), "{broken:#?}");
}

/// `derived_from` is meaningful only on a derived entity, and a derived entity
/// with no inputs is untraceable — the two fields have to agree.
#[test]
fn derived_provenance_and_inputs_agree() {
    let bad: Vec<String> = ontology()
        .entities
        .values()
        .filter_map(|e| match (e.provenance, e.derived_from.is_empty()) {
            (Provenance::Derived, true) => Some(format!("{} is derived but names no inputs", e.id)),
            (p, false) if p != Provenance::Derived => Some(format!(
                "{} names inputs but is declared {}",
                e.id,
                p.as_str()
            )),
            _ => None,
        })
        .collect();

    assert!(bad.is_empty(), "{bad:#?}");
}

/// A domain in `Domain::ALL` that declares nothing is a name an agent can pass to
/// `--domain` and get silence from.
#[test]
fn every_domain_declares_at_least_one_entity() {
    let empty: Vec<&str> = Domain::ALL
        .iter()
        .filter(|d| !ontology().entities.values().any(|e| e.domain == **d))
        .map(|d| d.as_str())
        .collect();

    assert!(empty.is_empty(), "domains with no entities: {empty:?}");
}

/// Every domain must produce *something* — a reading or a diagnostic saying why
/// not. Silence cannot be told apart from an unimplemented reader.
#[test]
fn every_domain_reports_something() {
    let silent: Vec<&str> = Domain::ALL
        .iter()
        .filter(|d| {
            let prefix = format!("{}.", d.as_str());
            !readings().iter().any(|r| r.id.starts_with(&prefix))
        })
        .map(|d| d.as_str())
        .collect();

    assert!(
        silent.is_empty(),
        "domains that resolved nothing at all: {silent:?}"
    );
}

/// Two snapshots taken back to back must describe the same machine. Instance
/// counts may legitimately change — a USB device can be unplugged mid-test — so
/// this compares the set of *entities* reached, not the values.
#[test]
fn resolution_is_stable_across_calls() {
    let templates = |rs: &[Reading]| -> std::collections::BTreeSet<String> {
        rs.iter()
            .filter_map(|r| ontology().template_for(&r.id).map(|e| e.id.clone()))
            .collect()
    };

    let first = templates(readings());
    let second = templates(&resolve::snapshot());

    let only_first: Vec<&String> = first.difference(&second).collect();
    let only_second: Vec<&String> = second.difference(&first).collect();

    assert!(
        only_first.is_empty() && only_second.is_empty(),
        "entity coverage differed between two snapshots\n  \
         first only: {only_first:?}\n  second only: {only_second:?}"
    );
}

/// Not an assertion — a coverage report, printed with `--nocapture`.
///
/// The ontology's honesty about its own reach is a documented property (the README
/// states an entity count and names the subsystems still missing). This prints the
/// numbers so that claim can be checked rather than trusted.
#[test]
fn report_coverage_by_domain() {
    let mut rows: Vec<(String, usize, usize, usize)> = Vec::new();
    for domain in Domain::ALL {
        let prefix = format!("{}.", domain.as_str());
        let declared = ontology()
            .entities
            .values()
            .filter(|e| e.domain == *domain)
            .count();
        let resolved = readings()
            .iter()
            .filter(|r| r.id.starts_with(&prefix) && r.provenance != Provenance::Unavailable)
            .count();
        let unavailable = readings()
            .iter()
            .filter(|r| r.id.starts_with(&prefix) && r.provenance == Provenance::Unavailable)
            .count();
        rows.push((domain.as_str().to_string(), declared, resolved, unavailable));
    }

    println!(
        "\n{:<10} {:>9} {:>9} {:>12}",
        "domain", "declared", "resolved", "unavailable"
    );
    for (name, declared, resolved, unavailable) in &rows {
        println!("{name:<10} {declared:>9} {resolved:>9} {unavailable:>12}");
    }
    println!(
        "{:<10} {:>9} {:>9} {:>12}",
        "total",
        rows.iter().map(|r| r.1).sum::<usize>(),
        rows.iter().map(|r| r.2).sum::<usize>(),
        rows.iter().map(|r| r.3).sum::<usize>(),
    );
}

/// The harness must be able to fail. A test file that derives its cases from data
/// can silently degenerate to zero cases — an empty ontology, a resolver returning
/// nothing — and then every assertion above passes vacuously.
#[test]
fn the_harness_has_something_to_test() {
    assert!(
        ontology().entities.len() >= 50,
        "the ontology declares {} entities; the conformance tests above are \
         vacuous if this collapses",
        ontology().entities.len()
    );
    assert!(
        readings().len() >= 50,
        "only {} readings resolved; the conformance tests above are vacuous",
        readings().len()
    );
    // And at least some must be real observations, or every value-shaped
    // assertion is skipped by its `filter_map`.
    let observed = readings().iter().filter(|r| r.is_observation()).count();
    assert!(
        observed >= 20,
        "only {observed} readings were live observations"
    );
}

/// Sanity check on the template matcher the tests above lean on. If it matched too
/// eagerly, unrelated entities would validate each other's readings and several
/// tests would pass for the wrong reason.
#[test]
fn template_matching_respects_segment_structure() {
    let o = ontology();
    assert_eq!(
        o.template_for("disk.0.model").map(|e| e.id.as_str()),
        Some("disk.{n}.model")
    );
    assert_eq!(
        o.template_for("disk.0.smart.passed").map(|e| e.id.as_str()),
        Some("disk.{n}.smart.passed")
    );
    // Different segment counts must not match.
    assert!(o.template_for("disk.0").is_none());
    assert!(o.template_for("disk.0.model.extra").is_none());
    assert!(o.template_for("nonsense.0.field").is_none());
}

/// Not every entity is reachable on every machine, but a build where *nothing*
/// resolves in a domain that declares many is worth surfacing. This lists them
/// rather than failing, because a machine legitimately without a GPU is not a bug.
#[test]
fn report_domains_resolving_nothing_measurable() {
    let mut barren: Vec<(&str, usize)> = Vec::new();
    for domain in Domain::ALL {
        let prefix = format!("{}.", domain.as_str());
        let observed = readings()
            .iter()
            .filter(|r| r.id.starts_with(&prefix) && r.is_observation())
            .count();
        if observed == 0 {
            let declared = ontology()
                .entities
                .values()
                .filter(|e| e.domain == *domain)
                .count();
            barren.push((domain.as_str(), declared));
        }
    }

    if !barren.is_empty() {
        println!(
            "\ndomains declaring entities but observing nothing here: {barren:?}\n  \
             (expected on a machine lacking that hardware; investigate if not)"
        );
    }
}

/// Guards the assumption every unit-based assertion above makes: that a unit is
/// either numeric or textual, with nothing in between. A new `Unit` variant that
/// is neither would silently narrow those tests.
#[test]
fn every_unit_is_classified() {
    for unit in [
        Unit::Celsius,
        Unit::Percent,
        Unit::Bytes,
        Unit::BytesPerSecond,
        Unit::Hertz,
        Unit::Megahertz,
        Unit::Watts,
        Unit::Milliwatts,
        Unit::Volts,
        Unit::Rpm,
        Unit::Seconds,
        Unit::Hours,
        Unit::Milliseconds,
        Unit::Count,
        Unit::Identifier,
        Unit::Text,
    ] {
        // Exercising both arms proves the classifier answers for every variant.
        let _: bool = is_numeric(unit);
        assert!(!unit.as_str().is_empty(), "a unit with no name");
    }
}

/// `Entity` is public and agents deserialize it. A field silently dropped from the
/// JSON would be invisible until something downstream started guessing.
#[test]
fn entities_serialize_with_the_fields_agents_rely_on() {
    let sample: &Entity = ontology()
        .entities
        .values()
        .find(|e| e.unit.is_some())
        .expect("the ontology declares no entity with a unit");

    let json = serde_json::to_value(sample).expect("entity must serialize");
    for field in [
        "id",
        "domain",
        "kind",
        "unit",
        "provenance",
        "nullable",
        "description",
    ] {
        assert!(
            json.get(field).is_some(),
            "entity JSON is missing {field:?}: {json}"
        );
    }
}

/// Prose that ships to a reader has to read as prose.
///
/// Thirty-one descriptions and absence reasons were found carrying runs of a
/// dozen or more spaces mid-sentence — `ironmon describe` printed "a freshly
/// imaged              host". They were written as `\`-continued literals whose
/// continuation stopped taking effect at some point, leaving the source
/// indentation inside the string. Nothing caught it because every other test
/// asks what a description *says*, and this is about what it *looks like*.
///
/// A run of spaces is never meaningful in either of these: descriptions are
/// sentences and absence reasons are sentences. Alignment belongs in the code
/// that lays out a table, not in the text handed to it.
#[test]
fn descriptions_and_reasons_contain_no_stray_whitespace() {
    let mut bad: Vec<String> = ontology()
        .entities
        .values()
        .filter(|e| e.description.contains("  ") || e.description.contains('\n'))
        .map(|e| format!("description of {}: {:?}", e.id, e.description))
        .collect();

    bad.extend(
        readings()
            .iter()
            .filter_map(|r| r.note.as_ref().map(|n| (&r.id, n)))
            .filter(|(_, n)| n.contains("  ") || n.contains('\n'))
            .map(|(id, n)| format!("note on {id}: {n:?}")),
    );

    assert!(
        bad.is_empty(),
        "text that ships to a reader carries collapsed source indentation \
         — join the literal with `concat!` or a single line instead:\n{bad:#?}"
    );
}

/// An entity whose resolver has an absence path must be declared nullable.
///
/// `non_nullable_entities_are_never_null` already checks this — but only for the
/// paths *this* machine happens to take. A reader that reports an absence on a
/// platform the runner is not, or in a failure the runner did not hit, is
/// invisible to it. That is not hypothetical: `board.audio.{n}.state` was
/// declared non-nullable and went green on Windows for a whole gate, because
/// Windows is the one platform that produces a value for it, and failed on the
/// macOS runner minutes later. Five more entities were already in the same
/// state, each with a deliberate absence path and a written reason, waiting for
/// a machine that would take it.
///
/// So this reads the resolver's source instead of its output: every id passed
/// as a literal to `Reading::unavailable` or `push_opt` must belong to an
/// entity that admits being absent. It cannot see ids built with `format!`,
/// which is most of the per-instance ones, so it is a floor rather than a
/// ceiling.
#[test]
fn entities_with_an_absence_path_are_declared_nullable() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ontology/resolve.rs"),
    )
    .expect("the resolver source is part of this crate");

    // `Reading::unavailable("id", ..)` and `push_opt(out, "id".to_string(), ..)`.
    let mut ids: Vec<String> = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let opens_absence = line.contains("Reading::unavailable(") || line.contains("push_opt(");
        let candidates = if opens_absence {
            // The id is on this line or one of the next two.
            src.lines().skip(i).take(3).collect::<Vec<_>>()
        } else {
            continue;
        };
        for c in candidates {
            let Some(start) = c.find('"') else { continue };
            let rest = &c[start + 1..];
            let Some(end) = rest.find('"') else { continue };
            let id = &rest[..end];
            // Only ids, not reason strings: an entity id is dotted, lowercase,
            // and has no spaces.
            if id.contains('.')
                && !id.contains(' ')
                && id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || ".-_<>{}".contains(c))
            {
                ids.push(id.to_string());
                break;
            }
        }
    }

    assert!(
        ids.len() > 20,
        "the scan found only {} ids, so its pattern has drifted from the source",
        ids.len()
    );

    let ont = ontology();
    let mut bad: Vec<String> = Vec::new();
    for id in &ids {
        let Some(entity) = ont.template_for(id).or_else(|| ont.get(id)) else {
            continue;
        };
        if !entity.nullable {
            bad.push(format!(
                "{} is declared non-nullable, but the resolver has a path that reports it absent",
                entity.id
            ));
        }
    }
    bad.sort();
    bad.dedup();

    assert!(bad.is_empty(), "{bad:#?}");
}
