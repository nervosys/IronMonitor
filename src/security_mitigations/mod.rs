//! CPU vulnerability mitigations and kernel security monitoring.
//!
//! Detects CPU vulnerabilities (Spectre, Meltdown, MDS, etc.) and their
//! mitigation status, kernel security modules (SELinux, AppArmor),
//! ASLR configuration, Secure Boot chain, and security feature compliance
//! scoring.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/devices/system/cpu/vulnerabilities/`, `/proc/sys/kernel/`, LSM
//! - **Windows**: `Get-SpeculationControlSettings`, Windows Security Center
//! - **macOS**: `sysctl kern.hv_support`, system integrity protection

use serde::{Deserialize, Serialize};

use crate::error::IronError;

/// CPU vulnerability status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MitigationStatus {
    /// Not affected by this vulnerability.
    NotAffected,
    /// Vulnerable and not mitigated.
    Vulnerable,
    /// Mitigated (with possible performance impact).
    Mitigated,
    /// Partially mitigated.
    PartiallyMitigated,
    /// Unknown status.
    Unknown,
}

impl std::fmt::Display for MitigationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAffected => write!(f, "Not Affected"),
            Self::Vulnerable => write!(f, "Vulnerable"),
            Self::Mitigated => write!(f, "Mitigated"),
            Self::PartiallyMitigated => write!(f, "Partially Mitigated"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// A CPU vulnerability and its mitigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuVulnerability {
    /// Vulnerability name (e.g. "spectre_v1", "meltdown", "mds").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// CVE identifier(s) if known.
    pub cve: Vec<String>,
    /// Mitigation status.
    pub status: MitigationStatus,
    /// Raw mitigation string from the kernel.
    pub raw_status: String,
    /// Whether the mitigation has a performance impact.
    pub performance_impact: bool,
    /// Estimated performance impact percentage (inferred).
    pub estimated_impact_pct: f64,
}

/// Kernel security module type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityModule {
    SELinux,
    AppArmor,
    Smack,
    TOMOYO,
    Yama,
    LoadPin,
    SafeSetID,
    Lockdown,
    BPF,
    Landlock,
    WindowsDefender,
    Gatekeeper, // macOS
    SIP,        // macOS System Integrity Protection
    None,
}

impl std::fmt::Display for SecurityModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SELinux => write!(f, "SELinux"),
            Self::AppArmor => write!(f, "AppArmor"),
            Self::Smack => write!(f, "Smack"),
            Self::TOMOYO => write!(f, "TOMOYO"),
            Self::Yama => write!(f, "Yama"),
            Self::LoadPin => write!(f, "LoadPin"),
            Self::SafeSetID => write!(f, "SafeSetID"),
            Self::Lockdown => write!(f, "Lockdown"),
            Self::BPF => write!(f, "BPF"),
            Self::Landlock => write!(f, "Landlock"),
            Self::WindowsDefender => write!(f, "Windows Defender"),
            Self::Gatekeeper => write!(f, "Gatekeeper"),
            Self::SIP => write!(f, "SIP"),
            Self::None => write!(f, "None"),
        }
    }
}

/// LSM (Linux Security Module) status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LsmStatus {
    /// Module type.
    pub module: SecurityModule,
    /// Whether the module is enabled/active.
    pub enabled: bool,
    /// Mode (e.g. "enforcing", "permissive", "disabled").
    pub mode: String,
}

