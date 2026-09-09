// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! What IronMonitor can and cannot do, as data an agent can read.
//!
//! # Why this is not the README
//!
//! The README has a "What IronMonitor cannot do" section, and prose is the wrong
//! medium for an agent. An agent deciding whether to trust a reading needs to
//! know *before asking* that macOS has no GPU reader and that the Windows CPU
//! frequency is a rated figure rather than a measured one. Parsing that out of
//! English is exactly the guessing this crate's ontology exists to remove.
//!
//! So the same facts live here as [`Capability`] rows: per surface, per
//! platform, with the evidence that establishes each claim.
//!
//! # The distinction this adds to the reading ontology
//!
//! [`super::Entity`] describes *what a reading means*. This describes *whether
//! IronMonitor can produce it at all, and how much that is worth*. They answer
//! different questions and both are needed: an agent that knows
//! `gpu.0.temperature` is degrees Celsius still cannot tell whether asking is
//! futile on this host.
//!
//! [`Support::Unverified`] is the variant that makes this honest rather than
//! promotional. Code that compiles and has never run on real hardware is not
//! "implemented", and this project has shipped several such paths — the Linux
//! profile writers among them. Calling that `Implemented` would be the same
//! class of error as reporting a nominal clock as a measured one.
//!
//! # Automated testing derives from this
//!
//! `tests/capability_conformance.rs` walks this catalogue and cross-checks it
//! against the code: every registered apply handler must be declared here and
//! every declaration must correspond to a handler; every detection rule the IDS
//! can emit must be declared; every ontology domain must be covered. Adding a
//! capability without declaring it fails the build, and declaring one that does
//! not exist fails too.
//!
//! That is the point of putting it in the ontology rather than in a document:
//! a claim that cannot drift is worth more than a claim that is merely true
//! today.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Which part of ironmon a capability belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// Reading hardware state.
    Reading,
    /// Writing a driver or firmware setting.
    Setting,
    /// Recommending, applying and verifying a profile.
    Tuning,
    /// Intrusion detection.
    Detection,
    /// A user or agent interface.
    Interface,
}

impl Surface {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reading => "reading",
            Self::Setting => "setting",
            Self::Tuning => "tuning",
            Self::Detection => "detection",
            Self::Interface => "interface",
        }
    }

    pub const ALL: &'static [Surface] = &[
        Surface::Reading,
        Surface::Setting,
        Surface::Tuning,
        Surface::Detection,
        Surface::Interface,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Linux,
    Windows,
    MacOS,
}

impl Platform {
    pub const ALL: &'static [Platform] = &[Platform::Linux, Platform::Windows, Platform::MacOS];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Windows => "windows",
            Self::MacOS => "macos",
        }
    }

    /// The platform this build targets.
    pub fn current() -> Option<Self> {
        #[cfg(target_os = "linux")]
        {
            Some(Self::Linux)
        }
        #[cfg(windows)]
        {
            Some(Self::Windows)
        }
        #[cfg(target_os = "macos")]
        {
            Some(Self::MacOS)
        }
        #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
        {
            None
        }
    }
}

/// How well a capability is supported on one platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "support")]
pub enum Support {
    /// Works, and something exercises it on real hardware.
    Implemented,
    /// Works for some of what the name suggests. `missing` says which part does
    /// not, because "partial" alone tells a caller nothing actionable.
    Partial { missing: String },
    /// Not implemented here, with the reason. A reason of "not written yet" and
    /// a reason of "the platform exposes no such interface" lead to different
    /// decisions.
    Unimplemented { reason: String },
    /// The code exists, compiles, and has never run on real hardware.
    ///
    /// Deliberately not `Implemented`. This project has shipped paths that were
    /// written by inspection and cross-compiled — the Linux profile writers are
    /// still in that state — and describing them as working would be the same
    /// error as reporting a nominal clock as a measured one.
    Unverified { reason: String },
}

