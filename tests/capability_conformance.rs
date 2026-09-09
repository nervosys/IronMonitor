// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Tests derived from the capability ontology rather than written beside it.
//!
//! # Why derive instead of list
//!
//! A hand-written list of "things that should be declared" is a second copy of
//! the thing it checks, and the two drift. This project has watched that happen
//! more than once: a `rust-version` nothing built against, a README claiming
//! platforms it had never run on, a metric registry described as populated when
//! it was empty.
//!
//! So these tests iterate [`capability::catalogue`] and the code it describes,
//! and assert the two agree in **both** directions. Adding an apply handler
//! without declaring it fails. Declaring a capability for a handler that does
//! not exist fails. Emitting a detection rule that no schema names fails.
//!
//! That is what makes the ontology agentic-first rather than decorative: an
//! agent can read the catalogue and rely on it, because nothing can quietly
//! stop being true.

use ironmonlib::ontology::capability::{self, Platform, Support, Surface};
use std::collections::BTreeSet;

fn ids_with_prefix(prefix: &str) -> BTreeSet<String> {
    capability::catalogue()
        .into_iter()
        .filter(|c| c.id.starts_with(prefix))
        .map(|c| c.id[prefix.len()..].to_string())
        .collect()
}

/// Every registered write handler is declared, and every declaration is real.
///
/// Both directions on purpose. One direction alone lets the catalogue fall
/// behind the code or run ahead of it, and both failures read the same to an
/// agent: a promise that does not match the machine.
#[test]
fn declared_settings_and_registered_handlers_are_the_same_set() {
    let registered: BTreeSet<String> = ironmonlib::profile::apply::builtin_handlers()
        .iter()
        .map(|h| h.setting_id().to_string())
        .collect();
    let declared = ids_with_prefix("setting.");

    let undeclared: Vec<&String> = registered.difference(&declared).collect();
    assert!(
        undeclared.is_empty(),
        "these settings have write handlers and no capability entry: {undeclared:?}. \
         An agent reading the catalogue would not know IronMonitor can write them."
    );

    let phantom: Vec<&String> = declared.difference(&registered).collect();
    assert!(
        phantom.is_empty(),
        "these settings are declared and have no handler on this platform: \
         {phantom:?}. A capability that cannot be exercised is a promise the \
         binary cannot keep."
    );
}

/// Every rule a detector can emit is declared in `ids::RULES`.
///
/// Scans the source rather than the runtime, because most rules only fire
/// against a machine in a particular state and a test that waits for an
/// intrusion is not a test.
#[test]
fn every_detection_rule_the_source_emits_is_declared() {
    let declared: BTreeSet<&str> = ironmonlib::ids::RULES.iter().map(|(r, _)| *r).collect();

    let mut emitted = BTreeSet::new();
    for file in ["src/ids/file.rs", "src/ids/network.rs"] {
        let src = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("reading {file}: {e}"));
        // `Finding::new(` is immediately followed by the rule literal.
        for (idx, _) in src.match_indices("Finding::new(") {
            let rest = &src[idx..];
            let Some(open) = rest.find('"') else { continue };
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else {
                continue;
            };
            let rule = &after[..close];
            // Guard against picking up a doc line or a format string.
            if rule.contains(' ') || rule.is_empty() {
                continue;
            }
            emitted.insert(rule.to_string());
        }
    }

    assert!(
        !emitted.is_empty(),
        "the scan found no rules at all, so it is checking nothing — the shape of \
         `Finding::new` must have changed"
    );

    let undeclared: Vec<&String> = emitted
        .iter()
        .filter(|r| !declared.contains(r.as_str()))
        .collect();
    assert!(
        undeclared.is_empty(),
        "these rules are emitted and not declared in ids::RULES: {undeclared:?}. \
         A finding with no schema entry is one an agent cannot interpret."
    );

    let unemitted: Vec<&&str> = declared.iter().filter(|r| !emitted.contains(**r)).collect();
    assert!(
        unemitted.is_empty(),
        "these rules are declared and never emitted: {unemitted:?}. Either a \
         detector was removed or the schema describes something that cannot \
         happen."
    );
}