/// Kernel hardening features.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelHardening {
    /// ASLR level (0=disabled, 1=partial, 2=full).
    pub aslr_level: u32,
    /// Whether kernel pointer leak protection is enabled (`kptr_restrict`),
    /// or `None` where the setting does not exist.
    ///
    /// These four are **Linux kernel tunables**. The Windows and macOS readers
    /// asserted `true` for all of them, which is not a translation of the
    /// mechanism onto those platforms — there is no `/proc/sys/kernel/
    /// kptr_restrict` to be true. Two of them feed the posture score directly,
    /// so the score was three points higher on Windows and macOS for hardening
    /// nothing had checked.
    pub kptr_restrict: Option<bool>,
    /// Whether dmesg is restricted to root. See [`Self::kptr_restrict`].
    pub dmesg_restrict: Option<bool>,
    /// Whether unprivileged BPF is disabled. See [`Self::kptr_restrict`].
    pub unprivileged_bpf_disabled: Option<bool>,
    /// Whether ptrace is restricted (Yama scope).
    pub ptrace_scope: u32,
    /// Whether kernel module loading is locked down.
    pub modules_locked: bool,
    /// Whether kernel lockdown is active.
    pub lockdown_mode: String,
    /// Whether Secure Boot is enabled, or `None` where it could not be read.
    ///
    /// The Windows reader shelled out to `Confirm-SecureBootUEFI`, which
    /// **requires elevation**, and `unwrap_or(false)` turned an unelevated run
    /// into "Secure Boot is off" — raising a medium-severity finding that may
    /// be untrue. `afb377d` established that the state is readable unelevated
    /// from `SYSTEM\CurrentControlSet\Control\SecureBoot\State`, and
    /// `platform::windows::secure_boot_enabled` already does exactly that.
    pub secure_boot: Option<bool>,
    /// Stack protector enabled, or `None` where it was not read.
    ///
    /// Every platform wrote `true`; the Linux one under a comment reading
    /// "Most modern kernels have this". Most is not this one.
    pub stack_protector: Option<bool>,
}

/// Overall security posture score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPosture {
    /// Score from 0-100.
    pub score: u32,
    /// Risk level.
    pub risk_level: String,
    /// Individual component scores.
    pub component_scores: Vec<(String, u32)>,
    /// Findings and recommendations.
    pub findings: Vec<SecurityFinding>,
}

/// A security finding with severity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    /// Finding severity: "critical", "high", "medium", "low", "info".
    pub severity: String,
    /// Short title.
    pub title: String,
    /// Detailed description.
    pub description: String,
    /// Suggested remediation.
    pub remediation: String,
}

/// Security mitigations monitor.
pub struct SecurityMitigationsMonitor {
    /// CPU vulnerabilities.
    pub vulnerabilities: Vec<CpuVulnerability>,
    /// Active security modules.
    pub security_modules: Vec<LsmStatus>,
    /// Kernel hardening status.
    pub hardening: KernelHardening,
    /// Overall security posture.
    pub posture: SecurityPosture,
}

impl SecurityMitigationsMonitor {
    /// Create a new monitor, detecting all mitigations.
    pub fn new() -> Result<Self, IronError> {
        let vulnerabilities = Self::detect_vulnerabilities()?;
        let security_modules = Self::detect_security_modules()?;
        let hardening = Self::detect_hardening()?;
        let posture = Self::compute_posture(&vulnerabilities, &security_modules, &hardening);

        Ok(Self {
            vulnerabilities,
            security_modules,
            hardening,
            posture,
        })
    }

