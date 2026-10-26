//! CPU microarchitecture detection and feature inference.
//!
//! Identifies CPU microarchitecture (Zen 4, Raptor Lake, Firestorm, etc.),
//! process node, pipeline features, ISA extensions, and infers performance
//! characteristics from model identification.
//!
//! ## Platform Support
//!
//! - **Linux**: `/proc/cpuinfo` (family/model/stepping), CPUID via flags
//! - **Windows**: `Win32_Processor` family/model, registry CPU info
//! - **macOS**: `sysctl machdep.cpu`, `hw.cpufamily`

use serde::{Deserialize, Serialize};

use crate::error::IronError;

/// What each platform's `read_cpu_info` returns:
/// `(model_name, family, model, stepping, flags, cores, threads)`.
type CpuInfoFields = (
    Option<String>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Vec<String>,
    Option<u32>,
    Option<u32>,
);

/// CPU vendor/designer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuVendor {
    Intel,
    AMD,
    Apple,
    Qualcomm,
    Ampere,
    AWS,       // Graviton
    Fujitsu,   // A64FX
    HiSilicon, // Kunpeng
    Samsung,
    MediaTek,
    RiscV,
    Unknown,
}

impl std::fmt::Display for CpuVendor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Intel => write!(f, "Intel"),
            Self::AMD => write!(f, "AMD"),
            Self::Apple => write!(f, "Apple"),
            Self::Qualcomm => write!(f, "Qualcomm"),
            Self::Ampere => write!(f, "Ampere"),
            Self::AWS => write!(f, "AWS"),
            Self::Fujitsu => write!(f, "Fujitsu"),
            Self::HiSilicon => write!(f, "HiSilicon"),
            Self::Samsung => write!(f, "Samsung"),
            Self::MediaTek => write!(f, "MediaTek"),
            Self::RiscV => write!(f, "RISC-V"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// CPU microarchitecture identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Microarchitecture {
    /// Microarchitecture name (e.g. "Zen 4", "Raptor Lake", "Firestorm").
    ///
    /// `None` where the part was not identified. A name such as "Unknown AMD"
    /// used to stand in for that, and surfaces as a name.
    pub name: Option<String>,
    /// Code name (e.g. "Raphael", "Raptor Lake-S"). Empty where not identified.
    pub codename: String,
    /// Manufacturing process node in nanometers (e.g. 5, 7, 10), or `None`
    /// where the generation is not identified or its node is not public.
    pub process_nm: Option<u32>,
    /// Year of introduction, or `None` where the generation is not identified.
    pub year: Option<u32>,
    /// CPU vendor.
    pub vendor: CpuVendor,
    /// Architecture family (e.g. "x86_64", "aarch64", "riscv64").
    pub arch: String,
    /// Whether this is a hybrid/big.LITTLE design, or `None` where the part was
    /// not identified. An unidentified Apple part used to be reported as hybrid
    /// because every Apple silicon part so far is.
    pub hybrid: Option<bool>,
    /// Performance core uarch name (for hybrid designs).
    pub p_core_uarch: Option<String>,
    /// Efficiency core uarch name (for hybrid designs).
    pub e_core_uarch: Option<String>,
}

/// ISA extension category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IsaCategory {
    Simd,
    Crypto,
    Virtualization,
    Security,
    MachineLearning,
    Atomic,
    FloatingPoint,
    BitManip,
    Memory,
    Other,
}

/// An ISA extension or CPU feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsaExtension {
    /// Extension name (e.g. "AVX-512", "AES-NI", "SVE").
    pub name: String,
    /// Category.
    pub category: IsaCategory,
    /// Whether this extension is present. `None` when no reader checked this
    /// feature on this platform. Absence from a list is not `false` unless the
    /// list was read in full.
    pub supported: Option<bool>,
    /// Brief description.
    pub description: String,
}

/// Inferred performance characteristics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredPerformance {
    /// Estimated single-thread performance score (0-100).
    pub single_thread_score: u32,
    /// Estimated multi-thread scaling efficiency (0-100).
    pub mt_scaling_efficiency: u32,
    /// Estimated IPC (instructions per clock) relative to baseline.
    pub relative_ipc: f64,
    /// Estimated max boost clock in MHz (from model inference).
    pub estimated_boost_mhz: u32,
    /// Estimated TDP in watts.
    pub estimated_tdp_watts: u32,
    /// Target workload classification.
    pub target_workloads: Vec<String>,
    /// Known limitations or notes.
    pub notes: Vec<String>,
}

/// Full CPU microarchitecture report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuMicroarchReport {
    /// CPU model name string, or `None` where the platform did not report one.
    /// Windows used to fill this with the literal "Unknown CPU" when
    /// `Win32_Processor` gave no name.
    pub model_name: Option<String>,
    /// CPUID family.
    /// CPUID family, where it is readable.
    ///
    /// `None` on Windows: WMI does not expose the CPUID triple, and the reader
    /// returned `(0, 0, 0)` under a comment saying so. Family 0 is not a value
    /// any modern x86 CPU reports — a Ryzen 9 9900X is family 0x1A — so the
    /// report published an impossible triple rather than an absent one. The
    /// ontology already withheld these on Windows; this is the same decision
    /// one layer down.
    pub family: Option<u32>,
    /// CPUID model.
    /// CPUID model, where it is readable. See [`Self::family`].
    pub model: Option<u32>,
    /// CPUID stepping.
    /// CPUID stepping, where it is readable. See [`Self::family`].
    pub stepping: Option<u32>,
    /// Detected microarchitecture.
    pub microarch: Microarchitecture,
    /// ISA extensions.
    pub extensions: Vec<IsaExtension>,
    /// Inferred performance.
    pub performance: InferredPerformance,
    /// Physical core count, `None` where the platform did not report it.
    ///
    /// Both counts were `u32` with 0 for "not read", which every reader had to
    /// know to test; and each platform filled an unread logical count with the
    /// physical one, so a machine whose logical count was not read had SMT
    /// "off".
    pub physical_cores: Option<u32>,
    /// Logical core count (with SMT/HT), `None` where not reported.
    pub logical_cores: Option<u32>,
    /// Whether SMT/Hyper-Threading is enabled: `None` unless both counts were
    /// read, since it is judged by comparing them.
    pub smt_enabled: Option<bool>,
}

/// CPU microarchitecture monitor.
pub struct CpuMicroarchMonitor {
    report: CpuMicroarchReport,
}

/// Pull `(family, model, stepping)` out of a Windows processor description.
///
/// Both `Win32_Processor.Description` and the `PROCESSOR_IDENTIFIER`
/// environment variable use the same shape:
///
/// ```text
/// AMD64 Family 26 Model 68 Stepping 0
/// AMD64 Family 26 Model 68 Stepping 0, AuthenticAMD
/// x86 Family 6 Model 142 Stepping 10
/// ```
///
/// Parsed by keyword rather than by position, so a vendor suffix or an extra
/// leading token does not shift the fields. All three must be present: two
/// thirds of a CPUID triple is not one, and reporting a family without a model
/// would invite exactly the "close enough" matching this crate avoids.
#[cfg(target_os = "windows")]
fn parse_cpuid_triple(desc: &str) -> Option<(u32, u32, u32)> {
    let after = |key: &str| -> Option<u32> {
        let tokens: Vec<&str> = desc.split_whitespace().collect();
        let idx = tokens.iter().position(|t| t.eq_ignore_ascii_case(key))?;
        tokens
            .get(idx + 1)?
            .trim_end_matches(',')
            .parse::<u32>()
            .ok()
    };
    Some((after("Family")?, after("Model")?, after("Stepping")?))
}

impl CpuMicroarchMonitor {
    /// Create a new monitor and detect microarchitecture.
    pub fn new() -> Result<Self, IronError> {
        let (model_name, family, model, stepping, flags, cores, threads) = Self::read_cpu_info()?;
        let name_text = model_name.as_deref().unwrap_or("");
        let microarch = Self::identify_microarch(name_text, family, model, stepping);
        // Only a list the platform enumerates in full can say a feature is absent.
        // Linux `/proc/cpuinfo` is one. The other readers return none or a partial
        // list, so their absent flags stay `None`.
        let flag_list =
            (cfg!(target_os = "linux") && !flags.is_empty()).then_some(flags.as_slice());
        let extensions = Self::detect_extensions(flag_list, &microarch);
        // An unknown count takes the smallest scaling bucket, as 0 always did;
        // the estimate is inferred either way, and is labelled so.
        let performance = Self::infer_performance(name_text, &microarch, cores.unwrap_or(0));
        let smt_enabled = cores.zip(threads).map(|(c, t)| t > c);

        Ok(Self {
            report: CpuMicroarchReport {
                model_name,
                family,
                model,
                stepping,
                microarch,
                extensions,
                performance,
                physical_cores: cores,
                logical_cores: threads,
                smt_enabled,
            },
        })
    }

    /// Get the full report.
    pub fn report(&self) -> &CpuMicroarchReport {
        &self.report
    }

    /// Get only extensions that are supported.
    pub fn supported_extensions(&self) -> Vec<&IsaExtension> {
        self.report
            .extensions
            .iter()
            .filter(|e| e.supported == Some(true))
            .collect()
    }