impl Support {
    /// Whether a caller can expect this to produce a real answer.
    pub fn is_usable(&self) -> bool {
        matches!(self, Support::Implemented | Support::Partial { .. })
    }
}

/// One thing IronMonitor does, and how well, per platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    /// Stable dotted id: `reading.gpu`, `setting.active_scheme_guid`,
    /// `detection.file.modified`.
    pub id: String,
    pub surface: Surface,
    pub summary: String,
    /// Support per platform. A platform absent from the map is not declared,
    /// which is itself a gap this module's tests refuse to let pass silently.
    pub support: BTreeMap<Platform, Support>,
    /// How the claim is known — a test, a measurement, a CI job. Absent only
    /// where the claim is that nothing is implemented.
    pub evidence: Option<String>,
    /// The command that exercises this, as it would be typed.
    ///
    /// `None` means the capability exists in the library and nothing on the
    /// command line reaches it. That is a real state and worth declaring rather
    /// than hiding: the intrusion detectors were library-only when this field
    /// was added, and nothing had noticed because capabilities and commands
    /// lived in separate catalogues that never met.
    ///
    /// Checked against `ironmon describe --commands` by
    /// `tests/capability_conformance.rs`, so a command named here always exists.
    pub command: Option<String>,
}

impl Capability {
    /// Support on the platform this build targets.
    pub fn here(&self) -> Option<&Support> {
        Platform::current().and_then(|p| self.support.get(&p))
    }
}

fn all(support: Support) -> BTreeMap<Platform, Support> {
    Platform::ALL
        .iter()
        .map(|p| (*p, support.clone()))
        .collect()
}

fn per(rows: &[(Platform, Support)]) -> BTreeMap<Platform, Support> {
    rows.iter().cloned().collect()
}

fn cap(
    id: &str,
    surface: Surface,
    summary: &str,
    support: BTreeMap<Platform, Support>,
    evidence: Option<&str>,
) -> Capability {
    Capability {
        id: id.to_string(),
        surface,
        summary: summary.to_string(),
        support,
        evidence: evidence.map(|s| s.to_string()),
        command: command_for(id).map(|c| c.to_string()),
    }
}

/// The command that exercises a capability, where one exists.
///
/// A lookup rather than a field on every call site, so the mapping is readable
/// in one place and the gaps are visible as gaps.
fn command_for(id: &str) -> Option<&'static str> {
    Some(match id {
        "reading.cpu" => "cli cpu",
        "reading.gpu" => "cli gpu",
        "reading.memory" => "cli memory",
        "reading.power" => "cli power",
        "reading.thermal" => "cli temperature",
        "reading.process" => "cli processes",
        "reading.board" => "cli board",
        "reading.usb" => "cli usb",
        "reading.disk" | "reading.network" | "reading.system" | "reading.pci" => "snapshot",
        "tuning.classify" | "tuning.apply" | "tuning.verify" => "tune",
        "interface.cli" => "cli",
        "interface.tui" => "tui",
        "interface.gui" => "gui",
        "interface.ontology" => "describe",
        "interface.daemon" => "daemon",
        "interface.http" => "serve",
        "interface.agent" => "ai",
        // Settings are written through one command regardless of which setting.
        _ if id.starts_with("setting.") => "profile",
        // Every detector runs in one pass, so they share a command.
        //
        // They had none at all until the capability-to-command join reported it:
        // real, tested, and reachable from nothing typeable. That the catalogue
        // found it is the argument for having built the join.
        _ if id.starts_with("detection.") => "ids",
        // MCP is spoken, not typed.
        "interface.mcp" => return None,
        _ => return None,
    })
}

