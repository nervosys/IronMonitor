// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Network intrusion detection: what is listening, and what changed.
//!
//! # Why this watches listeners rather than traffic
//!
//! IronMonitor can enumerate sockets ([`crate::connections`]); it cannot see packet
//! contents, and a detector built on what it cannot see would be a stub with a
//! confident name. What a socket table does answer well is the question that
//! matters most after a compromise: **what is accepting connections on this
//! machine that was not accepting them before, and what owns it?**
//!
//! A new listener is the durable footprint of most remote-access tooling. It is
//! also completely normal — a development server binds a port every day — which
//! is exactly why it is reported as a low-severity certainty rather than a
//! high-severity guess, and why the baseline exists.
//!
//! # Heuristics are labelled as heuristics
//!
//! The port-reputation check ([`Confidence::Possible`]) matches a small table of
//! ports conventionally used by remote-access tools. It will match a developer
//! who chose 4444 for a test server, and it says so: `Possible`, never
//! `Certain`. Nothing in this module may claim `Certain` unless it observed the
//! difference directly, which for network state means "compared against a
//! recorded baseline".
//!
//! # Unattributed is not unowned
//!
//! A socket whose process IronMonitor cannot read — commonly one owned by another user
//! without elevation — carries `process: None`, not `"unknown"`. On Windows the
//! owning PID is usually available and the *name* often is not; both are
//! reported independently for that reason.

use super::{now_secs, Confidence, Evidence, Finding, ScanStatus, Severity, Subject};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A listening socket, reduced to the parts worth comparing across scans.
///
/// Deliberately not the whole [`crate::connections::ConnectionInfo`]: a baseline
/// that includes ephemeral remote addresses would differ on every scan and the
/// tool would cry wolf until nobody read it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Listener {
    pub protocol: String,
    pub local_port: u16,
    /// Whether the socket is bound to a routable address rather than loopback.
    /// A service on 127.0.0.1 and the same service on 0.0.0.0 are different
    /// exposures, and the distinction is the point.
    pub externally_reachable: bool,
    pub process: Option<String>,
}

/// A recorded set of listeners.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    pub recorded_at: u64,
    pub listeners: BTreeSet<Listener>,
}

impl Baseline {
    pub fn is_empty(&self) -> bool {
        self.listeners.is_empty()
    }
    pub fn len(&self) -> usize {
        self.listeners.len()
    }
}

/// Ports conventionally used by remote-access and reverse-shell tooling.
///
/// A convention table, not an oracle. Everything it matches is reported as
/// [`Confidence::Possible`], because every one of these is also a port somebody
/// legitimately chose. The value is in ranking, not in verdicts.
const NOTABLE_PORTS: &[(u16, &str)] = &[
    (23, "telnet, unencrypted remote shell"),
    (1080, "SOCKS proxy"),
    (1337, "conventional in remote-access tooling"),
    (3389, "RDP"),
    (4444, "Metasploit default handler"),
    (5555, "adb and various remote-access tools"),
    (5900, "VNC"),
    (6666, "conventional in remote-access tooling"),
    (31337, "conventional in remote-access tooling"),
];

/// Read the current listening sockets.
///
/// Returns `Err` with a reason rather than an empty list when enumeration fails:
/// "no listeners" and "could not look" are the distinction this whole crate is
/// organised around, and here the second one masquerading as the first would
/// report a compromised machine as quiet.
pub fn current_listeners() -> Result<BTreeSet<Listener>, String> {
    use crate::connections::ConnectionMonitor;

    // `listening_sockets` rather than filtering `all_connections` here: the
    // definition of "listening" belongs to the connections module, and a second
    // copy of it in this file would drift from the first.
    let monitor = ConnectionMonitor::new().map_err(|e| e.to_string())?;
    let connections = monitor.listening_sockets().map_err(|e| e.to_string())?;

    let mut out = BTreeSet::new();
    for c in connections {
        // An unbound socket is not a listener. macOS reports these as `*.*` and
        // they parse to port 0; nothing can connect to them, and including them
        // would put entries in every baseline that differ between runs for
        // reasons no reader could act on.
        //
        // Found because a test asserted no listener has port 0 — true on
        // Windows, false on macOS, and only CI could say so.
        if c.local_port == 0 {
            continue;
        }
        let externally_reachable = !c.local_ip.is_loopback();
        out.insert(Listener {
            protocol: format!("{:?}", c.protocol).to_lowercase(),
            local_port: c.local_port,
            externally_reachable,
            process: c.process_name.clone(),
        });
    }
    Ok(out)
}

/// Record the current listeners as a baseline.
pub fn record() -> Result<Baseline, String> {
    Ok(Baseline {
        recorded_at: now_secs(),
        listeners: current_listeners()?,
    })
}

