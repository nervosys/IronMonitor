//! Kernel runtime parameters monitoring.
//!
//! Reads and categorizes sysctl / kernel parameters covering networking,
//! memory, security, and performance tuning. Provides analysis of
//! parameters that differ from recommended defaults and security
//! hardening assessment.
//!
//! ## Platform Support
//!
//! - **Linux**: `/proc/sys/` hierarchy, sysctl
//! - **Windows**: Registry-based kernel tuning
//! - **macOS**: sysctl parameters

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// Parameter category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParamCategory {
    Network,
    Memory,
    Security,
    FileSystem,
    Kernel,
    Vm,
    Debug,
    Unknown,
}

impl std::fmt::Display for ParamCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network => write!(f, "Network"),
            Self::Memory => write!(f, "Memory"),
            Self::Security => write!(f, "Security"),
            Self::FileSystem => write!(f, "Filesystem"),
            Self::Kernel => write!(f, "Kernel"),
            Self::Vm => write!(f, "VM/Memory"),
            Self::Debug => write!(f, "Debug"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// A kernel parameter and its value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelParam {
    /// Full parameter path (e.g. "net.ipv4.tcp_syncookies").
    pub name: String,
    /// Current value.
    pub value: String,
    /// Category.
    pub category: ParamCategory,
    /// Whether the current value matches recommended.
    pub is_recommended: bool,
    /// Recommended value (if known).
    pub recommended: Option<String>,
    /// Description.
    pub description: String,
}

/// Kernel parameters analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelParamsReport {
    /// All monitored parameters.
    pub params: Vec<KernelParam>,
    /// Security-relevant parameters.
    pub security_params: Vec<KernelParam>,
    /// Network tuning parameters.
    pub network_params: Vec<KernelParam>,
    /// Memory/VM parameters.
    pub vm_params: Vec<KernelParam>,
    /// Number of non-recommended settings.
    pub non_recommended_count: u32,
    /// Security hardening score (0-100).
    pub security_score: u32,
    /// Network tuning score (0-100).
    pub network_score: u32,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// Kernel parameters monitor.
pub struct KernelParamsMonitor {
    report: KernelParamsReport,
}

/// The TCP auto-tuning level every template agrees on, if they do.
///
/// `Get-NetTCPSetting` returns one row per template rather than one row for the
/// machine, and the reader took `Select-Object -First 1` -- which is the
/// `Automatic` template, the selector, carrying no level of its own and
/// returning an empty line. The other six rows on the development host all say
/// `Normal`, so the ontology reported "no kernel parameter was readable here"
/// about a setting the machine states six times.
///
/// Rows that state nothing are skipped. Rows that disagree describe different
/// destinations rather than one machine-wide level, and `None` is returned
/// rather than one of them being chosen to stand for all -- the answer to
/// "which template applies to this connection" is not this parameter.
#[cfg(target_os = "windows")]
fn agreed_tcp_level(cmdlet_output: &str) -> Option<String> {
    let levels: Vec<&str> = cmdlet_output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let first = levels.first()?;
    levels
        .iter()
        .all(|l| l.eq_ignore_ascii_case(first))
        .then(|| (*first).to_string())
}

impl KernelParamsMonitor {
    /// Create a new kernel parameters monitor.
    pub fn new() -> Result<Self, IronError> {
        let report = Self::analyze()?;
        Ok(Self { report })
    }

