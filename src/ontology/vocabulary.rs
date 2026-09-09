// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! The closed sets of values ironmon emits, and what each one means.
//!
//! # The gap this fills
//!
//! [`super::Entity`] says what a reading means. [`super::capability`] says
//! whether IronMonitor can produce it here. Neither says what `"no_baseline"` is, and
//! an agent that receives
//!
//! ```json
//! {"status":"no_baseline","recorded":12,"reason":"..."}
//! ```
//!
//! has no way to know whether that is one of three possible values or one of
//! thirty, nor which of them mean "act now". It can handle the case in front of
//! it and will be surprised by the next one.
//!
//! Every enum here is one an agent parses. Declaring the value set closes the
//! last place where ironmon's output required prior knowledge that ironmon did not
//! supply.
//!
//! # Why this is derived rather than described
//!
//! The obvious implementation is a hand-written list of strings, and it would be
//! wrong within a release — this project has watched a `rust-version`, a README
//! and a metric registry all drift from the code they described.
//!
//! So `tests/vocabulary_conformance.rs` constructs every real variant, serialises
//! it, and asserts the resulting set is exactly what is declared here. Adding a
//! variant without declaring it fails. Declaring one that does not exist fails.
//!
//! # What is deliberately not here
//!
//! Open sets. Setting ids, entity ids and detection rules are enumerable but
//! they grow, and they already have their own catalogues that tests keep honest.
//! This module is for the small closed vocabularies where an agent's handling
//! must be exhaustive.

use serde::{Deserialize, Serialize};

/// One value, and what it means to a caller deciding what to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Term {
    /// Exactly as it appears in JSON.
    pub value: String,
    pub meaning: String,
}

fn term(value: &str, meaning: &str) -> Term {
    Term {
        value: value.to_string(),
        meaning: meaning.to_string(),
    }
}

/// A closed set of values ironmon emits in one field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vocabulary {
    /// Dotted id: `vocabulary.scan_status`.
    pub id: String,
    /// Where these appear — the Rust type and the JSON field, so a reader can
    /// find both ends.
    pub emitted_by: String,
    pub summary: String,
    pub values: Vec<Term>,
}

fn vocab(id: &str, emitted_by: &str, summary: &str, values: Vec<Term>) -> Vocabulary {
    Vocabulary {
        id: id.to_string(),
        emitted_by: emitted_by.to_string(),
        summary: summary.to_string(),
        values,
    }
}

