//! CPU Frequency Scaling and Governor Control
//!
//! This module provides comprehensive CPU frequency management including:
//! - Frequency governor control (performance, powersave, ondemand, schedutil, etc.)
//! - Min/max frequency limits
//! - CPU online/offline control
//! - Per-core frequency monitoring
//! - C-state (CPU idle) monitoring
//! - Turbo boost control
//!
//! # Example
//!
//! ```no_run
//! use ironmonlib::cpufreq::{CpuFreqMonitor, Governor};
//!
//! let mut monitor = CpuFreqMonitor::new().unwrap();
//!
//! // Show current governor
//! println!("Governor: {:?}", monitor.current_governor());
//!
//! // Set performance mode
//! monitor.set_governor(Governor::Performance).unwrap();
//!
//! // Get CPU frequencies
//! for cpu in monitor.cpus() {
//!     println!("CPU{}: {} MHz", cpu.id, cpu.current_freq_mhz);
//! }
//! ```

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

/// CPU frequency governor types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Governor {
    /// Maximum performance, highest frequency
    Performance,
    /// Minimum power, lowest frequency
    Powersave,
    /// Dynamic scaling based on load (legacy)
    Ondemand,
    /// Conservative scaling with gradual changes
    Conservative,
    /// Userspace control (manual)
    Userspace,
    /// Scheduler-driven frequency scaling (modern)
    Schedutil,
    /// Intel P-state driver
    IntelPstate,
    /// AMD P-state driver
    AmdPstate,
    /// Interactive governor (Android/embedded)
    Interactive,
    /// Unknown governor
    Unknown(String),
}

impl std::fmt::Display for Governor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Governor::Performance => write!(f, "performance"),
            Governor::Powersave => write!(f, "powersave"),
            Governor::Ondemand => write!(f, "ondemand"),
            Governor::Conservative => write!(f, "conservative"),
            Governor::Userspace => write!(f, "userspace"),
            Governor::Schedutil => write!(f, "schedutil"),
            Governor::IntelPstate => write!(f, "intel_pstate"),
            Governor::AmdPstate => write!(f, "amd_pstate"),
            Governor::Interactive => write!(f, "interactive"),
            Governor::Unknown(s) => write!(f, "{}", s),
        }
    }
}

impl std::str::FromStr for Governor {
    type Err = IronError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "performance" => Ok(Governor::Performance),
            "powersave" => Ok(Governor::Powersave),
            "ondemand" => Ok(Governor::Ondemand),
            "conservative" => Ok(Governor::Conservative),
            "userspace" => Ok(Governor::Userspace),
            "schedutil" => Ok(Governor::Schedutil),
            "intel_pstate" => Ok(Governor::IntelPstate),
            "amd_pstate" | "amd-pstate" | "amd-pstate-epp" => Ok(Governor::AmdPstate),
            "interactive" => Ok(Governor::Interactive),
            other => Ok(Governor::Unknown(other.to_string())),
        }
    }
}

/// Energy Performance Preference (EPP) values for Intel/AMD P-state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnergyPreference {
    /// Maximum performance (0)
    Performance,
    /// Balance performance (128)
    BalancePerformance,
    /// Balance power (192)
    BalancePower,
    /// Maximum power saving (255)
    Power,
}

impl std::fmt::Display for EnergyPreference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnergyPreference::Performance => write!(f, "performance"),
            EnergyPreference::BalancePerformance => write!(f, "balance_performance"),
            EnergyPreference::BalancePower => write!(f, "balance_power"),
            EnergyPreference::Power => write!(f, "power"),
        }
    }
}

impl std::str::FromStr for EnergyPreference {
    type Err = IronError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "performance" | "default" => Ok(EnergyPreference::Performance),
            "balance_performance" => Ok(EnergyPreference::BalancePerformance),
            "balance_power" | "normal" => Ok(EnergyPreference::BalancePower),
            "power" | "powersave" => Ok(EnergyPreference::Power),
            _ => Err(IronError::InvalidValue(format!(
                "Unknown energy preference: {}",
                s
            ))),
        }
    }
}

/// CPU idle state (C-state)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuIdleState {
    /// State name (e.g., "POLL", "C1", "C6")
    pub name: String,
    /// State description
    pub desc: Option<String>,
    /// Latency to exit state (microseconds)
    pub latency_us: u32,
    /// Residency requirement (microseconds)
    pub residency_us: Option<u32>,
    /// Is state enabled
    pub enabled: bool,
    /// Times this state was entered
    pub usage: u64,
    /// Time spent in this state (microseconds)
    pub time_us: u64,
    /// Is state disabled by admin
    pub disable: bool,
}