    /// Refresh.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.report = Self::analyze()?;
        Ok(())
    }

    /// Get report.
    pub fn report(&self) -> &KernelParamsReport {
        &self.report
    }

    /// Get a specific parameter by name.
    pub fn param(&self, name: &str) -> Option<&KernelParam> {
        self.report.params.iter().find(|p| p.name == name)
    }

    fn analyze() -> Result<KernelParamsReport, IronError> {
        let params = Self::read_params();

        let security_params: Vec<KernelParam> = params
            .iter()
            .filter(|p| p.category == ParamCategory::Security)
            .cloned()
            .collect();
        let network_params: Vec<KernelParam> = params
            .iter()
            .filter(|p| p.category == ParamCategory::Network)
            .cloned()
            .collect();
        let vm_params: Vec<KernelParam> = params
            .iter()
            .filter(|p| p.category == ParamCategory::Vm || p.category == ParamCategory::Memory)
            .cloned()
            .collect();

        let non_recommended = params
            .iter()
            .filter(|p| !p.is_recommended && p.recommended.is_some())
            .count() as u32;

        let security_score = Self::compute_security_score(&security_params);
        let network_score = Self::compute_network_score(&network_params);

        let mut recommendations = Vec::new();
        for p in &params {
            if !p.is_recommended {
                if let Some(ref rec) = p.recommended {
                    recommendations.push(format!(
                        "{}: current={}, recommended={}",
                        p.name, p.value, rec
                    ));
                }
            }
        }

        Ok(KernelParamsReport {
            params,
            security_params,
            network_params,
            vm_params,
            non_recommended_count: non_recommended,
            security_score,
            network_score,
            recommendations,
        })
    }

    fn compute_security_score(params: &[KernelParam]) -> u32 {
        if params.is_empty() {
            return 0;
        }
        let total = params.iter().filter(|p| p.recommended.is_some()).count();
        if total == 0 {
            return 50;
        }
        let good = params
            .iter()
            .filter(|p| p.is_recommended && p.recommended.is_some())
            .count();
        ((good as f64 / total as f64) * 100.0) as u32
    }

    fn compute_network_score(params: &[KernelParam]) -> u32 {
        if params.is_empty() {
            return 0;
        }
        let total = params.iter().filter(|p| p.recommended.is_some()).count();
        if total == 0 {
            return 50;
        }
        let good = params
            .iter()
            .filter(|p| p.is_recommended && p.recommended.is_some())
            .count();
        ((good as f64 / total as f64) * 100.0) as u32
    }

    #[cfg(target_os = "linux")]
    fn read_params() -> Vec<KernelParam> {
        let checks: Vec<(&str, &str, ParamCategory, &str, &str)> = vec![
            // Security parameters
            (
                "kernel.randomize_va_space",
                "2",
                ParamCategory::Security,
                "ASLR level (0=off, 1=stack, 2=full)",
                "2",
            ),
            (
                "kernel.kptr_restrict",
                "1",
                ParamCategory::Security,
                "Restrict kernel pointer exposure",
                "1",
            ),
            (
                "kernel.dmesg_restrict",
                "1",
                ParamCategory::Security,
                "Restrict dmesg to privileged users",
                "1",
            ),
            (
                "kernel.yama.ptrace_scope",
                "1",
                ParamCategory::Security,
                "Ptrace scope restriction",
                "1",
            ),
            (
                "kernel.unprivileged_bpf_disabled",
                "1",
                ParamCategory::Security,
                "Disable unprivileged BPF",
                "1",
            ),
            (
                "kernel.kexec_load_disabled",
                "1",
                ParamCategory::Security,
                "Disable kexec_load",
                "1",
            ),
            (
                "kernel.sysrq",
                "0",
                ParamCategory::Security,
                "Magic SysRq key (0=disable)",
                "0",
            ),
            (
                "kernel.core_uses_pid",
                "1",
                ParamCategory::Security,
                "Core dumps include PID",
                "1",
            ),
            (
                "kernel.modules_disabled",
                "0",
                ParamCategory::Security,
                "Disable module loading (0=allow)",
                "0",
            ),
            // Network parameters
            (
                "net.ipv4.tcp_syncookies",
                "1",
                ParamCategory::Network,
                "SYN flood protection",
                "1",
            ),
            (
                "net.ipv4.conf.all.rp_filter",
                "1",
                ParamCategory::Network,
                "Reverse path filtering",
                "1",
            ),
            (
                "net.ipv4.conf.all.accept_redirects",
                "0",
                ParamCategory::Network,
                "ICMP redirect acceptance",
                "0",
            ),
            (
                "net.ipv4.conf.all.send_redirects",
                "0",
                ParamCategory::Network,
                "ICMP redirect sending",
                "0",
            ),
            (
                "net.ipv4.conf.all.accept_source_route",
                "0",
                ParamCategory::Network,
                "Source routing",
                "0",
            ),
            (
                "net.ipv4.conf.all.log_martians",
                "1",
                ParamCategory::Network,
                "Log martian packets",
                "1",
            ),
            (
                "net.ipv4.tcp_max_syn_backlog",
                "4096",
                ParamCategory::Network,
                "Max SYN backlog",
                "4096",
            ),
            (
                "net.core.somaxconn",
                "4096",
                ParamCategory::Network,
                "Max socket backlog",
                "4096",
            ),
            (
                "net.ipv4.tcp_tw_reuse",
                "1",
                ParamCategory::Network,
                "TIME_WAIT socket reuse",
                "1",
            ),
            (
                "net.ipv4.tcp_fin_timeout",
                "15",
                ParamCategory::Network,
                "FIN timeout seconds",
                "15",
            ),
            (
                "net.core.rmem_max",
                "16777216",
                ParamCategory::Network,
                "Max receive buffer",
                "16777216",
            ),
            (
                "net.core.wmem_max",
                "16777216",
                ParamCategory::Network,
                "Max send buffer",
                "16777216",
            ),
            // VM/Memory parameters
            (
                "vm.swappiness",
                "60",
                ParamCategory::Vm,
                "Swappiness (0-200)",
                "60",
            ),
            (
                "vm.dirty_ratio",
                "20",
                ParamCategory::Vm,
                "Dirty page ratio (% of RAM)",
                "20",
            ),
            (
                "vm.dirty_background_ratio",
                "10",
                ParamCategory::Vm,
                "Background dirty ratio",
                "10",
            ),
            (
                "vm.overcommit_memory",
                "0",
                ParamCategory::Vm,
                "Memory overcommit (0=heuristic)",
                "0",
            ),
            (
                "vm.max_map_count",
                "65530",
                ParamCategory::Vm,
                "Max memory mappings",
                "65530",
            ),
            (
                "vm.vfs_cache_pressure",
                "100",
                ParamCategory::Vm,
                "VFS cache pressure",
                "100",
            ),
            (
                "vm.min_free_kbytes",
                "65536",
                ParamCategory::Vm,
                "Minimum free memory (kB)",
                "65536",
            ),
            // Filesystem parameters
            (
                "fs.file-max",
                "1048576",
                ParamCategory::FileSystem,
                "Max open files system-wide",
                "1048576",
            ),
            (
                "fs.inotify.max_user_watches",
                "524288",
                ParamCategory::FileSystem,
                "Max inotify watches per user",
                "524288",
            ),
        ];

        let mut params = Vec::new();
        for (name, _default, category, description, recommended) in &checks {
            let sysctl_path = format!("/proc/sys/{}", name.replace('.', "/"));
            let value = std::fs::read_to_string(&sysctl_path)
                .ok()
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "N/A".into());

            let is_recommended = Self::check_recommended(&value, recommended);

            params.push(KernelParam {
                name: name.to_string(),
                value,
                category: *category,
                is_recommended,
                recommended: Some(recommended.to_string()),
                description: description.to_string(),
            });
        }
        params
    }

    #[cfg(target_os = "linux")]
    fn check_recommended(value: &str, recommended: &str) -> bool {
        if value == "N/A" {
            return false;
        }
        // Numeric comparison
        if let (Ok(v), Ok(r)) = (value.parse::<i64>(), recommended.parse::<i64>()) {
            return v >= r; // For most params, >= recommended is fine
        }
        value == recommended
    }

    #[cfg(target_os = "windows")]
    fn read_params() -> Vec<KernelParam> {
        // Read some Windows-equivalent tuning via registry/powershell
        let mut params = Vec::new();

        // TCP auto-tuning.
        //
        // `Get-NetTCPSetting` returns one row per template, not one row for the
        // machine, and `Select-Object -First 1` took the `Automatic` template --
        // the selector, which carries no level of its own and returns an empty
        // string. Every other row on this host says `Normal`, so the reader
        // reported "no kernel parameter was readable here" about a setting the
        // machine states six times.
        //
        // The rows that carry a level are read, and their value is published
        // only when they agree. Templates that disagree describe different
        // destinations rather than one machine-wide level, and picking one of
        // them would answer a question nobody asked; that case is left absent.
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-NetTCPSetting | ForEach-Object { $_.AutoTuningLevelLocal }",
            ])
            .output();

        if let Ok(out) = output {
            // A missing powershell, a failed cmdlet and a machine that genuinely
            // reports nothing all arrive here as no lines at all. Pushing the
            // parameter anyway published a named setting whose value was "",
            // which reads as a setting that exists and is blank rather than one
            // that was never read.
            if let Some(val) = agreed_tcp_level(&String::from_utf8_lossy(&out.stdout)) {
                params.push(KernelParam {
                    name: "net.tcp.autotuninglevel".into(),
                    value: val.clone(),
                    category: ParamCategory::Network,
                    is_recommended: val.to_lowercase() == "normal",
                    recommended: Some("Normal".into()),
                    description: "TCP auto-tuning level, agreed by every template that states one"
                        .into(),
                });
            }
        }

        params
    }

    #[cfg(target_os = "macos")]
    fn read_params() -> Vec<KernelParam> {
        let checks = vec![
            (
                "kern.maxfiles",
                ParamCategory::FileSystem,
                "Max open files",
                "49152",
            ),
            (
                "kern.maxfilesperproc",
                ParamCategory::FileSystem,
                "Max files per process",
                "24576",
            ),
            (
                "net.inet.tcp.msl",
                ParamCategory::Network,
                "TCP MSL (milliseconds)",
                "15000",
            ),
            (
                "net.inet.tcp.delayed_ack",
                ParamCategory::Network,
                "Delayed ACK",
                "0",
            ),
            ("vm.swapusage", ParamCategory::Vm, "Swap usage", ""),
        ];

        let mut params = Vec::new();
        for (name, category, description, recommended) in &checks {
            let output = std::process::Command::new("sysctl")
                .arg("-n")
                .arg(name)
                .output();

            let value = output
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|| "N/A".into());

            let is_recommended = if recommended.is_empty() {
                true
            } else if let (Ok(v), Ok(r)) = (value.parse::<i64>(), recommended.parse::<i64>()) {
                v >= r
            } else {
                value == *recommended
            };

            params.push(KernelParam {
                name: name.to_string(),
                value,
                category: *category,
                is_recommended,
                recommended: if recommended.is_empty() {
                    None
                } else {
                    Some(recommended.to_string())
                },
                description: description.to_string(),
            });
        }
        params
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn read_params() -> Vec<KernelParam> {
        Vec::new()
    }
}

