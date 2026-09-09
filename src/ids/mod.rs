// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Intrusion detection over the network and the filesystem.
//!
//! # What this is, and what it refuses to be
//!
//! It observes and reports. It does not block a connection, kill a process, or
//! quarantine a file. Every other write path in this crate requires explicit
//! confirmation and an audit record ([`crate::profile::apply`]), and a detector
//! that could sever a network connection on a heuristic would be the largest
//! unconfirmed write IronMonitor has ever made.
//!
//! # A detector that cannot say "I do not know" is a liar
//!
//! This is the same rule the ontology is built on — absent, unavailable and zero
//! are three different facts — applied to security, where getting it wrong is
//! more expensive.
//!
//! [`ScanStatus`] therefore distinguishes:
//!
//! - [`ScanStatus::NoBaseline`] — nothing has been recorded to compare against.
//!   **Not "clean".** A first run cannot report that a machine is uncompromised;
//!   it can only report that it now knows what the machine looks like. Reporting
//!   "0 findings" here would be the single most dangerous sentence this module
//!   could emit, because it is what a user most wants to read.
//! - [`ScanStatus::Clean`] — a baseline existed, N things were checked, and
//!   nothing differed. Carries N, because "clean" over three files and "clean"
//!   over three thousand are different claims.
//! - [`ScanStatus::Findings`] — differences, each with evidence.
//! - [`ScanStatus::Failed`] — the scan could not run, with the reason. Distinct
//!   from clean for the obvious reason.
//!
//! # Evidence is not optional
//!
//! [`Finding::new`] is the only constructor and it rejects an empty evidence
//! list. A finding without evidence is an accusation, and this crate's whole
//! position is that a reading without provenance is worse than no reading.
//! An analyst who cannot see what was observed cannot disagree with the verdict.
//!
//! # Confidence is separate from severity
//!
//! They are routinely conflated and they are orthogonal: a *possible* sign of a
//! rootkit is high severity and low confidence, while a *certain* new listening
//! port on 3000 is low severity and high confidence. Collapsing them into one
//! number is how alert fatigue starts.
//!
//! [`Confidence::Certain`] is reserved for facts that were observed directly —
//! this file's hash differs from the recorded one. No heuristic may claim it.
//!
//! # The agentic part, and its limit
//!
//! A model may triage: rank findings, group them, explain what they might mean
//! together. **It may not create a finding, alter its evidence, or raise its
//! confidence.** That is the same rule `tuning` applies to settings — a model
//! may classify the workload, it may not choose the power limit — and it exists
//! for the same reason: a fabricated security finding is indistinguishable from
//! a real one to everyone downstream. See [`triage`].

pub mod file;
pub mod network;
pub mod triage;

/// Every rule the detectors in this module can emit, with what it means.
///
/// Declared in one place so the capability catalogue can be derived from it
/// rather than kept in step by hand — see
/// [`crate::ontology::capability::catalogue`]. A rule emitted by a detector but
/// missing here would be a finding an agent has no schema for, which is the
/// undeclared-entity problem the reading ontology already refuses to have.
///
/// `tests/capability_conformance.rs` scans the detector source and fails if any
/// `Finding::new` names a rule that is not in this list.
pub const RULES: &[(&str, &str)] = &[
    (
        "file.became_unreadable",
        "a watched file was readable at baseline and is not now",
    ),
    ("file.created", "a watched path now exists where it did not"),
    ("file.deleted", "a watched file no longer exists"),
    ("file.modified", "a watched file's contents changed"),
    (
        "file.unbaselined",
        "a watched file has no baseline entry to compare against",
    ),
    ("file.unreadable", "a watched file could not be read"),
    (
        "net.listener_gone",
        "a port in the baseline is no longer listening",
    ),
    (
        "net.new_listener",
        "a port is listening that was not in the baseline",
    ),
    (
        "net.notable_port",
        "a port conventionally used by remote-access tooling is listening",
    ),
];

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// How sure the detector is that what it saw is what it says it is.
///
/// Deliberately not a percentage. A number invites arithmetic — averaging,
/// thresholding, multiplying — on a quantity that is a judgement, and the
/// arithmetic would look more principled than the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// A heuristic matched. It may be entirely benign.
    Possible,
    /// Consistent with the pattern and unlikely to be coincidence.
    Probable,
    /// Directly observed, not inferred: this hash differs from the recorded one.
    /// Heuristics may not claim this.
    Certain,
}

