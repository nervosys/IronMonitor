// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Did the setting help? Measure, decide, and put it back if it hurt.
//!
//! This closes the loop that [`crate::tuning::serve`] left open. 5.1.0 made
//! writes reversible; nothing yet decided *whether to reverse them*. A tuner
//! that can apply and undo but cannot tell the two apart is still guessing, it
//! just guesses in both directions.
//!
//! # The rule this module exists to enforce
//!
//! **A loop that invents a success criterion is the optimiser equivalent of a
//! model picking a power limit.** The same rule that governs proposed values
//! governs this: the criterion has to come from somewhere defensible, not from
//! whatever number happened to be readable.
//!
//! So the verdict space includes [`Verdict::Unverifiable`], and it is the
//! default rather than the exception. If no metric is declared for a setting, or
//! the metric cannot be read, or the machine is too idle for the metric to mean
//! anything, the answer is "unverifiable" — never "improved".
//!
//! # Why the registry is nearly empty
//!
//! Not an oversight, and not a stub to fill in later. Declaring a metric means
//! claiming that this number moves when that setting changes, and most of those
//! claims do not survive being checked.
//!
//! The obvious metric for a CPU power-scheme change is achieved clock speed, and
//! on Windows it does not work. `CallNtPowerInformation(ProcessorInformation)`
//! reports `CurrentMhz`, which is what [`crate::platform::windows::read_cpu_stats`]
//! surfaces as `CpuFrequency::current`. Measured on the development machine: 16
//! spinning threads took system idle from 79.7% to 11.4%, and every core
//! reported exactly 4400 MHz throughout, before and after. It is the nominal
//! clock, not a live one. A verifier built on it would have reported "no
//! change" for every power scheme in existence and been believed.
//!
//! That is the whole reason [`metric_for`] returns `None` for
//! `active_scheme_guid` on Windows. The setting is applied and reversible; it is
//! simply not measurable with anything this crate can currently read, and saying
//! so is the honest output.
//!
//! # Noise
//!
//! One reading before and one after is noise, not evidence. Every measurement is
//! a window of samples reduced to a median, with a median absolute deviation
//! carried alongside it. A difference counts only when it clears both the
//! metric's declared minimum effect *and* the noise observed in the two windows
//! themselves — so a metric that happens to be jittery today raises its own bar
//! rather than producing a confident verdict from noise.
//!
//! # What gets reverted
//!
//! Only [`Verdict::Regressed`]. Not `Unchanged`, and emphatically not
//! `Unverifiable` — reverting on "I could not tell" would undo nearly every
//! write this crate makes, which is a different way of ignoring the measurement
//! rather than a safer one. A setting that demonstrably made things worse comes
//! off; everything else is left where the caller put it, and reported.

use crate::profile::apply::{ApplyOutcome, ApplyStatus};
use crate::profile::SettingValue;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Which way is better for a given metric.
///
/// Explicit because it is not inferable. Higher GPU utilisation is usually
/// good; higher temperature is not; and nothing about the number itself says
/// which kind it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    HigherIsBetter,
    LowerIsBetter,
}

/// What a metric is, and what counts as it moving.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricSpec {
    pub id: String,
    pub direction: Direction,
    /// The smallest change worth calling a change, in the metric's own units.
    ///
    /// Set from what the metric means, not from what is statistically
    /// detectable: a 0.3% shift in GPU utilisation may be perfectly real and
    /// still not worth a machine reconfiguring itself over.
    pub min_effect: f64,
    /// Whether the metric is meaningless on an idle machine.
    ///
    /// Achieved clocks and utilisation say nothing when there is no work to do.
    /// A metric marked this way yields [`Verdict::Unverifiable`] rather than a
    /// confident reading taken from an idle box.
    pub requires_load: bool,
}

/// Something that can be sampled repeatedly.
///
/// A trait rather than an enum of known metrics so the comparison logic can be
/// tested against a source with known behaviour. The alternative — testing only
/// through real hardware readings — is how a verifier ends up validated on
/// exactly the machine that wrote it.
pub trait MetricSource: Send + Sync {
    fn spec(&self) -> MetricSpec;
    /// One reading, or `None` if it could not be taken.
    ///
    /// `None` is not zero. A metric that fails to read must not be averaged in
    /// as an absence of the quantity.
    fn sample(&self) -> Option<f64>;
}