    /// Check if a specific extension is supported by name.
    pub fn has_extension(&self, name: &str) -> bool {
        let upper = name.to_uppercase();
        self.report
            .extensions
            .iter()
            .any(|e| e.supported == Some(true) && e.name.to_uppercase().contains(&upper))
    }

    /// Identify microarchitecture from CPUID family/model and model name.
    fn identify_microarch(
        model_name: &str,
        family: Option<u32>,
        model: Option<u32>,
        _stepping: Option<u32>,
    ) -> Microarchitecture {
        let upper = model_name.to_uppercase();

        // Try vendor detection first — AMD checked before Intel to avoid
        // "CORE" matching in "16-Core Processor" AMD strings.
        //
        // The Arm vendors are checked before Intel too. "CORE" is an Intel marker
        // only by convention: "Snapdragon(R) X 12-core X1E80100" carries it, and
        // was reported as an Intel part with its name unidentified.
        let vendor = if upper.contains("AMD")
            || upper.contains("RYZEN")
            || upper.contains("EPYC")
            || upper.contains("THREADRIPPER")
            || upper.contains("ATHLON")
        {
            CpuVendor::AMD
        } else if upper.contains("GRAVITON") {
            CpuVendor::AWS
        } else if upper.contains("AMPERE") || upper.contains("ALTRA") {
            CpuVendor::Ampere
        } else if upper.contains("SNAPDRAGON") || upper.contains("QUALCOMM") {
            CpuVendor::Qualcomm
        } else if upper.contains("INTEL")
            || upper.contains("CORE")
            || upper.contains("XEON")
            || upper.contains("CELERON")
            || upper.contains("PENTIUM")
            || upper.contains("ATOM")
        {
            CpuVendor::Intel
        } else if upper.contains("APPLE")
            || upper.contains("M1")
            || upper.contains("M2")
            || upper.contains("M3")
            || upper.contains("M4")
        {
            CpuVendor::Apple
        } else {
            CpuVendor::Unknown
        };

        match vendor {
            CpuVendor::Intel => Self::identify_intel(&upper, family, model),
            CpuVendor::AMD => Self::identify_amd(&upper, family, model),
            CpuVendor::Apple => Self::identify_apple(&upper),
            CpuVendor::AWS => {
                // The generation is taken only from the name it is spelled in.
                // A Graviton with no generation in its name used to be called
                // Graviton2 with the 5 nm / 2023 figures of a later part, which
                // was true of none of them. Nodes come from the generation's
                // published part: Neoverse N1 (Graviton2) 7 nm 2020, Neoverse V1
                // (Graviton3) 5 nm 2021. Graviton4 (Neoverse V2, 2023) has no
                // published node, so none is given.
                let (codename, process_nm, year) = if upper.contains("GRAVITON4") {
                    ("Graviton4", None, Some(2023))
                } else if upper.contains("GRAVITON3") {
                    ("Graviton3", Some(5), Some(2021))
                } else if upper.contains("GRAVITON2") {
                    ("Graviton2", Some(7), Some(2020))
                } else {
                    ("", None, None)
                };
                Microarchitecture {
                    name: Some("Neoverse".into()),
                    codename: codename.into(),
                    process_nm,
                    year,
                    vendor,
                    arch: "aarch64".into(),
                    hybrid: Some(false),
                    p_core_uarch: None,
                    e_core_uarch: None,
                }
            }
            // Every Qualcomm and Ampere part is an Arm core, so the instruction set
            // is known from the vendor. It used to come from the build target, which
            // is x86_64 for an x64 build running under emulation on an Arm machine.
            CpuVendor::Qualcomm | CpuVendor::Ampere => Microarchitecture {
                name: None,
                codename: String::new(),
                process_nm: None,
                year: None,
                vendor,
                arch: "aarch64".into(),
                hybrid: None,
                p_core_uarch: None,
                e_core_uarch: None,
            },
            _ => Microarchitecture {
                // Not identified: no name, no node, no year, and no claim
                // about hybrid design. A "Unknown" name used to pass as one.
                name: None,
                codename: String::new(),
                process_nm: None,
                year: None,
                vendor,
                arch: if cfg!(target_arch = "x86_64") {
                    "x86_64".into()
                } else if cfg!(target_arch = "aarch64") {
                    "aarch64".into()
                } else {
                    "unknown".into()
                },
                hybrid: None,
                p_core_uarch: None,
                e_core_uarch: None,
            },
        }
    }

    fn identify_intel(name: &str, family: Option<u32>, model: Option<u32>) -> Microarchitecture {
        // Intel identification via model name patterns and CPUID family/model
        let (uarch, codename, process, year, hybrid, p_core, e_core) = if name.contains("ULTRA 9 2")
            || name.contains("ULTRA 7 2")
            || name.contains("ULTRA 5 2")
            || name.contains("ARROW LAKE")
        {
            (
                "Arrow Lake",
                "Arrow Lake",
                3,
                2024,
                true,
                Some("Lion Cove"),
                Some("Skymont"),
            )
        } else if name.contains("14TH GEN")
            || name.contains("14900")
            || name.contains("14700")
            || name.contains("14600")
            || name.contains("RAPTOR LAKE")
            || (family == Some(6) && matches!(model, Some(0xB7) | Some(0xBF)))
        {
            (
                "Raptor Lake",
                "Raptor Lake Refresh",
                7,
                2023,
                true,
                Some("Raptor Cove"),
                Some("Gracemont"),
            )
        } else if name.contains("13TH GEN")
            || name.contains("13900")
            || name.contains("13700")
            || name.contains("13600")
        {
            (
                "Raptor Lake",
                "Raptor Lake",
                7,
                2022,
                true,
                Some("Raptor Cove"),
                Some("Gracemont"),
            )
        } else if name.contains("12TH GEN")
            || name.contains("12900")
            || name.contains("12700")
            || name.contains("12600")
            || name.contains("ALDER LAKE")
            || (family == Some(6) && model == Some(0x97))
        {
            (
                "Alder Lake",
                "Alder Lake",
                7,
                2021,
                true,
                Some("Golden Cove"),
                Some("Gracemont"),
            )
        } else if name.contains("11TH GEN")
            || name.contains("11900")
            || name.contains("11700")
            || name.contains("ROCKET LAKE")
            || (family == Some(6) && model == Some(0xA7))
        {
            ("Cypress Cove", "Rocket Lake", 14, 2021, false, None, None)
        } else if name.contains("TIGER LAKE") || name.contains("1165") || name.contains("1185") {
            ("Willow Cove", "Tiger Lake", 10, 2020, false, None, None)
        } else if name.contains("10TH GEN")
            || name.contains("10900")
            || name.contains("10700")
            || name.contains("COMET LAKE")
        {
            ("Skylake", "Comet Lake", 14, 2020, false, None, None)
        } else if name.contains("ICE LAKE") || name.contains("1065") || name.contains("1035") {
            ("Sunny Cove", "Ice Lake", 10, 2019, false, None, None)
        } else if name.contains("9TH GEN") || name.contains("9900") || name.contains("COFFEE LAKE")
        {
            (
                "Skylake",
                "Coffee Lake Refresh",
                14,
                2018,
                false,
                None,
                None,
            )
        } else if name.contains("8TH GEN") || name.contains("8700") {
            ("Skylake", "Coffee Lake", 14, 2017, false, None, None)
        } else if name.contains("SAPPHIRE RAPIDS")
            || name.contains("W9-3")
            || name.contains("W7-3")
            || name.contains("W5-3")
        {
            ("Golden Cove", "Sapphire Rapids", 7, 2023, false, None, None)
        } else if name.contains("EMERALD RAPIDS") {
            ("Golden Cove", "Emerald Rapids", 7, 2023, false, None, None)
        } else if name.contains("GRANITE RAPIDS") {
            ("Redwood Cove", "Granite Rapids", 3, 2024, false, None, None)
        } else if name.contains("SIERRA FOREST") {
            ("Crestmont", "Sierra Forest", 3, 2024, false, None, None)
        } else if name.contains("LUNAR LAKE") {
            (
                "Lion Cove",
                "Lunar Lake",
                3,
                2024,
                true,
                Some("Lion Cove"),
                Some("Skymont"),
            )
        } else {
            ("Unknown Intel", "", 0, 0, false, None, None)
        };

        // A part outside the table has no name and no hybrid claim; its 0s are
        // placeholders in the table above, never published.
        let identified = !uarch.starts_with("Unknown");
        Microarchitecture {
            name: identified.then(|| uarch.to_string()),
            codename: codename.into(),
            process_nm: (process > 0).then_some(process),
            year: (year > 0).then_some(year),
            vendor: CpuVendor::Intel,
            arch: "x86_64".into(),
            hybrid: identified.then_some(hybrid),
            p_core_uarch: p_core.map(String::from),
            e_core_uarch: e_core.map(String::from),
        }
    }

