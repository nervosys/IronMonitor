// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Use-case detection and hardware profile recommendations.
//!
//! Two separable jobs, kept separate on purpose:
//!
//! 1. **What is this machine doing?** — [`classify`] answers with a [`UseCase`],
//!    a confidence, and the evidence it rested on. This is a judgement, and it is
//!    labelled as one.
//! 2. **What settings suit that?** — [`plan_from_settings`] answers with [`Recommendation`]s
//!    whose proposed values come from what the driver itself declared: an entry in
//!    the setting's `choices`, or its `default`. Never a number this module made up.
//!
//! # Why the split matters
//!
//! An optimiser is only as trustworthy as its worst suggestion, and the tempting
//! design — ask a language model for a power limit — produces a number with no
//! provenance that cannot be checked against anything the hardware said. That is
//! the failure this repository's ontology exists to prevent, and it would be
//! worse here than in a reading, because a reading is only believed while a
//! setting is *written*.
//!
//! So a model may classify the workload, where being wrong costs a suboptimal
//! profile. It may not choose values. [`Recommendation::basis`] records where each
//! proposed value came from, and
//! [`tests::a_recommendation_never_proposes_a_value_the_driver_did_not_offer`]
//! makes that enforceable rather than aspirational.
//!
//! # Nothing here writes
//!
//! This module computes plans. Applying one goes through
//! [`crate::profile::apply::apply_setting`], which refuses without explicit
//! confirmation and writes an audit record — the contract AGENTS.md states, and
//! which the automatic server in [`serve`] does not get to bypass.

use crate::profile::{Setting, SettingRisk, SettingValue, Subsystem};
use serde::{Deserialize, Serialize};

pub mod ledger;
pub mod serve;
pub mod verify;

/// What the machine is being used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UseCase {
    /// Training or fine-tuning a model: sustained GPU compute, high VRAM, long
    /// runs. Wants sustained throughput over latency, and tolerates heat.
    AiTraining,
    /// Serving or running inference: bursty GPU compute, latency-sensitive.
    AiInference,
    /// A game is running: latency-sensitive, wants clocks up and nothing parking.
    Gaming,
    /// A person is using the machine, but not for either of the above.
    Interactive,
    /// Nothing much is happening. Worth naming, because the right profile for an
    /// idle machine is not the right profile for a busy one, and "no evidence of
    /// load" is a finding rather than a failure to classify.
    Idle,
    /// The readings needed to tell the cases apart were not obtained. Not a
    /// use case the machine is in, so no setting has a policy for it and
    /// `ironmon tune` recommends nothing. Not offered as a model choice, and
    /// not accepted by [`UseCase::parse`].
    ///
    /// Classifying with a missing reading used to substitute 0% for it, and
    /// an absent GPU or CPU then read as an idle machine at 0.8 confidence.
    Unknown,
}

impl UseCase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AiTraining => "ai_training",
            Self::AiInference => "ai_inference",
            Self::Gaming => "gaming",
            Self::Interactive => "interactive",
            Self::Idle => "idle",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ai_training" | "training" => Some(Self::AiTraining),
            "ai_inference" | "inference" => Some(Self::AiInference),
            "gaming" | "game" => Some(Self::Gaming),
            "interactive" | "desktop" => Some(Self::Interactive),
            "idle" => Some(Self::Idle),
            _ => None,
        }
    }
}

/// How a classification was reached.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Process, GPU and workload signals only. Reproducible, and the fallback
    /// whenever a model is unavailable or answers something unusable.
    Signals,
    /// A language model chose among the [`UseCase`] variants, given the same
    /// signals. Named so a consumer can weigh it differently.
    Model { backend: String },
}

/// What the machine appears to be doing, and why that was concluded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Classification {
    pub use_case: UseCase,
    /// 0.0–1.0. Deliberately coarse: this is a judgement from indirect signals,
    /// and a precise-looking figure would overstate it.
    pub confidence: f32,
    pub method: Method,
    /// The observations behind the verdict, in the words a person would check
    /// them in. An agent that disagrees needs to see what was seen.
    pub evidence: Vec<String>,
}

/// One proposed change to one setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    pub setting_id: String,
    pub subsystem: Subsystem,
    pub display_name: String,
    pub current: SettingValue,
    pub proposed: SettingValue,
    pub risk: SettingRisk,
    /// Where the proposed value came from — the driver's own enumerated choice or
    /// its declared default. This is the field that distinguishes a
    /// recommendation from a guess, and it is not optional.
    pub basis: String,
    /// Why this change suits the detected use case.
    pub rationale: String,
}

/// A setting that was considered and passed over, with the reason.
///
/// Recorded rather than dropped for the same reason the ontology records
/// unavailable readings: silence cannot be told apart from "nothing applies", and
/// a caller wondering why its GPU was untouched deserves an answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skipped {
    pub setting_id: String,
    pub reason: String,
}

/// Recommendations for one use case, plus what was passed over.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub classification: Classification,
    pub recommendations: Vec<Recommendation>,
    pub skipped: Vec<Skipped>,
}

impl Plan {
    /// Recommendations at or below a risk ceiling.
    ///
    /// The automatic server uses this: unattended application is capped, and the
    /// cap is applied here rather than at the call site so every caller gets it.
    pub fn within_risk(&self, ceiling: SettingRisk) -> Vec<&Recommendation> {
        self.recommendations
            .iter()
            .filter(|r| risk_rank(r.risk) <= risk_rank(ceiling))
            .collect()
    }
}