/// Per-CPU frequency information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuFreqInfo {
    /// CPU ID (0, 1, 2, ...)
    pub id: u32,
    /// Is CPU online
    pub online: bool,
    /// Current frequency (kHz)
    pub current_freq_khz: u64,
    /// Current frequency (MHz) for convenience
    pub current_freq_mhz: u32,
    /// Minimum allowed frequency (kHz)
    pub min_freq_khz: u64,
    /// Maximum allowed frequency (kHz)
    pub max_freq_khz: u64,
    /// Hardware minimum frequency (kHz)
    pub cpuinfo_min_freq_khz: Option<u64>,
    /// Hardware maximum frequency (kHz)
    pub cpuinfo_max_freq_khz: Option<u64>,
    /// Base frequency (kHz) - non-turbo max
    pub base_freq_khz: Option<u64>,
    /// Current governor
    pub governor: Governor,
    /// Available governors
    pub available_governors: Vec<Governor>,
    /// Available frequencies (kHz)
    pub available_frequencies: Vec<u64>,
    /// CPU model name
    pub model: Option<String>,
    /// CPU idle states
    pub idle_states: Vec<CpuIdleState>,
    /// Energy Performance Preference (Intel/AMD P-state)
    pub energy_preference: Option<EnergyPreference>,
    /// Available energy preferences
    pub available_energy_preferences: Vec<EnergyPreference>,
    /// Scaling driver in use
    pub scaling_driver: Option<String>,
    /// Sysfs path
    #[serde(skip)]
    pub sysfs_path: Option<PathBuf>,
}

impl CpuFreqInfo {
    /// Create new CpuFreqInfo for given CPU ID
    pub fn new(id: u32) -> Self {
        Self {
            id,
            online: true,
            current_freq_khz: 0,
            current_freq_mhz: 0,
            min_freq_khz: 0,
            max_freq_khz: 0,
            cpuinfo_min_freq_khz: None,
            cpuinfo_max_freq_khz: None,
            base_freq_khz: None,
            governor: Governor::Unknown("unknown".to_string()),
            available_governors: Vec::new(),
            available_frequencies: Vec::new(),
            model: None,
            idle_states: Vec::new(),
            energy_preference: None,
            available_energy_preferences: Vec::new(),
            scaling_driver: None,
            sysfs_path: None,
        }
    }

    /// Get frequency as percentage of maximum
    pub fn freq_percent(&self) -> f32 {
        if self.max_freq_khz == 0 {
            return 0.0;
        }
        (self.current_freq_khz as f32 / self.max_freq_khz as f32) * 100.0
    }

    /// Check if CPU is at maximum frequency
    pub fn is_max_freq(&self) -> bool {
        self.current_freq_khz >= self.max_freq_khz
    }

    /// Check if CPU is at minimum frequency
    pub fn is_min_freq(&self) -> bool {
        self.current_freq_khz <= self.min_freq_khz
    }

    /// Whether the core is above its base frequency, or `None` when the base
    /// frequency is not known.
    ///
    /// This returned `bool`, so "cannot tell" and "not boosting" were the same
    /// answer — and the middle branch made a third case look like the first two:
    ///
    /// ```text
    /// } else if let Some(max) = self.cpuinfo_max_freq_khz {
    ///     // Assume turbo if current > 95% of max
    /// ```
    ///
    /// 95% of the maximum is not a test for turbo. A part whose non-turbo
    /// ceiling *is* its `cpuinfo_max_freq` reads as boosting whenever it is
    /// busy, and one boosting to 96% of max reads as boosting for a reason
    /// that has nothing to do with the base clock. Turbo means "above base",
    /// and without a base there is no answer to give.
    ///
    /// `scaling_cur_freq` above `base_frequency` is the real test, and
    /// `base_freq_khz` comes from `cpufreq/base_frequency`, which
    /// intel_pstate exposes and other governors do not.
    pub fn is_turbo(&self) -> Option<bool> {
        self.base_freq_khz.map(|base| self.current_freq_khz > base)
    }
}

/// System-wide CPU frequency policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuFreqPolicy {
    /// Policy ID
    pub id: u32,
    /// CPUs covered by this policy
    pub affected_cpus: Vec<u32>,
    /// Related CPUs
    pub related_cpus: Vec<u32>,
    /// Current governor
    pub governor: Governor,
    /// Minimum frequency (kHz)
    pub min_freq_khz: u64,
    /// Maximum frequency (kHz)
    pub max_freq_khz: u64,
    /// Scaling driver
    pub scaling_driver: Option<String>,
}

/// Turbo boost status
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TurboStatus {
    /// Is turbo available on this system
    pub available: bool,
    /// Is turbo currently enabled
    pub enabled: bool,
    /// Can turbo be controlled
    pub controllable: bool,
    /// Turbo frequency boost (MHz above base)
    pub boost_mhz: Option<u32>,
}

/// CPU frequency monitor and controller
pub struct CpuFreqMonitor {
    /// Per-CPU information
    cpus: Vec<CpuFreqInfo>,
    /// System policies
    policies: Vec<CpuFreqPolicy>,
    /// Turbo status
    turbo: TurboStatus,
    /// Last update time
    last_update: Instant,
    /// P-state driver type
    #[allow(dead_code)]
    pstate_driver: Option<String>,
}

impl CpuFreqMonitor {
    /// Create a new CPU frequency monitor
    pub fn new() -> Result<Self> {
        let mut monitor = Self {
            cpus: Vec::new(),
            policies: Vec::new(),
            turbo: TurboStatus {
                available: false,
                enabled: false,
                controllable: false,
                boost_mhz: None,
            },
            last_update: Instant::now(),
            pstate_driver: None,
        };

        monitor.discover()?;
        Ok(monitor)
    }

    /// Get all CPUs
    pub fn cpus(&self) -> &[CpuFreqInfo] {
        &self.cpus
    }