/// Every capability ironmon declares.
///
/// Hand-written where the claim is a judgement, and cross-checked against the
/// code by `tests/capability_conformance.rs` wherever the code can be asked.
pub fn catalogue() -> Vec<Capability> {
    let mut out = Vec::new();

    // ── Readings, one per ontology domain ────────────────────────────────────
    let implemented_everywhere = Some(
        "tests/ontology_conformance.rs resolves every declared entity on each CI \
         platform and checks the readings against their declared ranges",
    );

    for domain in super::Domain::ALL {
        use super::Domain as D;
        let (summary, support, evidence) = match domain {
            D::Cpu => (
                "per-core and aggregate CPU utilisation, cache topology, core counts",
                per(&[
                    (
                        Platform::Linux,
                        Support::Partial {
                            missing: "utilisation is an average since boot, derived from \
                                  cumulative tick counters, not an instantaneous rate"
                                .into(),
                        },
                    ),
                    (
                        Platform::Windows,
                        Support::Partial {
                            missing: "reported frequency is the rated clock, not a live one: \
                                  CallNtPowerInformation returned a constant 4400 MHz \
                                  while system idle moved from 79.7% to 11.4%"
                                .into(),
                        },
                    ),
                    (
                        Platform::MacOS,
                        Support::Partial {
                            missing: "utilisation is an average since boot; no live per-core \
                                  frequency is available unelevated"
                                .into(),
                        },
                    ),
                ]),
                implemented_everywhere,
            ),
            D::Gpu => (
                "adapter identity, utilisation, memory, clocks, power and temperature",
                per(&[
                    (Platform::Linux, Support::Implemented),
                    (Platform::Windows, Support::Implemented),
                    (
                        Platform::MacOS,
                        Support::Unimplemented {
                            reason: "no GPU reader is written for macOS".into(),
                        },
                    ),
                ]),
                Some(
                    "NVML on Linux and Windows; verified against three adapters on the \
                      development machine",
                ),
            ),
            D::Memory => (
                "physical memory totals and usage, swap, DIMM slot topology",
                all(Support::Implemented),
                implemented_everywhere,
            ),
            D::Disk => (
                "device enumeration, capacity, SMART and NVMe health",
                per(&[
                    (
                        Platform::Linux,
                        Support::Unverified {
                            reason: "the SMART and NVMe sysfs paths have executed once, in CI, \
                                 and never against real Linux hardware"
                                .into(),
                        },
                    ),
                    (
                        Platform::Windows,
                        Support::Partial {
                            missing: "the ATA SMART parser has never met a SATA drive; it is \
                                  tested only against buffers this project constructed"
                                .into(),
                        },
                    ),
                    (
                        Platform::MacOS,
                        Support::Partial {
                            missing: "per-device I/O counters: iostat reports rates rather than \
                                  cumulative counters and does not split reads from writes"
                                .into(),
                        },
                    ),
                ]),
                Some("HANDOFF.md open work 1 and 3"),
            ),
            D::Network => (
                "interfaces, addresses, throughput counters and socket tables",
                all(Support::Implemented),
                implemented_everywhere,
            ),
            D::Power => (
                "battery state and power rails",
                per(&[
                    (Platform::Linux, Support::Implemented),
                    (Platform::Windows, Support::Implemented),
                    (
                        Platform::MacOS,
                        Support::Unimplemented {
                            reason: "powermetrics requires root, so it may not be reachable \
                                 unelevated at all; that is worth establishing before \
                                 anything is written"
                                .into(),
                        },
                    ),
                ]),
                Some("HANDOFF.md open work 2"),
            ),
            D::Thermal => (
                "CPU, GPU, board and device temperatures",
                per(&[
                    (Platform::Linux, Support::Implemented),
                    (
                        Platform::Windows,
                        Support::Partial {
                            missing: "most board sensors need a signed kernel driver, and a \
                                  virtual machine usually exposes none"
                                .into(),
                        },
                    ),
                    (
                        Platform::MacOS,
                        Support::Unimplemented {
                            reason: "no temperature reader is written for macOS".into(),
                        },
                    ),
                ]),
                implemented_everywhere,
            ),
            D::Process => (
                "process list with CPU and memory attribution",
                all(Support::Implemented),
                implemented_everywhere,
            ),
            D::System => (
                "hostname, uptime, OS identity",
                all(Support::Implemented),
                implemented_everywhere,
            ),
            D::Board => (
                "motherboard, firmware, TPM and virtualization identity",
                per(&[
                    (Platform::Linux, Support::Implemented),
                    (Platform::Windows, Support::Implemented),
                    (
                        Platform::MacOS,
                        Support::Partial {
                            missing: "TPM and firmware detail; board identity is read".into(),
                        },
                    ),
                ]),
                implemented_everywhere,
            ),
            D::Pci => (
                "PCI device enumeration and PCIe link state",
                per(&[
                    (Platform::Linux, Support::Implemented),
                    (Platform::Windows, Support::Implemented),
                    (
                        Platform::MacOS,
                        Support::Unimplemented {
                            reason: "no PCI enumeration is written for macOS".into(),
                        },
                    ),
                ]),
                implemented_everywhere,
            ),
            D::Usb => (
                "USB device enumeration and descriptors",
                per(&[
                    (Platform::Linux, Support::Implemented),
                    (Platform::Windows, Support::Implemented),
                    (
                        Platform::MacOS,
                        Support::Unimplemented {
                            reason: "no USB enumeration is written for macOS".into(),
                        },
                    ),
                ]),
                implemented_everywhere,
            ),
        };
        out.push(cap(
            &format!("reading.{}", domain.as_str()),
            Surface::Reading,
            summary,
            support,
            evidence,
        ));
    }

    // ── Settings, one per registered apply handler ───────────────────────────
    //
    // Derived from `builtin_handlers()` rather than listed, so a handler added
    // later appears here without anyone remembering to add it. What cannot be
    // derived is how well it is known to work, which is why the support map is
    // decided by id below.
    for handler in crate::profile::apply::builtin_handlers() {
        let id = handler.setting_id().to_string();
        let (support, evidence) = setting_support(&id);
        out.push(cap(
            &format!("setting.{id}"),
            Surface::Setting,
            "a driver or firmware setting with a registered, confirmed, \
             audit-logged write handler",
            support,
            evidence,
        ));
    }

    // ── Tuning ───────────────────────────────────────────────────────────────
    out.push(cap(
        "tuning.classify",
        Surface::Tuning,
        "classify what the machine is being used for from process, GPU and \
         workload signals",
        all(Support::Implemented),
        Some("src/tuning/mod.rs tests; a model may classify but may not choose values"),
    ));
    out.push(cap(
        "tuning.apply",
        Surface::Tuning,
        "apply a recommendation through the confirmed, audit-logged write path, \
         recording the prior value so it can be undone",
        per(&[
            (
                Platform::Linux,
                Support::Unverified {
                    reason: "the Linux write handlers have never executed on any machine".into(),
                },
            ),
            (Platform::Windows, Support::Implemented),
            (
                Platform::MacOS,
                Support::Unimplemented {
                    reason: "no writable settings are registered on macOS".into(),
                },
            ),
        ]),
        Some(
            "round_trip_the_active_power_scheme_on_real_hardware, run deliberately \
              on Windows and cross-checked with powercfg",
        ),
    ));
    out.push(cap(
        "tuning.verify",
        Surface::Tuning,
        "measure whether an applied setting helped and revert it if it did not",
        all(Support::Partial {
            missing: "no metric is registered for any setting, so every verified \
                      apply reports `unverifiable`. The mechanism works; nothing \
                      it can measure has survived being checked"
                .into(),
        }),
        Some(
            "src/tuning/verify.rs metric_for returns None for every setting, \
              asserted by no_setting_claims_a_metric_it_has_not_earned",
        ),
    ));

    // ── Detection ────────────────────────────────────────────────────────────
    for (rule, summary) in crate::ids::RULES {
        out.push(cap(
            &format!("detection.{rule}"),
            Surface::Detection,
            summary,
            all(Support::Implemented),
            Some(
                "src/ids tests, and the network detectors verified against the \
                  live socket table of the development machine",
            ),
        ));
    }

    // ── Interfaces ───────────────────────────────────────────────────────────
    out.push(cap(
        "interface.cli",
        Surface::Interface,
        "structured command-line output, with JSON for every reading command",
        all(Support::Implemented),
        Some("tests/documentation_links.rs asserts every documented command exists"),
    ));
    out.push(cap(
        "interface.ontology",
        Surface::Interface,
        "a machine-readable schema of every reading, its unit, its provenance and \
         whether it may be null — plus this capability catalogue",
        all(Support::Implemented),
        Some("tests/ontology_conformance.rs and tests/capability_conformance.rs"),
    ));
    out.push(cap(
        "interface.agent",
        Surface::Interface,
        "natural-language queries over the same readings, and an MCP server",
        all(Support::Partial {
            missing: "a backend is required; there is no offline fallback".into(),
        }),
        Some("src/ai_api; local backends preferred so telemetry stays on the machine"),
    ));

    out.push(cap(
        "interface.tui",
        Surface::Interface,
        "an interactive terminal dashboard, and a headless frame renderer for scripted inspection",
        all(Support::Implemented),
        Some("tui::headless script tests render frames without a terminal"),
    ));
    out.push(cap(
        "interface.gui",
        Surface::Interface,
        "a native desktop window, and a headless frame renderer for scripted inspection",
        all(Support::Partial {
            missing: "the window needs a display server; the headless renderer does not and is what CI exercises"
                .into(),
        }),
        Some("every_gui_tab_paints_text renders each tab and asserts it carries substance rather than a spinner"),
    ));
    out.push(cap(
        "interface.mcp",
        Surface::Interface,
        "an MCP server, so an agent host can call ironmon as a tool",
        all(Support::Implemented),
        Some("src/ai_api/mcp_server.rs; the protocol version is exported alongside it"),
    ));
    out.push(cap(
        "interface.http",
        Surface::Interface,
        "an HTTP endpoint serving readings as JSON",
        all(Support::Implemented),
        Some("src/http_server.rs"),
    ));
    out.push(cap(
        "interface.daemon",
        Surface::Interface,
        "a long-running collector that samples on an interval",
        all(Support::Implemented),
        Some("src/daemon.rs"),
    ));

    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// What is known about one setting's write path.
fn setting_support(id: &str) -> (BTreeMap<Platform, Support>, Option<&'static str>) {
    match id {
        "active_scheme_guid" => (
            per(&[
                (Platform::Windows, Support::Implemented),
                (
                    Platform::Linux,
                    Support::Unimplemented {
                        reason: "Windows power schemes do not exist on Linux".into(),
                    },
                ),
                (
                    Platform::MacOS,
                    Support::Unimplemented {
                        reason: "Windows power schemes do not exist on macOS".into(),
                    },
                ),
            ]),
            Some("round-tripped on real hardware and cross-checked with powercfg"),
        ),
        "scaling_governor" | "perf_level" | "gt_max_freq_mhz" | "persistence_mode" => (
            per(&[
                (
                    Platform::Linux,
                    Support::Unverified {
                        reason: "written by inspection and checked by the compiler and CI; \
                             no Linux machine has run it"
                            .into(),
                    },
                ),
                (
                    Platform::Windows,
                    Support::Unimplemented {
                        reason: "a Linux sysfs interface".into(),
                    },
                ),
                (
                    Platform::MacOS,
                    Support::Unimplemented {
                        reason: "a Linux sysfs interface".into(),
                    },
                ),
            ]),
            Some("HANDOFF.md open work 13"),
        ),
        // A handler this catalogue has no opinion about is declared as such
        // rather than assumed to work. The conformance test requires every
        // platform be mentioned, so this stays visible.
        _ => (
            all(Support::Unverified {
                reason: "registered as a write handler and not yet described in the \
                         capability catalogue"
                    .into(),
            }),
            None,
        ),
    }
}

/// The optional features this binary was built with.
///
/// A capability is per-platform and also per-build: `ironmon ai models` exists
/// only where the `vault` feature was enabled, and an agent asking what a
/// binary can do needs to know which binary it is talking to. Reported from
/// `cfg!`, so it describes the running artefact rather than the manifest.
pub fn enabled_features() -> Vec<&'static str> {
    let mut out = Vec::new();
    for (name, on) in [
        ("cli", cfg!(feature = "cli")),
        ("gui", cfg!(feature = "gui")),
        ("vault", cfg!(feature = "vault")),
        ("nvidia", cfg!(feature = "nvidia")),
        ("amd", cfg!(feature = "amd")),
        ("intel", cfg!(feature = "intel")),
        ("apple", cfg!(feature = "apple")),
        ("npu", cfg!(feature = "npu")),
        ("remote-backends", cfg!(feature = "remote-backends")),
    ] {
        if on {
            out.push(name);
        }
    }
    out
}