    fn identify_amd(name: &str, _family: Option<u32>, _model: Option<u32>) -> Microarchitecture {
        // "RYZEN" contains "ZEN", so "AMD RYZEN 5" matched the "ZEN 5" marker and
        // every Ryzen 3, 4 or 5 part with no model-number hit was reported as Zen 3,
        // Zen 4 or Zen 5. The brand word is removed before the markers are tested.
        //
        // A mobile Ryzen SKU carries model numbers that are also desktop model
        // numbers ("3700U" is Zen+ Picasso, "5800U" is Cezanne, and the desktop table
        // says Matisse and Vermeer). Only desktop suffixes are matched against the
        // table; a mobile SKU is unidentified rather than given a desktop identity.
        let mobile_ryzen = name.contains("RYZEN")
            && !name.contains("THREADRIPPER")
            && ryzen_suffix_is_mobile(name);
        let name = &name.replace("RYZEN", " ");
        let (uarch, codename, process, year) = if mobile_ryzen {
            ("Unknown AMD", "", 0, 0)
        } else if name.contains("EPYC") {
            // The EPYC generation is the last digit of its 4-digit model number.
            // An EPYC name whose model number does not parse is unidentified; it
            // is never matched by the Ryzen-number markers below.
            epyc_generation(name).unwrap_or(("Unknown AMD", "", 0, 0))
        } else if name.contains("9950")
            || name.contains("9900")
            || name.contains("9700")
            || name.contains("9600")
            || name.contains("ZEN 5")
            || name.contains("GRANITE RIDGE")
        {
            ("Zen 5", "Granite Ridge", 4, 2024)
        } else if name.contains("7950")
            || name.contains("7900")
            || name.contains("7800")
            || name.contains("7700")
            || name.contains("7600")
            || name.contains("7500")
            || name.contains("ZEN 4")
        {
            ("Zen 4", "Raphael", 5, 2022)
        } else if name.contains("5950")
            || name.contains("5900")
            || name.contains("5800")
            || name.contains("5700")
            || name.contains("5600")
            || name.contains("5500")
            || name.contains("ZEN 3")
        {
            ("Zen 3", "Vermeer", 7, 2020)
        } else if name.contains("3950")
            || name.contains("3900")
            || name.contains("3800")
            || name.contains("3700")
            || name.contains("3600")
            || name.contains("ZEN 2")
        {
            ("Zen 2", "Matisse", 7, 2019)
        } else if name.contains("2700") || name.contains("2600") || name.contains("ZEN+") {
            ("Zen+", "Pinnacle Ridge", 12, 2018)
        } else if name.contains("1800") || name.contains("1700") || name.contains("ZEN 1") {
            ("Zen", "Summit Ridge", 14, 2017)
        } else if name.contains("GENOA") {
            ("Zen 4", "Genoa", 5, 2022)
        } else if name.contains("SIENA") {
            ("Zen 4c", "Siena", 5, 2023)
        } else if name.contains("TURIN") {
            ("Zen 5", "Turin", 4, 2024)
        // `PRO 7` and `PRO 5` alone matched Ryzen PRO mobile parts: "Ryzen 7 PRO
        // 7840U" is Phoenix, not Storm Peak, and "Ryzen 5 PRO 5650U" is not
        // Chagall. The Threadripper PRO names are spelled out instead.
        } else if name.contains("THREADRIPPER 7") || name.contains("THREADRIPPER PRO 7") {
            ("Zen 4", "Storm Peak", 5, 2023)
        } else if name.contains("THREADRIPPER 5") || name.contains("THREADRIPPER PRO 5") {
            ("Zen 3", "Chagall", 7, 2022)
        } else {
            ("Unknown AMD", "", 0, 0)
        };

        // Outside the table: no name, and the placeholder 0s are not published.
        let identified = !uarch.starts_with("Unknown");
        Microarchitecture {
            name: identified.then(|| uarch.to_string()),
            codename: codename.into(),
            process_nm: (process > 0).then_some(process),
            year: (year > 0).then_some(year),
            vendor: CpuVendor::AMD,
            arch: "x86_64".into(),
            hybrid: identified.then_some(false),
            p_core_uarch: None,
            e_core_uarch: None,
        }
    }

    fn identify_apple(name: &str) -> Microarchitecture {
        let (uarch, codename, process, year, hybrid) = if name.contains("M4 MAX")
            || name.contains("M4 PRO")
            || name.contains("M4 ULTRA")
        {
            ("Everest", "M4 Pro/Max/Ultra", 3, 2024, true)
        } else if name.contains("M4") {
            ("Everest", "M4", 3, 2024, true)
        } else if name.contains("M3 MAX") || name.contains("M3 PRO") || name.contains("M3 ULTRA") {
            ("Ibiza", "M3 Pro/Max/Ultra", 3, 2023, true)
        } else if name.contains("M3") {
            ("Ibiza", "M3", 3, 2023, true)
        } else if name.contains("M2 MAX") || name.contains("M2 PRO") || name.contains("M2 ULTRA") {
            ("Avalanche/Blizzard", "M2 Pro/Max/Ultra", 5, 2023, true)
        } else if name.contains("M2") {
            ("Avalanche/Blizzard", "M2", 5, 2022, true)
        } else if name.contains("M1 MAX") || name.contains("M1 PRO") || name.contains("M1 ULTRA") {
            ("Firestorm/Icestorm", "M1 Pro/Max/Ultra", 5, 2021, true)
        } else if name.contains("M1") {
            ("Firestorm/Icestorm", "M1", 5, 2020, true)
        } else {
            ("Unknown Apple", "", 0, 0, true)
        };

        // An unidentified Apple part is not called hybrid, and gets no core
        // names; the split below used to turn "Unknown Apple" into a P-core name.
        let identified = !uarch.starts_with("Unknown");
        Microarchitecture {
            name: identified.then(|| uarch.to_string()),
            codename: codename.into(),
            process_nm: (process > 0).then_some(process),
            year: (year > 0).then_some(year),
            vendor: CpuVendor::Apple,
            arch: "aarch64".into(),
            hybrid: identified.then_some(hybrid),
            // A single-name part ("Everest", "Ibiza") has no separate efficiency
            // core name in the table. It used to be given the placeholder "E-core",
            // which is not a microarchitecture.
            p_core_uarch: identified
                .then(|| uarch.split('/').next().map(String::from))
                .flatten(),
            e_core_uarch: identified
                .then(|| uarch.split('/').nth(1).map(String::from))
                .flatten(),
        }
    }

    /// Detect ISA extensions from CPU flags.
    fn detect_extensions(
        flags: Option<&[String]>,
        microarch: &Microarchitecture,
    ) -> Vec<IsaExtension> {
        Self::detect_extensions_with(flags, microarch, &x86_cpuid_feature)
    }