/// Compare current listeners against a baseline.
pub fn scan(baseline: &Baseline) -> ScanStatus {
    let current = match current_listeners() {
        Ok(c) => c,
        Err(reason) => {
            return ScanStatus::Failed {
                reason: format!("could not enumerate sockets: {reason}"),
            }
        }
    };

    if baseline.is_empty() {
        return ScanStatus::NoBaseline {
            recorded: current.len(),
            reason: "no listener baseline existed, so nothing could be compared. \
                     A baseline taken now includes whatever is already listening, \
                     including anything that should not be."
                .into(),
        };
    }

    let checked = current.len();
    let mut findings = compare(baseline, &current);

    // Reputation is evaluated on what is listening now, independent of the
    // baseline: a notable port that was present at baseline time is still worth
    // surfacing once, because the baseline may have been taken too late.
    findings.extend(reputation_findings(&current));

    if findings.is_empty() {
        ScanStatus::Clean { checked }
    } else {
        ScanStatus::Findings { checked, findings }
    }
}

/// Listeners that are new since the baseline, and ones that vanished.
pub fn compare(baseline: &Baseline, current: &BTreeSet<Listener>) -> Vec<Finding> {
    let mut findings = Vec::new();

    for l in current.difference(&baseline.listeners) {
        let severity = if l.externally_reachable {
            Severity::Medium
        } else {
            // A new loopback listener is a normal development event. Recorded,
            // not escalated — an alerting tool that shouts at `cargo run` is one
            // nobody keeps running.
            Severity::Low
        };
        let mut evidence = vec![
            Evidence::observed("net.protocol", &l.protocol),
            Evidence::observed("net.local_port", l.local_port.to_string()),
            Evidence::differs(
                "net.in_baseline",
                "false",
                "true for every other listener on this machine",
            ),
            Evidence::observed(
                "net.reachable",
                if l.externally_reachable {
                    "bound to a routable address"
                } else {
                    "loopback only"
                },
            ),
        ];
        if let Some(p) = &l.process {
            evidence.push(Evidence::observed("net.process", p));
        }
        if let Some(f) = Finding::new(
            "net.new_listener",
            "a port is listening that was not in the baseline",
            severity,
            // Directly observed against a record, not inferred.
            Confidence::Certain,
            subject_of(l),
            evidence,
        ) {
            findings.push(f);
        }
    }

    for l in baseline.listeners.difference(current) {
        // A service stopping is usually mundane and occasionally the point — a
        // security agent being shut down looks exactly like this. Info severity,
        // recorded, not escalated.
        if let Some(f) = Finding::new(
            "net.listener_gone",
            "a port in the baseline is no longer listening",
            Severity::Info,
            Confidence::Certain,
            subject_of(l),
            // Protocol included, or two findings that differ only by address
            // family print identically. Observed live: tcp and tcp6 on the same
            // port produced two rows a reader could not tell apart.
            vec![
                Evidence::observed("net.protocol", &l.protocol),
                Evidence::observed("net.local_port", l.local_port.to_string()),
                Evidence::differs("net.listening", "false", "true"),
            ],
        ) {
            findings.push(f);
        }
    }

    findings
}

/// Findings from the port convention table.
pub fn reputation_findings(current: &BTreeSet<Listener>) -> Vec<Finding> {
    let mut out = Vec::new();
    for l in current {
        let Some((_, why)) = NOTABLE_PORTS.iter().find(|(p, _)| *p == l.local_port) else {
            continue;
        };
        // Loopback-only is a materially weaker signal: it is not reachable from
        // off the machine at all.
        let severity = if l.externally_reachable {
            Severity::High
        } else {
            Severity::Low
        };
        let mut evidence = vec![
            Evidence::observed("net.local_port", l.local_port.to_string()),
            Evidence::observed("net.port_convention", *why),
            Evidence::observed(
                "net.reachable",
                if l.externally_reachable {
                    "bound to a routable address"
                } else {
                    "loopback only"
                },
            ),
        ];
        if let Some(p) = &l.process {
            evidence.push(Evidence::observed("net.process", p));
        }
        if let Some(f) = Finding::new(
            "net.notable_port",
            "a port conventionally used by remote-access tooling is listening",
            severity,
            // A convention table is a heuristic and says so. Every port here is
            // also a port somebody legitimately chose.
            Confidence::Possible,
            subject_of(l),
            evidence,
        ) {
            out.push(f);
        }
    }
    out
}

