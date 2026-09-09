// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! The automatic tuning server: watch what the machine is doing, and say what
//! would suit it.
//!
//! # It does not write unless told to, twice
//!
//! Default behaviour is [`Mode::Recommend`] — the loop classifies, plans, and
//! publishes. Nothing is applied. Turning that into [`Mode::Apply`] requires the
//! caller to pass a risk ceiling *and* confirmation, and even then every write
//! goes through [`crate::profile::apply::apply_setting`], which refuses without
//! `confirm` and writes an audit record.
//!
//! That is deliberately more friction than an optimiser usually has. AGENTS.md
//! states that writing requires `--confirm` and that the library never elevates
//! itself; a server that quietly rewrote power settings would make that sentence
//! false, and the sentence is load-bearing for anyone who deployed ironmon on the
//! strength of it.
//!
//! The ceiling defaults to [`SettingRisk::Safe`] and is capped at
//! [`SettingRisk::Moderate`] for unattended operation. `Dangerous` covers
//! power, thermal, voltage and MSR writes — the settings that can damage
//! hardware — and no unattended loop in this crate will apply one.

use super::{classify_from_signals, plan_from_settings, Classification, Plan, Signals};
use crate::profile::SettingRisk;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// What the server is allowed to do when it has a recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Publish recommendations. Write nothing. The default — and derived rather
    /// than hand-written so the default cannot be changed without changing which
    /// variant carries `#[default]`.
    #[default]
    Recommend,
    /// Apply recommendations at or below the ceiling, through the confirmed,
    /// audit-logged apply path.
    Apply {
        ceiling: SettingRisk,
        /// The caller's explicit confirmation, passed through to
        /// [`crate::profile::apply::apply_setting`]. Without it every write is
        /// refused, which makes an unconfirmed `Apply` mode equivalent to
        /// `Recommend` — by design, not by accident.
        confirm: bool,
    },
}

/// The highest risk an unattended loop may ever apply, regardless of what the
/// caller asks for.
///
/// A constant rather than a parameter because it is the safety property, and a
/// safety property that callers can raise is a default.
pub const UNATTENDED_CEILING: SettingRisk = SettingRisk::Moderate;