    /// `cpuid` answers for the features it can query directly, and `flags` is the
    /// platform's feature list when that list is complete. Both are injectable so
    /// the table can be tested without the host's CPU.
    fn detect_extensions_with(
        flags: Option<&[String]>,
        microarch: &Microarchitecture,
        cpuid: &dyn Fn(&str) -> Option<bool>,
    ) -> Vec<IsaExtension> {
        let flag_set: Option<std::collections::HashSet<String>> =
            flags.map(|list| list.iter().map(|f| f.to_lowercase()).collect());

        let mut extensions = Vec::new();

        // x86_64 extensions
        if microarch.arch == "x86_64" {
            let x86_exts = vec![
                ("sse", IsaCategory::Simd, "SSE", "Streaming SIMD Extensions"),
                (
                    "sse2",
                    IsaCategory::Simd,
                    "SSE2",
                    "Streaming SIMD Extensions 2",
                ),
                (
                    "sse3",
                    IsaCategory::Simd,
                    "SSE3",
                    "Streaming SIMD Extensions 3",
                ),
                ("ssse3", IsaCategory::Simd, "SSSE3", "Supplemental SSE3"),
                (
                    "sse4_1",
                    IsaCategory::Simd,
                    "SSE4.1",
                    "Streaming SIMD Extensions 4.1",
                ),
                (
                    "sse4_2",
                    IsaCategory::Simd,
                    "SSE4.2",
                    "Streaming SIMD Extensions 4.2",
                ),
                (
                    "avx",
                    IsaCategory::Simd,
                    "AVX",
                    "Advanced Vector Extensions",
                ),
                (
                    "avx2",
                    IsaCategory::Simd,
                    "AVX2",
                    "Advanced Vector Extensions 2",
                ),
                (
                    "avx512f",
                    IsaCategory::Simd,
                    "AVX-512F",
                    "AVX-512 Foundation",
                ),
                (
                    "avx512bw",
                    IsaCategory::Simd,
                    "AVX-512BW",
                    "AVX-512 Byte/Word",
                ),
                (
                    "avx512vl",
                    IsaCategory::Simd,
                    "AVX-512VL",
                    "AVX-512 Vector Length",
                ),
                (
                    "avx512cd",
                    IsaCategory::Simd,
                    "AVX-512CD",
                    "AVX-512 Conflict Detection",
                ),
                (
                    "avx512vnni",
                    IsaCategory::Simd,
                    "AVX-512 VNNI",
                    "Vector Neural Network Instructions",
                ),
                (
                    "avx512_bf16",
                    IsaCategory::MachineLearning,
                    "AVX-512 BF16",
                    "BFloat16 instructions",
                ),
                (
                    "avx512_fp16",
                    IsaCategory::FloatingPoint,
                    "AVX-512 FP16",
                    "Half-precision floating point",
                ),
                (
                    "amx_tile",
                    IsaCategory::MachineLearning,
                    "AMX-TILE",
                    "Advanced Matrix Extensions - Tile",
                ),
                (
                    "amx_bf16",
                    IsaCategory::MachineLearning,
                    "AMX-BF16",
                    "Advanced Matrix Extensions - BF16",
                ),
                (
                    "amx_int8",
                    IsaCategory::MachineLearning,
                    "AMX-INT8",
                    "Advanced Matrix Extensions - INT8",
                ),
                ("aes", IsaCategory::Crypto, "AES-NI", "AES New Instructions"),
                ("vaes", IsaCategory::Crypto, "VAES", "Vectorized AES"),
                (
                    "sha_ni",
                    IsaCategory::Crypto,
                    "SHA-NI",
                    "SHA New Instructions",
                ),
                (
                    "pclmulqdq",
                    IsaCategory::Crypto,
                    "PCLMULQDQ",
                    "Carry-Less Multiplication",
                ),
                (
                    "rdrand",
                    IsaCategory::Crypto,
                    "RDRAND",
                    "Hardware Random Number Generator",
                ),
                (
                    "rdseed",
                    IsaCategory::Crypto,
                    "RDSEED",
                    "Hardware Random Seed Generator",
                ),
                (
                    "vmx",
                    IsaCategory::Virtualization,
                    "VMX",
                    "Virtual Machine Extensions (VT-x)",
                ),
                (
                    "svm",
                    IsaCategory::Virtualization,
                    "SVM",
                    "Secure Virtual Machine (AMD-V)",
                ),
                (
                    "sgx",
                    IsaCategory::Security,
                    "SGX",
                    "Software Guard Extensions",
                ),
                (
                    "sev",
                    IsaCategory::Security,
                    "SEV",
                    "Secure Encrypted Virtualization",
                ),
                (
                    "sme",
                    IsaCategory::Security,
                    "SME",
                    "Secure Memory Encryption",
                ),
                (
                    "fma",
                    IsaCategory::FloatingPoint,
                    "FMA3",
                    "Fused Multiply-Add 3-operand",
                ),
                (
                    "f16c",
                    IsaCategory::FloatingPoint,
                    "F16C",
                    "Half-precision float conversion",
                ),
                (
                    "bmi1",
                    IsaCategory::BitManip,
                    "BMI1",
                    "Bit Manipulation Instructions 1",
                ),
                (
                    "bmi2",
                    IsaCategory::BitManip,
                    "BMI2",
                    "Bit Manipulation Instructions 2",
                ),
                (
                    "popcnt",
                    IsaCategory::BitManip,
                    "POPCNT",
                    "Population Count",
                ),
                (
                    "lzcnt",
                    IsaCategory::BitManip,
                    "LZCNT",
                    "Leading Zero Count",
                ),
                (
                    "cx16",
                    IsaCategory::Atomic,
                    "CMPXCHG16B",
                    "Compare and Exchange 16 bytes",
                ),
                (
                    "tsx",
                    IsaCategory::Memory,
                    "TSX",
                    "Transactional Synchronization Extensions",
                ),
                (
                    "mpx",
                    IsaCategory::Memory,
                    "MPX",
                    "Memory Protection Extensions",
                ),
                (
                    "clflushopt",
                    IsaCategory::Memory,
                    "CLFLUSHOPT",
                    "Optimized Cache Line Flush",
                ),
                ("clwb", IsaCategory::Memory, "CLWB", "Cache Line Write Back"),
            ];

            for (flag, cat, name, desc) in x86_exts {
                extensions.push(IsaExtension {
                    name: name.into(),
                    category: cat,
                    supported: supported_flag(flag, flag_set.as_ref(), cpuid),
                    description: desc.into(),
                });
            }
        }

        // AArch64 extensions (inferred from microarch when not available via flags)
        if microarch.arch == "aarch64" {
            let aarch_exts = vec![
                ("asimd", IsaCategory::Simd, "NEON/ASIMD", "Advanced SIMD"),
                ("sve", IsaCategory::Simd, "SVE", "Scalable Vector Extension"),
                (
                    "sve2",
                    IsaCategory::Simd,
                    "SVE2",
                    "Scalable Vector Extension 2",
                ),
                (
                    "aes",
                    IsaCategory::Crypto,
                    "AES",
                    "AES cryptographic instructions",
                ),
                (
                    "sha1",
                    IsaCategory::Crypto,
                    "SHA-1",
                    "SHA-1 cryptographic instructions",
                ),
                (
                    "sha2",
                    IsaCategory::Crypto,
                    "SHA-2",
                    "SHA-256 cryptographic instructions",
                ),
                (
                    "sha3",
                    IsaCategory::Crypto,
                    "SHA-3",
                    "SHA-3 cryptographic instructions",
                ),
                (
                    "pmull",
                    IsaCategory::Crypto,
                    "PMULL",
                    "Polynomial multiply long",
                ),
                (
                    "atomics",
                    IsaCategory::Atomic,
                    "LSE Atomics",
                    "Large System Extensions atomics",
                ),
                (
                    "fphp",
                    IsaCategory::FloatingPoint,
                    "FP16",
                    "Half-precision floating point",
                ),
                (
                    "bf16",
                    IsaCategory::MachineLearning,
                    "BF16",
                    "BFloat16 instructions",
                ),
                (
                    "i8mm",
                    IsaCategory::MachineLearning,
                    "I8MM",
                    "Int8 matrix multiply",
                ),
                (
                    "dotprod",
                    IsaCategory::MachineLearning,
                    "DotProd",
                    "Dot product instructions",
                ),
            ];

            for (flag, cat, name, desc) in aarch_exts {
                extensions.push(IsaExtension {
                    name: name.into(),
                    category: cat,
                    supported: supported_flag(flag, flag_set.as_ref(), cpuid),
                    description: desc.into(),
                });
            }
        }

        extensions
    }

    /// Infer performance characteristics from microarchitecture.
    fn infer_performance(
        model_name: &str,
        microarch: &Microarchitecture,
        cores: u32,
    ) -> InferredPerformance {
        let upper = model_name.to_uppercase();

        // Base IPC by microarchitecture
        let (base_ipc, st_score, tdp, boost) = match microarch.name.as_deref().unwrap_or("") {
            // Intel
            "Arrow Lake" | "Lion Cove" => (2.1, 92, 125, 5800),
            "Raptor Lake" | "Raptor Cove" => (2.0, 90, 125, 5800),
            "Alder Lake" | "Golden Cove" => (1.9, 85, 125, 5200),
            "Cypress Cove" => (1.7, 78, 125, 5300),
            "Willow Cove" => (1.8, 80, 28, 4800),
            "Sunny Cove" => (1.6, 72, 28, 3900),
            "Skylake" => (1.5, 68, 95, 5000),
            "Redwood Cove" => (2.0, 88, 350, 5300),
            "Crestmont" => (1.4, 55, 270, 3200),
            // AMD
            "Zen 5" => (2.2, 95, 170, 5700),
            "Zen 4" => (2.0, 90, 170, 5700),
            "Zen 3" => (1.9, 85, 105, 4900),
            "Zen 2" => (1.7, 77, 105, 4700),
            "Zen+" => (1.5, 68, 105, 4350),
            "Zen" => (1.4, 64, 95, 4100),
            "Zen 4c" => (1.8, 75, 225, 3700),
            // Apple
            "Everest" => (2.3, 98, 30, 4400),
            "Ibiza" => (2.2, 95, 22, 4100),
            "Avalanche/Blizzard" => (2.1, 92, 20, 3500),
            "Firestorm/Icestorm" => (2.0, 88, 15, 3200),
            // ARM server
            "Neoverse" => (1.6, 65, 250, 3400),
            _ => (1.0, 50, 65, 3000),
        };

        // Adjust for specific SKU
        let tdp_adjusted = if upper.contains("XEON") || upper.contains("EPYC") {
            tdp.max(200)
        } else if upper.contains("LAPTOP") || upper.contains("U)") || upper.contains("P)") {
            tdp.min(28)
        } else if upper.contains("H)") || upper.contains("HX)") {
            tdp.clamp(45, 55)
        } else {
            tdp
        };

        // MT scaling
        let mt_efficiency = if cores <= 4 {
            90
        } else if cores <= 8 {
            85
        } else if cores <= 16 {
            78
        } else if cores <= 32 {
            72
        } else {
            65
        };

        // Target workloads
        let mut workloads = Vec::new();
        if st_score >= 85 {
            workloads.push("Gaming".into());
            workloads.push("Desktop productivity".into());
        }
        if cores >= 8 {
            workloads.push("Content creation".into());
            workloads.push("Compilation".into());
        }
        if cores >= 16 {
            workloads.push("Video encoding".into());
        }
        if cores >= 32 || upper.contains("XEON") || upper.contains("EPYC") {
            workloads.push("Server workloads".into());
            workloads.push("Virtualization".into());
        }
        if microarch.vendor == CpuVendor::Apple {
            workloads.push("Efficient mobile computing".into());
        }

        let mut notes = Vec::new();
        if microarch.hybrid == Some(true) {
            notes.push("Hybrid architecture: scheduler-aware threading recommended".into());
        }
        if let Some(nm) = microarch.process_nm.filter(|&nm| nm <= 5) {
            notes.push(format!("Leading-edge {nm}nm process"));
        }

        InferredPerformance {
            single_thread_score: st_score,
            mt_scaling_efficiency: mt_efficiency,
            relative_ipc: base_ipc,
            estimated_boost_mhz: boost,
            estimated_tdp_watts: tdp_adjusted,
            target_workloads: workloads,
            notes,
        }
    }