/// Every ontology domain has a reading capability.
///
/// The reading ontology says what a value means; this says whether IronMonitor can
/// produce it here. A domain present in one and absent from the other leaves an
/// agent able to interpret a reading it cannot obtain, or able to request one it
/// cannot interpret.
#[test]
fn every_ontology_domain_declares_its_reading_support() {
    let declared = ids_with_prefix("reading.");
    for domain in ironmonlib::ontology::Domain::ALL {
        assert!(
            declared.contains(domain.as_str()),
            "the ontology declares the {} domain and the capability catalogue \
             says nothing about whether IronMonitor can read it",
            domain.as_str()
        );
    }
    for d in &declared {
        assert!(
            ironmonlib::ontology::Domain::ALL
                .iter()
                .any(|dom| dom.as_str() == d),
            "reading.{d} is declared and is not an ontology domain"
        );
    }
}

/// A capability that claims to work here should actually be reachable here.
///
/// Checked for the surfaces that can be asked cheaply. This is the test that
/// would have caught the macOS resolver having no reader wired in while the
/// documentation implied one.
#[test]
fn readings_claimed_usable_here_actually_resolve() {
    let snapshot = ironmonlib::ontology::resolve::snapshot();
    let here = Platform::current().expect("a named platform");

    for c in capability::catalogue() {
        if c.surface != Surface::Reading {
            continue;
        }
        let Some(support) = c.support.get(&here) else {
            continue;
        };
        if !support.is_usable() {
            continue;
        }
        let domain = c.id.trim_start_matches("reading.");
        let prefix = format!("{domain}.");

        // A capability describes what this *build* can read, not what this
        // *machine* has, so the requirement is that the domain is not silent -
        // not that it produced numbers.
        //
        // This assertion has been wrong twice. It first required an observation,
        // and failed on CI runners with no GPU. It then allowed an observation or
        // a `<domain>.<none>` diagnostic, and failed on runners with no battery,
        // where `power.battery.percentage` resolves as unavailable carrying "no
        // battery present" - a correct and complete answer that is neither of the
        // two things it accepted.
        //
        // Rows are the property worth asserting. Whether they carry values depends
        // on the hardware present, which a capability does not claim; that every
        // absent one carries a reason is guaranteed by
        // `every_absence_carries_a_usable_reason` in the ontology suite.
        let rows = snapshot
            .iter()
            .filter(|r| r.id.starts_with(&prefix))
            .count();

        assert!(
            rows > 0,
            "{} claims to be usable on {} and the snapshot contains no {} rows at all - not a reading, not an absence, not a diagnostic. Silence is the one answer this crate must never give.",
            c.id,
            here.as_str(),
            domain
        );
    }
}

/// Anything the catalogue says is unusable here must not be silently producing
/// confident readings anyway.
///
/// The inverse of the test above, and the one that catches a stale pessimistic
/// claim — a capability marked unimplemented long after somebody implemented it.
#[test]
fn readings_claimed_unimplemented_here_really_produce_nothing() {
    let snapshot = ironmonlib::ontology::resolve::snapshot();
    let here = Platform::current().expect("a named platform");

    for c in capability::catalogue() {
        if c.surface != Surface::Reading {
            continue;
        }
        let Some(Support::Unimplemented { .. }) = c.support.get(&here) else {
            continue;
        };
        let domain = c.id.trim_start_matches("reading.");
        let prefix = format!("{domain}.");

        let observed: Vec<&str> = snapshot
            .iter()
            .filter(|r| r.id.starts_with(&prefix))
            .filter(|r| r.is_observation())
            .map(|r| r.id.as_str())
            .take(5)
            .collect();

        assert!(
            observed.is_empty(),
            "{} is declared unimplemented on {} and yet these resolved as real \
             observations: {observed:?}. The catalogue is out of date, which is \
             worse than it being pessimistic — an agent planned around an absence \
             that is not there.",
            c.id,
            here.as_str()
        );
    }
}

