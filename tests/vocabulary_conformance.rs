// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Every declared vocabulary is checked against the enum it describes.
//!
//! Each test constructs the real variants, serialises them, and compares the
//! resulting set with what `ontology::vocabulary` declares. Both directions:
//! a variant added without being declared fails, and a declared value that no
//! variant produces fails.
//!
//! Serialising rather than listing the variant names, because what an agent
//! sees is the JSON. A `#[serde(rename_all)]` change would leave the Rust names
//! identical and every declared string wrong, and that is precisely the drift
//! this file exists to catch.

use ironmonlib::ontology::vocabulary;
use std::collections::BTreeSet;

/// The serialised tag or value of one instance.
///
/// Handles both plain enums (`"measured"`) and internally-tagged ones, where the
/// value an agent switches on is a field rather than the whole document.
fn tag_of<T: serde::Serialize>(value: &T, field: &str) -> String {
    let json = serde_json::to_value(value).expect("serialises");
    match &json {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(map) => map
            .get(field)
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("no {field:?} field in {json}"))
            .to_string(),
        other => panic!("unexpected shape {other}"),
    }
}

fn declared(id: &str) -> BTreeSet<String> {
    vocabulary::get(id)
        .unwrap_or_else(|| panic!("{id} is declared"))
        .values
        .into_iter()
        .map(|t| t.value)
        .collect()
}

fn assert_same(id: &str, actual: BTreeSet<String>) {
    let declared = declared(id);
    let undeclared: Vec<&String> = actual.difference(&declared).collect();
    assert!(
        undeclared.is_empty(),
        "{id}: these values are emitted and not declared: {undeclared:?}. An agent \
         switching on this field would meet a value its schema does not contain."
    );
    let phantom: Vec<&String> = declared.difference(&actual).collect();
    assert!(
        phantom.is_empty(),
        "{id}: these values are declared and never emitted: {phantom:?}. A schema \
         that lists cases which cannot occur teaches an agent to handle fiction."
    );
}

#[test]
fn provenance_matches_its_declaration() {
    use ironmonlib::ontology::Provenance::*;
    let actual = [Measured, Specification, Derived, Unavailable]
        .iter()
        .map(|p| tag_of(p, "provenance"))
        .collect();
    assert_same("vocabulary.provenance", actual);
}

#[test]
fn apply_status_matches_its_declaration() {
    use ironmonlib::profile::apply::ApplyStatus::*;
    let actual = [Applied, NeedsConfirm, NotWritable, Failed]
        .iter()
        .map(|s| tag_of(s, "status"))
        .collect();
    assert_same("vocabulary.apply_status", actual);
}

#[test]
fn verdict_matches_its_declaration() {
    use ironmonlib::tuning::verify::Verdict;
    let actual = [
        Verdict::Improved {
            metric: "m".into(),
            before: 0.0,
            after: 1.0,
            delta: 1.0,
        },
        Verdict::Unchanged {
            metric: "m".into(),
            before: 0.0,
            after: 0.0,
            delta: 0.0,
            threshold: 1.0,
        },
        Verdict::Regressed {
            metric: "m".into(),
            before: 1.0,
            after: 0.0,
            delta: -1.0,
        },
        Verdict::Unverifiable {
            reason: "none".into(),
        },
    ]
    .iter()
    .map(|v| tag_of(v, "verdict"))
    .collect();
    assert_same("vocabulary.verdict", actual);
}

#[test]
fn scan_status_matches_its_declaration() {
    use ironmonlib::ids::ScanStatus;
    let actual = [
        ScanStatus::NoBaseline {
            recorded: 0,
            reason: "r".into(),
        },
        ScanStatus::Clean { checked: 0 },
        ScanStatus::Findings {
            checked: 0,
            findings: vec![],
        },
        ScanStatus::Failed { reason: "r".into() },
    ]
    .iter()
    .map(|s| tag_of(s, "status"))
    .collect();
    assert_same("vocabulary.scan_status", actual);
}

#[test]
fn severity_and_confidence_match_their_declarations() {
    use ironmonlib::ids::{Confidence, Severity};
    let sev = [
        Severity::Info,
        Severity::Low,
        Severity::Medium,
        Severity::High,
        Severity::Critical,
    ]
    .iter()
    .map(|s| tag_of(s, "severity"))
    .collect();
    assert_same("vocabulary.severity", sev);

    let conf = [
        Confidence::Possible,
        Confidence::Probable,
        Confidence::Certain,
    ]
    .iter()
    .map(|c| tag_of(c, "confidence"))
    .collect();
    assert_same("vocabulary.confidence", conf);
}

#[test]
fn support_matches_its_declaration() {
    use ironmonlib::ontology::capability::Support;
    let actual = [
        Support::Implemented,
        Support::Partial {
            missing: "m".into(),
        },
        Support::Unimplemented { reason: "r".into() },
        Support::Unverified { reason: "r".into() },
    ]
    .iter()
    .map(|s| tag_of(s, "support"))
    .collect();
    assert_same("vocabulary.support", actual);
}

/// A vocabulary nobody checks is a vocabulary that drifts.
///
/// Every declared vocabulary must have a test above. Written as a count rather
/// than by name because the failure it guards against is somebody adding a
/// declaration and no check — at which point the count is the only thing that
/// notices.
#[test]
fn every_declared_vocabulary_is_covered_by_a_test_in_this_file() {
    let declared: BTreeSet<String> = vocabulary::vocabularies()
        .into_iter()
        .map(|v| v.id)
        .collect();

    let src = std::fs::read_to_string(file!())
        .or_else(|_| std::fs::read_to_string("tests/vocabulary_conformance.rs"))
        .expect("this test file is readable");

    let unchecked: Vec<&String> = declared
        .iter()
        .filter(|id| !src.contains(&format!("\"{id}\"")))
        .collect();

    assert!(
        unchecked.is_empty(),
        "these vocabularies are declared and never checked against their enum: \
         {unchecked:?}. Add a test that constructs every variant — a declaration \
         nothing verifies is a comment with extra syntax."
    );
}