    #[cfg(target_os = "linux")]
    fn read_cpu_info() -> Result<CpuInfoFields, IronError> {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").map_err(IronError::Io)?;

        let mut model_name: Option<String> = None;
        // `None` until /proc/cpuinfo supplies them. Zero was both the
        // "not seen yet" marker and a publishable value.
        let mut family: Option<u32> = None;
        let mut model: Option<u32> = None;
        let mut stepping: Option<u32> = None;
        // ARM kernels commonly omit both lines.
        let mut siblings: Option<u32> = None;

        for line in cpuinfo.lines() {
            if let Some((key, val)) = line.split_once(':') {
                let key = key.trim();
                let val = val.trim();
                match key {
                    "model name" if model_name.is_none() && !val.is_empty() => {
                        model_name = Some(val.into());
                    }
                    "cpu family" if family.is_none() => {
                        family = val.parse().ok();
                    }
                    "model" if model.is_none() => {
                        model = val.parse().ok();
                    }
                    "stepping" if stepping.is_none() => {
                        stepping = val.parse().ok();
                    }
                    "siblings" if siblings.is_none() => {
                        siblings = val.parse().ok().filter(|&n| n > 0);
                    }
                    _ => {}
                }
            }
        }

        // The physical count is unique (package, core) pairs, not `cpu cores`,
        // which is per package: a 2-socket machine reported half its cores.
        // `None` where the kernel gives no package or core ids (some ARM).
        let cores = physical_cores_from_cpuinfo(&cpuinfo);

        // No `siblings` line is no logical count. It used to become the
        // physical count, which made SMT "off" on any kernel that omits it.
        // The feature list is `flags` on x86 and `Features` on AArch64.
        let flags = parse_cpuinfo_flags(&cpuinfo);
        Ok((model_name, family, model, stepping, flags, cores, siblings))
    }

    #[cfg(target_os = "windows")]
    fn read_cpu_info() -> Result<CpuInfoFields, IronError> {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_Processor | Select-Object Name,Description,Family,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json",
            ])
            .output()
            .map_err(IronError::Io)?;

        let text = String::from_utf8_lossy(&output.stdout);
        let processors = parse_win32_processors(&text);

        // `Name` is trimmed in the parser. No name is `None`, not a stand-in.
        let model_name = processors.name;
        // Counts are summed over every package; see `parse_win32_processors`.
        // `None`, never `1`: a fallback of 1 was published as a measurement of
        // a single-core machine by `cpu.cores.physical` and both microarch
        // counts, with SMT off.
        let cores = processors.cores;
        // Not the physical count when absent: that made SMT "off".
        let threads = processors.threads;

        // WMI does expose the CPUID triple, and the comment that used to sit
        // here said it does not. `Win32_Processor.Description` is the string
        // "AMD64 Family 26 Model 68 Stepping 0", and `PROCESSOR_IDENTIFIER`
        // carries the same text -- which this file was already reading, two
        // functions down, for something else.
        //
        // Description first because it comes from the query already running;
        // the environment variable is the fallback, and is what a process
        // launched under an unusual environment might be missing.
        let (family, model, stepping) = processors
            .description
            .as_deref()
            .and_then(parse_cpuid_triple)
            .or_else(|| {
                std::env::var("PROCESSOR_IDENTIFIER")
                    .ok()
                    .as_deref()
                    .and_then(parse_cpuid_triple)
            })
            .map_or((None, None, None), |(f, m, s)| (Some(f), Some(m), Some(s)));

        // No feature list. The old reader asserted a fixed set of x86 flags from
        // the vendor name in `PROCESSOR_IDENTIFIER`, which is not a read. x86
        // features are now queried with CPUID in `detect_extensions`.
        let flags = Vec::new();

        Ok((model_name, family, model, stepping, flags, cores, threads))
    }

    #[cfg(target_os = "macos")]
    fn read_cpu_info() -> Result<CpuInfoFields, IronError> {
        let brand = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

        let family = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.family"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());

        let model = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.model"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());

        let stepping = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.stepping"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());

        // No feature list. `machdep.cpu.features` covers only the leaf-1 bits, and
        // Apple silicon has no such key. Absent flags therefore cannot be read as
        // `false`, and x86 features are queried with CPUID instead.
        let flags: Vec<String> = Vec::new();

        // `None` when unread, for the same reason as the Windows arm above.
        let count = |key: &str| {
            std::process::Command::new("sysctl")
                .args(["-n", key])
                .output()
                .ok()
                .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
                .filter(|&n: &u32| n > 0)
        };
        let cores = count("hw.physicalcpu");
        let threads = count("hw.logicalcpu");

        Ok((
            Some(brand).filter(|b| !b.is_empty()),
            family,
            model,
            stepping,
            flags,
            cores,
            threads,
        ))
    }

    /// A platform with no reader here reports nothing, including no core count.
    ///
    /// This returned `1, 1` -- one physical core and one logical -- which the
    /// resolver publishes as a measurement, because its "was this read" test is
    /// `> 0`. Every other field in the tuple was already empty or zero for
    /// exactly that reason; the two counts were the exception.
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn read_cpu_info() -> Result<CpuInfoFields, IronError> {
        Ok((None, None, None, None, Vec::new(), None, None))
    }
}

/// The generation of an EPYC part from its 4-digit model number, as AMD's model
/// grammar defines it: the series digit (7, 8 or 9) and the last digit.
/// `7xx1` Naples, `7xx2` Rome, `7xx3` Milan, `8xx4` Siena, `9xx4` Genoa,
/// `9xx5` Turin. Any other number, or no number, is `None`.
fn epyc_generation(name: &str) -> Option<(&'static str, &'static str, u32, u32)> {
    let after = name.split("EPYC ").nth(1)?;
    let digits: Vec<u8> = after.bytes().take_while(|b| b.is_ascii_digit()).collect();
    if digits.len() != 4 {
        return None;
    }
    Some(match (digits[0], digits[3]) {
        (b'7', b'1') => ("Zen", "Naples", 14, 2017),
        (b'7', b'2') => ("Zen 2", "Rome", 7, 2019),
        (b'7', b'3') => ("Zen 3", "Milan", 7, 2021),
        (b'8', b'4') => ("Zen 4c", "Siena", 5, 2023),
        (b'9', b'4') => ("Zen 4", "Genoa", 5, 2022),
        (b'9', b'5') => ("Zen 5", "Turin", 4, 2024),
        _ => return None,
    })
}

/// Whether a Ryzen name's model number carries a mobile suffix. The model number
/// is the first token with four leading digits ("7840U", "9900X"). Its letters
/// after the digits are a desktop CPU when they are X, XT, F, XE or X3D. G, GE
/// and GT are desktop APUs built on the mobile die (5600G is Cezanne, not
/// Vermeer), so they are treated like U, H, HS, HX or HN: not identified from
/// the desktop table. A name with no such token is not classed as mobile.
fn ryzen_suffix_is_mobile(name: &str) -> bool {
    let Some(token) = name
        .split_whitespace()
        .find(|t| t.bytes().take_while(|b| b.is_ascii_digit()).count() == 4)
    else {
        return false;
    };
    let suffix = token.trim_start_matches(|c: char| c.is_ascii_digit());
    !suffix.is_empty() && !matches!(suffix, "X" | "XT" | "F" | "XE" | "X3D")
}