/// How to take a window of samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SamplingPlan {
    pub count: usize,
    pub gap: Duration,
}

impl Default for SamplingPlan {
    /// Eight samples a quarter-second apart: two seconds of wall clock.
    ///
    /// Enough for a median to mean something without making a tuning cycle feel
    /// hung. Callers measuring something slow-moving should say so rather than
    /// relying on this.
    fn default() -> Self {
        Self {
            count: 8,
            gap: Duration::from_millis(250),
        }
    }
}

/// A window of samples, reduced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    /// Every reading that succeeded, in order taken. Kept so a reader can check
    /// the summary rather than trust it.
    pub samples: Vec<f64>,
    /// Middle sample. Median rather than mean: one scheduling hiccup should not
    /// move the result.
    pub median: f64,
    /// Median absolute deviation — how much this window disagreed with itself.
    pub mad: f64,
    /// Readings that were attempted and failed.
    pub missed: usize,
}

impl Measurement {
    /// Reduce raw readings. `None` when nothing was readable at all.
    pub fn from_samples(samples: Vec<f64>, missed: usize) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let median = median_of(&samples);
        let deviations: Vec<f64> = samples.iter().map(|s| (s - median).abs()).collect();
        let mad = median_of(&deviations);
        Some(Self {
            samples,
            median,
            mad,
            missed,
        })
    }
}

fn median_of(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    }
}

/// What the measurement concluded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "verdict")]
pub enum Verdict {
    /// The metric moved the good way by more than noise and more than the
    /// declared minimum effect.
    Improved {
        metric: String,
        before: f64,
        after: f64,
        delta: f64,
    },
    /// The metric was readable and did not move enough to call.
    ///
    /// Distinct from `Unverifiable`: this one means the measurement succeeded
    /// and found nothing, which is a result. The other means there was no
    /// measurement.
    Unchanged {
        metric: String,
        before: f64,
        after: f64,
        delta: f64,
        /// What the change would have had to clear.
        threshold: f64,
    },
    /// The metric moved the wrong way by more than noise. This is the only
    /// verdict that triggers a revert.
    Regressed {
        metric: String,
        before: f64,
        after: f64,
        delta: f64,
    },
    /// No verdict was reached, and why.
    ///
    /// The default outcome, not the failure case. Most settings have no
    /// defensible metric and this says so rather than inventing one.
    Unverifiable { reason: String },
}

impl Verdict {
    /// Whether this verdict calls for putting the setting back.
    pub fn warrants_revert(&self) -> bool {
        matches!(self, Verdict::Regressed { .. })
    }
}

/// Take a window of samples according to `plan`.
///
/// Sleeps between samples, so this costs `count * gap` of wall clock and is not
/// for a hot path.
pub fn measure(source: &dyn MetricSource, plan: &SamplingPlan) -> Option<Measurement> {
    let mut samples = Vec::with_capacity(plan.count);
    let mut missed = 0usize;
    for i in 0..plan.count {
        match source.sample() {
            Some(v) => samples.push(v),
            None => missed += 1,
        }
        if i + 1 < plan.count {
            std::thread::sleep(plan.gap);
        }
    }
    Measurement::from_samples(samples, missed)
}