/// Ordering over risk. `SettingRisk` derives no `Ord`, and inventing one by
/// declaration order would silently break if a variant were inserted.
fn risk_rank(risk: SettingRisk) -> u8 {
    match risk {
        SettingRisk::Informational => 0,
        SettingRisk::Safe => 1,
        SettingRisk::Moderate => 2,
        SettingRisk::Dangerous => 3,
    }
}

/// Signals gathered from the machine, in a form both the heuristics and a model
/// can read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Signals {
    /// Names of processes that look like games, from the running process list.
    /// `Some(empty)` means the list was read and nothing matched; `None` means
    /// the process list was not read, which is not the same thing.
    ///
    /// Defaulting this to an empty list used to report "no game process
    /// recognised" when the process monitor had failed.
    pub game_processes: Option<Vec<String>>,
    /// AI frameworks detected by [`crate::ai_workload`].
    pub ai_frameworks: Vec<String>,
    /// Whether any detected AI workload is training rather than inference.
    /// `None` when the AI workload probe did not run to completion.
    pub ai_training: Option<bool>,
    /// Whether any AI workload was detected at all. `Some(false)` is a probe
    /// that ran and found none; `None` is a probe that was not read.
    ///
    /// Defaulting this to `false` used to report "no AI workload detected" when
    /// the probe had failed, and an idle verdict followed from that.
    pub ai_present: Option<bool>,
    /// Busiest GPU utilisation seen, percent. `None` when no GPU reported one —
    /// which is not zero.
    pub gpu_utilization: Option<f32>,
    /// How many GPUs were enumerated. `Some(0)` is a finding: the machine has
    /// no GPU, so it has no GPU load and a quiet CPU is enough to call it idle.
    /// `None` means enumeration failed or was not attempted, which is not zero.
    pub gpu_count: Option<usize>,
    /// System-wide CPU utilisation, percent.
    pub cpu_utilization: Option<f32>,
}

impl Signals {
    /// A compact human-readable summary, used as the model prompt and as
    /// evidence. One place, so what the model saw and what the user is told
    /// cannot drift apart.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        match (self.gpu_utilization, self.gpu_count) {
            (Some(u), _) => parts.push(format!("busiest GPU at {u:.0}% utilisation")),
            (None, Some(0)) => parts.push("no GPU present".to_string()),
            (None, _) => parts.push("no GPU reported utilisation".to_string()),
        }
        match self.cpu_utilization {
            Some(u) => parts.push(format!("CPU at {u:.0}%", u = u)),
            None => parts.push("CPU utilisation unavailable".to_string()),
        }
        match (self.ai_present, self.ai_training) {
            (Some(true), training) => parts.push(format!(
                "AI workload detected ({}) doing {}",
                if self.ai_frameworks.is_empty() {
                    "framework unidentified".to_string()
                } else {
                    self.ai_frameworks.join(", ")
                },
                match training {
                    Some(true) => "training",
                    Some(false) => "inference",
                    None => "work of a kind not read",
                }
            )),
            (Some(false), _) => parts.push("no AI workload detected".to_string()),
            (None, _) => parts.push("AI workload probe not read".to_string()),
        }
        match &self.game_processes {
            Some(games) if games.is_empty() => {
                parts.push("no game process recognised".to_string());
            }
            Some(games) => parts.push(format!("game processes: {}", games.join(", "))),
            None => parts.push("game process list not read".to_string()),
        }
        parts.join("; ")
    }
}

/// Utilisation at or below this is treated as an idle machine.
const IDLE_GPU_PERCENT: f32 = 5.0;
const IDLE_CPU_PERCENT: f32 = 10.0;
/// Sustained GPU load consistent with training rather than serving.
const HEAVY_GPU_PERCENT: f32 = 50.0;