fn subject_of(l: &Listener) -> Subject {
    Subject::Network {
        local_port: l.local_port,
        remote: None,
        process: l.process.clone(),
        pid: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener(port: u16, external: bool, process: Option<&str>) -> Listener {
        Listener {
            protocol: "tcp".into(),
            local_port: port,
            externally_reachable: external,
            process: process.map(|s| s.to_string()),
        }
    }

    fn baseline_of(ls: &[Listener]) -> Baseline {
        Baseline {
            recorded_at: 0,
            listeners: ls.iter().cloned().collect(),
        }
    }

    #[test]
    fn a_new_listener_is_found_and_carries_what_was_seen() {
        let base = baseline_of(&[listener(22, true, Some("sshd"))]);
        let current: BTreeSet<Listener> = [
            listener(22, true, Some("sshd")),
            listener(8080, true, Some("node")),
        ]
        .into_iter()
        .collect();

        let findings = compare(&base, &current);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "net.new_listener");
        assert_eq!(findings[0].confidence, Confidence::Certain);
        assert!(findings[0]
            .evidence
            .iter()
            .any(|e| e.kind == "net.process" && e.observed == "node"));
    }

    /// Exposure is the distinction that decides severity, so it must not be
    /// collapsed.
    #[test]
    fn a_loopback_listener_is_ranked_below_a_routable_one() {
        let base = baseline_of(&[listener(1, true, None)]);
        let loopback: BTreeSet<Listener> = [listener(1, true, None), listener(9000, false, None)]
            .into_iter()
            .collect();
        let routable: BTreeSet<Listener> = [listener(1, true, None), listener(9000, true, None)]
            .into_iter()
            .collect();

        let lo = compare(&base, &loopback);
        let ro = compare(&base, &routable);
        assert_eq!(lo[0].severity, Severity::Low);
        assert_eq!(ro[0].severity, Severity::Medium);
        assert!(
            ro[0].severity > lo[0].severity,
            "a service reachable from off the machine is a different exposure \
             from one bound to 127.0.0.1, and an alerting tool that shouts at \
             `cargo run` is one nobody keeps running"
        );
    }

    #[test]
    fn a_vanished_listener_is_recorded_but_not_escalated() {
        let base = baseline_of(&[listener(22, true, Some("sshd"))]);
        let current: BTreeSet<Listener> = BTreeSet::new();
        let findings = compare(&base, &current);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "net.listener_gone");
        assert_eq!(findings[0].severity, Severity::Info);
    }

    /// The heuristic must never claim certainty.
    #[test]
    fn the_port_table_is_only_ever_possible() {
        let current: BTreeSet<Listener> = [listener(4444, true, Some("nc"))].into_iter().collect();
        let findings = reputation_findings(&current);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "net.notable_port");
        assert_eq!(
            findings[0].confidence,
            Confidence::Possible,
            "4444 is also a port a developer picked for a test server; a table \
             of conventions cannot produce a certainty"
        );
        assert_eq!(findings[0].severity, Severity::High);
        assert!(findings[0]
            .evidence
            .iter()
            .any(|e| e.kind == "net.port_convention"));
    }

    #[test]
    fn an_ordinary_port_matches_no_convention() {
        let current: BTreeSet<Listener> = [listener(8080, true, None)].into_iter().collect();
        assert!(reputation_findings(&current).is_empty());
    }

    /// An unattributed socket is not an unowned one.
    #[test]
    fn a_listener_with_no_readable_process_carries_none() {
        let base = baseline_of(&[listener(1, true, None)]);
        let current: BTreeSet<Listener> = [listener(1, true, None), listener(4711, true, None)]
            .into_iter()
            .collect();
        let findings = compare(&base, &current);
        assert!(
            !findings[0].evidence.iter().any(|e| e.kind == "net.process"),
            "no process evidence is emitted when none was readable, rather than \
             an 'unknown' string that reads like an observation"
        );
        match &findings[0].subject {
            Subject::Network { process, .. } => assert!(process.is_none()),
            other => panic!("expected a network subject, got {other:?}"),
        }
    }

    #[test]
    fn an_identical_socket_table_produces_nothing() {
        let ls = [
            listener(22, true, Some("sshd")),
            listener(80, true, Some("nginx")),
        ];
        let base = baseline_of(&ls);
        let current: BTreeSet<Listener> = ls.into_iter().collect();
        assert!(compare(&base, &current).is_empty());
    }

    #[test]
    fn a_baseline_round_trips_through_json() {
        let base = baseline_of(&[listener(22, true, Some("sshd"))]);
        let json = serde_json::to_string(&base).unwrap();
        let back: Baseline = serde_json::from_str(&json).unwrap();
        assert_eq!(back, base);
    }

    /// Against the machine this runs on: that enumeration either works or says
    /// why, and that what it returns is internally consistent.
    ///
    /// The first version said "not an assertion about what it finds — that
    /// varies" and then asserted every port was non-zero, which is exactly an
    /// assertion about what it finds. It held on Windows and failed on macOS,
    /// where unbound sockets report port 0. The filter for those now lives in
    /// `current_listeners`, so this checks an invariant rather than the habit of
    /// one platform.
    #[test]
    fn enumerating_this_machine_either_works_or_says_why() {
        match current_listeners() {
            Ok(ls) => {
                for l in &ls {
                    assert_ne!(
                        l.local_port, 0,
                        "current_listeners must filter unbound sockets: nothing can connect \n to them and they would churn every baseline",
                    );
                    assert!(
                        !l.protocol.is_empty(),
                        "a listener with no protocol is not a reading"
                    );
                }
            }
            Err(reason) => assert!(
                !reason.is_empty(),
                "a failure must carry a reason, or a caller cannot tell it from a quiet machine"
            ),
        }
    }
}