    /// Get CPU by ID
    pub fn get_cpu(&self, id: u32) -> Option<&CpuFreqInfo> {
        self.cpus.iter().find(|c| c.id == id)
    }

    /// Get online CPUs
    pub fn online_cpus(&self) -> Vec<&CpuFreqInfo> {
        self.cpus.iter().filter(|c| c.online).collect()
    }

    /// Get offline CPUs
    pub fn offline_cpus(&self) -> Vec<&CpuFreqInfo> {
        self.cpus.iter().filter(|c| !c.online).collect()
    }

    /// Get policies
    pub fn policies(&self) -> &[CpuFreqPolicy] {
        &self.policies
    }

    /// Get turbo status
    pub fn turbo_status(&self) -> &TurboStatus {
        &self.turbo
    }

    /// Get current governor (from first online CPU or policy)
    pub fn current_governor(&self) -> Option<Governor> {
        self.cpus
            .iter()
            .find(|c| c.online)
            .map(|c| c.governor.clone())
    }

    /// Get available governors
    pub fn available_governors(&self) -> Vec<Governor> {
        self.cpus
            .iter()
            .find(|c| c.online)
            .map(|c| c.available_governors.clone())
            .unwrap_or_default()
    }

    /// Refresh CPU frequency data
    pub fn refresh(&mut self) -> Result<()> {
        self.discover()?;
        self.last_update = Instant::now();
        Ok(())
    }