/// Whether the processor implements an x86 feature, from CPUID through
/// `std::arch`. `None` for names the standard library cannot query, and on
/// targets that are not x86.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn x86_cpuid_feature(flag: &str) -> Option<bool> {
    macro_rules! query {
        // `tt`, not `literal`: the standard library's macro only accepts its
        // feature name as a bare token.
        ($($table:literal => $feature:tt),* $(,)?) => {
            match flag {
                $($table => Some(std::arch::is_x86_feature_detected!($feature)),)*
                _ => None,
            }
        };
    }
    // `tsx` is the table's name for RTM, which is what CPUID reports for it.
    query! {
        "sse" => "sse",
        "sse2" => "sse2",
        "sse3" => "sse3",
        "ssse3" => "ssse3",
        "sse4_1" => "sse4.1",
        "sse4_2" => "sse4.2",
        "avx" => "avx",
        "avx2" => "avx2",
        "avx512f" => "avx512f",
        "avx512bw" => "avx512bw",
        "avx512vl" => "avx512vl",
        "avx512cd" => "avx512cd",
        "avx512vnni" => "avx512vnni",
        "avx512_bf16" => "avx512bf16",
        "aes" => "aes",
        "vaes" => "vaes",
        "sha_ni" => "sha",
        "pclmulqdq" => "pclmulqdq",
        "rdrand" => "rdrand",
        "rdseed" => "rdseed",
        "fma" => "fma",
        "f16c" => "f16c",
        "bmi1" => "bmi1",
        "bmi2" => "bmi2",
        "popcnt" => "popcnt",
        "lzcnt" => "lzcnt",
        "cx16" => "cmpxchg16b",
        "tsx" => "rtm",
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn x86_cpuid_feature(_flag: &str) -> Option<bool> {
    None
}

/// One table entry's state: the CPUID answer if there is one, else the
/// platform's complete feature list if it was read, else unknown.
fn supported_flag(
    flag: &str,
    flag_set: Option<&std::collections::HashSet<String>>,
    cpuid: &dyn Fn(&str) -> Option<bool>,
) -> Option<bool> {
    cpuid(flag).or_else(|| flag_set.map(|set| set.contains(flag)))
}

/// The feature list from `/proc/cpuinfo`: `flags` on x86, `Features` on ARM.
/// Empty when neither line is present.
#[cfg(any(target_os = "linux", test))]
fn parse_cpuinfo_flags(cpuinfo: &str) -> Vec<String> {
    for line in cpuinfo.lines() {
        if let Some((key, val)) = line.split_once(':') {
            if matches!(key.trim(), "flags" | "Features") {
                return val.split_whitespace().map(String::from).collect();
            }
        }
    }
    Vec::new()
}

/// Physical cores from `/proc/cpuinfo`: the number of distinct
/// `(physical id, core id)` pairs over all processor blocks.
///
/// `cpu cores` is per package, so it is not summed here: a two-socket machine
/// would be reported with half its cores. Each core appears once per hardware
/// thread, which is why the pairs are collected rather than counted. `None` when
/// no block carries both ids, as on ARM kernels, and never a count inferred from
/// the logical one.
///
/// `src/interconnect` uses this too, so the two cannot disagree.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn physical_cores_from_cpuinfo(cpuinfo: &str) -> Option<u32> {
    let mut seen = std::collections::HashSet::new();
    let (mut package, mut core): (Option<u32>, Option<u32>) = (None, None);
    // The final empty line closes the last block when the text does not end
    // with a blank line.
    for line in cpuinfo.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if let (Some(p), Some(c)) = (package.take(), core.take()) {
                seen.insert((p, c));
            }
            package = None;
            core = None;
            continue;
        }
        if let Some((key, val)) = line.split_once(':') {
            match key.trim() {
                "physical id" => package = val.trim().parse().ok(),
                "core id" => core = val.trim().parse().ok(),
                _ => {}
            }
        }
    }
    (!seen.is_empty()).then_some(seen.len() as u32)
}

/// What `Win32_Processor` reported, with counts summed over every package.
#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Win32Processors {
    pub name: Option<String>,
    pub description: Option<String>,
    pub cores: Option<u32>,
    pub threads: Option<u32>,
}

/// Parses `Get-CimInstance Win32_Processor | ... | ConvertTo-Json`.
///
/// PowerShell writes one object for one package and an array for several, so
/// both shapes are accepted. Counts are summed across the packages, and every
/// package must report a count, or the sum is short and no count is given. The
/// name and description come from the first package.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn parse_win32_processors(json: &str) -> Win32Processors {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json.trim()) else {
        return Win32Processors::default();
    };
    let packages: Vec<&serde_json::Value> = match &value {
        serde_json::Value::Array(items) => items.iter().collect(),
        serde_json::Value::Object(_) => vec![&value],
        _ => Vec::new(),
    };
    let Some(first) = packages.first() else {
        return Win32Processors::default();
    };

    let name = first["Name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let description = first["Description"].as_str().map(String::from);

    // A WMI count of 0 is not a processor. One package without a count makes
    // the total unknown rather than short.
    let sum = |key: &str| -> Option<u32> {
        let counts: Option<Vec<u32>> = packages
            .iter()
            .map(|p| p[key].as_u64().map(|v| v as u32).filter(|&n| n > 0))
            .collect();
        counts.map(|c| c.iter().sum())
    };

    Win32Processors {
        name,
        description,
        cores: sum("NumberOfCores"),
        threads: sum("NumberOfLogicalProcessors"),
    }
}

/// The CPU's identity -- its model name and physical core count -- probed
/// once per process.
///
/// **One cache for every surface that needs these two facts.** Before this,
/// the TUI kept its own `OnceLock` fed from a background thread, instance 25
/// added a second, blocking one to `backend.rs`, and `agent/state.rs` read
/// neither: it set `cores` to the logical count and, on macOS, named the CPU
/// "Apple Silicon / Intel". Four other modules each read
/// `machdep.cpu.brand_string` themselves. Every surface that disagrees with
/// another about how many cores a machine has is a surface that read the count
/// its own way, so there is now one way.
///
/// Both fields are `None` where the platform did not report them. A physical
/// count of `0` from the reader means "not reported", and the logical count is
/// never substituted for it -- that substitution is the defect this exists to
/// end.
#[derive(Debug, Clone, Default)]
pub struct CpuIdentity {
    /// Marketing name, e.g. "AMD Ryzen 9 9900X 12-Core Processor".
    pub model_name: Option<String>,
    /// Physical cores, distinct from hardware threads.
    pub physical_cores: Option<usize>,
}

static CPU_IDENTITY: std::sync::OnceLock<CpuIdentity> = std::sync::OnceLock::new();

/// The CPU's identity, probing on first use and blocking until it has it.
///
/// Probing can take a noticeable time on Windows, where it goes through WMI.
/// Call this only off any thread that draws: the CLI, the agent's `ask()` path
/// (already off the UI thread, and already waiting on a model), or a
/// background thread. A UI thread uses [`cpu_identity_if_known`].
pub fn cpu_identity() -> &'static CpuIdentity {
    CPU_IDENTITY.get_or_init(|| {
        let Ok(monitor) = CpuMicroarchMonitor::new() else {
            return CpuIdentity::default();
        };
        let report = monitor.report();
        CpuIdentity {
            model_name: report
                .model_name
                .as_deref()
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty()),
            physical_cores: report.physical_cores.map(|p| p as usize),
        }
    })
}

/// The CPU's identity if it has already been probed, without blocking.
///
/// `None` both before the probe has run and after it has run and found
/// nothing; a display treats the two the same, by showing what it does know.
pub fn cpu_identity_if_known() -> Option<&'static CpuIdentity> {
    CPU_IDENTITY.get()
}

/// Physical core count; see [`cpu_identity`] for when it may block.
pub fn physical_cores() -> Option<usize> {
    cpu_identity().physical_cores
}

/// Physical core count if already probed; never blocks.
pub fn physical_cores_if_known() -> Option<usize> {
    cpu_identity_if_known().and_then(|id| id.physical_cores)
}

impl Default for CpuMicroarchMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            report: CpuMicroarchReport {
                model_name: None,
                family: None,
                model: None,
                stepping: None,
                microarch: Microarchitecture {
                    name: None,
                    codename: String::new(),
                    process_nm: None,
                    year: None,
                    vendor: CpuVendor::Unknown,
                    arch: String::new(),
                    hybrid: None,
                    p_core_uarch: None,
                    e_core_uarch: None,
                },
                extensions: Vec::new(),
                performance: InferredPerformance {
                    single_thread_score: 0,
                    mt_scaling_efficiency: 0,
                    relative_ipc: 0.0,
                    estimated_boost_mhz: 0,
                    estimated_tdp_watts: 0,
                    target_workloads: Vec::new(),
                    notes: Vec::new(),
                },
                physical_cores: None,
                logical_cores: None,
                smt_enabled: None,
            },
        })
    }
}

#[cfg(all(test, target_os = "windows"))]
mod cpuid_triple_tests {
    use super::parse_cpuid_triple;

    #[test]
    fn a_windows_processor_description_yields_its_cpuid_triple() {
        // Exactly what `Win32_Processor.Description` returns on the machine
        // this was written against, and what `PROCESSOR_IDENTIFIER` returns
        // there with the vendor appended.
        assert_eq!(
            parse_cpuid_triple("AMD64 Family 26 Model 68 Stepping 0"),
            Some((26, 68, 0))
        );
        assert_eq!(
            parse_cpuid_triple("AMD64 Family 26 Model 68 Stepping 0, AuthenticAMD"),
            Some((26, 68, 0))
        );
        // An Intel form, with a different leading token.
        assert_eq!(
            parse_cpuid_triple("x86 Family 6 Model 142 Stepping 10"),
            Some((6, 142, 10))
        );
    }