/// Capabilities that will not produce an answer on this platform.
///
/// The question an agent should ask before planning around a reading.
pub fn unusable_here() -> Vec<Capability> {
    catalogue()
        .into_iter()
        .filter(|c| c.here().map(|s| !s.is_usable()).unwrap_or(true))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_capability_declares_every_platform() {
        for c in catalogue() {
            for p in Platform::ALL {
                assert!(
                    c.support.contains_key(p),
                    "{} says nothing about {}. An agent reading this catalogue \
                     cannot tell an undeclared platform from an unsupported one, \
                     which is the distinction the whole thing exists to make.",
                    c.id,
                    p.as_str()
                );
            }
        }
    }

    #[test]
    fn ids_are_unique_and_prefixed_by_surface() {
        let cat = catalogue();
        let mut seen = std::collections::BTreeSet::new();
        for c in &cat {
            assert!(
                seen.insert(c.id.clone()),
                "duplicate capability id {}",
                c.id
            );
            assert!(
                c.id.starts_with(c.surface.as_str()),
                "{} is a {} capability but its id does not say so",
                c.id,
                c.surface.as_str()
            );
        }
    }

    /// A claim of support has to say how it is known.
    #[test]
    fn anything_usable_anywhere_cites_its_evidence() {
        for c in catalogue() {
            let usable_somewhere = c.support.values().any(|s| s.is_usable());
            if usable_somewhere {
                assert!(
                    c.evidence.is_some(),
                    "{} claims to work somewhere and cites nothing. A capability \
                     claim without evidence is the same shape as a reading \
                     without provenance.",
                    c.id
                );
            }
        }
    }

    /// Every reason string has to be actionable.
    #[test]
    fn unsupported_reasons_are_not_empty_placeholders() {
        for c in catalogue() {
            for (p, s) in &c.support {
                let reason = match s {
                    Support::Unimplemented { reason } | Support::Unverified { reason } => {
                        Some(reason)
                    }
                    Support::Partial { missing } => Some(missing),
                    Support::Implemented => None,
                };
                if let Some(r) = reason {
                    assert!(
                        r.len() > 12,
                        "{} on {} gives the reason {r:?}, which tells a reader \
                         nothing they can act on",
                        c.id,
                        p.as_str()
                    );
                }
            }
        }
    }

    #[test]
    fn the_catalogue_serialises_for_an_agent() {
        let json = serde_json::to_string(&catalogue()).unwrap();
        assert!(
            json.contains("\"support\":\"unverified\""),
            "{}",
            &json[..200.min(json.len())]
        );
        let back: Vec<Capability> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), catalogue().len());
    }

    /// The variant that makes this catalogue honest rather than promotional.
    #[test]
    fn unverified_is_not_usable() {
        let s = Support::Unverified {
            reason: "written by inspection and never run".into(),
        };
        assert!(
            !s.is_usable(),
            "code that has never run on hardware is not something to plan around"
        );
    }

    #[test]
    fn this_platform_is_recognised() {
        assert!(
            Platform::current().is_some(),
            "the catalogue cannot answer `here()` on a platform it does not name"
        );
    }
}