/// Compare two windows of the same metric.
///
/// The threshold is the larger of the metric's declared minimum effect and the
/// noise the two windows showed, so a jittery metric demands a bigger move
/// before it is believed.
pub fn compare(spec: &MetricSpec, before: &Measurement, after: &Measurement) -> Verdict {
    // A window in which most readings failed is the median of whatever happened
    // to succeed, which can be a different metric's behaviour. `missed` was
    // recorded but never consulted, so a window of three good readings out of
    // eight attempts could decide a verdict as firmly as a clean one. Require a
    // strict majority of the attempts to have succeeded before believing either
    // window; otherwise the question stays open, and the verdict says so.
    for (window, label) in [(before, "before"), (after, "after")] {
        let succeeded = window.samples.len();
        let attempted = succeeded + window.missed;
        if succeeded * 2 <= attempted {
            return Verdict::Unverifiable {
                reason: format!(
                    "{} is inconclusive: only {succeeded} of {attempted} readings {label} \
                     the write succeeded, which is not a majority",
                    spec.id
                ),
            };
        }
    }

    let delta = after.median - before.median;
    let noise = before.mad + after.mad;
    let threshold = spec.min_effect.max(noise);

    if delta.abs() < threshold {
        return Verdict::Unchanged {
            metric: spec.id.clone(),
            before: before.median,
            after: after.median,
            delta,
            threshold,
        };
    }

    let better = match spec.direction {
        Direction::HigherIsBetter => delta > 0.0,
        Direction::LowerIsBetter => delta < 0.0,
    };

    if better {
        Verdict::Improved {
            metric: spec.id.clone(),
            before: before.median,
            after: after.median,
            delta,
        }
    } else {
        Verdict::Regressed {
            metric: spec.id.clone(),
            before: before.median,
            after: after.median,
            delta,
        }
    }
}

/// The metric declared for a setting, if any.
///
/// Returning `None` is the common case and is a real answer. See the module
/// documentation for why `active_scheme_guid` is not in here: its obvious
/// metric was checked and does not move.
pub fn metric_for(setting_id: &str) -> Option<Box<dyn MetricSource>> {
    // The registry is deliberately empty. Every candidate so far has failed the
    // test of "does this number actually move when the setting changes", and a
    // registry entry is a claim that it does.
    //
    // To add one: demonstrate the movement first, on real hardware, and put the
    // measurement in the commit message. An entry added because the mapping
    // seemed reasonable is exactly the invented criterion this module exists to
    // refuse. `no_setting_claims_a_metric_it_has_not_earned` is the reminder.
    let _ = setting_id;
    None
}

/// A write, its verdict, and whatever was done about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifiedOutcome {
    pub outcome: ApplyOutcome,
    pub verdict: Verdict,
    /// The revert, when the verdict warranted one. `None` means no revert was
    /// attempted — check `verdict` for whether that is because nothing was
    /// wrong or because nothing could be told.
    pub reverted: Option<ApplyOutcome>,
}

/// Apply a setting, measure whether it helped, and put it back if it hurt.
///
/// The sequence is: measure, write, wait out `settle`, measure again, decide.
/// The settle period is not optional and has no sensible default here — how long
/// a governor takes to take effect is a property of the setting, not of this
/// function.
///
/// A write that did not succeed is never measured: there is nothing to attribute
/// a change to, and sampling anyway would produce a verdict about the weather.
pub fn apply_verified(
    setting_id: &str,
    value: SettingValue,
    confirm: bool,
    settle: Duration,
    plan: &SamplingPlan,
) -> VerifiedOutcome {
    let Some(source) = metric_for(setting_id) else {
        // No metric: still apply, still reversible, but say plainly that nothing
        // was measured rather than implying the write was validated.
        let outcome = crate::profile::apply::apply_setting_reversible(setting_id, value, confirm);
        return VerifiedOutcome {
            outcome,
            verdict: Verdict::Unverifiable {
                reason: format!("no metric is declared for {setting_id}"),
            },
            reverted: None,
        };
    };

    let spec = source.spec();

    if spec.requires_load {
        let reason = match machine_is_loaded() {
            Some(true) => None,
            Some(false) => Some(format!(
                "{} is only meaningful under load and the machine is idle",
                spec.id
            )),
            // Not read is not idle: the load question stays open, and says so.
            None => Some(format!(
                concat!(
                    "{} is only meaningful under load, and CPU utilisation could not ",
                    "be read to tell whether the machine is loaded"
                ),
                spec.id
            )),
        };
        if let Some(reason) = reason {
            let outcome =
                crate::profile::apply::apply_setting_reversible(setting_id, value, confirm);
            return VerifiedOutcome {
                outcome,
                verdict: Verdict::Unverifiable { reason },
                reverted: None,
            };
        }
    }

    verify_around(
        source.as_ref(),
        settle,
        plan,
        || crate::profile::apply::apply_setting_reversible(setting_id, value, confirm),
        |o| crate::profile::apply::revert_setting(o, confirm),
    )
}