/// Classify from signals alone, with no model involved.
///
/// Deterministic and total: every input yields a verdict with its evidence. The
/// model path below falls back to this whenever it cannot do better, so this is
/// the floor of the feature rather than a stub.
pub fn classify_from_signals(signals: &Signals) -> Classification {
    let mut evidence = vec![signals.summary()];

    // An identified AI workload is direct evidence, not an inference from load,
    // so it outranks the utilisation heuristics below.
    if signals.ai_present == Some(true) {
        let classified = match signals.ai_training {
            Some(true) => Some((UseCase::AiTraining, 0.9)),
            Some(false) => Some((UseCase::AiInference, 0.8)),
            // An AI workload was seen but its kind was not read. Choosing
            // between training and inference would be a guess.
            None => None,
        };
        if let Some((use_case, confidence)) = classified {
            evidence.push("an AI framework was identified in a running process".to_string());
            return Classification {
                use_case,
                confidence,
                method: Method::Signals,
                evidence,
            };
        }
        evidence.push(
            "an AI workload was seen but whether it trains or infers was not read; \
             no use case is inferred"
                .to_string(),
        );
        return Classification {
            use_case: UseCase::Unknown,
            confidence: 0.0,
            method: Method::Signals,
            evidence,
        };
    }

    if signals
        .game_processes
        .as_ref()
        .is_some_and(|games| !games.is_empty())
    {
        evidence.push("a known game executable is running".to_string());
        return Classification {
            use_case: UseCase::Gaming,
            // Lower than the AI case: this rests on a name table, which is
            // necessarily incomplete and occasionally wrong.
            confidence: 0.7,
            method: Method::Signals,
            evidence,
        };
    }

    // Every verdict below this point is "nothing identifies the workload", which
    // is only a finding if both identifying probes ran and found nothing. With
    // either unread, Idle, the unidentified-load case, and the heavy-GPU case
    // would all be guesses, so no use case is inferred.
    let unread_probes: Vec<&str> = [
        (signals.ai_present.is_none(), "the AI workload probe"),
        (signals.game_processes.is_none(), "the game process list"),
    ]
    .into_iter()
    .filter_map(|(unread, name)| unread.then_some(name))
    .collect();
    if !unread_probes.is_empty() {
        evidence.push(format!(
            "{} not read; no use case is inferred",
            unread_probes.join(" and ")
        ));
        return Classification {
            use_case: UseCase::Unknown,
            confidence: 0.0,
            method: Method::Signals,
            evidence,
        };
    }

    // A reading that was not obtained is not 0%. Idle needs both processors to be
    // measured and quiet; the heavy-GPU case needs the GPU measured; anything
    // else needs both measured to say "some load". Otherwise the answer is unknown.
    // A machine enumerated as having no GPU has no GPU load to measure, so its
    // GPU side is known rather than missing.
    let no_gpu = signals.gpu_utilization.is_none() && signals.gpu_count == Some(0);
    let gpu_quiet = match signals.gpu_utilization {
        Some(gpu) => Some(gpu <= IDLE_GPU_PERCENT),
        None if no_gpu => Some(true),
        None => None,
    };
    if let (Some(true), Some(cpu)) = (gpu_quiet, signals.cpu_utilization) {
        if cpu <= IDLE_CPU_PERCENT {
            evidence.push(if no_gpu {
                "no GPU is present and the CPU is not meaningfully busy".to_string()
            } else {
                "neither processor is meaningfully busy".to_string()
            });
            return Classification {
                use_case: UseCase::Idle,
                confidence: 0.8,
                method: Method::Signals,
                evidence,
            };
        }
    }

    if let Some(gpu) = signals.gpu_utilization {
        if gpu >= HEAVY_GPU_PERCENT {
            // Sustained GPU load with nothing identifying it. Worth reporting as
            // *something* graphical, at low confidence, rather than as idle.
            evidence.push(format!(
                "sustained GPU load with no identified workload; {gpu:.0}% is consistent \
                 with graphics or compute this classifier cannot name"
            ));
            return Classification {
                use_case: UseCase::Gaming,
                confidence: 0.4,
                method: Method::Signals,
                evidence,
            };
        }
    }

    if (signals.gpu_utilization.is_none() && !no_gpu) || signals.cpu_utilization.is_none() {
        evidence.push(
            "a utilisation reading needed to classify was not obtained; no use case is inferred"
                .to_string(),
        );
        return Classification {
            use_case: UseCase::Unknown,
            confidence: 0.0,
            method: Method::Signals,
            evidence,
        };
    }

    evidence.push("some load, but nothing that identifies a workload".to_string());
    Classification {
        use_case: UseCase::Interactive,
        confidence: 0.5,
        method: Method::Signals,
        evidence,
    }
}

/// The instruction given to a model asked to classify. Kept beside the parser
/// that reads its answer, so the two cannot drift.
///
/// Read only by the classification path, which needs a backend to query and so
/// is compiled out of a build with no backend feature.
#[allow(dead_code)]
const CLASSIFY_SYSTEM_PROMPT: &str = "\
You classify what a computer is being used for, from hardware telemetry.
Answer with exactly one of these tokens and nothing else:
ai_training, ai_inference, gaming, interactive, idle.
No explanation, no punctuation, no formatting.";

/// Classify with a local model where one is running, falling back to signals.
///
/// The model is asked to pick from [`UseCase`]'s own variants and its answer is
/// parsed through [`UseCase::parse`]; anything unrecognised is discarded and the
/// deterministic path answers instead. That is the whole safety story for
/// involving a model here — it cannot widen the answer space, only choose within
/// it, so the worst outcome is a wrong label rather than an unknown one.
///
/// Falls back, always with the reason recorded in the evidence, when: no backend
/// is running, the crate was built without `remote-backends`, the query fails, or
/// the answer does not name a use case.
pub fn classify(signals: &Signals) -> Classification {
    let baseline = classify_from_signals(signals);

    // A model sees the same readings, and with one missing it has nothing better
    // to choose from than the signal path does, so it is not asked.
    if baseline.use_case == UseCase::Unknown {
        return baseline;
    }

    #[cfg(feature = "remote-backends")]
    {
        use crate::agent::backend::{BackendConfig, BackendDiscovery, BackendType};

        let discovery = BackendDiscovery::discover();
        // Only local backends. Sending a description of what someone is doing at
        // their desk to a hosted provider is not a trade this feature should make
        // silently, and IronMonitor is deployed on the strength of keeping telemetry on
        // the host.
        let Some(backend) = discovery
            .available()
            .iter()
            .find(|b| b.is_local() || b.runs_on_host())
            .cloned()
        else {
            return with_note(
                baseline,
                "no local model backend is running; used signals only",
            );
        };

        let config = match backend {
            BackendType::RemoteOllama => BackendConfig::ollama("llama3.2"),
            BackendType::RemoteLMStudio => BackendConfig::lm_studio("local-model"),
            ref other => return with_note(
                baseline,
                format!("{other:?} is available but this classifier only drives Ollama and LM Studio; used signals only"),
            ),
        };

        let Ok(client) = crate::agent::RemoteClient::new(config) else {
            return with_note(
                baseline,
                "the local model backend would not initialise; used signals only",
            );
        };

        match client.query(CLASSIFY_SYSTEM_PROMPT, &signals.summary()) {
            Ok((answer, _)) => match UseCase::parse(answer.trim()) {
                Some(use_case) => {
                    let mut evidence = baseline.evidence.clone();
                    evidence.push(format!(
                        "a local model classified this as {:?}; the signal-only classifier said {:?}",
                        use_case.as_str(),
                        baseline.use_case.as_str()
                    ));
                    Classification {
                        use_case,
                        // Never above the signal path's own confidence in a direct
                        // observation: a model agreeing with an identified
                        // framework adds nothing, and a model disagreeing with one
                        // should not raise certainty.
                        confidence: baseline.confidence.min(0.75),
                        method: Method::Model {
                            backend: format!("{backend:?}"),
                        },
                        evidence,
                    }
                }
                None => with_note(
                    baseline,
                    format!(
                        "the model answered {:?}, which names no use case; used signals only",
                        answer.trim().chars().take(40).collect::<String>()
                    ),
                ),
            },
            Err(e) => with_note(
                baseline,
                format!("the model query failed ({e}); used signals only"),
            ),
        }
    }

    #[cfg(not(feature = "remote-backends"))]
    {
        with_note(
            baseline,
            "built without the remote-backends feature, so no model is reachable; used signals only",
        )
    }
}