/// How bad it would be if the finding is real.
///
/// Independent of [`Confidence`] on purpose — see the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Worth recording, not worth waking anyone.
    Info,
    Low,
    Medium,
    High,
    /// Consistent with an active compromise.
    Critical,
}

/// One observation supporting a finding.
///
/// `observed` is what was actually seen; `expected` is what the baseline said,
/// when there was one. Both are kept so a reader can check the conclusion rather
/// than take it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// What kind of observation — `file.hash`, `net.listener`, `net.remote_port`.
    pub kind: String,
    pub observed: String,
    /// The recorded value this was compared against, when the check was a
    /// comparison. `None` for observations that stand alone.
    pub expected: Option<String>,
}

impl Evidence {
    pub fn observed(kind: impl Into<String>, observed: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            observed: observed.into(),
            expected: None,
        }
    }

    pub fn differs(
        kind: impl Into<String>,
        observed: impl Into<String>,
        expected: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            observed: observed.into(),
            expected: Some(expected.into()),
        }
    }
}

/// What a finding is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "subject")]
pub enum Subject {
    File {
        path: String,
    },
    Network {
        local_port: u16,
        remote: Option<String>,
        /// The owning process, when the platform would say. `None` means it was
        /// not readable — commonly a process owned by another user without
        /// elevation — not that the connection has no owner.
        process: Option<String>,
        pid: Option<u32>,
    },
}

/// One thing a detector noticed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable identifier for the *rule*, not the instance: `file.modified`,
    /// `net.new_listener`. Lets a caller suppress a rule without regex on prose.
    pub rule: String,
    pub title: String,
    pub severity: Severity,
    pub confidence: Confidence,
    pub subject: Subject,
    /// Never empty — see [`Finding::new`].
    pub evidence: Vec<Evidence>,
    /// Unix epoch seconds.
    pub observed_at: u64,
}

impl Finding {
    /// Build a finding. Returns `None` when the evidence list is empty.
    ///
    /// The only constructor, and the check is here rather than in a test because
    /// a finding with no evidence is an accusation. Callers that cannot supply
    /// evidence have not detected anything; they have had an opinion.
    pub fn new(
        rule: impl Into<String>,
        title: impl Into<String>,
        severity: Severity,
        confidence: Confidence,
        subject: Subject,
        evidence: Vec<Evidence>,
    ) -> Option<Self> {
        if evidence.is_empty() {
            return None;
        }
        Some(Self {
            rule: rule.into(),
            title: title.into(),
            severity,
            confidence,
            subject,
            evidence,
            observed_at: now_secs(),
        })
    }
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The outcome of one scan.
///
/// The variants exist to keep four different facts apart. See the module
/// documentation for why `NoBaseline` is not `Clean`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ScanStatus {
    /// No baseline to compare against. One has now been recorded, but this run
    /// found nothing because it could not look — not because there was nothing
    /// to find.
    NoBaseline {
        /// How many things the new baseline covers.
        recorded: usize,
        reason: String,
    },
    /// A baseline existed and nothing differed.
    Clean {
        checked: usize,
    },
    Findings {
        checked: usize,
        findings: Vec<Finding>,
    },
    Failed {
        reason: String,
    },
}

impl ScanStatus {
    /// Findings, if any. An empty slice from `Clean` and from `NoBaseline` mean
    /// different things, so callers that care must match on the variant.
    pub fn findings(&self) -> &[Finding] {
        match self {
            ScanStatus::Findings { findings, .. } => findings,
            _ => &[],
        }
    }