    /// Set governor for all CPUs
    pub fn set_governor(&mut self, governor: Governor) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_set_governor(governor)
        }

        #[cfg(target_os = "windows")]
        {
            self.windows_set_governor(governor)
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = governor;
            Err(IronError::UnsupportedPlatform(
                "CPU governor control not supported on this platform".to_string(),
            ))
        }
    }

    /// Set governor for specific CPU
    pub fn set_cpu_governor(&mut self, cpu_id: u32, governor: Governor) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_set_cpu_governor(cpu_id, governor)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (cpu_id, governor);
            Err(IronError::UnsupportedPlatform(
                "Per-CPU governor control not supported on this platform".to_string(),
            ))
        }
    }

    /// Set minimum frequency (kHz) for all CPUs
    pub fn set_min_freq(&mut self, freq_khz: u64) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_set_min_freq(freq_khz)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = freq_khz;
            Err(IronError::UnsupportedPlatform(
                "Frequency control not supported on this platform".to_string(),
            ))
        }
    }

    /// Set maximum frequency (kHz) for all CPUs
    pub fn set_max_freq(&mut self, freq_khz: u64) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_set_max_freq(freq_khz)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = freq_khz;
            Err(IronError::UnsupportedPlatform(
                "Frequency control not supported on this platform".to_string(),
            ))
        }
    }

    /// Set CPU online/offline
    pub fn set_cpu_online(&mut self, cpu_id: u32, online: bool) -> Result<()> {
        if cpu_id == 0 {
            return Err(IronError::InvalidValue(
                "Cannot take CPU0 offline".to_string(),
            ));
        }

        #[cfg(target_os = "linux")]
        {
            self.linux_set_cpu_online(cpu_id, online)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (cpu_id, online);
            Err(IronError::UnsupportedPlatform(
                "CPU hotplug not supported on this platform".to_string(),
            ))
        }
    }

    /// Enable/disable turbo boost
    pub fn set_turbo(&mut self, enabled: bool) -> Result<()> {
        if !self.turbo.controllable {
            return Err(IronError::UnsupportedPlatform(
                "Turbo boost control not available".to_string(),
            ));
        }

        #[cfg(target_os = "linux")]
        {
            self.linux_set_turbo(enabled)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = enabled;
            Err(IronError::UnsupportedPlatform(
                "Turbo boost control not supported on this platform".to_string(),
            ))
        }
    }

    /// Set energy preference (Intel/AMD P-state)
    pub fn set_energy_preference(&mut self, pref: EnergyPreference) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_set_energy_preference(pref)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = pref;
            Err(IronError::UnsupportedPlatform(
                "Energy preference control not supported on this platform".to_string(),
            ))
        }
    }

    /// Enable/disable CPU idle state
    pub fn set_idle_state(&mut self, cpu_id: u32, state_idx: usize, enabled: bool) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_set_idle_state(cpu_id, state_idx, enabled)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (cpu_id, state_idx, enabled);
            Err(IronError::UnsupportedPlatform(
                "CPU idle state control not supported on this platform".to_string(),
            ))
        }
    }

    /// Discover CPU frequency capabilities
    fn discover(&mut self) -> Result<()> {
        self.cpus.clear();
        self.policies.clear();

        #[cfg(target_os = "linux")]
        {
            self.linux_discover()?;
        }

        #[cfg(target_os = "windows")]
        {
            self.windows_discover()?;
        }

        #[cfg(target_os = "macos")]
        {
            self.macos_discover()?;
        }

        Ok(())
    }

    // ==================== Linux Implementation ====================

    #[cfg(target_os = "linux")]
    fn linux_discover(&mut self) -> Result<()> {
        use std::fs;

        let cpu_path = std::path::Path::new("/sys/devices/system/cpu");
        if !cpu_path.exists() {
            return Err(IronError::UnsupportedPlatform(
                "No CPU sysfs interface found".to_string(),
            ));
        }

        // Detect P-state driver
        let driver_path = cpu_path.join("cpu0/cpufreq/scaling_driver");
        self.pstate_driver = fs::read_to_string(&driver_path)
            .map(|s| s.trim().to_string())
            .ok();

        // Discover turbo boost
        self.linux_discover_turbo()?;

        // Read CPU model from /proc/cpuinfo
        let cpu_model = self.read_cpu_model();

        // Enumerate CPUs
        for entry in fs::read_dir(cpu_path)
            .map_err(|e| IronError::System(format!("Failed to read CPU sysfs: {}", e)))?
        {
            let entry =
                entry.map_err(|e| IronError::System(format!("Failed to read entry: {}", e)))?;

            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("cpu") {
                continue;
            }

            // Parse CPU ID
            let id_str = name.strip_prefix("cpu").unwrap_or("");
            let id: u32 = match id_str.parse() {
                Ok(id) => id,
                Err(_) => continue,
            };

            let cpu_dir = entry.path();
            let mut cpu = CpuFreqInfo::new(id);
            cpu.sysfs_path = Some(cpu_dir.clone());
            cpu.model = cpu_model.clone();

            // Check online status
            let online_file = cpu_dir.join("online");
            if online_file.exists() {
                cpu.online = fs::read_to_string(&online_file)
                    .map(|s| s.trim() == "1")
                    .unwrap_or(true);
            }

            if !cpu.online {
                self.cpus.push(cpu);
                continue;
            }

            let cpufreq_dir = cpu_dir.join("cpufreq");
            if !cpufreq_dir.exists() {
                self.cpus.push(cpu);
                continue;
            }

            // Read scaling driver
            cpu.scaling_driver = fs::read_to_string(cpufreq_dir.join("scaling_driver"))
                .map(|s| s.trim().to_string())
                .ok();

            // Read current frequency
            if let Ok(freq_str) = fs::read_to_string(cpufreq_dir.join("scaling_cur_freq")) {
                if let Ok(freq) = freq_str.trim().parse::<u64>() {
                    cpu.current_freq_khz = freq;
                    cpu.current_freq_mhz = (freq / 1000) as u32;
                }
            }

            // Read min/max scaling frequencies
            if let Ok(freq_str) = fs::read_to_string(cpufreq_dir.join("scaling_min_freq")) {
                cpu.min_freq_khz = freq_str.trim().parse().unwrap_or(0);
            }
            if let Ok(freq_str) = fs::read_to_string(cpufreq_dir.join("scaling_max_freq")) {
                cpu.max_freq_khz = freq_str.trim().parse().unwrap_or(0);
            }

            // Read hardware min/max frequencies
            if let Ok(freq_str) = fs::read_to_string(cpufreq_dir.join("cpuinfo_min_freq")) {
                cpu.cpuinfo_min_freq_khz = freq_str.trim().parse().ok();
            }
            if let Ok(freq_str) = fs::read_to_string(cpufreq_dir.join("cpuinfo_max_freq")) {
                cpu.cpuinfo_max_freq_khz = freq_str.trim().parse().ok();
            }

            // Read base frequency (non-turbo)
            if let Ok(freq_str) = fs::read_to_string(cpufreq_dir.join("base_frequency")) {
                cpu.base_freq_khz = freq_str.trim().parse().ok();
            }

            // Read current governor
            if let Ok(gov_str) = fs::read_to_string(cpufreq_dir.join("scaling_governor")) {
                cpu.governor = gov_str
                    .trim()
                    .parse()
                    .unwrap_or(Governor::Unknown("unknown".to_string()));
            }

            // Read available governors
            if let Ok(govs_str) =
                fs::read_to_string(cpufreq_dir.join("scaling_available_governors"))
            {
                cpu.available_governors = govs_str
                    .split_whitespace()
                    .filter_map(|s| s.parse().ok())
                    .collect();
            }

            // Read available frequencies
            if let Ok(freqs_str) =
                fs::read_to_string(cpufreq_dir.join("scaling_available_frequencies"))
            {
                cpu.available_frequencies = freqs_str
                    .split_whitespace()
                    .filter_map(|s| s.parse().ok())
                    .collect();
            }

            // Read energy preference (Intel/AMD P-state)
            if let Ok(epp_str) =
                fs::read_to_string(cpufreq_dir.join("energy_performance_preference"))
            {
                cpu.energy_preference = epp_str.trim().parse().ok();
            }
            if let Ok(epp_avail) =
                fs::read_to_string(cpufreq_dir.join("energy_performance_available_preferences"))
            {
                cpu.available_energy_preferences = epp_avail
                    .split_whitespace()
                    .filter_map(|s| s.parse().ok())
                    .collect();
            }

            // Read idle states
            let cpuidle_dir = cpu_dir.join("cpuidle");
            if cpuidle_dir.exists() {
                self.linux_read_idle_states(&cpuidle_dir, &mut cpu)?;
            }

            self.cpus.push(cpu);
        }

        // Sort CPUs by ID
        self.cpus.sort_by_key(|c| c.id);

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_read_idle_states(
        &self,
        cpuidle_dir: &std::path::Path,
        cpu: &mut CpuFreqInfo,
    ) -> Result<()> {
        use std::fs;

        for entry in fs::read_dir(cpuidle_dir)
            .map_err(|e| IronError::System(format!("Failed to read cpuidle: {}", e)))?
        {
            let entry =
                entry.map_err(|e| IronError::System(format!("Failed to read entry: {}", e)))?;

            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("state") {
                continue;
            }

            let state_dir = entry.path();
            let mut state = CpuIdleState {
                name: String::new(),
                desc: None,
                latency_us: 0,
                residency_us: None,
                enabled: true,
                usage: 0,
                time_us: 0,
                disable: false,
            };

            // Read state name
            state.name = fs::read_to_string(state_dir.join("name"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| name.clone());

            // Read description
            state.desc = fs::read_to_string(state_dir.join("desc"))
                .map(|s| s.trim().to_string())
                .ok();

            // Read latency
            state.latency_us = fs::read_to_string(state_dir.join("latency"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);

            // Read residency
            state.residency_us = fs::read_to_string(state_dir.join("residency"))
                .ok()
                .and_then(|s| s.trim().parse().ok());

            // Read disable status
            state.disable = fs::read_to_string(state_dir.join("disable"))
                .map(|s| s.trim() == "1")
                .unwrap_or(false);
            state.enabled = !state.disable;

            // Read usage count
            state.usage = fs::read_to_string(state_dir.join("usage"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);

            // Read time in state
            state.time_us = fs::read_to_string(state_dir.join("time"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);

            cpu.idle_states.push(state);
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_discover_turbo(&mut self) -> Result<()> {
        use std::fs;

        // Intel P-state no_turbo
        let intel_turbo = std::path::Path::new("/sys/devices/system/cpu/intel_pstate/no_turbo");
        if intel_turbo.exists() {
            self.turbo.available = true;
            self.turbo.controllable = true;
            if let Ok(val) = fs::read_to_string(intel_turbo) {
                self.turbo.enabled = val.trim() == "0"; // no_turbo=0 means turbo enabled
            }
            return Ok(());
        }

        // AMD P-state boost
        let amd_boost = std::path::Path::new("/sys/devices/system/cpu/amd_pstate/cpufreq/boost");
        if amd_boost.exists() {
            self.turbo.available = true;
            self.turbo.controllable = true;
            if let Ok(val) = fs::read_to_string(amd_boost) {
                self.turbo.enabled = val.trim() == "1";
            }
            return Ok(());
        }

        // Generic cpufreq boost
        let generic_boost = std::path::Path::new("/sys/devices/system/cpu/cpufreq/boost");
        if generic_boost.exists() {
            self.turbo.available = true;
            self.turbo.controllable = true;
            if let Ok(val) = fs::read_to_string(generic_boost) {
                self.turbo.enabled = val.trim() == "1";
            }
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn read_cpu_model(&self) -> Option<String> {
        use std::fs;

        if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
            for line in cpuinfo.lines() {
                if line.starts_with("model name") {
                    if let Some(model) = line.split(':').nth(1) {
                        return Some(model.trim().to_string());
                    }
                }
            }
        }
        None
    }

    #[cfg(target_os = "linux")]
    fn linux_set_governor(&mut self, governor: Governor) -> Result<()> {
        use std::fs;

        let gov_str = governor.to_string();

        for cpu in &self.cpus {
            if !cpu.online {
                continue;
            }

            if let Some(ref path) = cpu.sysfs_path {
                let gov_file = path.join("cpufreq/scaling_governor");
                if gov_file.exists() {
                    fs::write(&gov_file, &gov_str).map_err(|e| {
                        IronError::System(format!(
                            "Failed to set governor for CPU{} (need root?): {}",
                            cpu.id, e
                        ))
                    })?;
                }
            }
        }

        self.refresh()?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_set_cpu_governor(&mut self, cpu_id: u32, governor: Governor) -> Result<()> {
        use std::fs;

        let cpu = self
            .cpus
            .iter()
            .find(|c| c.id == cpu_id)
            .ok_or_else(|| IronError::DeviceNotFound(format!("CPU{} not found", cpu_id)))?;

        if !cpu.online {
            return Err(IronError::InvalidValue(format!("CPU{} is offline", cpu_id)));
        }

        if let Some(ref path) = cpu.sysfs_path {
            let gov_file = path.join("cpufreq/scaling_governor");
            fs::write(&gov_file, governor.to_string()).map_err(|e| {
                IronError::System(format!("Failed to set governor for CPU{}: {}", cpu_id, e))
            })?;
        }

        self.refresh()?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_set_min_freq(&mut self, freq_khz: u64) -> Result<()> {
        use std::fs;

        for cpu in &self.cpus {
            if !cpu.online {
                continue;
            }

            if let Some(ref path) = cpu.sysfs_path {
                let freq_file = path.join("cpufreq/scaling_min_freq");
                if freq_file.exists() {
                    fs::write(&freq_file, freq_khz.to_string()).map_err(|e| {
                        IronError::System(format!(
                            "Failed to set min freq for CPU{}: {}",
                            cpu.id, e
                        ))
                    })?;
                }
            }
        }

        self.refresh()?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_set_max_freq(&mut self, freq_khz: u64) -> Result<()> {
        use std::fs;

        for cpu in &self.cpus {
            if !cpu.online {
                continue;
            }

            if let Some(ref path) = cpu.sysfs_path {
                let freq_file = path.join("cpufreq/scaling_max_freq");
                if freq_file.exists() {
                    fs::write(&freq_file, freq_khz.to_string()).map_err(|e| {
                        IronError::System(format!(
                            "Failed to set max freq for CPU{}: {}",
                            cpu.id, e
                        ))
                    })?;
                }
            }
        }

        self.refresh()?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_set_cpu_online(&mut self, cpu_id: u32, online: bool) -> Result<()> {
        use std::fs;

        let online_file = format!("/sys/devices/system/cpu/cpu{}/online", cpu_id);
        let value = if online { "1" } else { "0" };

        fs::write(&online_file, value).map_err(|e| {
            IronError::System(format!("Failed to set CPU{} online status: {}", cpu_id, e))
        })?;

        self.refresh()?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_set_turbo(&mut self, enabled: bool) -> Result<()> {
        use std::fs;

        // Intel P-state
        let intel_turbo = std::path::Path::new("/sys/devices/system/cpu/intel_pstate/no_turbo");
        if intel_turbo.exists() {
            let value = if enabled { "0" } else { "1" }; // no_turbo is inverted
            fs::write(intel_turbo, value)
                .map_err(|e| IronError::System(format!("Failed to set turbo: {}", e)))?;
            self.turbo.enabled = enabled;
            return Ok(());
        }

        // Generic boost
        let generic_boost = std::path::Path::new("/sys/devices/system/cpu/cpufreq/boost");
        if generic_boost.exists() {
            let value = if enabled { "1" } else { "0" };
            fs::write(generic_boost, value)
                .map_err(|e| IronError::System(format!("Failed to set turbo: {}", e)))?;
            self.turbo.enabled = enabled;
            return Ok(());
        }

        Err(IronError::UnsupportedPlatform(
            "No turbo control interface found".to_string(),
        ))
    }

    #[cfg(target_os = "linux")]
    fn linux_set_energy_preference(&mut self, pref: EnergyPreference) -> Result<()> {
        use std::fs;

        let pref_str = pref.to_string();

        for cpu in &self.cpus {
            if !cpu.online {
                continue;
            }

            if let Some(ref path) = cpu.sysfs_path {
                let epp_file = path.join("cpufreq/energy_performance_preference");
                if epp_file.exists() {
                    fs::write(&epp_file, &pref_str).map_err(|e| {
                        IronError::System(format!("Failed to set EPP for CPU{}: {}", cpu.id, e))
                    })?;
                }
            }
        }

        self.refresh()?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_set_idle_state(&mut self, cpu_id: u32, state_idx: usize, enabled: bool) -> Result<()> {
        use std::fs;

        let disable_file = format!(
            "/sys/devices/system/cpu/cpu{}/cpuidle/state{}/disable",
            cpu_id, state_idx
        );

        let value = if enabled { "0" } else { "1" };
        fs::write(&disable_file, value).map_err(|e| {
            IronError::System(format!(
                "Failed to set idle state{} for CPU{}: {}",
                state_idx, cpu_id, e
            ))
        })?;

        self.refresh()?;
        Ok(())
    }

    // ==================== Windows Implementation ====================

    #[cfg(target_os = "windows")]
    fn windows_discover(&mut self) -> Result<()> {
        use windows::Win32::System::SystemInformation::*;

        // Get processor count
        let mut sys_info = SYSTEM_INFO::default();
        unsafe {
            GetSystemInfo(&mut sys_info);
        }
        let num_cpus = sys_info.dwNumberOfProcessors;

        // Try WMI for detailed CPU info
        let (name, current_mhz, max_mhz) = windows_query_wmi_cpu();

        // Get base frequency from registry
        let base_freq_mhz = windows_get_base_frequency();

        // Get current power scheme
        let (power_plan, governor) = windows_get_power_scheme();

        // Create CPU entries
        for id in 0..num_cpus {
            let mut cpu = CpuFreqInfo::new(id);
            cpu.online = true;
            cpu.governor = governor.clone();

            // Apply WMI info if available
            if let Some(current) = current_mhz {
                cpu.model = name.clone();
                cpu.current_freq_mhz = current;
                cpu.max_freq_khz = (max_mhz.unwrap_or(current) as u64) * 1000;
                cpu.current_freq_khz = (cpu.current_freq_mhz as u64) * 1000;
                // Windows exposes no minimum core frequency here. This was
                // `max_freq_khz / 4`, which is not a property of any CPU — it just
                // produced a plausible-looking number. Zero means "not read".
                cpu.min_freq_khz = 0;
                cpu.scaling_driver = Some("windows-wmi".to_string());
                cpu.available_governors.push(governor.clone());
            } else if base_freq_mhz > 0 {
                // Fallback to base frequency from registry
                cpu.current_freq_mhz = base_freq_mhz;
                cpu.current_freq_khz = (base_freq_mhz as u64) * 1000;
                cpu.max_freq_khz = cpu.current_freq_khz;
                cpu.min_freq_khz = 0; // Not read — see above
            }

            self.cpus.push(cpu);
        }

        // Set system policy
        if let Some(plan_name) = power_plan {
            self.policies.push(CpuFreqPolicy {
                id: 0,
                affected_cpus: (0..num_cpus).collect(),
                related_cpus: (0..num_cpus).collect(),
                governor: governor.clone(),
                min_freq_khz: self.cpus.first().map(|c| c.min_freq_khz).unwrap_or(0),
                max_freq_khz: self.cpus.first().map(|c| c.max_freq_khz).unwrap_or(0),
                scaling_driver: Some(plan_name),
            });
        }

        // Try to detect turbo
        if let (Some(_current), Some(max)) = (current_mhz, max_mhz) {
            let base = windows_get_base_frequency();
            if max > base {
                self.turbo = TurboStatus {
                    available: true,
                    enabled: true,
                    controllable: false,
                    boost_mhz: Some(max - base),
                };
            }
        }

        Ok(())
    }

    #[cfg(target_os = "windows")]
    #[allow(dead_code)]
    fn windows_read_power_scheme(&mut self) -> Result<()> {
        // Power scheme already read in windows_discover
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn windows_set_governor(&mut self, governor: Governor) -> Result<()> {
        // On Windows, this would set the power plan
        // High Performance = Performance governor equivalent
        // Balanced = Schedutil/Ondemand equivalent
        // Power Saver = Powersave equivalent

        let _ = governor;
        Err(IronError::UnsupportedPlatform(
            "Use Windows Power Options to change power plan".to_string(),
        ))
    }

    // ==================== macOS Implementation ====================

    #[cfg(target_os = "macos")]
    fn macos_discover(&mut self) -> Result<()> {
        use std::process::Command;

        // Get CPU count
        let output = Command::new("sysctl").args(["-n", "hw.ncpu"]).output();

        if let Ok(output) = output {
            if let Ok(count_str) = String::from_utf8(output.stdout) {
                if let Ok(count) = count_str.trim().parse::<u32>() {
                    for id in 0..count {
                        let mut cpu = CpuFreqInfo::new(id);
                        cpu.online = true;
                        self.cpus.push(cpu);
                    }
                }
            }
        }

        // Get CPU frequency (limited on modern Macs)
        let output = Command::new("sysctl")
            .args(["-n", "hw.cpufrequency_max"])
            .output();

        if let Ok(output) = output {
            if let Ok(freq_str) = String::from_utf8(output.stdout) {
                if let Ok(freq) = freq_str.trim().parse::<u64>() {
                    for cpu in &mut self.cpus {
                        cpu.max_freq_khz = freq / 1000;
                        cpu.current_freq_khz = freq / 1000; // Approximation
                        cpu.current_freq_mhz = (freq / 1_000_000) as u32;
                    }
                }
            }
        }

        Ok(())
    }
}

// ==================== Windows Helper Functions ====================

/// Query WMI for CPU information (name, current/max MHz)
#[cfg(target_os = "windows")]
fn windows_query_wmi_cpu() -> (Option<String>, Option<u32>, Option<u32>) {
    use wmi::{COMLibrary, WMIConnection};

    #[derive(serde::Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct Win32Processor {
        name: Option<String>,
        current_clock_speed: Option<u32>,
        max_clock_speed: Option<u32>,
    }

    if let Ok(com) = COMLibrary::new() {
        if let Ok(wmi) = WMIConnection::new(com) {
            if let Ok(processors) = wmi.query::<Win32Processor>() {
                if let Some(p) = processors.first() {
                    return (p.name.clone(), p.current_clock_speed, p.max_clock_speed);
                }
            }
        }
    }

    (None, None, None)
}

/// Get base CPU frequency from Windows registry
#[cfg(target_os = "windows")]
fn windows_get_base_frequency() -> u32 {
    use winreg::enums::*;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(cpu_key) = hklm.open_subkey("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0") {
        if let Ok(mhz) = cpu_key.get_value::<u32, _>("~MHz") {
            return mhz;
        }
    }
    0
}

/// Get Windows power scheme and map to Governor
#[cfg(target_os = "windows")]
fn windows_get_power_scheme() -> (Option<String>, Governor) {
    use std::process::Command;

    // Use powercfg to get active scheme
    if let Ok(output) = Command::new("powercfg").args(["/getactivescheme"]).output() {
        if output.status.success() {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                let lower = stdout.to_lowercase();

                // Look for well-known scheme names
                if lower.contains("high performance") {
                    return (Some("High Performance".to_string()), Governor::Performance);
                } else if lower.contains("balanced") {
                    return (Some("Balanced".to_string()), Governor::Schedutil);
                } else if lower.contains("power saver") {
                    return (Some("Power Saver".to_string()), Governor::Powersave);
                }

                // Extract scheme name from output (format: "Power Scheme GUID: xxx  (Name)")
                if let Some(start) = stdout.find('(') {
                    if let Some(end) = stdout.find(')') {
                        let name = stdout[start + 1..end].trim().to_string();
                        return (Some(name), Governor::Unknown("custom".to_string()));
                    }
                }
            }
        }
    }

    (None, Governor::Unknown("unknown".to_string()))
}

/// Summary of CPU frequency status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuFreqSummary {
    /// Total CPUs
    pub total_cpus: usize,
    /// Online CPUs
    pub online_cpus: usize,
    /// Current governor
    pub governor: Option<String>,
    /// Average frequency (MHz)
    pub avg_freq_mhz: u32,
    /// Maximum frequency seen (MHz)
    pub max_freq_mhz: u32,
    /// Minimum frequency seen (MHz)
    pub min_freq_mhz: u32,
    /// Turbo enabled
    pub turbo_enabled: bool,
    /// CPU model
    pub cpu_model: Option<String>,
    /// Scaling driver
    pub scaling_driver: Option<String>,
}

/// Get a summary of CPU frequency status
pub fn cpufreq_summary() -> Result<CpuFreqSummary> {
    let monitor = CpuFreqMonitor::new()?;
    let cpus = monitor.cpus();

    let online_cpus: Vec<_> = cpus.iter().filter(|c| c.online).collect();

    let total_freq: u64 = online_cpus.iter().map(|c| c.current_freq_khz).sum();
    let avg_freq = if online_cpus.is_empty() {
        0
    } else {
        (total_freq / online_cpus.len() as u64 / 1000) as u32
    };

    let max_freq = online_cpus
        .iter()
        .map(|c| c.current_freq_mhz)
        .max()
        .unwrap_or(0);

    let min_freq = online_cpus
        .iter()
        .map(|c| c.current_freq_mhz)
        .min()
        .unwrap_or(0);

    Ok(CpuFreqSummary {
        total_cpus: cpus.len(),
        online_cpus: online_cpus.len(),
        governor: monitor.current_governor().map(|g| g.to_string()),
        avg_freq_mhz: avg_freq,
        max_freq_mhz: max_freq,
        min_freq_mhz: min_freq,
        turbo_enabled: monitor.turbo_status().enabled,
        cpu_model: online_cpus.first().and_then(|c| c.model.clone()),
        scaling_driver: online_cpus.first().and_then(|c| c.scaling_driver.clone()),
    })
}

/// Get list of all CPU frequency info
pub fn list_cpus() -> Result<Vec<CpuFreqInfo>> {
    let monitor = CpuFreqMonitor::new()?;
    Ok(monitor.cpus.clone())
}

/// Get available governors
pub fn available_governors() -> Result<Vec<Governor>> {
    let monitor = CpuFreqMonitor::new()?;
    Ok(monitor.available_governors())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_governor_display_roundtrip() {
        let governors = vec![
            Governor::Performance,
            Governor::Powersave,
            Governor::Ondemand,
            Governor::Conservative,
            Governor::Userspace,
            Governor::Schedutil,
            Governor::IntelPstate,
            Governor::AmdPstate,
            Governor::Interactive,
        ];
        for gov in governors {
            let s = gov.to_string();
            let parsed: Governor = s.parse().unwrap();
            assert_eq!(parsed, gov);
        }
    }

    #[test]
    fn test_governor_unknown_variant() {
        let gov: Governor = "custom_gov".parse().unwrap();
        assert_eq!(gov, Governor::Unknown("custom_gov".to_string()));
        assert_eq!(gov.to_string(), "custom_gov");
    }

    #[test]
    fn test_energy_preference_display() {
        assert_eq!(EnergyPreference::Performance.to_string(), "performance");
        assert_eq!(
            EnergyPreference::BalancePerformance.to_string(),
            "balance_performance"
        );
        assert_eq!(EnergyPreference::BalancePower.to_string(), "balance_power");
        assert_eq!(EnergyPreference::Power.to_string(), "power");
    }

    #[test]
    fn test_governor_serde_roundtrip() {
        let gov = Governor::Schedutil;
        let json = serde_json::to_string(&gov).unwrap();
        let parsed: Governor = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, gov);

        let gov_unk = Governor::Unknown("turbo".into());
        let json = serde_json::to_string(&gov_unk).unwrap();
        let parsed: Governor = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, gov_unk);
    }

    #[test]
    fn test_energy_preference_serde_roundtrip() {
        for pref in &[
            EnergyPreference::Performance,
            EnergyPreference::BalancePerformance,
            EnergyPreference::BalancePower,
            EnergyPreference::Power,
        ] {
            let json = serde_json::to_string(pref).unwrap();
            let parsed: EnergyPreference = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, pref);
        }
    }

    /// "Cannot tell" and "not boosting" must not be the same answer.
    ///
    /// `is_turbo` returned `bool`. With no `base_frequency` it returned
    /// `false`, and with a maximum but no base it guessed from ">95% of max",
    /// which is not a test for turbo: a part whose non-turbo ceiling *is* its
    /// `cpuinfo_max_freq` reads as boosting whenever it is busy.
    #[test]
    fn turbo_is_unknown_without_a_base_frequency() {
        let mut cpu = CpuFreqInfo::new(0);
        cpu.current_freq_khz = 4_400_000;
        cpu.cpuinfo_max_freq_khz = Some(4_400_000);
        cpu.base_freq_khz = None;

        assert_eq!(
            cpu.is_turbo(),
            None,
            "at 100% of max with no base frequency, turbo is unknowable"
        );

        // Above base is boosting.
        cpu.base_freq_khz = Some(3_700_000);
        assert_eq!(cpu.is_turbo(), Some(true));

        // At or below base is not.
        cpu.current_freq_khz = 3_700_000;
        assert_eq!(cpu.is_turbo(), Some(false));
    }
}