/// Append a line of evidence explaining why the model path was not taken.
fn with_note(mut c: Classification, note: impl Into<String>) -> Classification {
    c.evidence.push(note.into());
    c
}

/// Build the recommendation set for a use case from settings the machine
/// actually reports.
///
/// `settings` is what the profile inspector read. Only settings that are
/// `writable`, carry a matching policy, and whose target value the driver itself
/// offered become recommendations; everything else is recorded in
/// [`Plan::skipped`] with the reason.
pub fn plan_from_settings(classification: Classification, settings: &[Setting]) -> Plan {
    let mut recommendations = Vec::new();
    let mut skipped = Vec::new();

    for setting in settings {
        let Some(policy) = policy_for(&setting.id, classification.use_case) else {
            continue;
        };

        if !setting.writable {
            skipped.push(Skipped {
                setting_id: setting.id.clone(),
                reason: "the provider does not expose this setting as writable".to_string(),
            });
            continue;
        }

        match choose_value(setting, policy.target) {
            Ok((proposed, basis)) => {
                if proposed == setting.value {
                    skipped.push(Skipped {
                        setting_id: setting.id.clone(),
                        reason: format!(
                            "already set to the value this use case wants ({})",
                            setting.value
                        ),
                    });
                    continue;
                }
                recommendations.push(Recommendation {
                    setting_id: setting.id.clone(),
                    subsystem: policy.subsystem,
                    display_name: setting.display_name.clone(),
                    current: setting.value.clone(),
                    proposed,
                    risk: setting.risk,
                    basis,
                    rationale: policy.rationale.to_string(),
                });
            }
            Err(reason) => skipped.push(Skipped {
                setting_id: setting.id.clone(),
                reason,
            }),
        }
    }

    Plan {
        classification,
        recommendations,
        skipped,
    }
}

/// What a use case wants from a particular setting.
struct Policy {
    subsystem: Subsystem,
    target: Target,
    rationale: &'static str,
}

/// The *intent* of a recommendation, resolved against the driver's own options.
///
/// Intent rather than value, because the value must come from the hardware. A
/// policy says "the highest-performing option this driver offers"; which option
/// that is, is the driver's answer and not this module's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// The choice whose name marks it as the performance-oriented one.
    HighestPerformance,
    /// The choice whose name marks it as the balanced one.
    Balanced,
    /// Whatever the driver reports as its own default.
    DriverDefault,
}

/// The policy table. Deliberately small and explicit.
///
/// Every entry names a setting IronMonitor has a real apply handler for; a policy for
/// something unwritable would produce recommendations nobody can act on. It is
/// keyed on setting id rather than on a category, because two GPUs' "performance
/// mode" are not interchangeable and pretending otherwise is how a tuner ends up
/// writing a vendor's value into another vendor's register.
fn policy_for(setting_id: &str, use_case: UseCase) -> Option<Policy> {
    match (setting_id, use_case) {
        // Windows power scheme. The one lever that exists unelevated on a
        // Windows desktop, and the highest-value one: the balanced scheme parks
        // cores and drops clocks, which shows up directly as inference latency
        // and frame pacing.
        ("active_scheme_guid", UseCase::AiTraining | UseCase::AiInference) => Some(Policy {
            subsystem: Subsystem::Cpu,
            target: Target::HighestPerformance,
            rationale: "AI work is bursty on the CPU while the GPU waits on it; core \
                        parking adds latency to every dispatch",
        }),
        ("active_scheme_guid", UseCase::Gaming) => Some(Policy {
            subsystem: Subsystem::Cpu,
            target: Target::HighestPerformance,
            rationale: "frame pacing suffers when cores park between frames",
        }),
        ("active_scheme_guid", UseCase::Idle | UseCase::Interactive) => Some(Policy {
            subsystem: Subsystem::Cpu,
            target: Target::Balanced,
            rationale: "no sustained workload to justify holding clocks up",
        }),

        // Linux CPU frequency governor, the same argument in the same place.
        ("scaling_governor", UseCase::AiTraining | UseCase::AiInference | UseCase::Gaming) => {
            Some(Policy {
                subsystem: Subsystem::Cpu,
                target: Target::HighestPerformance,
                rationale: "the ondemand governor ramps after the work arrives, which is \
                            latency added to every burst",
            })
        }
        ("scaling_governor", UseCase::Idle | UseCase::Interactive) => Some(Policy {
            subsystem: Subsystem::Cpu,
            target: Target::Balanced,
            rationale: "no sustained workload to justify holding clocks up",
        }),

        // AMD GPU performance level.
        ("power_dpm_force_performance_level", UseCase::AiTraining | UseCase::Gaming) => {
            Some(Policy {
                subsystem: Subsystem::Gpu,
                target: Target::HighestPerformance,
                rationale: "sustained GPU load; letting the driver clock down between \
                            batches costs throughput",
            })
        }
        ("power_dpm_force_performance_level", UseCase::Idle) => Some(Policy {
            subsystem: Subsystem::Gpu,
            target: Target::DriverDefault,
            rationale: "return the GPU to the driver's own scheduling when nothing needs it",
        }),

        // NVIDIA persistence mode: keeps the driver loaded so the first CUDA call
        // does not pay initialisation. Matters for inference servers specifically.
        ("persistence_mode", UseCase::AiTraining | UseCase::AiInference) => Some(Policy {
            subsystem: Subsystem::Gpu,
            target: Target::HighestPerformance,
            rationale: "without persistence the driver unloads between processes and the \
                        next CUDA context pays initialisation",
        }),

        _ => None,
    }
}