/// The tuning verification claim is checked against the code, not remembered.
#[test]
fn the_tuning_verify_capability_matches_the_metric_registry() {
    let cap = capability::catalogue()
        .into_iter()
        .find(|c| c.id == "tuning.verify")
        .expect("tuning.verify is declared");

    let any_metric = [
        "active_scheme_guid",
        "scaling_governor",
        "persistence_mode",
        "perf_level",
        "gt_max_freq_mhz",
    ]
    .iter()
    .any(|id| ironmonlib::tuning::verify::metric_for(id).is_some());

    let claims_partial = cap
        .support
        .values()
        .all(|s| matches!(s, Support::Partial { .. }));

    if any_metric {
        assert!(
            !claims_partial,
            "a metric is now registered, so tuning.verify can reach a verdict and \
             the catalogue still says it cannot"
        );
    } else {
        assert!(
            claims_partial,
            "no metric is registered, so every verified apply reports \
             unverifiable, and the catalogue must say so on every platform"
        );
    }
}

/// The catalogue and the README must not contradict each other.
///
/// They are two renderings of the same claims for two audiences, and the prose
/// one is the one that rots.
#[test]
fn the_readme_and_the_catalogue_agree_about_macos_gpu() {
    let readme = std::fs::read_to_string("README.md").expect("README.md");
    let says_no_gpu = readme.contains("macOS has no GPU, power or temperature readers")
        || readme.contains("macOS has no GPU, power or temperature reader");

    let cap = capability::catalogue()
        .into_iter()
        .find(|c| c.id == "reading.gpu")
        .expect("reading.gpu is declared");
    let declared_missing = matches!(
        cap.support.get(&Platform::MacOS),
        Some(Support::Unimplemented { .. })
    );

    assert_eq!(
        says_no_gpu, declared_missing,
        "the README and the capability catalogue disagree about whether macOS \
         has a GPU reader. Two renderings of one claim that differ means at least \
         one is lying to somebody."
    );
}

/// Every interface module ironmon ships is declared.
///
/// The first version of the catalogue declared three interfaces and IronMonitor had
/// eight: the gui, tui, MCP server, HTTP server and daemon were all missing, and
/// nothing noticed because the catalogue was only ever checked against itself.
/// An agent asking "how can I talk to this" would have been told about a third
/// of the answers.
#[test]
fn every_interface_module_is_declared() {
    // Module path to capability id. Listed rather than derived from the
    // directory because not every module is an interface, and a heuristic over
    // file names would either miss one or invent one.
    const INTERFACES: &[(&str, &str)] = &[
        ("src/tui", "interface.tui"),
        ("src/gui", "interface.gui"),
        ("src/ai_api/mcp_server.rs", "interface.mcp"),
        ("src/http_server.rs", "interface.http"),
        ("src/daemon.rs", "interface.daemon"),
        ("src/agent", "interface.agent"),
    ];

    let declared: BTreeSet<String> = capability::catalogue()
        .into_iter()
        .filter(|c| c.surface == Surface::Interface)
        .map(|c| c.id)
        .collect();

    for (path, id) in INTERFACES {
        assert!(
            std::path::Path::new(path).exists(),
            "{path} is listed as an interface and does not exist; either it was removed and {id} should go too, or the path is wrong"
        );
        assert!(
            declared.contains(*id),
            "{path} ships and {id} is not in the capability catalogue. An agent reading the catalogue would not know this way of talking to IronMonitor exists."
        );
    }
}

/// Every command a capability names actually exists in the binary.
///
/// Capabilities and commands were separate catalogues that never met: an agent
/// read one to learn what IronMonitor can do and the other to learn how to ask, with
/// nothing checking the two described the same program. This is the join.
///
/// It also makes the absence visible. A capability with no command is reachable
/// from the library and from nothing typeable, which is how the intrusion
/// detectors shipped — real, tested, and unreachable from the command line.
#[test]
fn every_command_a_capability_names_exists() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ironmon"))
        .args(["describe", "--commands", "--format", "json"])
        .output()
        .expect("ironmon describe runs");
    assert!(out.status.success(), "ironmon describe --commands failed");
    let catalog: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");

    let mut paths = BTreeSet::new();
    fn walk(node: &serde_json::Value, prefix: &str, out: &mut BTreeSet<String>) {
        let Some(subs) = node.get("subcommands").and_then(|s| s.as_array()) else {
            return;
        };
        for sub in subs {
            let Some(name) = sub.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix} {name}")
            };
            walk(sub, &path, out);
            out.insert(path);
        }
    }
    walk(&catalog, "", &mut paths);
    assert!(!paths.is_empty(), "the command catalog parsed to nothing");

    for c in capability::catalogue() {
        let Some(command) = &c.command else { continue };
        assert!(
            paths.contains(command),
            "{} names the command {command:?} and the binary does not accept it.              A capability that tells an agent how to invoke it must be right about that, or the catalogue is worse than silent.",
            c.id
        );
    }
}

