//! Cryptographic hardware acceleration detection and monitoring.
//!
//! Detects CPU-accelerated cryptographic instructions (AES-NI, SHA, etc.),
//! hardware random number generators, and TPM crypto capabilities.
//!
//! ## Platform Support
//!
//! - **Linux**: `/proc/cpuinfo` flags, `/dev/hwrng`, `/sys/class/tpm/`
//! - **Windows (x86, x86_64)**: CPUID through `std::arch::is_x86_feature_detected!`
//! - **macOS**: `sysctl hw.optional.*`, Secure Enclave detection

use serde::{Deserialize, Serialize};

use crate::error::IronError;

/// Flag spellings the SHA-1 / SHA-256 extension reports under: `sha_ni` and
/// `sha` on x86, `sha1` and `sha2` on ARMv8.
const SHA_FLAGS: [&str; 4] = ["sha_ni", "sha", "sha2", "sha1"];

/// Category of cryptographic acceleration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CryptoCategory {
    /// Symmetric encryption (AES, etc.).
    SymmetricEncryption,
    /// Hashing (SHA, etc.).
    Hashing,
    /// Carry-less multiplication (GCM, CRC).
    CarrylessMultiply,
    /// Random number generation.
    RandomNumberGen,
    /// Public key / asymmetric operations.
    AsymmetricCrypto,
    /// Key management / secure enclave.
    KeyManagement,
    /// Vector/SIMD crypto operations.
    VectorCrypto,
}

impl std::fmt::Display for CryptoCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SymmetricEncryption => write!(f, "Symmetric Encryption"),
            Self::Hashing => write!(f, "Hashing"),
            Self::CarrylessMultiply => write!(f, "Carry-less Multiply"),
            Self::RandomNumberGen => write!(f, "RNG"),
            Self::AsymmetricCrypto => write!(f, "Asymmetric Crypto"),
            Self::KeyManagement => write!(f, "Key Management"),
            Self::VectorCrypto => write!(f, "Vector Crypto"),
        }
    }
}

/// A detected cryptographic acceleration feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoFeature {
    /// Feature name (e.g. "AES-NI", "SHA Extensions").
    pub name: String,
    /// CPU flag or identifier, spelled as the platform reports it.
    pub cpu_flag: String,
    /// Category.
    pub category: CryptoCategory,
    /// Whether hardware-accelerated.
    pub hardware_accelerated: bool,
    // There is no per-primitive throughput field. The constants this struct
    // used to carry (AES 10 GB/s, VAES 40 GB/s and so on) were not measured on
    // this processor, and the ontology published them as
    // `cpu.crypto.feature.N.throughput`. They are gone; that id now reports
    // unavailable.
    /// Description.
    pub description: String,
}

/// Hardware RNG source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareRng {
    /// RNG name.
    pub name: String,
    /// Whether available.
    pub available: bool,
    /// Quality/entropy bits per sample.
    pub quality: Option<u32>,
    /// Source type.
    pub source_type: RngSource,
}

/// RNG source type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RngSource {
    /// CPU instruction (RDRAND/RDSEED).
    CpuInstruction,
    /// Hardware RNG device.
    HardwareDevice,
    /// TPM RNG.
    Tpm,
    /// Secure Enclave.
    SecureEnclave,
    Unknown,
}

/// TPM cryptographic capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpmCrypto {
    /// TPM version from `tpm_version_major` (and `tpm_version_minor` when that
    /// is present), or `None` when sysfs did not give a major version.
    pub version: Option<String>,
    /// Supported algorithms. Always `None`: sysfs does not list them, and the
    /// list used to be a fixed set (RSA-2048, ECC-P256, AES-128, SHA-256) that
    /// was never read from the chip.
    pub algorithms: Option<Vec<String>>,
    /// RSA support. Always `None`: not exposed by sysfs. It used to be `true`
    /// for any TPM.
    pub rsa: Option<bool>,
    /// ECC support. Always `None`, for the same reason as `rsa`.
    pub ecc: Option<bool>,
    /// AES support. Always `None`, for the same reason as `rsa`.
    pub aes: Option<bool>,
    /// SHA-256 support. Always `None`, for the same reason as `rsa`.
    pub sha256: Option<bool>,
}