/// The measure–write–settle–measure–decide sequence, with the write and the
/// revert supplied by the caller.
///
/// Separated from [`apply_verified`] so the decision path can be driven with a
/// scripted metric and fake writes. Without this the branch that matters most —
/// a regression actually causing a revert — would be reachable only through a
/// real registered setting on real hardware, which is to say untested. That is
/// the same gap that let a one-way write path look complete in 5.0.0.
fn verify_around<A, R>(
    source: &dyn MetricSource,
    settle: Duration,
    plan: &SamplingPlan,
    apply: A,
    revert: R,
) -> VerifiedOutcome
where
    A: FnOnce() -> ApplyOutcome,
    R: FnOnce(&ApplyOutcome) -> ApplyOutcome,
{
    let spec = source.spec();

    // Baseline first: after the write there is no way back to it.
    let before = measure(source, plan);

    let outcome = apply();
    if outcome.status != ApplyStatus::Applied {
        return VerifiedOutcome {
            outcome,
            verdict: Verdict::Unverifiable {
                reason: "the write did not take effect, so there is nothing to measure".to_string(),
            },
            reverted: None,
        };
    }

    let Some(before) = before else {
        return VerifiedOutcome {
            outcome,
            verdict: Verdict::Unverifiable {
                reason: format!("{} could not be read before the write", spec.id),
            },
            reverted: None,
        };
    };

    std::thread::sleep(settle);

    let Some(after) = measure(source, plan) else {
        return VerifiedOutcome {
            outcome,
            verdict: Verdict::Unverifiable {
                reason: format!("{} could not be read after the write", spec.id),
            },
            reverted: None,
        };
    };

    let verdict = compare(&spec, &before, &after);
    let reverted = if verdict.warrants_revert() {
        Some(revert(&outcome))
    } else {
        None
    };

    VerifiedOutcome {
        outcome,
        verdict,
        reverted,
    }
}

