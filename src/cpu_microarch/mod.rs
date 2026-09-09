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
    String,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Vec<String>,
    u32,
    u32,
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
    pub name: String,
    /// Code name (e.g. "Raphael", "Raptor Lake-S").
    pub codename: String,
    /// Manufacturing process node in nanometers (e.g. 5, 7, 10).
    pub process_nm: u32,
    /// Year of introduction.
    pub year: u32,
    /// CPU vendor.
    pub vendor: CpuVendor,
    /// Architecture family (e.g. "x86_64", "aarch64", "riscv64").
    pub arch: String,
    /// Whether this is a hybrid/big.LITTLE design.
    pub hybrid: bool,
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
    /// Whether this extension is present.
    pub supported: bool,
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
    /// CPU model name string.
    pub model_name: String,
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
    /// Physical core count.
    pub physical_cores: u32,
    /// Logical core count (with SMT/HT).
    pub logical_cores: u32,
    /// Whether SMT/Hyper-Threading is enabled.
    pub smt_enabled: bool,
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
        let microarch = Self::identify_microarch(&model_name, family, model, stepping);
        let extensions = Self::detect_extensions(&flags, &microarch);
        let performance = Self::infer_performance(&model_name, &microarch, cores);
        let smt_enabled = threads > cores && cores > 0;

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
            .filter(|e| e.supported)
            .collect()
    }

    /// Check if a specific extension is supported by name.
    pub fn has_extension(&self, name: &str) -> bool {
        let upper = name.to_uppercase();
        self.report
            .extensions
            .iter()
            .any(|e| e.supported && e.name.to_uppercase().contains(&upper))
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
        // "CORE" matching in "16-Core Processor" AMD strings
        let vendor = if upper.contains("AMD")
            || upper.contains("RYZEN")
            || upper.contains("EPYC")
            || upper.contains("THREADRIPPER")
            || upper.contains("ATHLON")
        {
            CpuVendor::AMD
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
        } else if upper.contains("GRAVITON") {
            CpuVendor::AWS
        } else if upper.contains("AMPERE") || upper.contains("ALTRA") {
            CpuVendor::Ampere
        } else if upper.contains("SNAPDRAGON") || upper.contains("QUALCOMM") {
            CpuVendor::Qualcomm
        } else {
            CpuVendor::Unknown
        };

        match vendor {
            CpuVendor::Intel => Self::identify_intel(&upper, family, model),
            CpuVendor::AMD => Self::identify_amd(&upper, family, model),
            CpuVendor::Apple => Self::identify_apple(&upper),
            CpuVendor::AWS => Microarchitecture {
                name: "Neoverse".into(),
                codename: if upper.contains("GRAVITON4") {
                    "Graviton4".into()
                } else if upper.contains("GRAVITON3") {
                    "Graviton3".into()
                } else {
                    "Graviton2".into()
                },
                process_nm: 5,
                year: 2023,
                vendor,
                arch: "aarch64".into(),
                hybrid: false,
                p_core_uarch: None,
                e_core_uarch: None,
            },
            _ => Microarchitecture {
                name: "Unknown".into(),
                codename: String::new(),
                process_nm: 0,
                year: 0,
                vendor,
                arch: if cfg!(target_arch = "x86_64") {
                    "x86_64".into()
                } else if cfg!(target_arch = "aarch64") {
                    "aarch64".into()
                } else {
                    "unknown".into()
                },
                hybrid: false,
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

        Microarchitecture {
            name: uarch.into(),
            codename: codename.into(),
            process_nm: process,
            year,
            vendor: CpuVendor::Intel,
            arch: "x86_64".into(),
            hybrid,
            p_core_uarch: p_core.map(String::from),
            e_core_uarch: e_core.map(String::from),
        }
    }

    fn identify_amd(name: &str, _family: Option<u32>, _model: Option<u32>) -> Microarchitecture {
        let (uarch, codename, process, year) = if name.contains("9950")
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
        } else if name.contains("EPYC 9") || name.contains("GENOA") {
            ("Zen 4", "Genoa", 5, 2022)
        } else if name.contains("EPYC 8") || name.contains("SIENA") {
            ("Zen 4c", "Siena", 5, 2023)
        } else if name.contains("EPYC 7")
            && (name.contains("73") || name.contains("75") || name.contains("79"))
        {
            ("Zen 3", "Milan", 7, 2021)
        } else if name.contains("TURIN") {
            ("Zen 5", "Turin", 4, 2024)
        } else if name.contains("THREADRIPPER 7") || name.contains("PRO 7") {
            ("Zen 4", "Storm Peak", 5, 2023)
        } else if name.contains("THREADRIPPER 5") || name.contains("PRO 5") {
            ("Zen 3", "Chagall", 7, 2022)
        } else {
            ("Unknown AMD", "", 0, 0)
        };

        Microarchitecture {
            name: uarch.into(),
            codename: codename.into(),
            process_nm: process,
            year,
            vendor: CpuVendor::AMD,
            arch: "x86_64".into(),
            hybrid: false,
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

        Microarchitecture {
            name: uarch.into(),
            codename: codename.into(),
            process_nm: process,
            year,
            vendor: CpuVendor::Apple,
            arch: "aarch64".into(),
            hybrid,
            p_core_uarch: Some(uarch.split('/').next().unwrap_or("P-core").into()),
            e_core_uarch: Some(uarch.split('/').nth(1).unwrap_or("E-core").into()),
        }
    }

    /// Detect ISA extensions from CPU flags.
    fn detect_extensions(flags: &[String], microarch: &Microarchitecture) -> Vec<IsaExtension> {
        let flag_set: std::collections::HashSet<String> =
            flags.iter().map(|f| f.to_lowercase()).collect();

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
                    supported: flag_set.contains(flag),
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
                    supported: flag_set.contains(flag),
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
        let (base_ipc, st_score, tdp, boost) = match microarch.name.as_str() {
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
        if microarch.hybrid {
            notes.push("Hybrid architecture: scheduler-aware threading recommended".into());
        }
        if microarch.process_nm > 0 && microarch.process_nm <= 5 {
            notes.push(format!("Leading-edge {}nm process", microarch.process_nm));
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

        let mut model_name = String::new();
        // `None` until /proc/cpuinfo supplies them. Zero was both the
        // "not seen yet" marker and a publishable value.
        let mut family: Option<u32> = None;
        let mut model: Option<u32> = None;
        let mut stepping: Option<u32> = None;
        let mut flags = Vec::new();
        let mut siblings = 0u32;
        let mut cores = 0u32;

        for line in cpuinfo.lines() {
            if let Some((key, val)) = line.split_once(':') {
                let key = key.trim();
                let val = val.trim();
                match key {
                    "model name" if model_name.is_empty() => model_name = val.into(),
                    "cpu family" if family.is_none() => {
                        family = val.parse().ok();
                    }
                    "model" if model.is_none() => {
                        model = val.parse().ok();
                    }
                    "stepping" if stepping.is_none() => {
                        stepping = val.parse().ok();
                    }
                    "flags" if flags.is_empty() => {
                        flags = val.split_whitespace().map(String::from).collect();
                    }
                    "siblings" if siblings == 0 => {
                        siblings = val.parse().unwrap_or(0);
                    }
                    "cpu cores" if cores == 0 => {
                        cores = val.parse().unwrap_or(0);
                    }
                    _ => {}
                }
            }
        }

        if siblings == 0 {
            siblings = cores;
        }

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
        let val: serde_json::Value = serde_json::from_str(text.trim()).unwrap_or_default();

        // `Win32_Processor.Name` is space-padded to a fixed width; the same
        // padding reached hardware_ai before it was trimmed there.
        let model_name = val["Name"]
            .as_str()
            .unwrap_or("Unknown CPU")
            .trim()
            .to_string();
        // `0`, not `1`. The resolver decides whether a core count was read by
        // testing `physical_cores > 0` and reports "the platform reported no
        // physical core count" when it is zero -- so a fallback of 1 does not
        // merely guess low, it **defeats the guard**: `cpu.cores.physical`,
        // `cpu.microarch.physical_cores` and `cpu.microarch.logical_cores` all
        // publish 1 as a measurement, and `smt_enabled` publishes false.
        //
        // Same shape as the cache line size that sat at a sentinel of 64 and
        // slipped past the guard written to catch it. The Linux arm has always
        // used 0 here, and so does `Default`.
        let cores = val["NumberOfCores"].as_u64().unwrap_or(0) as u32;
        let threads = val["NumberOfLogicalProcessors"]
            .as_u64()
            .unwrap_or(cores as u64) as u32;

        // WMI does expose the CPUID triple, and the comment that used to sit
        // here said it does not. `Win32_Processor.Description` is the string
        // "AMD64 Family 26 Model 68 Stepping 0", and `PROCESSOR_IDENTIFIER`
        // carries the same text -- which this file was already reading, two
        // functions down, for something else.
        //
        // Description first because it comes from the query already running;
        // the environment variable is the fallback, and is what a process
        // launched under an unusual environment might be missing.
        let (family, model, stepping) = val["Description"]
            .as_str()
            .and_then(parse_cpuid_triple)
            .or_else(|| {
                std::env::var("PROCESSOR_IDENTIFIER")
                    .ok()
                    .as_deref()
                    .and_then(parse_cpuid_triple)
            })
            .map_or((None, None, None), |(f, m, s)| (Some(f), Some(m), Some(s)));

        let flags = Self::read_windows_cpu_features();

        Ok((model_name, family, model, stepping, flags, cores, threads))
    }

    #[cfg(target_os = "windows")]
    fn read_windows_cpu_features() -> Vec<String> {
        // NOTE: the flags below are asserted from the architecture rather than
        // read from CPUID, which is why the ontology publishes them with
        // `specification` provenance rather than `measured`. Every x86_64 part
        // has them; that is a fact about the instruction set, not about this
        // chip.
        // Detect features from environment or registry
        // This is a simplified version - real implementation would use CPUID
        let mut flags = Vec::new();

        // Try PROCESSOR_IDENTIFIER env var
        if let Ok(id) = std::env::var("PROCESSOR_IDENTIFIER") {
            let upper = id.to_uppercase();
            if upper.contains("INTEL") || upper.contains("AMD") {
                // Modern x86_64 CPUs all have these
                flags.extend(
                    [
                        "sse", "sse2", "sse3", "ssse3", "sse4_1", "sse4_2", "popcnt", "cx16",
                    ]
                    .iter()
                    .map(|s| s.to_string()),
                );
            }
        }

        flags
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

        let features_str = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.features"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

        let flags: Vec<String> = features_str
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .collect();

        // `0` for the same reason as the Windows arm above: the resolver reads
        // zero as "not reported" and says so, and a fallback of 1 publishes a
        // single-core machine instead.
        let cores = std::process::Command::new("sysctl")
            .args(["-n", "hw.physicalcpu"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
            .unwrap_or(0);

        let threads = std::process::Command::new("sysctl")
            .args(["-n", "hw.logicalcpu"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
            .unwrap_or(cores);

        Ok((brand, family, model, stepping, flags, cores, threads))
    }

    /// A platform with no reader here reports nothing, including no core count.
    ///
    /// This returned `1, 1` -- one physical core and one logical -- which the
    /// resolver publishes as a measurement, because its "was this read" test is
    /// `> 0`. Every other field in the tuple was already empty or zero for
    /// exactly that reason; the two counts were the exception.
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn read_cpu_info() -> Result<CpuInfoFields, IronError> {
        Ok((String::new(), 0, 0, 0, Vec::new(), 0, 0))
    }
}

impl Default for CpuMicroarchMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            report: CpuMicroarchReport {
                model_name: String::new(),
                family: None,
                model: None,
                stepping: None,
                microarch: Microarchitecture {
                    name: "Unknown".into(),
                    codename: String::new(),
                    process_nm: 0,
                    year: 0,
                    vendor: CpuVendor::Unknown,
                    arch: String::new(),
                    hybrid: false,
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
                physical_cores: 0,
                logical_cores: 0,
                smt_enabled: false,
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
        assert_eq!(uarch.name, "Raptor Lake");
        assert_eq!(uarch.vendor, CpuVendor::Intel);
        assert!(uarch.hybrid);
        assert_eq!(uarch.process_nm, 7);
    }

    #[test]
    fn test_amd_zen4() {
        let uarch = CpuMicroarchMonitor::identify_microarch(
            "AMD Ryzen 9 7950X 16-Core Processor",
            Some(25),
            Some(97),
            Some(2),
        );
        assert_eq!(uarch.name, "Zen 4");
        assert_eq!(uarch.vendor, CpuVendor::AMD);
        assert!(!uarch.hybrid);
        assert_eq!(uarch.process_nm, 5);
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
        assert_eq!(
            report.model_name.trim(),
            report.model_name,
            "the model name kept its WMI padding: {:?}",
            report.model_name
        );
    }

    #[test]
    fn test_apple_m3() {
        // Apple reports no x86 CPUID triple; identification is by name.
        let uarch = CpuMicroarchMonitor::identify_microarch("Apple M3 Pro", None, None, None);
        assert_eq!(uarch.name, "Ibiza");
        assert_eq!(uarch.vendor, CpuVendor::Apple);
        assert!(uarch.hybrid);
        assert_eq!(uarch.process_nm, 3);
    }

    #[test]
    fn test_performance_inference() {
        let uarch = Microarchitecture {
            name: "Zen 4".into(),
            codename: "Raphael".into(),
            process_nm: 5,
            year: 2022,
            vendor: CpuVendor::AMD,
            arch: "x86_64".into(),
            hybrid: false,
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
            name: "Zen 4".into(),
            codename: "".into(),
            process_nm: 5,
            year: 2022,
            vendor: CpuVendor::AMD,
            arch: "x86_64".into(),
            hybrid: false,
            p_core_uarch: None,
            e_core_uarch: None,
        };
        let exts = CpuMicroarchMonitor::detect_extensions(&flags, &uarch);
        let avx2 = exts.iter().find(|e| e.name == "AVX2").unwrap();
        assert!(avx2.supported);
        let avx512 = exts.iter().find(|e| e.name == "AVX-512F").unwrap();
        assert!(avx512.supported);
    }

    #[test]
    fn test_monitor_default() {
        let monitor = CpuMicroarchMonitor::default();
        let _report = monitor.report();
    }

    #[test]
    fn test_serialization() {
        let uarch = Microarchitecture {
            name: "Zen 4".into(),
            codename: "Raphael".into(),
            process_nm: 5,
            year: 2022,
            vendor: CpuVendor::AMD,
            arch: "x86_64".into(),
            hybrid: false,
            p_core_uarch: None,
            e_core_uarch: None,
        };
        let json = serde_json::to_string(&uarch).unwrap();
        assert!(json.contains("Zen 4"));
        let _: Microarchitecture = serde_json::from_str(&json).unwrap();
    }
}