/// Crypto acceleration summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoAccelReport {
    /// Detected CPU crypto features.
    pub features: Vec<CryptoFeature>,
    /// Hardware RNG sources.
    pub rng_sources: Vec<HardwareRng>,
    /// TPM crypto (if present).
    pub tpm_crypto: Option<TpmCrypto>,
    /// Overall crypto acceleration score (0-100).
    pub acceleration_score: u32,
    /// Categories with hardware acceleration.
    pub accelerated_categories: Vec<CryptoCategory>,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// Crypto acceleration monitor.
pub struct CryptoAccelMonitor {
    report: CryptoAccelReport,
}

impl CryptoAccelMonitor {
    /// Create a new crypto acceleration monitor.
    pub fn new() -> Result<Self, IronError> {
        let report = Self::detect()?;
        Ok(Self { report })
    }

    /// Refresh detection.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.report = Self::detect()?;
        Ok(())
    }

    /// Get the full report.
    pub fn report(&self) -> &CryptoAccelReport {
        &self.report
    }

    /// Check if a specific feature is available.
    pub fn has_feature(&self, flag: &str) -> bool {
        self.report
            .features
            .iter()
            .any(|f| f.cpu_flag.eq_ignore_ascii_case(flag) && f.hardware_accelerated)
    }

    fn detect() -> Result<CryptoAccelReport, IronError> {
        let cpu_flags = Self::read_cpu_flags()?;
        let features = Self::detect_features(&cpu_flags);
        let rng_sources = Self::detect_rng(&features);
        let tpm_crypto = Self::detect_tpm();

        let accelerated_categories: Vec<CryptoCategory> = features
            .iter()
            .filter(|f| f.hardware_accelerated)
            .map(|f| f.category)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let acceleration_score = Self::compute_score(&features, &rng_sources, &tpm_crypto);
        let recommendations = Self::generate_recommendations(&features, &rng_sources);

        Ok(CryptoAccelReport {
            features,
            rng_sources,
            tpm_crypto,
            acceleration_score,
            accelerated_categories,
            recommendations,
        })
    }

    // One push per primitive reads better as a list than a single `vec!` of
    // struct literals, and the ARM and Key Locker rows are conditional.
    #[allow(clippy::vec_init_then_push)]
    fn detect_features(flags: &[String]) -> Vec<CryptoFeature> {
        let flag_set: std::collections::HashSet<&str> = flags.iter().map(|s| s.as_str()).collect();
        let mut features = Vec::new();

        // The AES feature is named for the instruction set the machine has.
        // "AES-NI" is the x86 name; on ARM the same `aes` flag is the ARMv8
        // Cryptographic Extension. The ARM names are read from the flags, not
        // the build target, so the answer follows the machine that was probed.
        // `sha1`, `sha2`, `pmull` and `rndr` are ARM-only spellings; the macOS
        // probe reports no `asimd` flag, so it is needed too.
        let arm = ["asimd", "sha1", "sha2", "pmull", "rndr"]
            .iter()
            .any(|f| flag_set.contains(f));
        let aes_name = if arm { "AES" } else { "AES-NI" };

        // AES-NI (x86) or AES (ARMv8)
        features.push(CryptoFeature {
            name: aes_name.into(),
            cpu_flag: "aes".into(),
            category: CryptoCategory::SymmetricEncryption,
            hardware_accelerated: flag_set.contains("aes"),
            description: "AES encryption/decryption in hardware".into(),
        });

        // VAES (Vector AES)
        features.push(CryptoFeature {
            name: "VAES".into(),
            cpu_flag: "vaes".into(),
            category: CryptoCategory::VectorCrypto,
            hardware_accelerated: flag_set.contains("vaes"),
            description: "Vectorized AES for parallel encryption (AVX-512)".into(),
        });

        // PCLMULQDQ (carry-less multiply)
        features.push(CryptoFeature {
            name: "PCLMULQDQ".into(),
            cpu_flag: "pclmulqdq".into(),
            category: CryptoCategory::CarrylessMultiply,
            hardware_accelerated: flag_set.contains("pclmulqdq"),
            description: "Carry-less multiplication for GCM and CRC".into(),
        });

        // VPCLMULQDQ (Vector PCLMULQDQ)
        features.push(CryptoFeature {
            name: "VPCLMULQDQ".into(),
            cpu_flag: "vpclmulqdq".into(),
            category: CryptoCategory::VectorCrypto,
            hardware_accelerated: flag_set.contains("vpclmulqdq"),
            description: "Vectorized carry-less multiply for parallel GCM".into(),
        });

        // SHA extensions. The entry carries whichever spelling the platform
        // reported, so an ARM machine is never named with the x86 `sha_ni`.
        let sha_flag = SHA_FLAGS.iter().copied().find(|f| flag_set.contains(f));
        features.push(CryptoFeature {
            name: "SHA Extensions".into(),
            cpu_flag: sha_flag.unwrap_or("sha_ni").into(),
            category: CryptoCategory::Hashing,
            hardware_accelerated: sha_flag.is_some(),
            description: "SHA-1 and SHA-256 hardware acceleration".into(),
        });

        // SHA-512 (AVX-512 based)
        let has_sha512 = flag_set.contains("sha512") || flag_set.contains("avx512sha");
        features.push(CryptoFeature {
            name: "SHA-512 Extensions".into(),
            cpu_flag: "sha512".into(),
            category: CryptoCategory::Hashing,
            hardware_accelerated: has_sha512,
            description: "SHA-512 hardware acceleration".into(),
        });

        // SM3 (Chinese hash standard)
        features.push(CryptoFeature {
            name: "SM3".into(),
            cpu_flag: "sm3".into(),
            category: CryptoCategory::Hashing,
            hardware_accelerated: flag_set.contains("sm3"),
            description: "SM3 hash function (Chinese national standard)".into(),
        });

        // SM4 (Chinese block cipher)
        features.push(CryptoFeature {
            name: "SM4".into(),
            cpu_flag: "sm4".into(),
            category: CryptoCategory::SymmetricEncryption,
            hardware_accelerated: flag_set.contains("sm4"),
            description: "SM4 block cipher (Chinese national standard)".into(),
        });

        // RDRAND
        features.push(CryptoFeature {
            name: "RDRAND".into(),
            cpu_flag: "rdrand".into(),
            category: CryptoCategory::RandomNumberGen,
            hardware_accelerated: flag_set.contains("rdrand"),
            description: "Hardware random number generator instruction".into(),
        });

        // RDSEED
        features.push(CryptoFeature {
            name: "RDSEED".into(),
            cpu_flag: "rdseed".into(),
            category: CryptoCategory::RandomNumberGen,
            hardware_accelerated: flag_set.contains("rdseed"),
            description: "True random seed for seeding PRNGs".into(),
        });

        // RNDR (AArch64, FEAT_RNG): the ARM counterpart of RDRAND. It has its
        // own flag rather than sharing `rdrand`, which would publish an x86
        // instruction name on an ARM machine.
        features.push(CryptoFeature {
            name: "RNDR".into(),
            cpu_flag: "rndr".into(),
            category: CryptoCategory::RandomNumberGen,
            hardware_accelerated: flag_set.contains("rndr"),
            description: "AArch64 random number instruction (FEAT_RNG)".into(),
        });

        // AArch64 crypto extensions
        if flag_set.contains("aes") && flag_set.contains("pmull") {
            features.push(CryptoFeature {
                name: "ARMv8 Crypto".into(),
                cpu_flag: "pmull".into(),
                category: CryptoCategory::SymmetricEncryption,
                hardware_accelerated: true,
                description: "ARMv8 Cryptographic Extensions".into(),
            });
        }

        // Intel Key Locker
        let has_kl = flag_set.contains("keylocker") || flag_set.contains("kl");
        features.push(CryptoFeature {
            name: "Key Locker".into(),
            cpu_flag: "keylocker".into(),
            category: CryptoCategory::KeyManagement,
            hardware_accelerated: has_kl,
            description: "Intel Key Locker for AES key protection".into(),
        });

        features
    }

    /// The hardware random sources this machine has, from the two places that
    /// know: the instruction set, and the Linux device tree.
    ///
    /// The instruction half is new, and it closes a contradiction the ontology
    /// was publishing about itself. This function consulted `/dev/hwrng` and
    /// `/sys/class/misc/hw_random` and nothing else, so on Windows and macOS it
    /// returned an empty list -- and `cpu.crypto.rng.<none>` said "the hardware
    /// random source probe enumerated nothing, but the feature list above
    /// reports a random number instruction". Two rows of the same snapshot, one
    /// naming RDRAND and RDSEED and the other reporting no random source, with
    /// the reason left to apologise for the difference.
    ///
    /// `RngSource::CpuInstruction` was declared for exactly this and produced by
    /// nothing. The features were already detected from CPUID by the caller;
    /// they are passed in rather than re-detected, so the two answers cannot
    /// disagree.
    ///
    /// `quality` stays `None`: RDRAND declares no entropy-bits-per-sample figure
    /// anywhere readable, and the entity's absence reason says that assuming one
    /// would be worse than saying nothing.
    fn detect_rng(features: &[CryptoFeature]) -> Vec<HardwareRng> {
        // `mut` for the Linux block below, which is the only one that pushes
        // after this; on every other platform the instruction sources are all
        // there is.
        #[allow(unused_mut)]
        let mut sources: Vec<HardwareRng> = features
            .iter()
            .filter(|f| f.hardware_accelerated && f.category == CryptoCategory::RandomNumberGen)
            .map(|f| HardwareRng {
                name: f.name.clone(),
                available: true,
                quality: None,
                source_type: RngSource::CpuInstruction,
            })
            .collect();

        #[cfg(target_os = "linux")]
        {
            // Check /dev/hwrng
            if std::path::Path::new("/dev/hwrng").exists() {
                sources.push(HardwareRng {
                    name: "Hardware RNG".into(),
                    available: true,
                    quality: None,
                    source_type: RngSource::HardwareDevice,
                });
            }

            // Check /sys/class/misc/hw_random/rng_available
            if let Ok(avail) = std::fs::read_to_string("/sys/class/misc/hw_random/rng_available") {
                for rng_name in avail.split_whitespace() {
                    if rng_name == "tpm-rng" || rng_name.contains("tpm") {
                        sources.push(HardwareRng {
                            name: rng_name.to_string(),
                            available: true,
                            // No entropy figure is readable for the TPM source.
                            // This used to report 1000, a constant that the
                            // ontology published as a bits-per-sample value.
                            quality: None,
                            source_type: RngSource::Tpm,
                        });
                    }
                }
            }
        }

        sources
    }

    fn detect_tpm() -> Option<TpmCrypto> {
        #[cfg(target_os = "linux")]
        {
            let tpm_path = std::path::Path::new("/sys/class/tpm/tpm0");
            if tpm_path.exists() {
                let read = |file: &str| std::fs::read_to_string(tpm_path.join(file)).ok();
                let version = tpm_version_from_sysfs(
                    read("tpm_version_major").as_deref(),
                    read("tpm_version_minor").as_deref(),
                );

                // The algorithm flags are not exposed by sysfs. They used to be
                // hard-coded true for every TPM, which named algorithms the chip
                // was never asked about.
                return Some(TpmCrypto {
                    version,
                    algorithms: None,
                    rsa: None,
                    ecc: None,
                    aes: None,
                    sha256: None,
                });
            }
        }
        None
    }

    fn compute_score(
        features: &[CryptoFeature],
        rng: &[HardwareRng],
        tpm: &Option<TpmCrypto>,
    ) -> u32 {
        let mut score = 0u32;

        // AES-NI: 25 points
        if features
            .iter()
            .any(|f| f.cpu_flag == "aes" && f.hardware_accelerated)
        {
            score += 25;
        }

        // SHA: 15 points
        if features
            .iter()
            .any(|f| SHA_FLAGS.contains(&f.cpu_flag.as_str()) && f.hardware_accelerated)
        {
            score += 15;
        }

        // PCLMULQDQ: 10 points
        if features
            .iter()
            .any(|f| f.cpu_flag == "pclmulqdq" && f.hardware_accelerated)
        {
            score += 10;
        }

        // VAES: 10 points
        if features
            .iter()
            .any(|f| f.cpu_flag == "vaes" && f.hardware_accelerated)
        {
            score += 10;
        }

        // RDRAND (x86) or RNDR (AArch64): 10 points
        if features
            .iter()
            .any(|f| (f.cpu_flag == "rdrand" || f.cpu_flag == "rndr") && f.hardware_accelerated)
        {
            score += 10;
        }

        // RDSEED: 5 points
        if features
            .iter()
            .any(|f| f.cpu_flag == "rdseed" && f.hardware_accelerated)
        {
            score += 5;
        }

        // HW RNG: 10 points
        if !rng.is_empty() {
            score += 10;
        }

        // TPM: 15 points
        if tpm.is_some() {
            score += 15;
        }

        score.min(100)
    }

    fn generate_recommendations(features: &[CryptoFeature], _rng: &[HardwareRng]) -> Vec<String> {
        let mut recs = Vec::new();

        if !features
            .iter()
            .any(|f| f.cpu_flag == "aes" && f.hardware_accelerated)
        {
            recs.push(
                "No hardware AES detected; encryption workloads will be significantly slower"
                    .into(),
            );
        }

        if features
            .iter()
            .any(|f| f.cpu_flag == "vaes" && f.hardware_accelerated)
        {
            recs.push(
                "VAES available; use AES-GCM with libraries that support vectorized encryption"
                    .into(),
            );
        }

        if !features
            .iter()
            .any(|f| SHA_FLAGS.contains(&f.cpu_flag.as_str()) && f.hardware_accelerated)
        {
            recs.push("No SHA extensions; consider BLAKE3 for faster software hashing".into());
        }

        recs
    }

    /// The feature list from `/proc/cpuinfo`: `flags` on x86, `Features` on ARM.
    ///
    /// An error when neither line is present. An empty list would be read as
    /// "the processor has no accelerated primitive", which is a claim about the
    /// hardware made from a file that was not read.
    ///
    /// On ARM the kernel's `rng` token is the ARMv8.5 RNDR instruction
    /// (FEAT_RNG). It is published as `rndr`, the name the macOS probe uses for
    /// the same feature. `rng` used to be dropped, so an aarch64 Linux machine
    /// with RNDR reported no random number instruction.
    #[cfg(any(target_os = "linux", test))]
    fn parse_cpuinfo_flags(cpuinfo: &str) -> Result<Vec<String>, IronError> {
        for line in cpuinfo.lines() {
            if line.starts_with("flags") {
                if let Some((_, flags_str)) = line.split_once(':') {
                    return Ok(flags_str.split_whitespace().map(String::from).collect());
                }
            }
            if line.starts_with("Features") {
                if let Some((_, flags_str)) = line.split_once(':') {
                    let mut flags: Vec<String> =
                        flags_str.split_whitespace().map(String::from).collect();
                    if flags.iter().any(|f| f == "rng") {
                        flags.push("rndr".into());
                    }
                    return Ok(flags);
                }
            }
        }
        Err(IronError::Parse(
            "/proc/cpuinfo has no flags or Features line".into(),
        ))
    }

    #[cfg(target_os = "linux")]
    fn read_cpu_flags() -> Result<Vec<String>, IronError> {
        let content = std::fs::read_to_string("/proc/cpuinfo").map_err(IronError::Io)?;
        Self::parse_cpuinfo_flags(&content)
    }

    /// CPUID through `std::arch`, one query per flag. Each entry pairs the flag
    /// name as the Linux `/proc/cpuinfo` path spells it with the feature name
    /// the standard library queries it by, so `detect_features` reads both
    /// platforms the same way. The answers come from the processor, not from
    /// the product name, which the old PowerShell caption probe never read.
    #[cfg(all(
        target_os = "windows",
        any(target_arch = "x86", target_arch = "x86_64")
    ))]
    fn read_cpu_flags() -> Result<Vec<String>, IronError> {
        macro_rules! probe {
            // `tt`, not `literal`: the standard library's macro only accepts its
            // feature name as a bare token.
            ($($flag:literal => $feature:tt),* $(,)?) => {{
                let mut flags = Vec::new();
                $(
                    if std::arch::is_x86_feature_detected!($feature) {
                        flags.push($flag.to_string());
                    }
                )*
                flags
            }};
        }
        Ok(probe! {
            "aes" => "aes",
            "pclmulqdq" => "pclmulqdq",
            "vaes" => "vaes",
            "vpclmulqdq" => "vpclmulqdq",
            "sha_ni" => "sha",
            "rdrand" => "rdrand",
            "rdseed" => "rdseed",
        })
    }

    /// Windows on ARM: the feature registers are not queried here, so nothing
    /// is read. An error, so the crypto rows say so instead of reporting no
    /// acceleration.
    #[cfg(all(
        target_os = "windows",
        not(any(target_arch = "x86", target_arch = "x86_64"))
    ))]
    fn read_cpu_flags() -> Result<Vec<String>, IronError> {
        Err(IronError::UnsupportedPlatform(
            "CPU crypto flags are not read on this Windows architecture; only x86 and x86_64 are probed".into(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn read_cpu_flags() -> Result<Vec<String>, IronError> {
        let mut flags = Vec::new();

        // Flag names are the ones the Linux path reports for the same feature on
        // ARM, so one table of names serves both. The ARM rows name their own
        // feature: FEAT_SHA256 is `sha2`, not the x86 `sha_ni`, and FEAT_RNG is
        // `rndr`, not `rdrand`.
        let checks = &[
            ("hw.optional.aes", "aes"),
            ("hw.optional.arm.FEAT_AES", "aes"),
            ("hw.optional.armv8_crc32", "crc32"),
            ("hw.optional.arm.FEAT_SHA1", "sha1"),
            ("hw.optional.arm.FEAT_SHA256", "sha2"),
            ("hw.optional.arm.FEAT_PMULL", "pmull"),
            ("hw.optional.arm.FEAT_RNG", "rndr"),
        ];

        for (sysctl, flag) in checks {
            // A key this machine does not have makes sysctl exit non-zero: that
            // is an absent feature. Failing to run sysctl at all means the
            // flags were not read, and that is an error.
            let out = std::process::Command::new("sysctl")
                .arg("-n")
                .arg(sysctl)
                .output()
                .map_err(IronError::Io)?;
            if out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "1" {
                flags.push(flag.to_string());
            }
        }

        Ok(flags)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn read_cpu_flags() -> Result<Vec<String>, IronError> {
        Err(IronError::UnsupportedPlatform(
            "CPU crypto flags are not read on this platform".into(),
        ))
    }
}

impl Default for CryptoAccelMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            report: CryptoAccelReport {
                features: Vec::new(),
                rng_sources: Vec::new(),
                tpm_crypto: None,
                acceleration_score: 0,
                accelerated_categories: Vec::new(),
                recommendations: Vec::new(),
            },
        })
    }
}