/// Is there enough work running for a load-dependent metric to mean anything?
///
/// Threshold rather than any-activity: an otherwise idle machine still shows a
/// few percent from the monitor itself, and treating that as load is how an
/// idle box produces confident numbers.
///
/// `None` when the CPU reading was not obtained. That used to be `false` (not
/// loaded), which sent every unreadable CPU down the idle path.
fn machine_is_loaded() -> Option<bool> {
    const BUSY_PERCENT: f32 = 20.0;
    // The CPU reader directly, not `collect_signals`. That function also builds
    // an AI-workload monitor, snapshots every GPU through NVML and enumerates
    // the process table twice, and this needs one number from it. Measured on
    // the development machine: 1465 ms against 0.2 ms, a factor of about seven
    // thousand, paid on every verified apply of a load-dependent metric.
    //
    // `serve::read_cpu_utilization` carries a comment explaining that it does
    // not go through `ontology::resolve::snapshot()` because resolving every
    // domain to obtain one number took tens of seconds per cycle. This made the
    // same mistake three functions below that warning.
    crate::tuning::serve::read_cpu_utilization().map(|u| u >= BUSY_PERCENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A metric with scripted readings, so the comparison logic is testable
    /// without hardware that happens to agree with it.
    struct Scripted {
        spec: MetricSpec,
        readings: std::sync::Mutex<Vec<Option<f64>>>,
    }

    impl Scripted {
        fn new(direction: Direction, min_effect: f64, readings: Vec<Option<f64>>) -> Self {
            Self {
                spec: MetricSpec {
                    id: "scripted".to_string(),
                    direction,
                    min_effect,
                    requires_load: false,
                },
                readings: std::sync::Mutex::new(readings),
            }
        }
    }

    impl MetricSource for Scripted {
        fn spec(&self) -> MetricSpec {
            self.spec.clone()
        }
        fn sample(&self) -> Option<f64> {
            let mut r = self.readings.lock().unwrap();
            if r.is_empty() {
                None
            } else {
                r.remove(0)
            }
        }
    }

    fn spec(direction: Direction, min_effect: f64) -> MetricSpec {
        MetricSpec {
            id: "m".to_string(),
            direction,
            min_effect,
            requires_load: false,
        }
    }

    fn measurement(values: &[f64]) -> Measurement {
        Measurement::from_samples(values.to_vec(), 0).expect("non-empty")
    }

    #[test]
    fn a_clear_move_the_good_way_is_an_improvement() {
        let before = measurement(&[10.0, 10.0, 10.0, 10.0]);
        let after = measurement(&[20.0, 20.0, 20.0, 20.0]);
        let v = compare(&spec(Direction::HigherIsBetter, 1.0), &before, &after);
        assert!(matches!(v, Verdict::Improved { .. }), "got {v:?}");
        assert!(!v.warrants_revert());
    }

    #[test]
    fn the_same_move_is_a_regression_when_lower_is_better() {
        let before = measurement(&[10.0, 10.0, 10.0, 10.0]);
        let after = measurement(&[20.0, 20.0, 20.0, 20.0]);
        let v = compare(&spec(Direction::LowerIsBetter, 1.0), &before, &after);
        assert!(matches!(v, Verdict::Regressed { .. }), "got {v:?}");
        assert!(
            v.warrants_revert(),
            "a demonstrated regression is the one case that must come back off"
        );
    }

    #[test]
    fn a_move_smaller_than_the_declared_minimum_effect_is_not_a_result() {
        let before = measurement(&[10.0, 10.0, 10.0, 10.0]);
        let after = measurement(&[10.4, 10.4, 10.4, 10.4]);
        // Both windows are perfectly steady, so noise is zero and only the
        // declared minimum effect stands between this and a confident verdict.
        let v = compare(&spec(Direction::HigherIsBetter, 1.0), &before, &after);
        match v {
            Verdict::Unchanged { threshold, .. } => assert_eq!(threshold, 1.0),
            other => panic!("expected Unchanged, got {other:?}"),
        }
    }

    /// The property that stops this module reporting noise as success.
    #[test]
    fn a_noisy_metric_raises_its_own_bar() {
        // Same 5-unit shift in both cases, same declared minimum effect.
        let steady_before = measurement(&[10.0, 10.0, 10.0, 10.0]);
        let steady_after = measurement(&[15.0, 15.0, 15.0, 15.0]);
        let steady = compare(
            &spec(Direction::HigherIsBetter, 1.0),
            &steady_before,
            &steady_after,
        );
        assert!(
            matches!(steady, Verdict::Improved { .. }),
            "a clean 5-unit move should read as improvement, got {steady:?}"
        );

        let noisy_before = measurement(&[0.0, 10.0, 10.0, 20.0]);
        let noisy_after = measurement(&[5.0, 15.0, 15.0, 25.0]);
        let noisy = compare(
            &spec(Direction::HigherIsBetter, 1.0),
            &noisy_before,
            &noisy_after,
        );
        assert!(
            matches!(noisy, Verdict::Unchanged { .. }),
            "the same 5-unit move inside 10 units of scatter is not evidence, got {noisy:?}"
        );
    }

    #[test]
    fn a_window_that_mostly_failed_to_read_gives_no_verdict() {
        // A clear improvement, but the after-window kept failing: three good
        // readings out of eight attempts is not a majority, so no verdict.
        let before = measurement(&[10.0, 10.0, 10.0, 10.0, 10.0]);
        let after = Measurement::from_samples(vec![50.0, 50.0, 50.0], 5).expect("non-empty");
        let v = compare(&spec(Direction::HigherIsBetter, 1.0), &before, &after);
        assert!(
            matches!(v, Verdict::Unverifiable { .. }),
            "3 of 8 readings must not decide an improvement, got {v:?}"
        );

        // The same move with a majority of the attempts read is still a result.
        let after =
            Measurement::from_samples(vec![50.0, 50.0, 50.0, 50.0, 50.0], 3).expect("non-empty");
        let v = compare(&spec(Direction::HigherIsBetter, 1.0), &before, &after);
        assert!(
            matches!(v, Verdict::Improved { .. }),
            "5 of 8 readings is a majority and must still count, got {v:?}"
        );
    }

    #[test]
    fn a_metric_that_never_reads_yields_no_measurement() {
        let source = Scripted::new(Direction::HigherIsBetter, 1.0, vec![None, None, None]);
        let plan = SamplingPlan {
            count: 3,
            gap: Duration::from_millis(0),
        };
        assert!(
            measure(&source, &plan).is_none(),
            "no readings must give no measurement, not a measurement of zero"
        );
    }

    #[test]
    fn failed_readings_are_counted_not_averaged_in() {
        let source = Scripted::new(
            Direction::HigherIsBetter,
            1.0,
            vec![Some(10.0), None, Some(10.0)],
        );
        let plan = SamplingPlan {
            count: 3,
            gap: Duration::from_millis(0),
        };
        let m = measure(&source, &plan).expect("two readings succeeded");
        assert_eq!(m.samples, vec![10.0, 10.0]);
        assert_eq!(m.missed, 1);
        assert_eq!(
            m.median, 10.0,
            "a missed reading must not drag the median toward zero"
        );
    }

    /// The distinction the module is built around.
    #[test]
    fn unverifiable_and_unchanged_are_different_facts_once_serialised() {
        let unchanged = Verdict::Unchanged {
            metric: "m".into(),
            before: 10.0,
            after: 10.0,
            delta: 0.0,
            threshold: 1.0,
        };
        let unverifiable = Verdict::Unverifiable {
            reason: "no metric".into(),
        };
        let a = serde_json::to_string(&unchanged).unwrap();
        let b = serde_json::to_string(&unverifiable).unwrap();
        assert_ne!(a, b);
        assert!(a.contains("\"verdict\":\"unchanged\""), "{a}");
        assert!(b.contains("\"verdict\":\"unverifiable\""), "{b}");
        assert!(
            !unverifiable.warrants_revert(),
            "reverting on 'I could not tell' would undo nearly every write this \
             crate makes, which is not the safe choice it looks like"
        );
    }

    #[test]
    fn no_setting_claims_a_metric_it_has_not_earned() {
        // Guards the module's central claim. If a metric is added, the commit
        // that adds it must show the number moving on real hardware; this test
        // is here so that addition is deliberate rather than incidental.
        for id in [
            "active_scheme_guid",
            "scaling_governor",
            "persistence_mode",
            "perf_level",
            "gt_max_freq_mhz",
        ] {
            assert!(
                metric_for(id).is_none(),
                "{id} declares a metric; the commit adding it must demonstrate \
                 that the metric moves when the setting changes"
            );
        }
    }

    fn fake_outcome(status: ApplyStatus) -> ApplyOutcome {
        ApplyOutcome {
            setting_id: "fake".into(),
            subsystem: Some(crate::profile::Subsystem::Cpu),
            requested: SettingValue::Text("new".into()),
            status,
            message: String::new(),
            timestamp: 0,
            previous: Some(SettingValue::Text("old".into())),
        }
    }

    /// The branch the whole module exists for: a measured regression puts the
    /// setting back, through the caller's revert, without being asked twice.
    #[test]
    fn a_regression_is_reverted() {
        let reverted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = reverted.clone();

        // Four steady readings at 10, then four at 20, on a lower-is-better
        // metric: the write made things worse by a clear margin.
        let source = Scripted::new(
            Direction::LowerIsBetter,
            1.0,
            vec![
                Some(10.0),
                Some(10.0),
                Some(10.0),
                Some(10.0),
                Some(20.0),
                Some(20.0),
                Some(20.0),
                Some(20.0),
            ],
        );
        let plan = SamplingPlan {
            count: 4,
            gap: Duration::from_millis(0),
        };

        let out = verify_around(
            &source,
            Duration::from_millis(0),
            &plan,
            || fake_outcome(ApplyStatus::Applied),
            |o| {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                fake_outcome(o.status)
            },
        );

        assert!(
            matches!(out.verdict, Verdict::Regressed { .. }),
            "got {:?}",
            out.verdict
        );
        assert!(
            reverted.load(std::sync::atomic::Ordering::SeqCst),
            "a regression that is measured and then left in place is worse than \
             not measuring at all"
        );
        assert!(out.reverted.is_some(), "the revert must be reported too");
    }

    /// The mirror image, and the one that keeps this from being a machine that
    /// undoes its own work: an improvement is left alone.
    #[test]
    fn an_improvement_is_left_in_place() {
        let reverted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = reverted.clone();

        let source = Scripted::new(
            Direction::LowerIsBetter,
            1.0,
            vec![
                Some(20.0),
                Some(20.0),
                Some(20.0),
                Some(20.0),
                Some(10.0),
                Some(10.0),
                Some(10.0),
                Some(10.0),
            ],
        );
        let plan = SamplingPlan {
            count: 4,
            gap: Duration::from_millis(0),
        };

        let out = verify_around(
            &source,
            Duration::from_millis(0),
            &plan,
            || fake_outcome(ApplyStatus::Applied),
            |o| {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                fake_outcome(o.status)
            },
        );

        assert!(
            matches!(out.verdict, Verdict::Improved { .. }),
            "got {:?}",
            out.verdict
        );
        assert!(
            !reverted.load(std::sync::atomic::Ordering::SeqCst),
            "nothing should have been undone"
        );
    }

    /// A refused or failed write is never measured. Sampling anyway would
    /// attribute whatever the machine did next to a change that never happened.
    #[test]
    fn a_write_that_did_not_take_effect_is_not_measured() {
        let source = Scripted::new(
            Direction::HigherIsBetter,
            1.0,
            vec![Some(10.0), Some(10.0), Some(99.0), Some(99.0)],
        );
        let plan = SamplingPlan {
            count: 2,
            gap: Duration::from_millis(0),
        };

        let out = verify_around(
            &source,
            Duration::from_millis(0),
            &plan,
            || fake_outcome(ApplyStatus::Refused),
            |o| fake_outcome(o.status),
        );

        match out.verdict {
            Verdict::Unverifiable { reason } => {
                assert!(reason.contains("did not take effect"), "{reason}")
            }
            other => panic!("expected Unverifiable, got {other:?}"),
        }
        assert!(out.reverted.is_none());
    }

    /// An unreadable baseline is not a baseline of zero.
    #[test]
    fn an_unreadable_before_window_yields_no_verdict() {
        let source = Scripted::new(
            Direction::HigherIsBetter,
            1.0,
            vec![None, None, Some(50.0), Some(50.0)],
        );
        let plan = SamplingPlan {
            count: 2,
            gap: Duration::from_millis(0),
        };

        let out = verify_around(
            &source,
            Duration::from_millis(0),
            &plan,
            || fake_outcome(ApplyStatus::Applied),
            |o| fake_outcome(o.status),
        );

        match out.verdict {
            Verdict::Unverifiable { reason } => {
                assert!(reason.contains("before the write"), "{reason}")
            }
            other => panic!("expected Unverifiable, got {other:?}"),
        }
    }

    #[test]
    fn a_setting_with_no_metric_is_still_applied_and_reported_honestly() {
        // `confirm: false`, so nothing is written to this machine — the write is
        // refused, which is itself one of the paths that must report
        // Unverifiable rather than a verdict.
        let out = apply_verified(
            "definitely_not_a_real_setting",
            SettingValue::Bool(true),
            false,
            Duration::from_millis(0),
            &SamplingPlan {
                count: 1,
                gap: Duration::from_millis(0),
            },
        );
        match out.verdict {
            Verdict::Unverifiable { reason } => {
                assert!(reason.contains("no metric"), "{reason}");
            }
            other => panic!("expected Unverifiable, got {other:?}"),
        }
        assert!(
            out.reverted.is_none(),
            "nothing measurable happened, so nothing should be undone"
        );
    }
}