/// What one pass of the loop concluded and did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cycle {
    /// Seconds since the server started, so a series of cycles can be read as a
    /// timeline without the reader having to stamp them.
    pub elapsed_secs: u64,
    pub plan: Plan,
    /// Outcomes of any writes attempted, empty in [`Mode::Recommend`].
    pub applied: Vec<AppliedOutcome>,
    /// Recommendations that were within the plan but not applied, and why.
    pub withheld: Vec<Withheld>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedOutcome {
    pub setting_id: String,
    pub status: String,
    pub message: String,
    /// The full outcome of the write, carried so the cycle can be undone.
    ///
    /// Summarising the write to a status string threw away the value it
    /// overwrote, which left an autonomous loop able to change a machine and
    /// unable to change it back. [`revert_cycle`] needs this.
    #[serde(default)]
    pub outcome: Option<crate::profile::apply::ApplyOutcome>,
    /// Whether the write measurably helped, when the cycle was run with
    /// [`cycle_verified`].
    ///
    /// `None` means verification was not attempted — a plain [`cycle`] does not
    /// measure. That is deliberately distinct from
    /// [`Verdict::Unverifiable`](crate::tuning::verify::Verdict::Unverifiable),
    /// which means it *was* attempted and produced no answer. "Nobody looked"
    /// and "we looked and could not tell" are different facts about a machine.
    #[serde(default)]
    pub verdict: Option<crate::tuning::verify::Verdict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Withheld {
    pub setting_id: String,
    pub reason: String,
}

/// Put back everything a cycle applied, most recent write first.
///
/// An autonomous tuner that can only move a machine in one direction is not
/// something to leave running unattended. This is the way back: hand it a
/// [`Cycle`] and it undoes that cycle's writes through
/// [`crate::profile::apply::revert_setting`], which is confirmed and
/// audit-logged exactly like the writes it undoes.
///
/// Reverts in reverse order so that settings applied together come off in the
/// order they went on. Writes that were refused, failed, or recorded no prior
/// value are reported as un-revertible rather than skipped silently — a caller
/// asking "is this machine back where it started" deserves the whole answer,
/// including the parts that are not.
///
/// Writes that [`cycle_verified`] already undid are skipped: verification has
/// put them back, and reverting a revert would write the value the loop had just
/// decided was worse.
pub fn revert_cycle(cycle: &Cycle, confirm: bool) -> Vec<crate::profile::apply::ApplyOutcome> {
    cycle
        .applied
        .iter()
        .rev()
        .filter(|a| {
            !a.verdict
                .as_ref()
                .is_some_and(crate::tuning::verify::Verdict::warrants_revert)
        })
        .filter_map(|a| a.outcome.as_ref())
        .map(|o| crate::profile::apply::revert_setting(o, confirm))
        .collect()
}

/// Gather the signals the classifier reads.
///
/// Every field is independently fallible and independently absent: a machine
/// with no GPU still has a CPU, and a failure to read one signal must not empty
/// the others. `None` means "not read", which the classifier is careful to
/// distinguish from zero.
pub fn collect_signals() -> Signals {
    let mut signals = Signals::default();

    if let Ok(mut monitor) = crate::ai_workload::AiWorkloadMonitor::new() {
        if let Ok(workloads) = monitor.detect_workloads() {
            signals.ai_present = !workloads.is_empty();
            signals.ai_training = workloads.iter().any(|w| w.is_training());
            // An unidentified framework is dropped rather than listed. Without
            // this the set carries the literal "unknown" as if it were a
            // framework name -- the same absence-word defect `push_str_as`
            // guards against in the ontology, arriving through a different door.
            signals.ai_frameworks = workloads
                .iter()
                .filter(|w| w.framework != crate::ai_workload::AiFramework::Unknown)
                .map(|w| format!("{:?}", w.framework).to_lowercase())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
        }
    }

    if let Ok(monitor) = crate::UnifiedMonitor::new() {
        if let Ok(gpus) = monitor.snapshot_gpus() {
            // `None` when no GPU was enumerated at all, which the classifier
            // reports as an absent reading rather than as 0%.
            // ...and now also `None` when no enumerated GPU reported a
            // utilization counter, which the bare `u8` could not distinguish
            // from every device sitting at 0%.
            signals.gpu_utilization = gpus
                .iter()
                .filter_map(|g| g.utilization())
                .map(f32::from)
                .fold(None, |acc: Option<f32>, u| {
                    Some(acc.map_or(u, |a| a.max(u)))
                });
        }
    }

    signals.cpu_utilization = read_cpu_utilization();

    signals.game_processes = detect_game_processes();

    signals
}

/// System-wide CPU busy percentage, read from the platform directly.
///
/// Deliberately *not* via `ontology::resolve::snapshot()`, which would be the
/// obvious reuse: that resolves every domain — every disk's SMART, every PCI
/// device, every USB descriptor — to obtain one number, and took tens of seconds
/// per cycle when this was written that way. A tuning loop that expensive cannot
/// run at the interval it needs to be useful at.
pub(crate) fn read_cpu_utilization() -> Option<f32> {
    #[cfg(windows)]
    {
        crate::platform::windows::read_cpu_stats()
            .ok()
            .map(|s| 100.0 - s.total.idle)
    }
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::read_cpu_stats()
            .ok()
            .map(|s| 100.0 - s.total.idle)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

/// Processes whose names match known game launchers or engines.
///
/// A name table is a blunt instrument and will miss most games; it is used only
/// as corroborating evidence, and the classifier assigns it lower confidence
/// than an identified AI framework for exactly that reason. It is not a
/// substitute for asking the graphics driver what is presenting, which is the
/// right long-term answer and is not implemented here.
fn detect_game_processes() -> Vec<String> {
    const MARKERS: [&str; 8] = [
        "steam",
        "epicgameslauncher",
        "battle.net",
        "gog galaxy",
        "riotclientservices",
        "eadesktop",
        "ubisoftconnect",
        "unrealengine",
    ];

    let Ok(mut monitor) = crate::process_monitor::ProcessMonitor::new() else {
        return Vec::new();
    };
    let Ok(processes) = monitor.processes() else {
        return Vec::new();
    };

    let mut found: Vec<String> = processes
        .iter()
        .map(|p| p.name.to_ascii_lowercase())
        .filter(|name| MARKERS.iter().any(|m| name.contains(m)))
        .collect();
    found.sort();
    found.dedup();
    found
}

/// How a verifying cycle measures.
///
/// A parameter rather than a constant because both numbers are properties of
/// the setting being changed — how long a governor takes to take effect, and
/// how long the metric needs to be watched — and this module does not know
/// which setting is coming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyPolicy {
    /// How long to wait after the write before measuring again.
    pub settle: Duration,
    pub plan: crate::tuning::verify::SamplingPlan,
}

impl Default for VerifyPolicy {
    fn default() -> Self {
        Self {
            settle: Duration::from_secs(5),
            plan: crate::tuning::verify::SamplingPlan::default(),
        }
    }
}

/// Run one pass: collect, classify, plan, and apply if the mode says to.
pub fn cycle(mode: Mode, started: Instant) -> Cycle {
    let signals = collect_signals();
    let classification = classify_from_signals(&signals);
    cycle_with(mode, started, classification)
}

/// Run one pass, and measure whether each write helped.
///
/// The closed loop: measure, write, settle, measure, and undo anything that
/// demonstrably made the machine worse. See [`crate::tuning::verify`] for what
/// "demonstrably" is allowed to mean — in particular, that most settings have
/// no defensible metric and are reported as unverifiable rather than as
/// successes.
///
/// Costs real time: each verified write spends two sampling windows plus the
/// settle period. A loop running this needs an interval that accommodates it.
pub fn cycle_verified(mode: Mode, started: Instant, policy: &VerifyPolicy) -> Cycle {
    let mut scratch = crate::tuning::ledger::Ledger::new();
    cycle_verified_with(mode, started, policy, &mut scratch)
}

/// A verified pass that remembers, across cycles, what it already learned.
///
/// Without a ledger the loop relearns every result forever: a setting measured
/// as a regression is reverted, then proposed again next cycle, applied again,
/// measured again, reverted again. Two writes and several seconds of sampling
/// per interval, producing a machine that flaps between two configurations.
/// [`cycle_verified`] passes a scratch ledger and therefore still does exactly
/// that; it is kept for callers who genuinely want each cycle judged fresh, and
/// this is the one to use for anything long-running.
pub fn cycle_verified_with(
    mode: Mode,
    started: Instant,
    policy: &VerifyPolicy,
    ledger: &mut crate::tuning::ledger::Ledger,
) -> Cycle {
    let signals = collect_signals();
    let classification = classify_from_signals(&signals);
    cycle_inner(mode, started, classification, Some(policy), Some(ledger))
}

/// One pass with the classification already made, so a caller that classified by
/// some other means — a model, or an explicit override — reuses the rest.
pub fn cycle_with(mode: Mode, started: Instant, classification: Classification) -> Cycle {
    cycle_inner(mode, started, classification, None, None)
}

/// `policy` present means measure each write and undo the ones that hurt.
fn cycle_inner(
    mode: Mode,
    started: Instant,
    classification: Classification,
    policy: Option<&VerifyPolicy>,
    mut ledger: Option<&mut crate::tuning::ledger::Ledger>,
) -> Cycle {
    let settings = current_settings();
    let plan = plan_from_settings(classification, &settings);

    let mut applied = Vec::new();
    let mut withheld = Vec::new();

    match mode {
        Mode::Recommend => {
            for rec in &plan.recommendations {
                withheld.push(Withheld {
                    setting_id: rec.setting_id.clone(),
                    reason: "server is in recommend mode and will not write".to_string(),
                });
            }
        }
        Mode::Apply { ceiling, confirm } => {
            // The ceiling the caller asked for, floored to what an unattended
            // loop may do. Asking for `Dangerous` does not get `Dangerous`.
            let effective = min_risk(ceiling, UNATTENDED_CEILING);

            for rec in &plan.recommendations {
                if !plan.within_risk(effective).contains(&rec) {
                    withheld.push(Withheld {
                        setting_id: rec.setting_id.clone(),
                        reason: format!(
                            "risk {:?} exceeds the unattended ceiling {:?}",
                            rec.risk, effective
                        ),
                    });
                    continue;
                }
                // Unattended writes must be reversible. `apply_setting_reversible`
                // refuses when the prior value could not be read, rather than
                // making a change this loop could never undo -- there is nobody
                // watching to be told that the way back was lost.
                // What the loop already learned about this setting, before it
                // spends two sampling windows learning it again.
                if let Some(skip) = ledger.as_ref().and_then(|l| l.skip(&rec.setting_id)) {
                    withheld.push(Withheld {
                        setting_id: rec.setting_id.clone(),
                        reason: skip.reason(&rec.setting_id),
                    });
                    continue;
                }

                let (outcome, verdict) = match policy {
                    Some(p) => {
                        let v = crate::tuning::verify::apply_verified(
                            &rec.setting_id,
                            rec.proposed.clone(),
                            confirm,
                            p.settle,
                            &p.plan,
                        );
                        // A reverted write is reported as what it was — a write
                        // that happened and was undone — not quietly dropped
                        // from the cycle. A reader asking what this loop did to
                        // the machine is owed both halves.
                        (v.outcome, Some(v.verdict))
                    }
                    None => (
                        crate::profile::apply::apply_setting_reversible(
                            &rec.setting_id,
                            rec.proposed.clone(),
                            confirm,
                        ),
                        None,
                    ),
                };
                if let (Some(l), Some(v)) = (ledger.as_mut(), verdict.as_ref()) {
                    l.record(&rec.setting_id, v.clone());
                }
                applied.push(AppliedOutcome {
                    setting_id: rec.setting_id.clone(),
                    status: format!("{:?}", outcome.status),
                    message: outcome.message.clone(),
                    outcome: Some(outcome),
                    verdict,
                });
            }
        }
    }

    Cycle {
        elapsed_secs: started.elapsed().as_secs(),
        plan,
        applied,
        withheld,
    }
}

fn min_risk(a: SettingRisk, b: SettingRisk) -> SettingRisk {
    let rank = |r: SettingRisk| match r {
        SettingRisk::Informational => 0u8,
        SettingRisk::Safe => 1,
        SettingRisk::Moderate => 2,
        SettingRisk::Dangerous => 3,
    };
    if rank(a) <= rank(b) {
        a
    } else {
        b
    }
}

/// Every setting the profile inspector can read, flattened.
fn current_settings() -> Vec<crate::profile::Setting> {
    let mut inspector = crate::profile::ProfileInspector::new();
    let snapshot = inspector.snapshot_all();
    // Every subsystem the inspector knows, so a policy added later for NVMe or
    // display needs no edit here.
    snapshot
        .providers
        .values()
        .flat_map(|groups| groups.iter())
        .flat_map(|group| group.settings.iter().cloned())
        .collect()
}

/// Run the loop until `stop` returns true.
///
/// `on_cycle` receives each pass. The caller decides what publishing means —
/// stdout, the HTTP surface, a log — so this stays testable without a socket.
pub fn run<F, S>(mode: Mode, interval: Duration, mut on_cycle: F, mut stop: S)
where
    F: FnMut(&Cycle),
    S: FnMut() -> bool,
{
    let started = Instant::now();
    while !stop() {
        let c = cycle(mode, started);
        on_cycle(&c);
        // Sleep in slices so a stop request is noticed promptly rather than after
        // a whole interval, which at a minute makes the difference between
        // responsive and apparently hung.
        let deadline = Instant::now() + interval;
        while Instant::now() < deadline {
            if stop() {
                return;
            }
            std::thread::sleep(Duration::from_millis(200).min(interval));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Subsystem;
    use crate::tuning::{Method, Recommendation, UseCase};

    fn plan_with(risk: SettingRisk) -> Classification {
        let _ = risk;
        Classification {
            use_case: UseCase::AiTraining,
            confidence: 0.9,
            method: Method::Signals,
            evidence: vec!["test".into()],
        }
    }

    /// The default must not write. If this ever fails, a deployment that took
    /// AGENTS.md at its word is being written to without asking.
    #[test]
    fn the_default_mode_is_recommend_only() {
        assert_eq!(Mode::default(), Mode::Recommend);
    }

    /// Recommend mode withholds everything and says so, rather than silently
    /// producing an empty applied list that reads like "nothing to do".
    #[test]
    fn recommend_mode_withholds_with_a_reason() {
        let c = cycle_with(
            Mode::Recommend,
            Instant::now(),
            plan_with(SettingRisk::Safe),
        );
        assert!(c.applied.is_empty(), "recommend mode must not write");
        for w in &c.withheld {
            assert!(w.reason.contains("recommend mode"), "{}", w.reason);
        }
        assert_eq!(c.withheld.len(), c.plan.recommendations.len());
    }

    /// A caller asking for `Dangerous` does not get it. This is the property that
    /// keeps an unattended loop away from voltage and power registers.
    #[test]
    fn an_unattended_apply_is_capped_below_dangerous() {
        assert_eq!(
            min_risk(SettingRisk::Dangerous, UNATTENDED_CEILING),
            SettingRisk::Moderate
        );
        assert_eq!(
            min_risk(SettingRisk::Safe, UNATTENDED_CEILING),
            SettingRisk::Safe
        );
    }

    /// A `Dangerous` recommendation is withheld with the ceiling named, even when
    /// the caller confirmed — confirmation authorises a write, it does not raise
    /// the unattended ceiling.
    #[test]
    fn a_dangerous_recommendation_is_withheld_even_when_confirmed() {
        let plan = Plan {
            classification: plan_with(SettingRisk::Dangerous),
            recommendations: vec![Recommendation {
                setting_id: "some_voltage_thing".into(),
                subsystem: Subsystem::Gpu,
                display_name: "Voltage".into(),
                current: crate::profile::SettingValue::Int(1000),
                proposed: crate::profile::SettingValue::Int(1100),
                risk: SettingRisk::Dangerous,
                basis: "test".into(),
                rationale: "test".into(),
            }],
            skipped: vec![],
        };
        let effective = min_risk(SettingRisk::Dangerous, UNATTENDED_CEILING);
        assert!(
            plan.within_risk(effective).is_empty(),
            "a Dangerous recommendation must not survive the unattended ceiling"
        );
    }

    /// The loop must exit on request rather than after a full interval, or a
    /// minute-long interval makes shutdown look like a hang.
    #[test]
    fn the_loop_stops_promptly_when_asked() {
        // A wide fraction of a deliberately long interval, not a small absolute
        // time. The first draft asserted "under 5 seconds", which passed alone and
        // failed inside the full suite: a cycle takes ~2.5 s and the suite runs
        // hundreds of tests in parallel, so the machine's load decided the
        // outcome rather than the code. What is actually being asserted is that
        // the sleep was cut short, and this gap says that however loaded the box.
        const INTERVAL: Duration = Duration::from_secs(300);
        const GENEROUS: Duration = Duration::from_secs(60);

        let mut cycles = 0;
        let started = Instant::now();
        run(Mode::Recommend, INTERVAL, |_| cycles += 1, {
            let mut calls = 0;
            move || {
                calls += 1;
                // Run one cycle, then stop at the first in-sleep check.
                calls > 1
            }
        });
        assert_eq!(cycles, 1);
        assert!(
            started.elapsed() < GENEROUS,
            "stopping took {:?} against a {INTERVAL:?} interval, so the stop request \
             was not noticed until the sleep ended",
            started.elapsed()
        );
    }

    /// Signal collection must survive a machine where every source fails; an
    /// optimiser that panics on an unusual box is worse than one that says it
    /// cannot tell.
    #[test]
    fn collecting_signals_never_panics() {
        let s = collect_signals();
        let c = classify_from_signals(&s);
        assert!(!c.evidence.is_empty());
    }

    /// A cycle that applied nothing has nothing to undo, and says so by
    /// returning nothing rather than by erroring.
    #[test]
    fn reverting_a_recommend_only_cycle_is_a_no_op() {
        let cycle = cycle(Mode::Recommend, Instant::now());
        assert!(cycle.applied.is_empty(), "Recommend mode must not write");
        assert!(revert_cycle(&cycle, true).is_empty());
    }

    /// Writes come off in the reverse of the order they went on, and a write
    /// that recorded no prior value is reported rather than skipped: a caller
    /// asking whether the machine is back where it started must not be told
    /// "yes" by silence.
    #[test]
    fn revert_undoes_in_reverse_and_reports_the_unrevertible() {
        use crate::profile::apply::{ApplyOutcome, ApplyStatus};
        use crate::profile::{SettingValue, Subsystem};

        let mk = |id: &str, previous: Option<&str>| AppliedOutcome {
            setting_id: id.into(),
            status: "Applied".into(),
            message: String::new(),
            outcome: Some(ApplyOutcome {
                setting_id: id.into(),
                subsystem: Subsystem::Cpu,
                requested: SettingValue::Text("new".into()),
                status: ApplyStatus::Applied,
                message: String::new(),
                timestamp: 0,
                previous: previous.map(|p| SettingValue::Text(p.into())),
            }),
            verdict: None,
        };

        let cycle = Cycle {
            elapsed_secs: 0,
            plan: Plan {
                classification: Classification {
                    use_case: crate::tuning::UseCase::Idle,
                    confidence: 0.0,
                    method: crate::tuning::Method::Signals,
                    evidence: vec![],
                },
                recommendations: vec![],
                skipped: vec![],
            },
            applied: vec![mk("first", Some("old")), mk("second", None)],
            withheld: vec![],
        };

        let reverts = revert_cycle(&cycle, true);
        assert_eq!(reverts.len(), 2, "every applied write is accounted for");
        assert_eq!(
            reverts[0].setting_id, "second",
            "the last write applied is the first undone"
        );
        // "second" recorded no prior value, so it cannot be put back, and the
        // refusal is visible in the result.
        assert_eq!(reverts[0].status, ApplyStatus::NotWritable);
        assert!(reverts[0].message.contains("no prior value"));
    }

    /// The loop stops relearning a result it already has.
    ///
    /// Without the ledger this is an infinite thrash: a setting measured as a
    /// regression is reverted, proposed again next cycle, applied again,
    /// measured again, reverted again — two writes and several seconds of
    /// sampling per interval, forever, on a machine that flaps between two
    /// configurations.
    ///
    /// Driven through the ledger rather than a live cycle because a live cycle
    /// writes to the machine it runs on. What is under test is the gate: that a
    /// recorded regression becomes a withheld recommendation carrying a reason,
    /// and that an unverifiable result does not.
    #[test]
    fn a_measured_regression_is_withheld_on_the_next_cycle() {
        use crate::tuning::ledger::{Ledger, Skip};

        let mut ledger = Ledger::new();
        ledger.record(
            "scaling_governor",
            crate::tuning::verify::Verdict::Regressed {
                metric: "m".into(),
                before: 10.0,
                after: 20.0,
                delta: 10.0,
            },
        );

        let skip = ledger.skip("scaling_governor");
        assert_eq!(skip, Some(Skip::Regressed));

        let reason = skip.unwrap().reason("scaling_governor");
        assert!(
            reason.contains("scaling_governor") && reason.contains("regression"),
            "a withheld recommendation must say what was measured, not merely that it was skipped: {reason}"
        );

        // The exception that keeps the loop alive: every setting is currently
        // unverifiable, so if that counted as a result the loop would stop
        // acting entirely after one cycle.
        ledger.record(
            "active_scheme_guid",
            crate::tuning::verify::Verdict::Unverifiable {
                reason: "no metric is declared".into(),
            },
        );
        assert_eq!(
            ledger.skip("active_scheme_guid"),
            None,
            "an absent measurement must not be treated as evidence against a setting"
        );
    }

    /// A write that verification already undid must not be undone again.
    ///
    /// Reverting a revert writes back the value the loop had just measured as
    /// worse — the tuner would undo its own correction and report success at
    /// having done so.
    #[test]
    fn a_write_verification_already_reverted_is_not_reverted_twice() {
        use crate::profile::apply::{ApplyOutcome, ApplyStatus};
        use crate::profile::SettingValue;

        let mk = |id: &str, verdict: Option<crate::tuning::verify::Verdict>| AppliedOutcome {
            setting_id: id.into(),
            status: "Applied".into(),
            message: String::new(),
            outcome: Some(ApplyOutcome {
                setting_id: id.into(),
                subsystem: Subsystem::Cpu,
                requested: SettingValue::Text("new".into()),
                status: ApplyStatus::Applied,
                message: String::new(),
                timestamp: 0,
                previous: Some(SettingValue::Text("old".into())),
            }),
            verdict,
        };

        let regressed = crate::tuning::verify::Verdict::Regressed {
            metric: "m".into(),
            before: 10.0,
            after: 20.0,
            delta: 10.0,
        };
        let improved = crate::tuning::verify::Verdict::Improved {
            metric: "m".into(),
            before: 20.0,
            after: 10.0,
            delta: -10.0,
        };

        let cycle = Cycle {
            elapsed_secs: 0,
            plan: Plan {
                classification: Classification {
                    use_case: crate::tuning::UseCase::Idle,
                    confidence: 0.0,
                    method: crate::tuning::Method::Signals,
                    evidence: vec![],
                },
                recommendations: vec![],
                skipped: vec![],
            },
            applied: vec![
                mk("kept", Some(improved)),
                mk("already_undone", Some(regressed)),
                mk("unmeasured", None),
            ],
            withheld: vec![],
        };

        let reverts = revert_cycle(&cycle, true);
        let ids: Vec<&str> = reverts.iter().map(|r| r.setting_id.as_str()).collect();
        assert!(
            !ids.contains(&"already_undone"),
            "verification put this one back; reverting it again would restore the value that was measured as worse. got {ids:?}"
        );
        assert!(
            ids.contains(&"kept"),
            "an improvement is still the caller's to undo if they ask"
        );
        assert!(
            ids.contains(&"unmeasured"),
            "a write nobody measured must still be revertible"
        );
    }
}
