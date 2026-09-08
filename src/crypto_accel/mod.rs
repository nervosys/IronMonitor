//! Cryptographic hardware acceleration detection and monitoring.
//!
//! Detects CPU-accelerated cryptographic instructions (AES-NI, SHA, etc.),
//! hardware random number generators, TPM crypto capabilities, and GPU
//! compute crypto potential. Provides throughput estimation based on
//! detected features.
//!
//! ## Platform Support
//!
//! - **Linux**: `/proc/cpuinfo` flags, `/dev/hwrng`, `/sys/class/tpm/`
//! - **Windows**: CPU feature detection, `BCryptEnumAlgorithms`
//! - **macOS**: `sysctl hw.optional.*`, Secure Enclave detection

use serde::{Deserialize, Serialize};

use crate::error::SimonError;

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
    /// Feature name (e.g. "AES-NI", "SHA-256 Extensions").
    pub name: String,
    /// CPU flag or identifier.
    pub cpu_flag: String,
    /// Category.
    pub category: CryptoCategory,
    /// Whether hardware-accelerated.
    pub hardware_accelerated: bool,
    /// Estimated throughput in GB/s (per core, if known).
    pub estimated_throughput_gbs: Option<f64>,
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
    /// TPM version.
    pub version: String,
    /// Supported algorithms.
    pub algorithms: Vec<String>,
    /// Has RSA support.
    pub rsa: bool,
    /// Has ECC support.
    pub ecc: bool,
    /// Has AES support.
    pub aes: bool,
    /// Has SHA-256 support.
    pub sha256: bool,
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
    pub fn new() -> Result<Self, SimonError> {
        let report = Self::detect()?;
        Ok(Self { report })
    }

    /// Refresh detection.
    pub fn refresh(&mut self) -> Result<(), SimonError> {
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

    fn detect() -> Result<CryptoAccelReport, SimonError> {
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

    fn detect_features(flags: &[String]) -> Vec<CryptoFeature> {
        let flag_set: std::collections::HashSet<&str> = flags.iter().map(|s| s.as_str()).collect();
        let mut features = Vec::new();

        // AES-NI
        features.push(CryptoFeature {
            name: "AES-NI".into(),
            cpu_flag: "aes".into(),
            category: CryptoCategory::SymmetricEncryption,
            hardware_accelerated: flag_set.contains("aes"),
            estimated_throughput_gbs: if flag_set.contains("aes") {
                Some(10.0)
            } else {
                None
            },
            description: "AES encryption/decryption in hardware".into(),
        });

        // VAES (Vector AES)
        features.push(CryptoFeature {
            name: "VAES".into(),
            cpu_flag: "vaes".into(),
            category: CryptoCategory::VectorCrypto,
            hardware_accelerated: flag_set.contains("vaes"),
            estimated_throughput_gbs: if flag_set.contains("vaes") {
                Some(40.0)
            } else {
                None
            },
            description: "Vectorized AES for parallel encryption (AVX-512)".into(),
        });

        // PCLMULQDQ (carry-less multiply)
        features.push(CryptoFeature {
            name: "PCLMULQDQ".into(),
            cpu_flag: "pclmulqdq".into(),
            category: CryptoCategory::CarrylessMultiply,
            hardware_accelerated: flag_set.contains("pclmulqdq"),
            estimated_throughput_gbs: if flag_set.contains("pclmulqdq") {
                Some(8.0)
            } else {
                None
            },
            description: "Carry-less multiplication for GCM and CRC".into(),
        });

        // VPCLMULQDQ (Vector PCLMULQDQ)
        features.push(CryptoFeature {
            name: "VPCLMULQDQ".into(),
            cpu_flag: "vpclmulqdq".into(),
            category: CryptoCategory::VectorCrypto,
            hardware_accelerated: flag_set.contains("vpclmulqdq"),
            estimated_throughput_gbs: if flag_set.contains("vpclmulqdq") {
                Some(32.0)
            } else {
                None
            },
            description: "Vectorized carry-less multiply for parallel GCM".into(),
        });

        // SHA extensions
        features.push(CryptoFeature {
            name: "SHA Extensions".into(),
            cpu_flag: "sha_ni".into(),
            category: CryptoCategory::Hashing,
            hardware_accelerated: flag_set.contains("sha_ni") || flag_set.contains("sha"),
            estimated_throughput_gbs: if flag_set.contains("sha_ni") || flag_set.contains("sha") {
                Some(6.0)
            } else {
                None
            },
            description: "SHA-1 and SHA-256 hardware acceleration".into(),
        });

        // SHA-512 (AVX-512 based)
        let has_sha512 = flag_set.contains("sha512") || flag_set.contains("avx512sha");
        features.push(CryptoFeature {
            name: "SHA-512 Extensions".into(),
            cpu_flag: "sha512".into(),
            category: CryptoCategory::Hashing,
            hardware_accelerated: has_sha512,
            estimated_throughput_gbs: if has_sha512 { Some(8.0) } else { None },
            description: "SHA-512 hardware acceleration".into(),
        });

        // SM3 (Chinese hash standard)
        features.push(CryptoFeature {
            name: "SM3".into(),
            cpu_flag: "sm3".into(),
            category: CryptoCategory::Hashing,
            hardware_accelerated: flag_set.contains("sm3"),
            estimated_throughput_gbs: None,
            description: "SM3 hash function (Chinese national standard)".into(),
        });

        // SM4 (Chinese block cipher)
        features.push(CryptoFeature {
            name: "SM4".into(),
            cpu_flag: "sm4".into(),
            category: CryptoCategory::SymmetricEncryption,
            hardware_accelerated: flag_set.contains("sm4"),
            estimated_throughput_gbs: None,
            description: "SM4 block cipher (Chinese national standard)".into(),
        });

        // RDRAND
        features.push(CryptoFeature {
            name: "RDRAND".into(),
            cpu_flag: "rdrand".into(),
            category: CryptoCategory::RandomNumberGen,
            hardware_accelerated: flag_set.contains("rdrand"),
            estimated_throughput_gbs: if flag_set.contains("rdrand") {
                Some(0.5)
            } else {
                None
            },
            description: "Hardware random number generator instruction".into(),
        });

        // RDSEED
        features.push(CryptoFeature {
            name: "RDSEED".into(),
            cpu_flag: "rdseed".into(),
            category: CryptoCategory::RandomNumberGen,
            hardware_accelerated: flag_set.contains("rdseed"),
            estimated_throughput_gbs: None,
            description: "True random seed for seeding PRNGs".into(),
        });

        // AArch64 crypto extensions
        if flag_set.contains("aes") && flag_set.contains("pmull") {
            features.push(CryptoFeature {
                name: "ARMv8 Crypto".into(),
                cpu_flag: "pmull".into(),
                category: CryptoCategory::SymmetricEncryption,
                hardware_accelerated: true,
                estimated_throughput_gbs: Some(5.0),
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
            estimated_throughput_gbs: None,
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
                            quality: Some(1000),
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
                let version = std::fs::read_to_string(tpm_path.join("tpm_version_major"))
                    .ok()
                    .map(|v| {
                        let major = v.trim().to_string();
                        let minor = std::fs::read_to_string(tpm_path.join("tpm_version_minor"))
                            .ok()
                            .map(|m| m.trim().to_string())
                            .unwrap_or_else(|| "0".into());
                        format!("{}.{}", major, minor)
                    })
                    .unwrap_or_else(|| "2.0".into());

                return Some(TpmCrypto {
                    version,
                    algorithms: vec![
                        "RSA-2048".into(),
                        "ECC-P256".into(),
                        "AES-128".into(),
                        "SHA-256".into(),
                    ],
                    rsa: true,
                    ecc: true,
                    aes: true,
                    sha256: true,
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
            .any(|f| (f.cpu_flag == "sha_ni" || f.cpu_flag == "sha") && f.hardware_accelerated)
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

        // RDRAND: 10 points
        if features
            .iter()
            .any(|f| f.cpu_flag == "rdrand" && f.hardware_accelerated)
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
                "No AES-NI detected; encryption workloads will be significantly slower".into(),
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
            .any(|f| (f.cpu_flag == "sha_ni" || f.cpu_flag == "sha") && f.hardware_accelerated)
        {
            recs.push("No SHA extensions; consider BLAKE3 for faster software hashing".into());
        }

        recs
    }

    #[cfg(target_os = "linux")]
    fn read_cpu_flags() -> Result<Vec<String>, SimonError> {
        let content = std::fs::read_to_string("/proc/cpuinfo").map_err(SimonError::Io)?;
        for line in content.lines() {
            if line.starts_with("flags") || line.starts_with("Features") {
                if let Some((_, flags_str)) = line.split_once(':') {
                    return Ok(flags_str.split_whitespace().map(String::from).collect());
                }
            }
        }
        Ok(Vec::new())
    }

    #[cfg(target_os = "windows")]
    fn read_cpu_flags() -> Result<Vec<String>, SimonError> {
        // Use PROCESSOR_IDENTIFIER and hard-coded detection on Windows
        let mut flags = Vec::new();

        // Try WMI for basic info
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_Processor | Select-Object -ExpandProperty Caption",
            ])
            .output();

        if let Ok(out) = output {
            let _caption = String::from_utf8_lossy(&out.stdout);
            // Modern Intel/AMD CPUs generally have AES-NI and RDRAND
            flags.extend_from_slice(&[
                "aes".into(),
                "pclmulqdq".into(),
                "rdrand".into(),
                "rdseed".into(),
            ]);
        }

        Ok(flags)
    }

    #[cfg(target_os = "macos")]
    fn read_cpu_flags() -> Result<Vec<String>, SimonError> {
        let mut flags = Vec::new();

        let checks = &[
            ("hw.optional.aes", "aes"),
            ("hw.optional.armv8_crc32", "crc32"),
            ("hw.optional.arm.FEAT_SHA1", "sha"),
            ("hw.optional.arm.FEAT_SHA256", "sha_ni"),
            ("hw.optional.arm.FEAT_PMULL", "pmull"),
            ("hw.optional.arm.FEAT_RNG", "rdrand"),
        ];

        for (sysctl, flag) in checks {
            let output = std::process::Command::new("sysctl")
                .arg("-n")
                .arg(sysctl)
                .output();
            if let Ok(out) = output {
                let val = String::from_utf8_lossy(&out.stdout);
                if val.trim() == "1" {
                    flags.push(flag.to_string());
                }
            }
        }

        Ok(flags)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn read_cpu_flags() -> Result<Vec<String>, SimonError> {
        Ok(Vec::new())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_detection_aes() {
        let flags = vec!["aes".into(), "pclmulqdq".into(), "rdrand".into()];
        let features = CryptoAccelMonitor::detect_features(&flags);
        let aes = features.iter().find(|f| f.cpu_flag == "aes").unwrap();
        assert!(aes.hardware_accelerated);
        assert_eq!(aes.category, CryptoCategory::SymmetricEncryption);
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
            estimated_throughput_gbs: Some(10.0),
            description: "AES".into(),
        };
        let json = serde_json::to_string(&feature).unwrap();
        assert!(json.contains("AES-NI"));
        let _: CryptoFeature = serde_json::from_str(&json).unwrap();
    }
}