/// Every closed vocabulary ironmon emits.
pub fn vocabularies() -> Vec<Vocabulary> {
    let mut out = vec![
        vocab(
            "vocabulary.provenance",
            "ontology::Provenance, the `provenance` field of every Reading",
            "how a reading was arrived at — the field that separates a measurement \
             from an inference from an absence",
            vec![
                term("measured", "read directly from the hardware or the OS"),
                term(
                    "derived",
                    "computed from other readings; correct, but no sensor reported it",
                ),
                term(
                    "specification",
                    "a published constant: a spec sheet, a vendor table, a documented default. True of the hardware, and not observed on this machine right now",
                ),
                term(
                    "unavailable",
                    "not read, with a reason in `note`. Never zero, never a guess: \
                     the single most important value in this vocabulary",
                ),
            ],
        ),
        vocab(
            "vocabulary.apply_status",
            "profile::apply::ApplyStatus, the `status` of every write attempt",
            "what happened when ironmon tried to change a setting",
            vec![
                term("applied", "the write took effect"),
                term(
                    "needs_confirm",
                    "refused because no explicit confirmation was given; passing \
                     `--confirm` would proceed",
                ),
                term(
                    "not_writable",
                    "refused because nothing can write it here — no handler on this \
                     platform, or no prior value recorded to revert to. Confirming \
                     would not help",
                ),
                term("failed", "attempted and the platform rejected it"),
            ],
        ),
        vocab(
            "vocabulary.verdict",
            "tuning::verify::Verdict, the `verdict` tag on a verified apply",
            "whether an applied setting measurably helped",
            vec![
                term("improved", "the metric moved the good way by more than noise"),
                term(
                    "unchanged",
                    "the metric was readable and did not move enough to call. A \
                     result",
                ),
                term(
                    "regressed",
                    "the metric moved the wrong way by more than noise. The only \
                     verdict that triggers an automatic revert",
                ),
                term(
                    "unverifiable",
                    "no verdict was reached — no metric is declared, or it could not \
                     be read, or the machine was too idle for it to mean anything. \
                     Distinct from `unchanged`: that is looking and finding nothing, \
                     this is not being able to look",
                ),
            ],
        ),
        vocab(
            "vocabulary.scan_status",
            "ids::ScanStatus, the `status` of every intrusion scan",
            "the outcome of one detection scan",
            vec![
                term(
                    "no_baseline",
                    "nothing was recorded to compare against, so nothing was \
                     checked. **Not clean.** A baseline has now been taken, and one \
                     taken after a compromise records the compromise",
                ),
                term(
                    "clean",
                    "a baseline existed, `checked` things were compared, and nothing \
                     differed",
                ),
                term("findings", "differences were found, each carrying evidence"),
                term(
                    "failed",
                    "the scan could not run, with the reason. Distinct from clean",
                ),
            ],
        ),
        vocab(
            "vocabulary.severity",
            "ids::Severity, the `severity` of every finding",
            "how bad a finding would be if it is real — independent of how sure \
             IronMonitor is that it is",
            vec![
                term("info", "worth recording, not worth waking anyone"),
                term("low", "worth looking at when convenient"),
                term("medium", "worth looking at today"),
                term("high", "worth looking at now"),
                term("critical", "consistent with an active compromise"),
            ],
        ),
        vocab(
            "vocabulary.confidence",
            "ids::Confidence, the `confidence` of every finding",
            "how sure IronMonitor is that a finding is what it says it is — orthogonal to \
             severity, and conflating the two is how alert fatigue starts",
            vec![
                term(
                    "possible",
                    "a heuristic matched and it may be entirely benign. Every \
                     port-convention finding is this",
                ),
                term(
                    "probable",
                    "consistent with the pattern and unlikely to be coincidence",
                ),
                term(
                    "certain",
                    "directly observed rather than inferred — this hash differs from \
                     the recorded one. No heuristic may claim it",
                ),
            ],
        ),
        vocab(
            "vocabulary.support",
            "ontology::capability::Support, the `support` of every capability",
            "how well ironmon supports something on one platform",
            vec![
                term(
                    "implemented",
                    "works, and something exercises it on real hardware",
                ),
                term(
                    "partial",
                    "works for some of what the name suggests; `missing` says which \
                     part does not",
                ),
                term(
                    "unimplemented",
                    "not implemented here, with the reason — \"not written\" and \
                     \"the platform exposes no such interface\" lead to different \
                     decisions",
                ),
                term(
                    "unverified",
                    "the code exists, compiles, and has never run on real hardware. \
                     Deliberately not `implemented`",
                ),
            ],
        ),
    ];
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Look up one vocabulary by id.
pub fn get(id: &str) -> Option<Vocabulary> {
    vocabularies().into_iter().find(|v| v.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_namespaced() {
        let mut seen = std::collections::BTreeSet::new();
        for v in vocabularies() {
            assert!(
                v.id.starts_with("vocabulary."),
                "{} is not namespaced",
                v.id
            );
            assert!(seen.insert(v.id.clone()), "duplicate vocabulary {}", v.id);
        }
    }

    #[test]
    fn every_term_explains_itself() {
        for v in vocabularies() {
            assert!(!v.values.is_empty(), "{} declares no values", v.id);
            for t in &v.values {
                assert!(
                    !t.value.is_empty() && t.value == t.value.to_ascii_lowercase(),
                    "{} has the value {:?}, which is not what appears in JSON",
                    v.id,
                    t.value
                );
                assert!(
                    t.meaning.len() > 15,
                    "{} declares {:?} with the meaning {:?}, which tells an agent \
                     nothing it can act on",
                    v.id,
                    t.value,
                    t.meaning
                );
            }
        }
    }

    /// The distinctions this crate exists to preserve must be spelled out where
    /// an agent will read them, not only in Rust documentation.
    #[test]
    fn the_load_bearing_distinctions_are_stated_in_the_vocabulary() {
        let scan = get("vocabulary.scan_status").expect("declared");
        let no_baseline = scan
            .values
            .iter()
            .find(|t| t.value == "no_baseline")
            .expect("declared");
        assert!(
            no_baseline.meaning.to_lowercase().contains("not clean"),
            "an agent reading no_baseline must be told it is not a clean result, \
             because that is the one misreading with real consequences"
        );

        let verdict = get("vocabulary.verdict").expect("declared");
        let unverifiable = verdict
            .values
            .iter()
            .find(|t| t.value == "unverifiable")
            .expect("declared");
        assert!(
            unverifiable.meaning.contains("unchanged"),
            "unverifiable must be explained against unchanged, since telling them \
             apart is the whole point of having both"
        );
    }

    #[test]
    fn the_vocabulary_serialises_for_an_agent() {
        let json = serde_json::to_string(&vocabularies()).unwrap();
        assert!(json.contains("\"value\":\"unavailable\""));
        let back: Vec<Vocabulary> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, vocabularies());
    }
}