    /// Whether this scan actually compared anything against a baseline.
    ///
    /// The question a caller should ask before writing "no intrusions detected"
    /// anywhere.
    pub fn is_conclusive(&self) -> bool {
        matches!(self, ScanStatus::Clean { .. } | ScanStatus::Findings { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject() -> Subject {
        Subject::File {
            path: "/etc/passwd".into(),
        }
    }

    /// The rule that keeps a finding from being an accusation.
    #[test]
    fn a_finding_without_evidence_cannot_be_built() {
        assert!(
            Finding::new(
                "file.modified",
                "something happened",
                Severity::Critical,
                Confidence::Certain,
                subject(),
                vec![],
            )
            .is_none(),
            "a finding with no evidence is an opinion, and this is the only \
             constructor, so there is no way around it"
        );
    }

    #[test]
    fn a_finding_with_evidence_is_built_and_timestamped() {
        let f = Finding::new(
            "file.modified",
            "hash differs",
            Severity::High,
            Confidence::Certain,
            subject(),
            vec![Evidence::differs("file.hash", "abc", "def")],
        )
        .expect("evidence supplied");
        assert_eq!(f.evidence.len(), 1);
        assert_eq!(f.evidence[0].expected.as_deref(), Some("def"));
    }

    /// The distinction the module exists to protect.
    #[test]
    fn no_baseline_is_not_clean_and_neither_is_a_failure() {
        let none = ScanStatus::NoBaseline {
            recorded: 12,
            reason: "first run".into(),
        };
        let clean = ScanStatus::Clean { checked: 12 };
        let failed = ScanStatus::Failed {
            reason: "permission denied".into(),
        };

        assert_ne!(none, clean);
        assert!(
            !none.is_conclusive(),
            "a first run has not established that a machine is uncompromised; \
             reporting it as clean is the most dangerous sentence this module \
             could emit"
        );
        assert!(!failed.is_conclusive());
        assert!(clean.is_conclusive());

        // And they must survive serialisation as different things, since that is
        // how they reach an agent.
        let a = serde_json::to_string(&none).unwrap();
        let b = serde_json::to_string(&clean).unwrap();
        let c = serde_json::to_string(&failed).unwrap();
        assert!(a.contains("\"status\":\"no_baseline\""), "{a}");
        assert!(b.contains("\"status\":\"clean\""), "{b}");
        assert!(c.contains("\"status\":\"failed\""), "{c}");
        assert_ne!(a, b);
        assert_ne!(b, c);
    }

    /// Both empty, both meaning something different.
    #[test]
    fn findings_is_empty_for_two_unrelated_reasons() {
        let none = ScanStatus::NoBaseline {
            recorded: 0,
            reason: "first run".into(),
        };
        let clean = ScanStatus::Clean { checked: 500 };
        assert!(none.findings().is_empty());
        assert!(clean.findings().is_empty());
        assert!(
            !none.is_conclusive() && clean.is_conclusive(),
            "an empty findings list is not a result on its own — the variant is"
        );
    }

    /// Ordering matters because triage sorts by it.
    #[test]
    fn severity_and_confidence_order_the_way_a_reader_expects() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::Info < Severity::Low);
        assert!(Confidence::Certain > Confidence::Probable);
        assert!(Confidence::Probable > Confidence::Possible);
    }

    /// They are orthogonal, and the type system should not let one stand in for
    /// the other.
    #[test]
    fn a_high_severity_finding_may_be_low_confidence() {
        let f = Finding::new(
            "net.suspicious_port",
            "outbound connection to an unusual port",
            Severity::High,
            Confidence::Possible,
            Subject::Network {
                local_port: 51234,
                remote: Some("203.0.113.5:4444".into()),
                process: None,
                pid: None,
            },
            vec![Evidence::observed("net.remote_port", "4444")],
        )
        .unwrap();
        assert_eq!(f.severity, Severity::High);
        assert_eq!(f.confidence, Confidence::Possible);
    }

    /// An unreadable owner is not an absent owner.
    #[test]
    fn an_unknown_process_is_none_rather_than_a_placeholder_string() {
        let s = Subject::Network {
            local_port: 22,
            remote: None,
            process: None,
            pid: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"process\":null"), "{json}");
        assert!(
            !json.contains("unknown"),
            "a placeholder string would be an absence dressed as a reading: {json}"
        );
    }
}