/// Capabilities the command line cannot reach, reported rather than hidden.
///
/// Not a failure — the MCP server is spoken rather than typed, and a library
/// API is a legitimate way to ship something. It is printed so the gap stays
/// visible, because the intrusion detectors sat in exactly this state without
/// anyone noticing.
#[test]
fn capabilities_with_no_command_are_named() {
    let stranded: Vec<String> = capability::catalogue()
        .into_iter()
        .filter(|c| c.command.is_none())
        .map(|c| c.id)
        .collect();
    if !stranded.is_empty() {
        eprintln!(
            "capabilities reachable only from the library ({}): {stranded:?}",
            stranded.len()
        );
    }
    // Detection is the one that should eventually gain a command. Asserted so
    // that if it does, this note stops being true and someone updates it.
    let detection_stranded = stranded
        .iter()
        .filter(|s| s.starts_with("detection."))
        .count();
    let detection_total = capability::catalogue()
        .iter()
        .filter(|c| c.surface == Surface::Detection)
        .count();
    // All-or-nothing, not "all stranded". The first version asserted the state
    // at the time rather than the invariant, and failed the moment the detectors
    // gained a command — which is the change it should have welcomed.
    assert!(
        detection_stranded == 0 || detection_stranded == detection_total,
        "some detection capabilities now name a command and others do not. Either all of them are reachable or none are; a half-wired surface is the state an agent cannot reason about."
    );
}

/// The feature list describes the binary that reports it.
///
/// A capability is per-platform and also per-build. `ironmon ai models` exists
/// only where `vault` was enabled, so an agent that knows the platform and not
/// the feature set still cannot tell what this binary does. This checks the
/// two agree: a feature reported as on must have brought its command with it.
#[test]
fn reported_features_match_the_commands_the_binary_accepts() {
    let features = capability::enabled_features();
    assert!(
        features.contains(&"cli"),
        "the test binary links the library, and these tests run the CLI binary;          if `cli` is not reported the feature list is not describing this build"
    );

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ironmon"))
        .args(["describe", "--commands", "--format", "json"])
        .output()
        .expect("ironmon describe runs");
    let catalog: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
    let names: BTreeSet<String> = catalog
        .get("subcommands")
        .and_then(|s| s.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    // Each feature that adds a top-level command, and the command it adds.
    for (feature, command) in [("gui", "gui"), ("cli", "cli")] {
        if features.contains(&feature) {
            assert!(
                names.contains(command),
                "the feature list reports {feature:?} and the binary has no                  {command:?} command. One of the two is describing a different build."
            );
        }
    }
}

/// Surfaces are covered rather than declared and unused.
#[test]
fn every_surface_has_at_least_one_capability() {
    let cat = capability::catalogue();
    let handlers = ironmonlib::profile::apply::builtin_handlers().len();

    for surface in Surface::ALL {
        let declared = cat.iter().filter(|c| c.surface == *surface).count();

        // Settings are derived from the registered write handlers, and a platform
        // may legitimately register none - macOS does. An empty Setting surface
        // there is the truth rather than a gap, and the first version of this test
        // asserted otherwise and failed in CI.
        if *surface == Surface::Setting {
            assert_eq!(
                declared > 0,
                handlers > 0,
                "the setting surface has {declared} capabilities and {handlers} registered handlers. Those must agree: capabilities without handlers promise writes the binary cannot perform, and handlers without capabilities hide writes it can."
            );
            continue;
        }

        assert!(
            declared > 0,
            "the {} surface is declared in the type and described by nothing",
            surface.as_str()
        );
    }
}

/// What an agent would ask first: what is not worth trying here.
#[test]
fn unusable_here_is_answerable_and_explains_itself() {
    for c in capability::unusable_here() {
        let support = c.here().expect("filtered on this platform");
        let reason = match support {
            Support::Unimplemented { reason } | Support::Unverified { reason } => reason.clone(),
            other => panic!("{} is listed unusable while declaring {other:?}", c.id),
        };
        assert!(
            !reason.trim().is_empty(),
            "{} is unusable here and does not say why",
            c.id
        );
    }
}