    /// Refresh all data.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.vulnerabilities = Self::detect_vulnerabilities()?;
        self.security_modules = Self::detect_security_modules()?;
        self.hardening = Self::detect_hardening()?;
        self.posture = Self::compute_posture(
            &self.vulnerabilities,
            &self.security_modules,
            &self.hardening,
        );
        Ok(())
    }

    /// Get unmitigated vulnerabilities.
    pub fn unmitigated(&self) -> Vec<&CpuVulnerability> {
        self.vulnerabilities
            .iter()
            .filter(|v| v.status == MitigationStatus::Vulnerable)
            .collect()
    }

    /// Total estimated performance impact of all active mitigations.
    pub fn total_performance_impact(&self) -> f64 {
        self.vulnerabilities
            .iter()
            .filter(|v| v.status == MitigationStatus::Mitigated && v.performance_impact)
            .map(|v| v.estimated_impact_pct)
            .sum()
    }

    fn compute_posture(
        vulns: &[CpuVulnerability],
        lsms: &[LsmStatus],
        hardening: &KernelHardening,
    ) -> SecurityPosture {
        let mut score: i32 = 50; // Base score
        let mut findings = Vec::new();
        let mut components = Vec::new();

        // CPU vulnerability scoring (max 25 points)
        let vuln_count = vulns.len();
        let mitigated = vulns
            .iter()
            .filter(|v| {
                v.status == MitigationStatus::Mitigated || v.status == MitigationStatus::NotAffected
            })
            .count();
        let vulnerable = vulns
            .iter()
            .filter(|v| v.status == MitigationStatus::Vulnerable)
            .count();

        let vuln_score = if vuln_count > 0 {
            ((mitigated as f64 / vuln_count as f64) * 25.0) as u32
        } else {
            25
        };
        score += vuln_score as i32;
        components.push(("CPU Vulnerability Mitigations".into(), vuln_score));

        if vulnerable > 0 {
            findings.push(SecurityFinding {
                severity: "critical".into(),
                title: format!("{} unmitigated CPU vulnerabilities", vulnerable),
                description: "CPU vulnerabilities without active mitigations can be exploited for information disclosure or privilege escalation.".into(),
                remediation: "Update kernel and CPU microcode to latest versions.".into(),
            });
        }

        // LSM scoring (max 15 points)
        let active_lsm = lsms.iter().filter(|l| l.enabled).count();
        let lsm_score = (active_lsm as u32 * 5).min(15);
        score += lsm_score as i32;
        components.push(("Security Modules".into(), lsm_score));

        if active_lsm == 0 {
            findings.push(SecurityFinding {
                severity: "high".into(),
                title: "No mandatory access control active".into(),
                description: "No security module (SELinux, AppArmor, etc.) is enforcing mandatory access controls.".into(),
                remediation: "Enable SELinux or AppArmor in enforcing mode.".into(),
            });
        }

        // Hardening scoring (max 10 points)
        let mut hard_score: u32 = 0;
        if hardening.aslr_level >= 2 {
            hard_score += 2;
        } else {
            findings.push(SecurityFinding {
                severity: "medium".into(),
                title: "ASLR not at full level".into(),
                description: format!("ASLR level is {}, maximum is 2.", hardening.aslr_level),
                remediation: "Set kernel.randomize_va_space = 2".into(),
            });
        }
        // Only credit hardening that was actually observed. `Some(false)` and
        // `None` both score nothing, but only the first is a finding.
        if hardening.kptr_restrict == Some(true) {
            hard_score += 2;
        }
        if hardening.dmesg_restrict == Some(true) {
            hard_score += 1;
        }
        if hardening.ptrace_scope >= 1 {
            hard_score += 2;
        }
        match hardening.secure_boot {
            Some(true) => hard_score += 3,
            // Only report it off when it was read and is off. An unread state
            // is not a security finding about the machine.
            Some(false) => findings.push(SecurityFinding {
                severity: "medium".into(),
                title: "Secure Boot not enabled".into(),
                description: "UEFI Secure Boot is not active.".into(),
                remediation: "Enable Secure Boot in UEFI firmware settings.".into(),
            }),
            None => findings.push(SecurityFinding {
                severity: "info".into(),
                title: "Secure Boot state not read".into(),
                description: "IronMonitor could not read the Secure Boot state on this platform."
                    .into(),
                remediation: "Check it in the UEFI firmware settings directly.".into(),
            }),
        }
        score += hard_score as i32;
        components.push(("Kernel Hardening".into(), hard_score));

        let final_score = score.clamp(0, 100) as u32;
        let risk_level = match final_score {
            0..=30 => "Critical",
            31..=50 => "High",
            51..=70 => "Medium",
            71..=85 => "Low",
            86..=100 => "Minimal",
            _ => "Unknown",
        }
        .to_string();

        SecurityPosture {
            score: final_score,
            risk_level,
            component_scores: components,
            findings,
        }
    }

    #[cfg(target_os = "linux")]
    fn detect_vulnerabilities() -> Result<Vec<CpuVulnerability>, IronError> {
        let vuln_dir = std::path::Path::new("/sys/devices/system/cpu/vulnerabilities");
        let mut vulns = Vec::new();

        if !vuln_dir.exists() {
            return Ok(vulns);
        }

        let known_vulns = vec![
            (
                "spectre_v1",
                "Spectre Variant 1 (Bounds Check Bypass)",
                vec!["CVE-2017-5753"],
            ),
            (
                "spectre_v2",
                "Spectre Variant 2 (Branch Target Injection)",
                vec!["CVE-2017-5715"],
            ),
            (
                "meltdown",
                "Meltdown (Rogue Data Cache Load)",
                vec!["CVE-2017-5754"],
            ),
            (
                "spec_store_bypass",
                "Speculative Store Bypass",
                vec!["CVE-2018-3639"],
            ),
            (
                "l1tf",
                "L1 Terminal Fault",
                vec!["CVE-2018-3615", "CVE-2018-3620"],
            ),
            (
                "mds",
                "Microarchitectural Data Sampling",
                vec!["CVE-2018-12126", "CVE-2018-12127"],
            ),
            (
                "tsx_async_abort",
                "TSX Asynchronous Abort",
                vec!["CVE-2019-11135"],
            ),
            ("itlb_multihit", "ITLB Multihit", vec![]),
            (
                "srbds",
                "Special Register Buffer Data Sampling",
                vec!["CVE-2020-0543"],
            ),
            ("mmio_stale_data", "MMIO Stale Data", vec!["CVE-2022-21123"]),
            (
                "retbleed",
                "Return Address Branch Target Injection",
                vec!["CVE-2022-29900", "CVE-2022-29901"],
            ),
            (
                "spec_rstack_overflow",
                "Speculative Return Stack Overflow",
                vec!["CVE-2023-20569"],
            ),
            (
                "gather_data_sampling",
                "Gather Data Sampling (Downfall)",
                vec!["CVE-2022-40982"],
            ),
            (
                "rfds",
                "Register File Data Sampling",
                vec!["CVE-2023-28746"],
            ),
            ("gds", "Gather Data Sampling", vec!["CVE-2022-40982"]),
        ];

        for (file, desc, cves) in &known_vulns {
            let path = vuln_dir.join(file);
            if let Ok(status_str) = std::fs::read_to_string(&path) {
                let raw = status_str.trim().to_string();
                let (status, perf_impact, impact_pct) = Self::parse_vuln_status(&raw);

                vulns.push(CpuVulnerability {
                    name: file.to_string(),
                    description: desc.to_string(),
                    cve: cves.iter().map(|s| s.to_string()).collect(),
                    status,
                    raw_status: raw,
                    performance_impact: perf_impact,
                    estimated_impact_pct: impact_pct,
                });
            }
        }

        Ok(vulns)
    }

    #[cfg(target_os = "linux")]
    fn parse_vuln_status(raw: &str) -> (MitigationStatus, bool, f64) {
        let lower = raw.to_lowercase();
        if lower.starts_with("not affected") {
            (MitigationStatus::NotAffected, false, 0.0)
        } else if lower.starts_with("vulnerable") {
            (MitigationStatus::Vulnerable, false, 0.0)
        } else if lower.starts_with("mitigation:") {
            let perf_impact = lower.contains("retpoline")
                || lower.contains("ibrs")
                || lower.contains("stibp")
                || lower.contains("verw")
                || lower.contains("force");
            let impact = if lower.contains("retpoline") {
                3.0
            } else if lower.contains("ibrs") {
                5.0
            } else if lower.contains("verw") {
                2.0
            } else if perf_impact {
                1.5
            } else {
                0.5
            };
            (MitigationStatus::Mitigated, perf_impact, impact)
        } else {
            (MitigationStatus::Unknown, false, 0.0)
        }
    }

    #[cfg(target_os = "windows")]
    fn detect_vulnerabilities() -> Result<Vec<CpuVulnerability>, IronError> {
        // Returning an empty list meant `unmitigated()` reported zero, which
        // reads as "this machine has no unmitigated CPU vulnerabilities" when
        // it means "nothing was checked". Of every absence in this crate that
        // one is the least acceptable to guess at: a reassuring security
        // reading is acted on.
        Err(IronError::UnsupportedPlatform(
            "CPU vulnerability status is read from `/sys/devices/system/cpu/vulnerabilities`; Windows exposes it only through Get-SpeculationControlSettings, which needs Administrator"
                .into(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn detect_vulnerabilities() -> Result<Vec<CpuVulnerability>, IronError> {
        // Returning an empty list meant `unmitigated()` reported zero, which
        // reads as "this machine has no unmitigated CPU vulnerabilities" when
        // it means "nothing was checked". Of every absence in this crate that
        // one is the least acceptable to guess at: a reassuring security
        // reading is acted on.
        Err(IronError::UnsupportedPlatform(
            "CPU vulnerability status is read from `/sys/devices/system/cpu/vulnerabilities`; macOS does not expose it"
                .into(),
        ))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn detect_vulnerabilities() -> Result<Vec<CpuVulnerability>, IronError> {
        // Returning an empty list meant `unmitigated()` reported zero, which
        // reads as "this machine has no unmitigated CPU vulnerabilities" when
        // it means "nothing was checked". Of every absence in this crate that
        // one is the least acceptable to guess at: a reassuring security
        // reading is acted on.
        Err(IronError::UnsupportedPlatform(
            "CPU vulnerability status is read from `/sys/devices/system/cpu/vulnerabilities`; this platform does not expose it"
                .into(),
        ))
    }

    #[cfg(target_os = "linux")]
    fn detect_security_modules() -> Result<Vec<LsmStatus>, IronError> {
        let mut modules = Vec::new();

        // Check active LSMs
        if let Ok(lsm_list) = std::fs::read_to_string("/sys/kernel/security/lsm") {
            let lsms: Vec<&str> = lsm_list.trim().split(',').collect();
            for lsm in &lsms {
                let module = match *lsm {
                    "selinux" => SecurityModule::SELinux,
                    "apparmor" => SecurityModule::AppArmor,
                    "smack" => SecurityModule::Smack,
                    "tomoyo" => SecurityModule::TOMOYO,
                    "yama" => SecurityModule::Yama,
                    "loadpin" => SecurityModule::LoadPin,
                    "safesetid" => SecurityModule::SafeSetID,
                    "lockdown" => SecurityModule::Lockdown,
                    "bpf" => SecurityModule::BPF,
                    "landlock" => SecurityModule::Landlock,
                    _ => continue,
                };

                let mode = match module {
                    SecurityModule::SELinux => std::fs::read_to_string("/sys/fs/selinux/enforce")
                        .ok()
                        .map(|s| {
                            if s.trim() == "1" {
                                "enforcing"
                            } else {
                                "permissive"
                            }
                        })
                        .unwrap_or("unknown")
                        .to_string(),
                    _ => "active".to_string(),
                };

                modules.push(LsmStatus {
                    module,
                    enabled: true,
                    mode,
                });
            }
        }

        Ok(modules)
    }

    #[cfg(target_os = "windows")]
    fn detect_security_modules() -> Result<Vec<LsmStatus>, IronError> {
        // Check Windows Defender
        let defender = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-MpComputerStatus | Select-Object RealTimeProtectionEnabled | ConvertTo-Json",
            ])
            .output();

        let mut modules = Vec::new();
        if let Ok(out) = defender {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                let enabled = text.contains("true");
                modules.push(LsmStatus {
                    module: SecurityModule::WindowsDefender,
                    enabled,
                    mode: if enabled {
                        "real-time".into()
                    } else {
                        "disabled".into()
                    },
                });
            }
        }

        Ok(modules)
    }

    #[cfg(target_os = "macos")]
    fn detect_security_modules() -> Result<Vec<LsmStatus>, IronError> {
        let mut modules = Vec::new();

        // Check SIP status
        let sip = std::process::Command::new("csrutil").arg("status").output();

        if let Ok(out) = sip {
            let text = String::from_utf8_lossy(&out.stdout);
            let enabled = text.contains("enabled");
            modules.push(LsmStatus {
                module: SecurityModule::SIP,
                enabled,
                mode: if enabled {
                    "enabled".into()
                } else {
                    "disabled".into()
                },
            });
        }

        Ok(modules)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn detect_security_modules() -> Result<Vec<LsmStatus>, IronError> {
        Ok(Vec::new())
    }

    #[cfg(target_os = "linux")]
    fn detect_hardening() -> Result<KernelHardening, IronError> {
        let read_sysctl = |path: &str| -> String {
            std::fs::read_to_string(path)
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };

        let aslr = read_sysctl("/proc/sys/kernel/randomize_va_space")
            .parse::<u32>()
            .unwrap_or(0);

        // An unreadable or absent sysctl is not a disabled setting: older
        // kernels do not have all of these, and `unwrap_or(0)` reported every
        // missing file as hardening switched off.
        let kptr = read_sysctl("/proc/sys/kernel/kptr_restrict")
            .parse::<u32>()
            .ok()
            .map(|v| v > 0);

        let dmesg = read_sysctl("/proc/sys/kernel/dmesg_restrict")
            .parse::<u32>()
            .ok()
            .map(|v| v > 0);

        let bpf = read_sysctl("/proc/sys/kernel/unprivileged_bpf_disabled")
            .parse::<u32>()
            .ok()
            .map(|v| v > 0);

        let ptrace = read_sysctl("/proc/sys/kernel/yama/ptrace_scope")
            .parse::<u32>()
            .unwrap_or(0);

        let lockdown = read_sysctl("/sys/kernel/security/lockdown");

        // Secure boot detection
        // This tested only that the efivar *exists*, which it does on every
        // UEFI system whether Secure Boot is on or off — so any UEFI Linux
        // machine reported Secure Boot enabled. The variable's last byte
        // carries the state, which `boot_config` already reads correctly.
        let secure_boot = std::fs::read(
            "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c",
        )
        .ok()
        .and_then(|data| data.last().map(|b| *b == 1));

        Ok(KernelHardening {
            aslr_level: aslr,
            kptr_restrict: kptr,
            dmesg_restrict: dmesg,
            unprivileged_bpf_disabled: bpf,
            ptrace_scope: ptrace,
            modules_locked: false,
            lockdown_mode: if lockdown.is_empty() {
                "none".into()
            } else {
                lockdown
            },
            secure_boot,
            stack_protector: None, // Most modern kernels have this
        })
    }

    #[cfg(target_os = "windows")]
    fn detect_hardening() -> Result<KernelHardening, IronError> {
        // `Confirm-SecureBootUEFI` needs elevation and returned `false` for a
        // permission error. The registry carries the same state unelevated.
        let sb = crate::platform::windows::secure_boot_enabled();

        Ok(KernelHardening {
            aslr_level: 2, // Windows always has ASLR
            // These four are Linux kernel tunables with no Windows equivalent.
            // Asserting `true` credited the posture score for hardening that
            // was never checked and does not exist in this form here.
            kptr_restrict: None,
            dmesg_restrict: None,
            unprivileged_bpf_disabled: None,
            ptrace_scope: 0,
            modules_locked: false,
            lockdown_mode: "none".into(),
            secure_boot: sb,
            stack_protector: None,
        })
    }

    #[cfg(target_os = "macos")]
    fn detect_hardening() -> Result<KernelHardening, IronError> {
        Ok(KernelHardening {
            aslr_level: 2, // macOS always has ASLR
            // Linux tunables with no macOS equivalent; none of this is read.
            kptr_restrict: None,
            dmesg_restrict: None,
            unprivileged_bpf_disabled: None,
            ptrace_scope: 1,
            modules_locked: false,
            lockdown_mode: "none".into(),
            // "Apple Silicon always has Secure Boot" is not a reading, and this
            // code runs on Intel Macs too, where it depends on the T2 chip and
            // the user's Startup Security Utility setting.
            secure_boot: None,
            stack_protector: None,
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn detect_hardening() -> Result<KernelHardening, IronError> {
        Ok(KernelHardening {
            aslr_level: 0,
            kptr_restrict: None,
            dmesg_restrict: None,
            unprivileged_bpf_disabled: None,
            ptrace_scope: 0,
            modules_locked: false,
            lockdown_mode: "unknown".into(),
            secure_boot: None,
            stack_protector: None,
        })
    }
}

impl Default for SecurityMitigationsMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            vulnerabilities: Vec::new(),
            security_modules: Vec::new(),
            hardening: KernelHardening {
                aslr_level: 0,
                kptr_restrict: None,
                dmesg_restrict: None,
                unprivileged_bpf_disabled: None,
                ptrace_scope: 0,
                modules_locked: false,
                lockdown_mode: "unknown".into(),
                secure_boot: None,
                stack_protector: None,
            },
            posture: SecurityPosture {
                score: 0,
                risk_level: "Unknown".into(),
                component_scores: Vec::new(),
                findings: Vec::new(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mitigation_status_display() {
        assert_eq!(MitigationStatus::Mitigated.to_string(), "Mitigated");
        assert_eq!(MitigationStatus::Vulnerable.to_string(), "Vulnerable");
        assert_eq!(MitigationStatus::NotAffected.to_string(), "Not Affected");
    }

    #[test]
    fn test_posture_scoring() {
        let vulns = vec![
            CpuVulnerability {
                name: "spectre_v1".into(),
                description: "Spectre V1".into(),
                cve: vec!["CVE-2017-5753".into()],
                status: MitigationStatus::Mitigated,
                raw_status: "Mitigation: usercopy".into(),
                performance_impact: false,
                estimated_impact_pct: 0.5,
            },
            CpuVulnerability {
                name: "meltdown".into(),
                description: "Meltdown".into(),
                cve: vec!["CVE-2017-5754".into()],
                status: MitigationStatus::NotAffected,
                raw_status: "Not affected".into(),
                performance_impact: false,
                estimated_impact_pct: 0.0,
            },
        ];

        let lsms = vec![LsmStatus {
            module: SecurityModule::AppArmor,
            enabled: true,
            mode: "enforce".into(),
        }];

        let hardening = KernelHardening {
            aslr_level: 2,
            kptr_restrict: None,
            dmesg_restrict: None,
            unprivileged_bpf_disabled: None,
            ptrace_scope: 1,
            modules_locked: false,
            lockdown_mode: "none".into(),
            secure_boot: None,
            stack_protector: None,
        };

        let posture = SecurityMitigationsMonitor::compute_posture(&vulns, &lsms, &hardening);
        assert!(posture.score >= 70); // Good security posture
        assert!(posture.risk_level == "Low" || posture.risk_level == "Minimal");
    }

    #[test]
    fn test_performance_impact() {
        let monitor = SecurityMitigationsMonitor {
            vulnerabilities: vec![
                CpuVulnerability {
                    name: "spectre_v2".into(),
                    description: "".into(),
                    cve: vec![],
                    status: MitigationStatus::Mitigated,
                    raw_status: "Mitigation: Retpolines".into(),
                    performance_impact: true,
                    estimated_impact_pct: 3.0,
                },
                CpuVulnerability {
                    name: "mds".into(),
                    description: "".into(),
                    cve: vec![],
                    status: MitigationStatus::Mitigated,
                    raw_status: "Mitigation: VERW".into(),
                    performance_impact: true,
                    estimated_impact_pct: 2.0,
                },
            ],
            security_modules: Vec::new(),
            hardening: KernelHardening {
                aslr_level: 2,
                kptr_restrict: None,
                dmesg_restrict: None,
                unprivileged_bpf_disabled: None,
                ptrace_scope: 0,
                modules_locked: false,
                lockdown_mode: "none".into(),
                secure_boot: None,
                stack_protector: None,
            },
            posture: SecurityPosture {
                score: 50,
                risk_level: "Medium".into(),
                component_scores: vec![],
                findings: vec![],
            },
        };

        let impact = monitor.total_performance_impact();
        assert!((impact - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_monitor_default() {
        let monitor = SecurityMitigationsMonitor::default();
        let _unmitigated = monitor.unmitigated();
    }

    #[test]
    fn test_serialization() {
        let finding = SecurityFinding {
            severity: "high".into(),
            title: "Test finding".into(),
            description: "A test".into(),
            remediation: "Fix it".into(),
        };
        let json = serde_json::to_string(&finding).unwrap();
        assert!(json.contains("high"));
        let _: SecurityFinding = serde_json::from_str(&json).unwrap();
    }

    /// A platform that cannot read CPU vulnerabilities produces no report.
    ///
    /// `detect_vulnerabilities` returns `UnsupportedPlatform` off Linux, with
    /// a comment worth keeping: *"a reassuring security reading is acted on"*.
    /// That error is what makes `detect_hardening`'s non-Linux branches
    /// unreachable — they asserted `kptr_restrict`, `dmesg_restrict`,
    /// `unprivileged_bpf_disabled` and `stack_protector` as `true`, which are
    /// Linux kernel tunables with no equivalent there, and two of them feed
    /// the posture score.
    ///
    /// They are `None` now, but this test is the one that matters: **if
    /// someone implements vulnerability detection for another platform, the
    /// hardening branch beside it becomes live in the same commit.** This
    /// fails then, which is the moment to look at it.
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn a_platform_without_vulnerability_data_publishes_no_posture() {
        let built = SecurityMitigationsMonitor::new();
        assert!(
            built.is_err(),
            concat!(
                "this platform now produces a security posture. Check that ",
                "detect_hardening does not assert Linux kernel tunables it ",
                "cannot read -- two of them add to the score."
            )
        );
    }

    /// Secure Boot is read from the variable's value, not its existence.
    ///
    /// The Linux reader tested `Path::exists()` on the efivar, which is
    /// present on every UEFI system whether Secure Boot is on or off, so any
    /// UEFI machine reported it enabled. `boot_config` already read the last
    /// byte correctly.
    #[cfg(target_os = "linux")]
    #[test]
    fn secure_boot_is_read_from_the_variable_not_its_presence() {
        let monitor = SecurityMitigationsMonitor::new().expect("a report on Linux");

        let path = std::path::Path::new(
            "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c",
        );
        if !path.exists() {
            assert_eq!(
                monitor.hardening.secure_boot, None,
                "no efivar to read, so no state to report"
            );
            return;
        }

        // Present: the answer must match the byte, not the file's existence.
        let expected = std::fs::read(path)
            .ok()
            .and_then(|d| d.last().map(|b| *b == 1));
        assert_eq!(monitor.hardening.secure_boot, expected);
    }

    /// An unread Secure Boot state is not a Secure Boot finding.
    #[cfg(target_os = "linux")]
    #[test]
    fn an_unread_secure_boot_state_is_not_a_medium_finding() {
        let monitor = SecurityMitigationsMonitor::new().expect("a report on Linux");
        if monitor.hardening.secure_boot.is_none() {
            assert!(
                !monitor
                    .posture
                    .findings
                    .iter()
                    .any(|f| f.title == "Secure Boot not enabled"),
                "Secure Boot was not read, so it must not be reported as off"
            );
        }
    }
}