/// TPM version from the sysfs major and minor numbers. The minor is appended
/// only when it was read. Without a major number there is no version: the
/// `"2.0"` default used to stand in for it.
#[cfg(any(target_os = "linux", test))]
fn tpm_version_from_sysfs(major: Option<&str>, minor: Option<&str>) -> Option<String> {
    let major = major.map(str::trim).filter(|m| !m.is_empty())?;
    Some(match minor.map(str::trim).filter(|m| !m.is_empty()) {
        Some(minor) => format!("{major}.{minor}"),
        None => major.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tpm_version_uses_major_and_minor_when_both_are_read() {
        assert_eq!(
            tpm_version_from_sysfs(Some("2\n"), Some("0\n")).as_deref(),
            Some("2.0")
        );
    }

    #[test]
    fn tpm_version_without_a_minor_is_the_major_alone() {
        assert_eq!(
            tpm_version_from_sysfs(Some("2\n"), None).as_deref(),
            Some("2")
        );
    }

    #[test]
    fn tpm_version_without_a_major_is_unknown_not_two_point_zero() {
        assert_eq!(tpm_version_from_sysfs(None, Some("0\n")), None);
        assert_eq!(tpm_version_from_sysfs(Some("  \n"), None), None);
    }

    #[test]
    fn test_feature_detection_aes() {
        let flags = vec!["aes".into(), "pclmulqdq".into(), "rdrand".into()];
        let features = CryptoAccelMonitor::detect_features(&flags);
        let aes = features.iter().find(|f| f.cpu_flag == "aes").unwrap();
        assert!(aes.hardware_accelerated);
        assert_eq!(aes.category, CryptoCategory::SymmetricEncryption);
    }

    #[test]
    fn test_cpuinfo_x86_flags_line() {
        let cpuinfo = "processor\t: 0\nvendor_id\t: AuthenticAMD\nflags\t\t: fpu aes pclmulqdq rdrand rdseed sha_ni vaes vpclmulqdq\nbugs\t\t: sysret_ss_attrs\n";
        let flags = CryptoAccelMonitor::parse_cpuinfo_flags(cpuinfo).unwrap();
        assert!(flags.contains(&"aes".to_string()));
        assert!(flags.contains(&"sha_ni".to_string()));
        assert!(!flags.contains(&"sysret_ss_attrs".to_string()));
    }

    #[test]
    fn test_cpuinfo_arm_features_line() {
        let cpuinfo = "processor\t: 0\nFeatures\t: fp asimd aes pmull sha1 sha2 crc32\n";
        let flags = CryptoAccelMonitor::parse_cpuinfo_flags(cpuinfo).unwrap();
        assert!(flags.contains(&"pmull".to_string()));
        assert!(flags.contains(&"sha2".to_string()));
    }

    #[test]
    fn aarch64_rng_token_is_the_rndr_instruction() {
        // Captured shape of an aarch64 Linux Features line with FEAT_RNG present.
        let cpuinfo = "processor\t: 0\nBogoMIPS\t: 50.00\nFeatures\t: fp asimd evtstrm aes pmull sha1 sha2 crc32 atomics fphp asimdhp cpuid asimdrdm lrcpc dcpop asimddp ssbs rng\nCPU implementer\t: 0x41\n";
        let flags = CryptoAccelMonitor::parse_cpuinfo_flags(cpuinfo).unwrap();
        assert!(flags.contains(&"rng".to_string()));
        assert!(flags.contains(&"rndr".to_string()));

        let features = CryptoAccelMonitor::detect_features(&flags);
        let rndr = features.iter().find(|f| f.name == "RNDR").unwrap();
        assert!(rndr.hardware_accelerated);
        assert_eq!(rndr.category, CryptoCategory::RandomNumberGen);
    }

    #[test]
    fn aarch64_without_rng_token_reports_no_rndr() {
        // Captured shape of an aarch64 Features line from a core without FEAT_RNG.
        let cpuinfo =
            "processor\t: 0\nFeatures\t: fp asimd evtstrm aes pmull sha1 sha2 crc32 cpuid\n";
        let flags = CryptoAccelMonitor::parse_cpuinfo_flags(cpuinfo).unwrap();
        assert!(!flags.contains(&"rndr".to_string()));
        let features = CryptoAccelMonitor::detect_features(&flags);
        let rndr = features.iter().find(|f| f.name == "RNDR").unwrap();
        assert!(!rndr.hardware_accelerated);
    }

    #[test]
    fn the_aes_feature_is_named_for_the_architecture() {
        let arm: Vec<String> = ["fp", "asimd", "aes"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let arm_features = CryptoAccelMonitor::detect_features(&arm);
        assert!(arm_features
            .iter()
            .any(|f| f.name == "AES" && f.cpu_flag == "aes"));
        assert!(!arm_features.iter().any(|f| f.name == "AES-NI"));

        let x86: Vec<String> = ["aes", "pclmulqdq", "rdrand"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let x86_features = CryptoAccelMonitor::detect_features(&x86);
        assert!(x86_features
            .iter()
            .any(|f| f.name == "AES-NI" && f.cpu_flag == "aes"));
    }

    #[test]
    fn test_cpuinfo_without_flags_line_is_an_error() {
        // An empty list here would read as "no accelerated primitive".
        let cpuinfo = "processor\t: 0\nmodel name\t: Unknown\n";
        assert!(CryptoAccelMonitor::parse_cpuinfo_flags(cpuinfo).is_err());
    }

    #[test]
    fn test_arm_flags_use_arm_names() {
        let flags: Vec<String> = ["aes", "pmull", "sha1", "sha2", "rndr"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let features = CryptoAccelMonitor::detect_features(&flags);

        let rng: Vec<&str> = features
            .iter()
            .filter(|f| f.hardware_accelerated && f.category == CryptoCategory::RandomNumberGen)
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(rng, vec!["RNDR"]);

        let sha = features
            .iter()
            .find(|f| f.name == "SHA Extensions")
            .unwrap();
        assert!(sha.hardware_accelerated);
        assert_eq!(sha.cpu_flag, "sha2");
    }

    #[test]
    fn test_x86_random_names() {
        let flags: Vec<String> = ["rdrand", "rdseed"].iter().map(|s| s.to_string()).collect();
        let features = CryptoAccelMonitor::detect_features(&flags);
        let accelerated = |name: &str| {
            features
                .iter()
                .any(|f| f.name == name && f.hardware_accelerated)
        };
        assert!(accelerated("RDRAND"));
        assert!(accelerated("RDSEED"));
        assert!(!accelerated("RNDR"));
    }

    #[test]
    fn test_scoring() {
        let flags = vec![
            "aes".into(),
            "sha_ni".into(),
            "pclmulqdq".into(),
            "rdrand".into(),
            "rdseed".into(),
        ];
        let features = CryptoAccelMonitor::detect_features(&flags);
        let score = CryptoAccelMonitor::compute_score(&features, &[], &None);
        // AES(25) + SHA(15) + PCLMULQDQ(10) + RDRAND(10) + RDSEED(5) = 65
        assert_eq!(score, 65);
    }

    #[test]
    fn test_category_display() {
        assert_eq!(
            CryptoCategory::SymmetricEncryption.to_string(),
            "Symmetric Encryption"
        );
        assert_eq!(CryptoCategory::RandomNumberGen.to_string(), "RNG");
    }

    #[test]
    fn test_monitor_default() {
        let monitor = CryptoAccelMonitor::default();
        let _report = monitor.report();
    }

    #[test]
    fn test_serialization() {
        let feature = CryptoFeature {
            name: "AES-NI".into(),
            cpu_flag: "aes".into(),
            category: CryptoCategory::SymmetricEncryption,
            hardware_accelerated: true,
            description: "AES".into(),
        };
        let json = serde_json::to_string(&feature).unwrap();
        assert!(json.contains("AES-NI"));
        let _: CryptoFeature = serde_json::from_str(&json).unwrap();
    }
}