/// Resolve a [`Target`] against what the driver declared for this setting.
///
/// Returns the value and a description of where it came from, or the reason no
/// value could be justified. **There is no branch here that invents one** — that
/// is the whole contract of this module, and the reason the error case is a
/// `String` a caller can show rather than a silent `None`.
fn choose_value(setting: &Setting, target: Target) -> Result<(SettingValue, String), String> {
    match target {
        Target::DriverDefault => match &setting.default {
            Some(v) => Ok((v.clone(), "the driver's own reported default".to_string())),
            None => Err("the driver reports no default to return to".to_string()),
        },
        Target::HighestPerformance => pick_choice(
            setting,
            &[
                "ultimate",
                "high performance",
                "performance",
                "high",
                "max",
                "1",
            ],
        ),
        Target::Balanced => pick_choice(setting, &["balanced", "balance", "medium", "schedutil"]),
    }
}

/// Find the driver-declared choice whose name matches one of `wanted`, in order.
///
/// Matching is on the *driver's own* choice labels. A setting with no enumerated
/// choices yields an error rather than a constructed value: without a list to
/// pick from there is nothing to pick, and guessing a GUID or a governor name
/// would be exactly the invented value this module refuses to produce.
fn pick_choice(setting: &Setting, wanted: &[&str]) -> Result<(SettingValue, String), String> {
    let Some(choices) = &setting.choices else {
        return Err(format!(
            "the provider enumerated no choices for {}, so there is no declared value to \
             select — this module will not construct one",
            setting.id
        ));
    };
    if choices.is_empty() {
        return Err(format!(
            "the provider enumerated an empty choice list for {}",
            setting.id
        ));
    }

    // Pass 1: exact (case-insensitive) match on the label or the bare value.
    // "1" must find a choice named "1", never one named "10".
    for want in wanted {
        let want_lc = want.trim().to_ascii_lowercase();
        for (label, value) in choices {
            if label.trim().eq_ignore_ascii_case(&want_lc) || bare_value_equals(value, &want_lc) {
                return Ok((
                    value.clone(),
                    format!("the driver's own choice {label:?} for this setting"),
                ));
            }
        }
    }

    // Pass 2: looser substring match on words only. A numeric token never
    // matches by substring, and a numeric label is never picked this way.
    // This used to match "1" inside "10" and "100".
    for want in wanted {
        if is_numeric_token(want) {
            continue;
        }
        let want_lc = want.trim().to_ascii_lowercase();
        for (label, value) in choices {
            if is_numeric_token(label) {
                continue;
            }
            if label.to_ascii_lowercase().contains(&want_lc) {
                return Ok((
                    value.clone(),
                    format!("the driver's own choice {label:?} for this setting"),
                ));
            }
        }
    }

    Err(format!(
        "none of the choices the driver offers for {} ({}) match what this use case wants",
        setting.id,
        choices
            .iter()
            .map(|(l, _)| l.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// A wanted token that is all digits, such as "1" or "100".
fn is_numeric_token(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

/// Whether a choice's bare value (not its `Display` form, which appends hex to
/// integers) equals the lowercased wanted token.
fn bare_value_equals(value: &SettingValue, want_lc: &str) -> bool {
    match value {
        SettingValue::Text(s) => s.trim().eq_ignore_ascii_case(want_lc),
        SettingValue::Uint(n) => want_lc.parse::<u64>().is_ok_and(|w| w == *n),
        SettingValue::Int(i) => want_lc.parse::<i64>().is_ok_and(|w| w == *i),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scheme_setting(current: &str, choices: &[(&str, &str)]) -> Setting {
        Setting {
            id: "active_scheme_guid".into(),
            display_name: "Active power scheme".into(),
            value: SettingValue::Text(current.into()),
            unit: None,
            default: Some(SettingValue::Text(
                "381b4222-f694-41f0-9685-ff5bb260df2e".into(),
            )),
            choices: Some(
                choices
                    .iter()
                    .map(|(l, v)| ((*l).to_string(), SettingValue::Text((*v).to_string())))
                    .collect(),
            ),
            range: None,
            description: None,
            risk: SettingRisk::Moderate,
            source: "test".into(),
            writable: true,
        }
    }

    const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
    const HIGH_PERF: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

    fn windows_choices() -> Vec<(&'static str, &'static str)> {
        vec![("Balanced", BALANCED), ("High performance", HIGH_PERF)]
    }

    fn training() -> Classification {
        Classification {
            use_case: UseCase::AiTraining,
            confidence: 0.9,
            method: Method::Signals,
            evidence: vec!["test".into()],
        }
    }

    fn uint_choices_setting(choices: &[(&str, u64)]) -> Setting {
        let mut s = scheme_setting("", &[]);
        s.choices = Some(
            choices
                .iter()
                .map(|(l, v)| ((*l).to_string(), SettingValue::Uint(*v)))
                .collect(),
        );
        s
    }

    #[test]
    fn wanted_one_does_not_match_a_choice_named_ten_or_hundred() {
        // Only "10" and "100" are offered: a wanted "1" must not substring-match.
        let s = uint_choices_setting(&[("10", 10), ("100", 100)]);
        assert!(pick_choice(&s, &["1"]).is_err());
    }

    #[test]
    fn wanted_one_prefers_the_exact_choice_over_longer_numbers() {
        // "10" comes first in the list, but the exact "1" must win.
        let s = uint_choices_setting(&[("10", 10), ("100", 100), ("1", 1)]);
        let (value, _) = pick_choice(&s, &["1"]).expect("exact match exists");
        assert!(matches!(value, SettingValue::Uint(1)));
    }

    #[test]
    fn exact_match_on_bare_value_counts_even_if_label_differs() {
        let s = uint_choices_setting(&[("Level ten", 10), ("Level one", 1)]);
        let (value, _) = pick_choice(&s, &["1"]).expect("value 1 is offered");
        assert!(matches!(value, SettingValue::Uint(1)));
    }

    #[test]
    fn exact_match_is_case_insensitive_and_beats_substring() {
        // "high" appears as a substring of "Highest", but "High" exactly matches.
        let s = scheme_setting("", &[("Highest", "guid-highest"), ("High", "guid-high")]);
        let (value, _) = pick_choice(&s, &["HIGH"]).expect("exact match exists");
        assert_eq!(value, SettingValue::Text("guid-high".into()));
    }

    #[test]
    fn word_substring_still_matches_when_no_exact_choice_exists() {
        let s = scheme_setting("", &windows_choices());
        let (value, _) = pick_choice(&s, &["high performance"]).expect("exact label");
        assert_eq!(value, SettingValue::Text(HIGH_PERF.into()));
        let (value, _) = pick_choice(&s, &["perf"]).expect("substring of a word");
        assert_eq!(value, SettingValue::Text(HIGH_PERF.into()));
    }

    /// The contract of the whole module: a proposed value is one the driver
    /// offered. If this can be made to fail, the optimiser is writing numbers it
    /// invented into hardware registers.
    #[test]
    fn a_recommendation_never_proposes_a_value_the_driver_did_not_offer() {
        let setting = scheme_setting(BALANCED, &windows_choices());
        let plan = plan_from_settings(training(), std::slice::from_ref(&setting));

        assert_eq!(plan.recommendations.len(), 1);
        let rec = &plan.recommendations[0];

        let offered: Vec<&SettingValue> = setting
            .choices
            .as_ref()
            .unwrap()
            .iter()
            .map(|(_, v)| v)
            .collect();
        assert!(
            offered.contains(&&rec.proposed),
            "proposed {:?} is not among the driver's choices {offered:?}",
            rec.proposed
        );
        assert!(
            !rec.basis.is_empty(),
            "a recommendation must say where its value came from"
        );
    }

    /// A setting with no enumerated choices must yield nothing at all. The
    /// tempting failure is to synthesise a plausible value — a governor name, a
    /// well-known GUID — which would be correct on the developer's machine and
    /// wrong somewhere else.
    #[test]
    fn a_setting_with_no_choices_is_skipped_rather_than_guessed() {
        let mut setting = scheme_setting(BALANCED, &[]);
        setting.choices = None;
        let plan = plan_from_settings(training(), &[setting]);

        assert!(plan.recommendations.is_empty());
        assert_eq!(plan.skipped.len(), 1);
        assert!(
            plan.skipped[0].reason.contains("will not construct one"),
            "the skip reason must say why: {}",
            plan.skipped[0].reason
        );
    }

    /// An unwritable setting cannot be acted on, so recommending it would be
    /// advice nobody can take.
    #[test]
    fn an_unwritable_setting_is_skipped_with_a_reason() {
        let mut setting = scheme_setting(BALANCED, &windows_choices());
        setting.writable = false;
        let plan = plan_from_settings(training(), &[setting]);

        assert!(plan.recommendations.is_empty());
        assert_eq!(plan.skipped.len(), 1);
        assert!(plan.skipped[0].reason.contains("writable"));
    }

    /// Recommending a change to the value already in place is noise, and noise in
    /// an advisory surface is how people learn to ignore it.
    #[test]
    fn a_setting_already_correct_produces_no_recommendation() {
        let setting = scheme_setting(HIGH_PERF, &windows_choices());
        let plan = plan_from_settings(training(), &[setting]);

        assert!(plan.recommendations.is_empty());
        assert_eq!(plan.skipped.len(), 1);
        assert!(plan.skipped[0].reason.contains("already set"));
    }

    /// Idle wants the balanced scheme, not the performance one — the policy is
    /// per use case, and getting this backwards would hold clocks up forever.
    #[test]
    fn idle_and_training_pull_the_same_setting_in_opposite_directions() {
        let setting = scheme_setting(HIGH_PERF, &windows_choices());

        let idle = plan_from_settings(
            Classification {
                use_case: UseCase::Idle,
                confidence: 0.8,
                method: Method::Signals,
                evidence: vec![],
            },
            std::slice::from_ref(&setting),
        );
        assert_eq!(idle.recommendations.len(), 1);
        assert_eq!(
            idle.recommendations[0].proposed,
            SettingValue::Text(BALANCED.into())
        );

        let busy = plan_from_settings(training(), &[scheme_setting(BALANCED, &windows_choices())]);
        assert_eq!(
            busy.recommendations[0].proposed,
            SettingValue::Text(HIGH_PERF.into())
        );
    }

    /// The risk ceiling is what stands between the automatic server and a
    /// voltage register.
    #[test]
    fn the_risk_ceiling_excludes_anything_above_it() {
        let mut moderate = scheme_setting(BALANCED, &windows_choices());
        moderate.risk = SettingRisk::Moderate;
        let mut dangerous = scheme_setting(BALANCED, &windows_choices());
        dangerous.id = "power_dpm_force_performance_level".into();
        dangerous.risk = SettingRisk::Dangerous;

        let plan = plan_from_settings(
            Classification {
                use_case: UseCase::AiTraining,
                confidence: 0.9,
                method: Method::Signals,
                evidence: vec![],
            },
            &[moderate, dangerous],
        );
        assert_eq!(plan.recommendations.len(), 2);
        assert_eq!(plan.within_risk(SettingRisk::Moderate).len(), 1);
        assert_eq!(plan.within_risk(SettingRisk::Safe).len(), 0);
        assert_eq!(plan.within_risk(SettingRisk::Dangerous).len(), 2);
    }

    // ── Classification ───────────────────────────────────────────────────────

    /// Both identifying probes ran and found nothing. Idle, Interactive and
    /// the unidentified-load verdicts are only findings given this.
    fn probes_read_nothing_found() -> Signals {
        Signals {
            ai_present: Some(false),
            game_processes: Some(vec![]),
            ..Default::default()
        }
    }

    #[test]
    fn an_identified_training_workload_outranks_utilisation() {
        // Low utilisation between batches must not read as idle when the
        // framework itself has been identified.
        let signals = Signals {
            ai_present: Some(true),
            ai_training: Some(true),
            ai_frameworks: vec!["pytorch".into()],
            gpu_utilization: Some(2.0),
            cpu_utilization: Some(3.0),
            ..Default::default()
        };
        let c = classify_from_signals(&signals);
        assert_eq!(c.use_case, UseCase::AiTraining);
        assert!(c.confidence > 0.8);
    }

    #[test]
    fn inference_and_training_are_distinguished() {
        let signals = Signals {
            ai_present: Some(true),
            ai_training: Some(false),
            gpu_utilization: Some(40.0),
            ..Default::default()
        };
        assert_eq!(
            classify_from_signals(&signals).use_case,
            UseCase::AiInference
        );
    }

    #[test]
    fn a_quiet_machine_is_idle_rather_than_interactive() {
        let signals = Signals {
            gpu_utilization: Some(1.0),
            cpu_utilization: Some(2.0),
            ..probes_read_nothing_found()
        };
        assert_eq!(classify_from_signals(&signals).use_case, UseCase::Idle);
    }

    /// An absent GPU reading is not 0%, and an idle verdict needs it measured.
    /// With the GPU missing and the CPU quiet, the answer is unknown, and the
    /// evidence says the GPU was not reported.
    #[test]
    fn a_missing_gpu_reading_is_unknown_not_idle() {
        let signals = Signals {
            gpu_utilization: None,
            cpu_utilization: Some(2.0),
            ..probes_read_nothing_found()
        };
        let c = classify_from_signals(&signals);
        assert_eq!(c.use_case, UseCase::Unknown);
        assert!(
            c.evidence.iter().any(|e| e.contains("no GPU reported")),
            "evidence must distinguish an absent reading from a zero: {:?}",
            c.evidence
        );
    }

    /// The idle verdict used to come from `unwrap_or(0.0)` on both readings, so
    /// an unreadable CPU was an idle CPU at 0.8 confidence.
    #[test]
    fn a_missing_cpu_reading_is_unknown_not_idle() {
        let signals = Signals {
            gpu_utilization: Some(1.0),
            cpu_utilization: None,
            ..probes_read_nothing_found()
        };
        let c = classify_from_signals(&signals);
        assert_eq!(c.use_case, UseCase::Unknown);
        assert_eq!(c.confidence, 0.0);
        assert_eq!(classify(&signals).use_case, UseCase::Unknown);
    }

    /// No GPU enumerated is a finding, not a missing reading: a quiet CPU on a
    /// machine with no GPU is idle, while a failed enumeration stays unknown.
    #[test]
    fn a_machine_with_no_gpu_can_be_idle() {
        let mut signals = Signals {
            gpu_utilization: None,
            gpu_count: Some(0),
            cpu_utilization: Some(2.0),
            ..probes_read_nothing_found()
        };
        assert_eq!(classify_from_signals(&signals).use_case, UseCase::Idle);
        signals.cpu_utilization = Some(60.0);
        assert_eq!(
            classify_from_signals(&signals).use_case,
            UseCase::Interactive
        );
        signals.gpu_count = None;
        assert_eq!(classify_from_signals(&signals).use_case, UseCase::Unknown);
    }

    /// Both readings measured and quiet is still idle.
    #[test]
    fn measured_quiet_readings_are_still_idle() {
        let signals = Signals {
            gpu_utilization: Some(1.0),
            cpu_utilization: Some(2.0),
            ..probes_read_nothing_found()
        };
        assert_eq!(classify_from_signals(&signals).use_case, UseCase::Idle);
    }

    /// Unknown names no setting: `ironmon tune` must recommend nothing for it.
    #[test]
    fn unknown_has_no_policy_for_any_setting() {
        for id in [
            "active_scheme_guid",
            "scaling_governor",
            "power_dpm_force_performance_level",
            "persistence_mode",
        ] {
            assert!(policy_for(id, UseCase::Unknown).is_none(), "{id}");
        }
    }

    #[test]
    fn a_game_process_classifies_as_gaming() {
        let signals = Signals {
            game_processes: Some(vec!["cyberpunk2077.exe".into()]),
            gpu_utilization: Some(95.0),
            cpu_utilization: Some(60.0),
            ..Default::default()
        };
        let c = classify_from_signals(&signals);
        assert_eq!(c.use_case, UseCase::Gaming);
    }

    /// Unattributed GPU load is reported at low confidence rather than
    /// confidently mislabelled.
    #[test]
    fn unidentified_gpu_load_is_low_confidence() {
        let signals = Signals {
            gpu_utilization: Some(80.0),
            cpu_utilization: Some(20.0),
            ..probes_read_nothing_found()
        };
        let c = classify_from_signals(&signals);
        assert!(
            c.confidence < 0.5,
            "a guess from load alone must not look confident, got {}",
            c.confidence
        );
    }

    #[test]
    fn every_use_case_round_trips_through_its_string() {
        for uc in [
            UseCase::AiTraining,
            UseCase::AiInference,
            UseCase::Gaming,
            UseCase::Interactive,
            UseCase::Idle,
        ] {
            assert_eq!(UseCase::parse(uc.as_str()), Some(uc));
        }
        assert_eq!(UseCase::parse("nonsense"), None);
        // Not a choice the classifier or a model may force.
        assert_eq!(UseCase::parse("unknown"), None);
    }

    /// Classification is a judgement and always carries its evidence; a verdict
    /// with none is unauditable.
    #[test]
    fn every_classification_carries_evidence() {
        for signals in [
            Signals::default(),
            Signals {
                ai_present: Some(true),
                ai_training: Some(true),
                ..Default::default()
            },
            Signals {
                game_processes: Some(vec!["game.exe".into()]),
                ..Default::default()
            },
        ] {
            let c = classify_from_signals(&signals);
            assert!(!c.evidence.is_empty(), "no evidence for {signals:?}");
            assert!((0.0..=1.0).contains(&c.confidence));
        }
    }

    /// An AI probe that was never read is not "no AI workload". Idle needs the
    /// probe to have run and found nothing, so a quiet machine with the AI
    /// probe failed is unknown, and the evidence names the probe.
    #[test]
    fn an_unread_ai_probe_is_unknown_not_idle() {
        let signals = Signals {
            ai_present: None,
            game_processes: Some(vec![]),
            gpu_utilization: Some(1.0),
            cpu_utilization: Some(2.0),
            ..Default::default()
        };
        let c = classify_from_signals(&signals);
        assert_eq!(c.use_case, UseCase::Unknown);
        assert_eq!(c.confidence, 0.0);
        assert!(
            c.evidence
                .iter()
                .any(|e| e.contains("AI workload probe not read")),
            "evidence must name the missing probe: {:?}",
            c.evidence
        );
        assert_eq!(classify(&signals).use_case, UseCase::Unknown);
    }

    /// Same for the process list: a failed process monitor must not read as
    /// "no game running", and must not let a busy GPU be labelled Gaming.
    #[test]
    fn an_unread_process_list_is_unknown_not_idle_or_gaming() {
        let quiet = Signals {
            ai_present: Some(false),
            game_processes: None,
            gpu_utilization: Some(1.0),
            cpu_utilization: Some(2.0),
            ..Default::default()
        };
        let c = classify_from_signals(&quiet);
        assert_eq!(c.use_case, UseCase::Unknown);
        assert!(
            c.evidence
                .iter()
                .any(|e| e.contains("game process list not read")),
            "evidence must name the missing probe: {:?}",
            c.evidence
        );

        let busy = Signals {
            game_processes: None,
            gpu_utilization: Some(95.0),
            cpu_utilization: Some(60.0),
            ..quiet
        };
        assert_eq!(classify_from_signals(&busy).use_case, UseCase::Unknown);
    }

    /// An AI workload seen with its kind unread cannot be named as training or
    /// inference, and is not a reason to fall through to a load-based guess.
    #[test]
    fn an_ai_workload_of_unread_kind_is_unknown() {
        let signals = Signals {
            ai_present: Some(true),
            ai_training: None,
            game_processes: Some(vec![]),
            gpu_utilization: Some(40.0),
            ..Default::default()
        };
        let c = classify_from_signals(&signals);
        assert_eq!(c.use_case, UseCase::Unknown);
        assert_eq!(c.confidence, 0.0);
    }

    /// A positive detection does not need the other probe. A game found while
    /// the AI probe is unread is still Gaming, and an AI workload found while
    /// the process list is unread is still an AI use case.
    #[test]
    fn positive_detections_survive_an_unread_probe() {
        let game = Signals {
            ai_present: None,
            game_processes: Some(vec!["steam.exe".into()]),
            ..Default::default()
        };
        assert_eq!(classify_from_signals(&game).use_case, UseCase::Gaming);

        let ai = Signals {
            ai_present: Some(true),
            ai_training: Some(false),
            game_processes: None,
            ..Default::default()
        };
        assert_eq!(classify_from_signals(&ai).use_case, UseCase::AiInference);
    }

    /// The summary distinguishes a probe that found nothing from one not read.
    #[test]
    fn the_summary_says_not_read_for_unread_probes() {
        let unread = Signals::default().summary();
        assert!(unread.contains("AI workload probe not read"), "{unread}");
        assert!(unread.contains("game process list not read"), "{unread}");
        assert!(!unread.contains("no AI workload detected"), "{unread}");
        assert!(!unread.contains("no game process recognised"), "{unread}");

        let empty = probes_read_nothing_found().summary();
        assert!(empty.contains("no AI workload detected"), "{empty}");
        assert!(empty.contains("no game process recognised"), "{empty}");
    }
}