impl Default for KernelParamsMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            report: KernelParamsReport {
                params: Vec::new(),
                security_params: Vec::new(),
                network_params: Vec::new(),
                vm_params: Vec::new(),
                non_recommended_count: 0,
                security_score: 0,
                network_score: 0,
                recommendations: Vec::new(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_display() {
        assert_eq!(ParamCategory::Network.to_string(), "Network");
        assert_eq!(ParamCategory::Security.to_string(), "Security");
        assert_eq!(ParamCategory::Vm.to_string(), "VM/Memory");
    }

    #[test]
    fn test_security_scoring() {
        let params = vec![
            KernelParam {
                name: "a".into(),
                value: "2".into(),
                category: ParamCategory::Security,
                is_recommended: true,
                recommended: Some("2".into()),
                description: "".into(),
            },
            KernelParam {
                name: "b".into(),
                value: "0".into(),
                category: ParamCategory::Security,
                is_recommended: false,
                recommended: Some("1".into()),
                description: "".into(),
            },
        ];
        let score = KernelParamsMonitor::compute_security_score(&params);
        assert_eq!(score, 50);
    }

    #[test]
    fn test_perfect_score() {
        let params = vec![
            KernelParam {
                name: "a".into(),
                value: "1".into(),
                category: ParamCategory::Security,
                is_recommended: true,
                recommended: Some("1".into()),
                description: "".into(),
            },
            KernelParam {
                name: "b".into(),
                value: "1".into(),
                category: ParamCategory::Security,
                is_recommended: true,
                recommended: Some("1".into()),
                description: "".into(),
            },
        ];
        let score = KernelParamsMonitor::compute_security_score(&params);
        assert_eq!(score, 100);
    }

    #[test]
    fn test_monitor_default() {
        let monitor = KernelParamsMonitor::default();
        let _report = monitor.report();
    }

    #[test]
    fn test_serialization() {
        let param = KernelParam {
            name: "net.ipv4.tcp_syncookies".into(),
            value: "1".into(),
            category: ParamCategory::Network,
            is_recommended: true,
            recommended: Some("1".into()),
            description: "SYN cookies".into(),
        };
        let json = serde_json::to_string(&param).unwrap();
        assert!(json.contains("tcp_syncookies"));
        let _: KernelParam = serde_json::from_str(&json).unwrap();
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tcp_tests {
    use super::agreed_tcp_level;

    /// The cmdlet's output on the development host: the `Automatic` template
    /// first, stating nothing, and six templates stating `Normal`.
    #[test]
    fn the_empty_first_template_does_not_hide_the_six_that_answer() {
        let out = "\nNormal\nNormal\nNormal\nNormal\nNormal\nNormal\n";
        assert_eq!(agreed_tcp_level(out).as_deref(), Some("Normal"));
    }

    #[test]
    fn templates_that_disagree_state_nothing_machine_wide() {
        assert_eq!(agreed_tcp_level("Normal\nDisabled\n"), None);
    }

    #[test]
    fn no_output_is_no_reading() {
        assert_eq!(agreed_tcp_level(""), None);
        assert_eq!(agreed_tcp_level("\n  \n"), None);
    }
}
