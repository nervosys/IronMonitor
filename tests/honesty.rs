// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! The claims IronMonitor makes about itself, checked against ironmon.
//!
//! Documentation drifts. A README that was true when written and false a year
//! later is worse than no README, because it is believed — and a hardware
//! monitor whose whole position is "an honest absence beats a confident wrong
//! answer" cannot make an exception for its own capability claims.
//!
//! So the limits section is not prose that someone remembered to write. Where a
//! claim in it corresponds to something checkable in the code, it is checked
//! here, and changing the code without changing the claim fails the build.
//!
//! This is the same idea as `tests/documentation_links.rs`, which enforces that
//! every documented command exists. That test caught a sentence in `HANDOFF.md`
//! that read as an invented subcommand. This one guards the opposite direction:
//! not "does the documented thing exist" but "is the documented limitation still
//! true".

use std::path::Path;

fn read(name: &str) -> String {
    std::fs::read_to_string(Path::new(name)).unwrap_or_else(|e| panic!("reading {name}: {e}"))
}

/// A source file with its `#[cfg(test)]` module removed.
///
/// The first version of `the_intrusion_detector_has_no_write_path` scanned whole
/// files and failed on `src/ids/file.rs`, because its tests create and delete
/// temporary fixtures. That is a test writing to a temp directory, not a
/// detector writing to a monitored host, and a check that cannot tell them apart
/// would push someone to weaken it rather than fix it.
fn production_source(name: &str) -> String {
    let src = read(name);
    match src.find("\n#[cfg(test)]") {
        Some(at) => src[..at].to_string(),
        None => src,
    }
}

/// Words that promise completeness. Every one of them is a claim no monitor can
/// keep, and each was in this project's own description until it was checked.
const ABSOLUTES: &[&str] = &[
    "comprehensive",
    "complete hardware",
    "all platforms",
    "every platform",
    "any hardware",
    "everything about",
];

#[test]
fn the_crate_description_promises_nothing_it_cannot_keep() {
    let manifest = read("Cargo.toml");
    let description = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("description"))
        .expect("the manifest has a description")
        .to_ascii_lowercase();

    for word in ABSOLUTES {
        assert!(
            !description.contains(word),
            "the crate description claims {word:?}. That line is what crates.io \
             shows to someone deciding whether to trust this tool, and IronMonitor does \
             not monitor everything on every platform — macOS has no GPU, power or \
             temperature reader, and the README says so. Describe what it reads."
        );
    }
}

/// The section exists, and is near the top rather than buried.
#[test]
fn the_readme_states_its_limits_where_a_reader_will_reach_them() {
    let readme = read("README.md");
    let heading = "## What IronMonitor cannot do";
    let at = readme
        .find(heading)
        .unwrap_or_else(|| panic!("README.md has no {heading:?} section"));

    let features = readme
        .find("\n## Features")
        .expect("README.md has a Features section");
    assert!(
        at < features,
        "the limits section sits after the feature list. A reader deciding \
         whether this tool suits them meets the promises first and the caveats \
         only if they keep going, which is the arrangement that makes caveats \
         decorative."
    );
}

/// The README says the tuning loop cannot currently verify anything, because
/// its metric registry is empty. If a metric is ever registered that becomes
/// false, and this fails until the sentence is rewritten.
#[test]
fn the_claim_that_tuning_cannot_verify_matches_the_code() {
    use ironmonlib::tuning::verify::metric_for;

    let registered: Vec<&str> = [
        "active_scheme_guid",
        "scaling_governor",
        "persistence_mode",
        "perf_level",
        "gt_max_freq_mhz",
    ]
    .into_iter()
    .filter(|id| metric_for(id).is_some())
    .collect();

    let readme = read("README.md");
    let claims_empty = readme.contains("metric registry is deliberately empty");

    if registered.is_empty() {
        assert!(
            claims_empty,
            "no metric is registered, so `ironmon tune` genuinely cannot verify a \
             setting, and the README should keep saying so"
        );
    } else {
        panic!(
            "metrics are now registered for {registered:?}, so the README claim \
             that the registry is deliberately empty is out of date. Rewrite it — \
             and the commit that registered them should show the measurement \
             proving the number moves when the setting changes."
        );
    }
}

/// The README promises the intrusion detector cannot report a first scan clean.
/// That is a property of the code, so it is checked as one.
#[test]
fn a_first_intrusion_scan_really_cannot_report_clean() {
    use ironmonlib::ids::{file, ScanStatus};

    let status = file::scan(&[], &file::Baseline::default());
    assert!(
        matches!(status, ScanStatus::NoBaseline { .. }),
        "the README tells a reader that a first scan returns no_baseline rather \
         than clean. It returned {status:?}"
    );
    assert!(
        !status.is_conclusive(),
        "a first scan must not be conclusive, whatever it is called"
    );
}