    #[test]
    fn two_thirds_of_a_triple_is_not_a_triple() {
        // A partial reading is an absent one. Publishing a family with no
        // model invites matching a CPU against the wrong microarchitecture,
        // which is the whole reason these three travel together.
        assert_eq!(parse_cpuid_triple("AMD64 Family 26 Model 68"), None);
        assert_eq!(parse_cpuid_triple("AMD64 Family 26"), None);
        assert_eq!(parse_cpuid_triple(""), None);
        assert_eq!(parse_cpuid_triple("Intel64 Family Model Stepping"), None);
        // A vendor string that merely mentions the words.
        assert_eq!(parse_cpuid_triple("Family Values Model Agency"), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intel_raptor_lake() {
        let uarch = CpuMicroarchMonitor::identify_microarch(
            "13th Gen Intel(R) Core(TM) i9-13900K",
            Some(6),
            Some(0xB7),
            Some(1),
        );
        assert_eq!(uarch.name.as_deref(), Some("Raptor Lake"));
        assert_eq!(uarch.vendor, CpuVendor::Intel);
        assert_eq!(uarch.hybrid, Some(true));
        assert_eq!(uarch.process_nm, Some(7));
    }

    #[test]
    fn test_amd_zen4() {
        let uarch = CpuMicroarchMonitor::identify_microarch(
            "AMD Ryzen 9 7950X 16-Core Processor",
            Some(25),
            Some(97),
            Some(2),
        );
        assert_eq!(uarch.name.as_deref(), Some("Zen 4"));
        assert_eq!(uarch.vendor, CpuVendor::AMD);
        assert_eq!(uarch.hybrid, Some(false));
        assert_eq!(uarch.process_nm, Some(5));
    }

    /// The published CPUID triple is absent where it was not read, not zero.
    ///
    /// `read_cpu_info` on Windows returned `(0, 0, 0)` under a comment saying
    /// WMI does not expose the triple. Family 0 is not a value any modern x86
    /// CPU reports — a Ryzen 9 9900X is family 0x1A — so the report carried an
    /// impossible triple rather than an absent one, and `ironmon`'s own ontology
    /// had to test `family > 0` to work out whether to publish it.
    #[test]
    fn an_unread_cpuid_triple_is_absent_not_zero() {
        let report = CpuMicroarchMonitor::new()
            .expect("a report")
            .report()
            .clone();

        // Whatever the platform, the three go together: all read or none.
        let present = [
            report.family.is_some(),
            report.model.is_some(),
            report.stepping.is_some(),
        ];
        assert!(
            present.iter().all(|p| *p) || present.iter().all(|p| !*p),
            concat!(
                "family/model/stepping are decoded from one CPUID leaf ",
                "and must not be published separately: {:?}"
            ),
            present
        );

        assert_ne!(
            report.family,
            Some(0),
            "family 0 is not a value any modern x86 CPU reports"
        );
    }

    /// `Win32_Processor.Name` is space-padded to a fixed width.
    #[test]
    fn the_model_name_carries_no_padding() {
        let report = CpuMicroarchMonitor::new()
            .expect("a report")
            .report()
            .clone();
        if let Some(name) = &report.model_name {
            assert_eq!(
                name.trim(),
                name,
                "the model name kept its WMI padding: {:?}",
                name
            );
            assert_ne!(name.trim(), "Unknown CPU", "a stand-in name was published");
        }
    }

    #[test]
    fn an_unidentified_part_has_no_name_and_no_hybrid_claim() {
        // Apple parts are hybrid, but an unlisted Apple part is not known to be.
        let apple = CpuMicroarchMonitor::identify_microarch("Apple M9 Ultra", None, None, None);
        assert_eq!(apple.name, None);
        assert_eq!(apple.hybrid, None);
        assert_eq!(apple.p_core_uarch, None);
        assert_eq!(apple.e_core_uarch, None);
        assert_eq!(apple.process_nm, None);
        assert_eq!(apple.year, None);

        let amd = CpuMicroarchMonitor::identify_microarch("AMD Mystery 1000", None, None, None);
        assert_eq!(amd.name, None);
        assert_eq!(amd.hybrid, None);

        let intel = CpuMicroarchMonitor::identify_microarch("Intel Mystery", None, None, None);
        assert_eq!(intel.name, None);
        assert_eq!(intel.hybrid, None);

        // A Graviton with no generation in its name gets none.
        let graviton = CpuMicroarchMonitor::identify_microarch("AWS Graviton", None, None, None);
        assert_eq!(graviton.codename, "");
        assert_eq!(graviton.process_nm, None);
        assert_eq!(graviton.year, None);
    }

    #[test]
    fn graviton_generations_carry_only_their_published_figures() {
        let g2 = CpuMicroarchMonitor::identify_microarch("AWS GRAVITON2", None, None, None);
        assert_eq!(g2.codename, "Graviton2");
        assert_eq!(g2.process_nm, Some(7));
        assert_eq!(g2.year, Some(2020));

        let g3 = CpuMicroarchMonitor::identify_microarch("AWS GRAVITON3", None, None, None);
        assert_eq!(g3.codename, "Graviton3");
        assert_eq!(g3.process_nm, Some(5));
        assert_eq!(g3.year, Some(2021));

        // Neoverse V2's node is not public, so Graviton4 has no node.
        let g4 = CpuMicroarchMonitor::identify_microarch("AWS GRAVITON4", None, None, None);
        assert_eq!(g4.codename, "Graviton4");
        assert_eq!(g4.process_nm, None);
        assert_eq!(g4.year, Some(2023));
    }

    #[test]
    fn linux_physical_cores_are_unique_package_core_pairs() {
        // Two sockets of two cores each, two threads per core. Each processor
        // block repeats its core id per thread, and "cpu cores" is per package.
        let mut text = String::new();
        let mut processor = 0;
        for package in 0..2 {
            for core in 0..2 {
                for _thread in 0..2 {
                    text.push_str(&format!(
                        concat!(
                            "processor\t: {processor}\n",
                            "vendor_id\t: GenuineIntel\n",
                            "model name\t: Intel(R) Xeon(R) Gold 6138 CPU @ 2.00GHz\n",
                            "siblings\t: 4\n",
                            "cpu cores\t: 2\n",
                            "physical id\t: {package}\n",
                            "core id\t\t: {core}\n\n",
                        ),
                        processor = processor,
                        package = package,
                        core = core,
                    ));
                    processor += 1;
                }
            }
        }
        assert_eq!(physical_cores_from_cpuinfo(&text), Some(4));
        // A single socket with the same shape.
        let single: String = text
            .split("\n\n")
            .take(4)
            .map(|b| format!("{b}\n\n"))
            .collect();
        assert_eq!(physical_cores_from_cpuinfo(&single), Some(2));
    }

    #[test]
    fn cpuinfo_without_package_or_core_ids_has_no_physical_count() {
        // ARM kernels print a CPU part and no topology: a count would be a guess.
        let arm = concat!(
            "processor\t: 0\n",
            "Features\t: fp asimd\n",
            "CPU implementer\t: 0x41\n",
            "CPU part\t: 0xd0c\n\n",
            "processor\t: 1\n",
            "Features\t: fp asimd\n",
            "CPU part\t: 0xd0c\n\n",
        );
        assert_eq!(physical_cores_from_cpuinfo(arm), None);
        assert_eq!(physical_cores_from_cpuinfo(""), None);
    }

    #[test]
    fn windows_processor_object_and_array_shapes_both_parse() {
        // One package: ConvertTo-Json writes an object.
        // WMI pads Name to a fixed width; the padding is built here, not written.
        let pad = " ".repeat(17);
        let one = format!(
            r#"{{"Name":"AMD Ryzen 9 9900X 12-Core Processor{pad}","Description":"AMD64 Family 26 Model 68 Stepping 0","Family":107,"NumberOfCores":12,"NumberOfLogicalProcessors":24}}"#
        );
        let one = one.as_str();
        let p = parse_win32_processors(one);
        assert_eq!(
            p.name.as_deref(),
            Some("AMD Ryzen 9 9900X 12-Core Processor")
        );
        assert_eq!(
            p.description.as_deref(),
            Some("AMD64 Family 26 Model 68 Stepping 0")
        );
        assert_eq!(p.cores, Some(12));
        assert_eq!(p.threads, Some(24));

        // Two packages: an array. Counts sum; the name is the first package's.
        let two = r#"[{"Name":"Intel(R) Xeon(R) Gold 6138 CPU @ 2.00GHz","Description":"x86 Family 6 Model 85 Stepping 4","NumberOfCores":20,"NumberOfLogicalProcessors":40},{"Name":"Intel(R) Xeon(R) Gold 6138 CPU @ 2.00GHz","Description":"x86 Family 6 Model 85 Stepping 4","NumberOfCores":20,"NumberOfLogicalProcessors":40}]"#;
        let p = parse_win32_processors(two);
        assert_eq!(
            p.name.as_deref(),
            Some("Intel(R) Xeon(R) Gold 6138 CPU @ 2.00GHz")
        );
        assert_eq!(p.cores, Some(40));
        assert_eq!(p.threads, Some(80));
        assert_ne!(p.name.as_deref(), Some("Unknown CPU"));
    }

    #[test]
    fn windows_processor_without_a_name_or_count_is_none() {
        let empty = parse_win32_processors("");
        assert_eq!(empty, Win32Processors::default());
        assert_eq!(empty.name, None);

        // A package with no count makes the total unknown, not short.
        let partial = r#"[{"Name":"CPU A","NumberOfCores":8,"NumberOfLogicalProcessors":16},{"Name":"CPU A","NumberOfLogicalProcessors":16}]"#;
        let p = parse_win32_processors(partial);
        assert_eq!(p.cores, None);
        assert_eq!(p.threads, Some(32));

        let blank = parse_win32_processors(r#"{"Name":"   ","NumberOfCores":0}"#);
        assert_eq!(blank.name, None);
        assert_eq!(blank.cores, None);
    }

    #[test]
    fn test_apple_m3() {
        // Apple reports no x86 CPUID triple; identification is by name.
        let uarch = CpuMicroarchMonitor::identify_microarch("Apple M3 Pro", None, None, None);
        assert_eq!(uarch.name.as_deref(), Some("Ibiza"));
        assert_eq!(uarch.vendor, CpuVendor::Apple);
        assert_eq!(uarch.hybrid, Some(true));
        assert_eq!(uarch.process_nm, Some(3));
    }

    /// An Arm part whose name carries "core" is not an Intel part.
    #[test]
    fn an_arm_part_with_a_core_count_in_its_name_is_not_intel() {
        let snapdragon = CpuMicroarchMonitor::identify_microarch(
            "Snapdragon(R) X 12-core X1E80100 @ 3.40 GHz",
            None,
            None,
            None,
        );
        assert_eq!(snapdragon.vendor, CpuVendor::Qualcomm);
        assert_eq!(snapdragon.arch, "aarch64");
        assert_eq!(snapdragon.name, None);
        assert_eq!(snapdragon.hybrid, None);
    }

    /// Ryzen PRO mobile parts share the "PRO 7" and "PRO 5" spellings with the
    /// Threadripper PRO line, and must not take its codenames.
    #[test]
    fn ryzen_pro_mobile_parts_do_not_take_threadripper_codenames() {
        for name in ["AMD Ryzen 7 PRO 7840U", "AMD Ryzen 5 PRO 5650U"] {
            let uarch = CpuMicroarchMonitor::identify_microarch(name, None, None, None);
            assert_eq!(uarch.name, None, "{name} was given a generation");
            assert_eq!(uarch.codename, "", "{name} took a codename");
            assert_eq!(uarch.process_nm, None, "{name} took a process node");
        }

        // "RYZEN" contains "ZEN": a Ryzen 3 with no model-number hit is not Zen 3.
        let r3 = CpuMicroarchMonitor::identify_microarch("AMD Ryzen 3 3200G", None, None, None);
        assert_eq!(r3.name, None, "a Ryzen 3 was matched by the ZEN 3 marker");

        let wx = CpuMicroarchMonitor::identify_microarch(
            "AMD Ryzen Threadripper PRO 7995WX",
            None,
            None,
            None,
        );
        assert_eq!(wx.codename, "Storm Peak");
        let wx5 = CpuMicroarchMonitor::identify_microarch(
            "AMD Ryzen Threadripper PRO 5995WX",
            None,
            None,
            None,
        );
        assert_eq!(wx5.codename, "Chagall");
    }

    /// A single-name Apple part has no efficiency-core name in the table.
    #[test]
    fn a_single_name_apple_part_has_no_efficiency_core_name() {
        let m3 = CpuMicroarchMonitor::identify_microarch("Apple M3 Pro", None, None, None);
        assert_eq!(m3.e_core_uarch, None);
        let m1 = CpuMicroarchMonitor::identify_microarch("Apple M1", None, None, None);
        assert_eq!(m1.p_core_uarch.as_deref(), Some("Firestorm"));
        assert_eq!(m1.e_core_uarch.as_deref(), Some("Icestorm"));
    }

    /// The EPYC generation is the last digit of the model number.
    #[test]
    fn epyc_generation_comes_from_the_model_number() {
        let id = |n: &str| CpuMicroarchMonitor::identify_microarch(n, None, None, None);
        let rome = id("AMD EPYC 7502 32-Core Processor");
        assert_eq!(
            (rome.name.as_deref(), rome.codename.as_str()),
            (Some("Zen 2"), "Rome")
        );
        let milan = id("AMD EPYC 7763 64-Core Processor");
        assert_eq!(
            (milan.name.as_deref(), milan.codename.as_str()),
            (Some("Zen 3"), "Milan")
        );
        let genoa = id("AMD EPYC 9654 96-Core Processor");
        assert_eq!(
            (genoa.name.as_deref(), genoa.codename.as_str()),
            (Some("Zen 4"), "Genoa")
        );
        // A 5 on the series 9 part is Turin, and a model number that does not
        // parse is unidentified rather than matched by a substring.
        let turin = id("AMD EPYC 9655 96-Core Processor");
        assert_eq!(turin.codename, "Turin");
        let odd = id("AMD EPYC 7 Series Processor");
        assert_eq!(odd.name, None);
        assert_eq!(odd.codename, "");
    }

    /// Mobile Ryzen SKUs share model numbers with desktop parts, and must not take
    /// the desktop identity. The desktop 9900X, which has an X suffix, keeps its own.
    #[test]
    fn mobile_ryzen_skus_do_not_take_desktop_identities() {
        for name in ["AMD Ryzen 7 3700U", "AMD Ryzen 7 5800U", "AMD Ryzen 5 5600G"] {
            let uarch = CpuMicroarchMonitor::identify_microarch(name, None, None, None);
            assert_eq!(uarch.name, None, "{name} took a desktop generation");
            assert_eq!(uarch.codename, "", "{name} took a desktop codename");
        }
        let x = CpuMicroarchMonitor::identify_microarch(
            "AMD Ryzen 9 9900X 12-Core Processor",
            None,
            None,
            None,
        );
        assert_eq!(x.name.as_deref(), Some("Zen 5"));
        assert_eq!(x.codename, "Granite Ridge");
    }

    #[test]
    fn test_performance_inference() {
        let uarch = Microarchitecture {
            name: Some("Zen 4".into()),
            codename: "Raphael".into(),
            process_nm: Some(5),
            year: Some(2022),
            vendor: CpuVendor::AMD,
            arch: "x86_64".into(),
            hybrid: Some(false),
            p_core_uarch: None,
            e_core_uarch: None,
        };
        let perf = CpuMicroarchMonitor::infer_performance("AMD Ryzen 9 7950X", &uarch, 16);
        assert!(perf.single_thread_score >= 85);
        assert!(perf.relative_ipc >= 1.8);
        assert!(perf.estimated_tdp_watts > 100);
    }

    #[test]
    fn test_extension_detection() {
        let flags: Vec<String> = vec!["avx2", "aes", "fma", "bmi2", "popcnt", "avx512f"]
            .into_iter()
            .map(String::from)
            .collect();
        let uarch = Microarchitecture {
            name: Some("Zen 4".into()),
            codename: "".into(),
            process_nm: Some(5),
            year: Some(2022),
            vendor: CpuVendor::AMD,
            arch: "x86_64".into(),
            hybrid: Some(false),
            p_core_uarch: None,
            e_core_uarch: None,
        };
        // A complete list, with CPUID unable to answer: listed names are present,
        // and an unlisted name is absent.
        let exts =
            CpuMicroarchMonitor::detect_extensions_with(Some(&flags), &uarch, &|_: &str| None);
        let avx2 = exts.iter().find(|e| e.name == "AVX2").unwrap();
        assert_eq!(avx2.supported, Some(true));
        let avx512 = exts.iter().find(|e| e.name == "AVX-512F").unwrap();
        assert_eq!(avx512.supported, Some(true));
        let avx = exts.iter().find(|e| e.name == "AVX").unwrap();
        assert_eq!(avx.supported, Some(false));
    }

    #[test]
    fn an_unread_feature_is_unknown_not_absent() {
        let uarch = Microarchitecture {
            name: Some("Zen 4".into()),
            codename: "".into(),
            process_nm: Some(5),
            year: Some(2022),
            vendor: CpuVendor::AMD,
            arch: "x86_64".into(),
            hybrid: Some(false),
            p_core_uarch: None,
            e_core_uarch: None,
        };
        // Neither a feature list nor CPUID: every entry is unknown.
        let exts = CpuMicroarchMonitor::detect_extensions_with(None, &uarch, &|_: &str| None);
        assert!(!exts.is_empty());
        assert!(exts.iter().all(|e| e.supported.is_none()));

        // CPUID answers for the features it can query, and with no list the rest
        // stay unknown.
        let exts = CpuMicroarchMonitor::detect_extensions_with(None, &uarch, &|f: &str| {
            (f == "avx2").then_some(true)
        });
        assert_eq!(
            exts.iter().find(|e| e.name == "AVX2").unwrap().supported,
            Some(true)
        );
        // Not in the list, and CPUID has no answer for it: unknown, not false.
        assert_eq!(
            exts.iter()
                .find(|e| e.name == "AVX-512F")
                .unwrap()
                .supported,
            None
        );
    }

    #[test]
    fn linux_cpuinfo_feature_lines_are_read_on_x86_and_arm() {
        let x86 = "processor\t: 0\nvendor_id\t: GenuineIntel\nflags\t\t: fpu sse2 avx2\n";
        assert_eq!(
            parse_cpuinfo_flags(x86),
            vec!["fpu".to_string(), "sse2".into(), "avx2".into()]
        );

        // ARM kernels name the list `Features`, and the table's AArch64 names
        // are read from it.
        let arm = "processor\t: 0\nFeatures\t: fp asimd evtstrm aes pmull sha1 sha2 atomics\n";
        let flags = parse_cpuinfo_flags(arm);
        assert!(flags.contains(&"asimd".to_string()));
        assert!(flags.contains(&"atomics".to_string()));

        assert!(parse_cpuinfo_flags("processor\t: 0\n").is_empty());
    }

    #[test]
    fn test_monitor_default() {
        let monitor = CpuMicroarchMonitor::default();
        let _report = monitor.report();
    }

    #[test]
    fn test_serialization() {
        let uarch = Microarchitecture {
            name: Some("Zen 4".into()),
            codename: "Raphael".into(),
            process_nm: Some(5),
            year: Some(2022),
            vendor: CpuVendor::AMD,
            arch: "x86_64".into(),
            hybrid: Some(false),
            p_core_uarch: None,
            e_core_uarch: None,
        };
        let json = serde_json::to_string(&uarch).unwrap();
        assert!(json.contains("Zen 4"));
        let _: Microarchitecture = serde_json::from_str(&json).unwrap();
    }
}