/// The README promises the detector observes only. A write path appearing in the
/// ids module would make that false, and it is the kind of thing added with good
/// intentions during an incident.
#[test]
fn the_intrusion_detector_has_no_write_path() {
    for file in ["src/ids/mod.rs", "src/ids/network.rs", "src/ids/triage.rs"] {
        let src = production_source(file);
        for forbidden in ["fn block", "fn kill_", "fn quarantine", "TerminateProcess"] {
            assert!(
                !src.contains(forbidden),
                "{file} contains {forbidden:?}. The README tells a reader this \
                 module observes and never acts, and severing a connection or \
                 killing a process on a heuristic would be the largest \
                 unconfirmed write this crate has ever made. If that changes, it \
                 goes through the confirmed, audit-logged apply layer, and the \
                 README stops promising otherwise."
            );
        }
    }
    // The file module writes nothing either, but it does open files for reading,
    // so it is checked for the destructive calls specifically — and only outside
    // its tests, which legitimately create fixtures to hash.
    let src = production_source("src/ids/file.rs");
    for forbidden in ["remove_file", "OpenOptions", "fs::write"] {
        assert!(
            !src.contains(&format!("std::fs::{forbidden}")),
            "src/ids/file.rs calls {forbidden}. File integrity monitoring reads."
        );
    }
}

/// Absent, unavailable and zero are the crate's central distinction. The README
/// says an unreadable entity resolves with a reason rather than being omitted.
#[test]
fn unreadable_entities_carry_a_reason_rather_than_vanishing() {
    let readme = read("README.md");
    assert!(
        readme.contains("no resolver bound on this build"),
        "the README quotes the exact phrase an agent will see for an entity \
         IronMonitor does not read yet. If the wording changed, the README should \
         match what the tool actually emits — a quoted string that does not \
         appear in the output teaches a reader to distrust the rest."
    );

    let snapshot = ironmonlib::ontology::resolve::snapshot();
    let unavailable_without_reason = snapshot
        .iter()
        .filter(|r| r.provenance == ironmonlib::ontology::Provenance::Unavailable)
        .filter(|r| r.note.as_deref().unwrap_or("").trim().is_empty())
        .count();
    assert_eq!(
        unavailable_without_reason, 0,
        "an unavailable reading with no reason is exactly the silence this crate \
         claims not to produce"
    );
}

/// An entity declared as a live observation must resolve as one, or be absent.
///
/// The gap this closes was found by reading declarations, not by a test.
/// `resolve_codecs` chooses provenance per row — a capability the driver was
/// asked about resolves `Measured`, one concluded from the GPU model resolves
/// `Specification` — while all five entities those rows populate declared
/// `Measured`. The schema therefore promised a live observation for values that
/// had never been observed, and nothing anywhere compared the two.
///
/// **The assertion is deliberately one-directional.** Requiring equality would
/// be wrong: `Entity` carries a single provenance and cannot say "varies per
/// row", so a cluster whose rows legitimately differ has to declare the weakest
/// one its rows can carry and then over-deliver on the queried rows. A reading
/// resolving *stronger* than its declaration costs a consumer nothing. The
/// reverse hands an agent a spec-sheet inference wearing a measurement's
/// authority, which is the one thing this crate exists to prevent.
///
/// Only the `Measured` direction is checked, and that is on purpose too.
/// Ranking `Derived` against `Specification` is not meaningful — a value derived
/// from measured inputs is live, one derived from a vendor table is not — so a
/// total order over `Provenance` would invent a comparison the type does not
/// support and fail on entities that are perfectly honest.
#[test]
fn a_reading_never_resolves_weaker_than_its_entity_declares() {
    use ironmonlib::ontology::Provenance;

    let ontology = ironmonlib::ontology::Ontology::build();
    let violations: Vec<String> = ironmonlib::ontology::resolve::snapshot()
        .iter()
        .filter(|r| r.provenance != Provenance::Unavailable)
        .filter_map(|r| {
            let entity = ontology.template_for(&r.id)?;
            if entity.provenance != Provenance::Measured {
                return None;
            }
            if r.provenance == Provenance::Measured {
                return None;
            }
            Some(format!(
                "{} is declared measured but resolved {}",
                r.id,
                r.provenance.as_str()
            ))
        })
        .collect();

    assert!(
        violations.is_empty(),
        "these entities promise a live observation and delivered something \
         weaker. Either the resolver should measure them, or the declaration \
         should carry the weakest provenance its rows can have: {violations:#?}"
    );
}
