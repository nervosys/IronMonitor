//! Power profile and energy management monitoring.
//!
//! Reads OS power plans, CPU frequency governor, battery charge policy,
//! and provides inferred energy efficiency metrics.
//!
//! # Platform Support
//!
//! - **Linux**: cpufreq governors, TLP, power-profiles-daemon, `/sys/class/power_supply/`
//! - **Windows**: `powercfg`, `Win32_PowerPlan`
//! - **macOS**: `pmset`, System Preferences Energy Saver settings
//!
//! # Inference
//!
//! Infers system power behavior from governor + CPU frequency ranges + battery
//! status to classify the system as performance-focused, balanced, or power-saving.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::power_profile::PowerProfileMonitor;
//!
//! let monitor = PowerProfileMonitor::new().unwrap();
//! println!("Active profile: {:?}", monitor.active_profile());
//! println!("Inferred behavior: {:?}", monitor.inferred_behavior());
//! ```

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// Named power profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerProfile {
    Performance,
    Balanced,
    PowerSaver,
    UltraPowerSaver,
    Custom(String),
    Unknown,
}

/// CPU frequency governor (Linux).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuGovernor {
    Performance,
    Powersave,
    Ondemand,
    Conservative,
    Schedutil,
    Userspace,
    Other(String),
}

/// Battery charge policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargePolicy {
    /// Charge to 100%
    Full,
    /// Stop charging at threshold (e.g., 80%)
    Threshold(u8),
    /// Adaptive based on usage patterns
    Adaptive,
    /// No battery present
    NoBattery,
    Unknown,
}

/// Inferred system power behavior based on hardware + software config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredPowerBehavior {
    /// Classification
    pub classification: PowerProfile,
    /// Confidence (0.0-1.0)
    pub confidence: f32,
    /// Estimated idle power draw (watts)
    pub estimated_idle_watts: f32,
    /// Estimated max power draw (watts)
    pub estimated_max_watts: f32,
    /// Efficiency score (0-100, higher = more efficient configuration)
    pub efficiency_score: u8,
    /// Recommendations
    pub recommendations: Vec<String>,
}

/// Power plan info (Windows).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerPlanInfo {
    /// Plan GUID
    pub guid: String,
    /// Plan name
    pub name: String,
    /// Whether this plan is active
    pub active: bool,
}

/// CPU frequency configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuFreqConfig {
    /// Current governor
    pub governor: CpuGovernor,
    /// Current frequency (MHz)
    pub current_freq_mhz: u32,
    /// Minimum frequency (MHz)
    pub min_freq_mhz: u32,
    /// Maximum frequency (MHz)
    pub max_freq_mhz: u32,
    /// Base (P1) frequency if known
    pub base_freq_mhz: Option<u32>,
    /// Boost/turbo enabled
    pub boost_enabled: bool,
    /// Energy Performance Preference (Linux: /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference)
    pub energy_perf_preference: String,
}

/// Full power profile state.
pub struct PowerProfileMonitor {
    active_profile: PowerProfile,
    power_plans: Vec<PowerPlanInfo>,
    cpu_freq: Option<CpuFreqConfig>,
    charge_policy: ChargePolicy,
    inferred: Option<InferredPowerBehavior>,
    /// Whether the system is on AC power.
    ///
    /// `None` when it was not established. It was a `bool` initialised to
    /// `true`, so a machine where nothing had been read was on mains, and the
    /// Windows reader set it with `text != "1"` on the output of a PowerShell
    /// query -- an empty string from a failed query is not "1", so a failure
    /// also read as mains. `on_ac_power` gates every battery recommendation
    /// this module makes, so the failure mode was silence about a laptop's
    /// power settings while it ran on battery.
    pub on_ac_power: Option<bool>,
    /// Display brightness (0-100)
    pub display_brightness: Option<u8>,
    /// Auto-sleep timeout (seconds)
    pub sleep_timeout_secs: Option<u32>,
    /// Disk standby timeout (seconds)
    pub disk_standby_secs: Option<u32>,
}

impl PowerProfileMonitor {
    pub fn new() -> Result<Self, IronError> {
        let mut monitor = Self {
            active_profile: PowerProfile::Unknown,
            power_plans: Vec::new(),
            cpu_freq: None,
            charge_policy: ChargePolicy::Unknown,
            inferred: None,
            on_ac_power: None,
            display_brightness: None,
            sleep_timeout_secs: None,
            disk_standby_secs: None,
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.power_plans.clear();

        #[cfg(target_os = "linux")]
        self.refresh_linux()?;

        #[cfg(target_os = "windows")]
        self.refresh_windows()?;

        #[cfg(target_os = "macos")]
        self.refresh_macos()?;

        // Run inference
        self.inferred = Some(self.infer_behavior());

        Ok(())
    }

    pub fn active_profile(&self) -> &PowerProfile {
        &self.active_profile
    }

    pub fn power_plans(&self) -> &[PowerPlanInfo] {
        &self.power_plans
    }

    pub fn cpu_freq_config(&self) -> Option<&CpuFreqConfig> {
        self.cpu_freq.as_ref()
    }

    pub fn charge_policy(&self) -> &ChargePolicy {
        &self.charge_policy
    }

    pub fn inferred_behavior(&self) -> Option<&InferredPowerBehavior> {
        self.inferred.as_ref()
    }

    /// Infer power behavior from collected data.
    fn infer_behavior(&self) -> InferredPowerBehavior {
        let mut score = 50u8; // Start balanced
        let mut confidence = 0.5f32;
        let mut recommendations = Vec::new();

        // Governor analysis
        if let Some(ref freq) = self.cpu_freq {
            match &freq.governor {
                CpuGovernor::Performance => {
                    score = score.saturating_sub(20); // Less efficient
                    confidence += 0.15;
                }
                CpuGovernor::Powersave => {
                    score = score.saturating_add(20);
                    confidence += 0.15;
                }
                CpuGovernor::Schedutil => {
                    score = score.saturating_add(10); // Good default
                    confidence += 0.1;
                }
                CpuGovernor::Ondemand => {
                    score = score.saturating_add(5);
                    confidence += 0.1;
                }
                _ => {}
            }

            // Boost analysis
            if freq.boost_enabled {
                score = score.saturating_sub(5);
                if self.on_ac_power == Some(false) {
                    recommendations.push(
                        "Consider disabling CPU boost on battery for better battery life".into(),
                    );
                }
            }

            // Frequency range analysis
            if freq.max_freq_mhz > 0 && freq.min_freq_mhz > 0 {
                let ratio = freq.min_freq_mhz as f32 / freq.max_freq_mhz as f32;
                if ratio > 0.8 {
                    // Min freq locked high — wasteful
                    score = score.saturating_sub(10);
                    recommendations.push(
                        "CPU minimum frequency is high — consider lowering for power savings"
                            .into(),
                    );
                }
            }

            // EPP analysis
            match freq.energy_perf_preference.as_str() {
                "performance" => score = score.saturating_sub(10),
                "power" | "power_save" => score = score.saturating_add(10),
                "balance_performance" => {}
                "balance_power" => score = score.saturating_add(5),
                _ => {}
            }
        }

        // Battery / AC analysis
        if self.on_ac_power == Some(false) {
            match &self.active_profile {
                PowerProfile::Performance => {
                    recommendations.push(
                        "Running performance profile on battery — battery life will be reduced"
                            .into(),
                    );
                    score = score.saturating_sub(15);
                }
                PowerProfile::PowerSaver | PowerProfile::UltraPowerSaver => {
                    score = score.saturating_add(10);
                }
                _ => {}
            }
        }

        // Charge policy
        match &self.charge_policy {
            ChargePolicy::Threshold(t) if *t <= 80 => {
                score = score.saturating_add(5);
                confidence += 0.05;
            }
            ChargePolicy::Full => {
                recommendations.push(
                    "Consider setting a charge threshold (e.g., 80%) to extend battery lifespan"
                        .into(),
                );
            }
            _ => {}
        }

        // Display brightness
        if let Some(brightness) = self.display_brightness {
            if brightness > 80 && self.on_ac_power == Some(false) {
                recommendations.push(
                    "Display brightness is high on battery — consider reducing for power savings"
                        .into(),
                );
                score = score.saturating_sub(5);
            }
        }

        let classification = if score >= 70 {
            PowerProfile::PowerSaver
        } else if score >= 40 {
            PowerProfile::Balanced
        } else {
            PowerProfile::Performance
        };

        // Estimate power draw based on profile
        let (idle_watts, max_watts) = match &classification {
            PowerProfile::Performance => (25.0, 250.0),
            PowerProfile::Balanced => (15.0, 200.0),
            PowerProfile::PowerSaver => (8.0, 150.0),
            PowerProfile::UltraPowerSaver => (5.0, 100.0),
            _ => (15.0, 200.0),
        };

        InferredPowerBehavior {
            classification,
            confidence: confidence.min(1.0),
            estimated_idle_watts: idle_watts,
            estimated_max_watts: max_watts,
            efficiency_score: score.min(100),
            recommendations,
        }
    }

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) -> Result<(), IronError> {
        // Every source here is optional -- a kernel without cpufreq has no
        // governor, a desktop has no battery -- so nothing in this reader is
        // an enumeration whose failure means "no power profiles". The
        // signature is `Result` for consistency with the other two.
        // CPU frequency governor
        let gov = std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
            .unwrap_or_default()
            .trim()
            .to_string();

        let governor = match gov.as_str() {
            "performance" => CpuGovernor::Performance,
            "powersave" => CpuGovernor::Powersave,
            "ondemand" => CpuGovernor::Ondemand,
            "conservative" => CpuGovernor::Conservative,
            "schedutil" => CpuGovernor::Schedutil,
            "userspace" => CpuGovernor::Userspace,
            "" => CpuGovernor::Powersave,
            other => CpuGovernor::Other(other.to_string()),
        };

        let cur_freq =
            std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(0)
                / 1000; // kHz to MHz

        let min_freq =
            std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq")
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(0)
                / 1000;

        let max_freq =
            std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq")
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(0)
                / 1000;

        let base_freq =
            std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/base_frequency")
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .map(|f| f / 1000);

        let boost_enabled = std::fs::read_to_string("/sys/devices/system/cpu/cpufreq/boost")
            .or_else(|_| std::fs::read_to_string("/sys/devices/system/cpu/intel_pstate/no_turbo"))
            .map(|s| {
                let val = s.trim();
                // boost file: 1 = enabled; no_turbo: 0 = enabled (inverted)
                val == "1" || val == "0"
            })
            .unwrap_or(false);

        let epp = std::fs::read_to_string(
            "/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference",
        )
        .unwrap_or_default()
        .trim()
        .to_string();

        self.cpu_freq = Some(CpuFreqConfig {
            governor,
            current_freq_mhz: cur_freq,
            min_freq_mhz: min_freq,
            max_freq_mhz: max_freq,
            base_freq_mhz: base_freq,
            boost_enabled,
            energy_perf_preference: epp,
        });

        // power-profiles-daemon
        if let Ok(output) = std::process::Command::new("powerprofilesctl")
            .arg("get")
            .output()
        {
            let profile = String::from_utf8(output.stdout)
                .unwrap_or_default()
                .trim()
                .to_string();
            self.active_profile = match profile.as_str() {
                "performance" => PowerProfile::Performance,
                "balanced" => PowerProfile::Balanced,
                "power-saver" => PowerProfile::PowerSaver,
                _ => PowerProfile::Unknown,
            };
        } else {
            // Infer from governor
            self.active_profile = match &self.cpu_freq.as_ref().map(|f| &f.governor) {
                Some(CpuGovernor::Performance) => PowerProfile::Performance,
                Some(CpuGovernor::Powersave) => PowerProfile::PowerSaver,
                _ => PowerProfile::Balanced,
            };
        }

        // AC power status
        // `read_dir` yields a Result; the doubled loop was iterating that Result as
        // if it were a collection, which reads as two levels of directory when
        // there is only one.
        if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
            for e in entries.flatten() {
                let path = e.path();
                if let Ok(ptype) = std::fs::read_to_string(path.join("type")) {
                    if ptype.trim() == "Mains" {
                        if let Ok(online) = std::fs::read_to_string(path.join("online")) {
                            self.on_ac_power = Some(online.trim() == "1");
                        }
                    }
                }
            }
        }

        // Charge threshold (ThinkPad, ASUS, etc.)
        let threshold_paths = [
            "/sys/class/power_supply/BAT0/charge_control_end_threshold",
            "/sys/class/power_supply/BAT1/charge_control_end_threshold",
        ];
        for path in &threshold_paths {
            if let Ok(val) = std::fs::read_to_string(path) {
                if let Ok(t) = val.trim().parse::<u8>() {
                    self.charge_policy = if t >= 100 {
                        ChargePolicy::Full
                    } else {
                        ChargePolicy::Threshold(t)
                    };
                    break;
                }
            }
        }

        // Brightness
        // Only the first backlight is read — the loop ended in an unconditional
        // `break`, so it never iterated. `next()` says that outright.
        if let Some(entry) = std::fs::read_dir("/sys/class/backlight")
            .ok()
            .and_then(|mut entries| entries.next())
            .and_then(|e| e.ok())
        {
            let path = entry.path();
            let cur = std::fs::read_to_string(path.join("brightness"))
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok());
            let max = std::fs::read_to_string(path.join("max_brightness"))
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok());
            if let (Some(c), Some(m)) = (cur, max) {
                if m > 0 {
                    self.display_brightness = Some(((c as f32 / m as f32) * 100.0) as u8);
                }
            }
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), IronError> {
        // `powercfg /list` is the enumeration. It ships with Windows, so a
        // failure to run it is a failure -- and an empty plan list is what the
        // resolver publishes as `power.profile.<none>`, a claim that this
        // machine has no power plans.
        {
            let text = crate::core::command::capture("powercfg", &["/list"])?;
            for line in text.lines() {
                // "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced) *"
                if line.contains("GUID:") {
                    let active = line.ends_with('*');
                    // Extract GUID
                    if let Some(guid_start) = line.find("GUID:") {
                        let after = &line[guid_start + 6..];
                        let guid = after.split_whitespace().next().unwrap_or("").to_string();
                        // Extract name in parentheses
                        let name = after
                            .split('(')
                            .nth(1)
                            .and_then(|s| s.split(')').next())
                            .unwrap_or("")
                            .to_string();

                        if active {
                            self.active_profile =
                                if name.to_lowercase().contains("high performance")
                                    || name.to_lowercase().contains("ultimate")
                                {
                                    PowerProfile::Performance
                                } else if name.to_lowercase().contains("power saver") {
                                    PowerProfile::PowerSaver
                                } else if name.to_lowercase().contains("balanced") {
                                    PowerProfile::Balanced
                                } else {
                                    PowerProfile::Custom(name.clone())
                                };
                        }

                        self.power_plans.push(PowerPlanInfo { guid, name, active });
                    }
                }
            }
        }

        // AC power status.
        //
        // This spawned PowerShell for `(Get-CimInstance Win32_Battery).BatteryStatus`
        // and set `on_ac_power = text != "1"`. A desktop has no battery and the
        // query returns nothing, which is not "1"; a query that fails also
        // returns nothing, which is also not "1". `GetSystemPowerStatus` asks
        // the kernel directly and has a value for the third case:
        // `ACLineStatus` is 0 offline, 1 online, 255 unknown.
        {
            use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
            let mut status = SYSTEM_POWER_STATUS::default();
            if unsafe { GetSystemPowerStatus(&mut status) }.is_ok() {
                self.on_ac_power = match status.ACLineStatus {
                    0 => Some(false),
                    1 => Some(true),
                    _ => None,
                };
            }
        }

        // Brightness
        if let Ok(output) = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command",
                "(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness).CurrentBrightness"])
            .output()
        {
            if let Ok(b) = String::from_utf8(output.stdout).unwrap_or_default().trim().parse::<u8>()
            {
                self.display_brightness = Some(b);
            }
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), IronError> {
        // `pmset` ships with macOS, so a failure to run it is a failure.
        {
            let text = crate::core::command::capture("pmset", &["-g", "custom"])?;
            let mut sleep = None;
            let mut disk_sleep = None;
            let mut in_battery_section = false;

            for line in text.lines() {
                let line = line.trim();
                if line.starts_with("Battery Power:") {
                    in_battery_section = true;
                } else if line.starts_with("AC Power:") {
                    in_battery_section = false;
                }

                if line.starts_with("sleep") {
                    if let Some(val) = line.split_whitespace().nth(1) {
                        if let Ok(v) = val.parse::<u32>() {
                            sleep = Some(v * 60); // minutes to seconds
                        }
                    }
                }
                if line.starts_with("disksleep") {
                    if let Some(val) = line.split_whitespace().nth(1) {
                        if let Ok(v) = val.parse::<u32>() {
                            disk_sleep = Some(v * 60);
                        }
                    }
                }
            }

            self.sleep_timeout_secs = sleep;
            self.disk_standby_secs = disk_sleep;

            let _ = in_battery_section;
        }

        // Infer profile from pmset settings
        if let Ok(output) = std::process::Command::new("pmset").args(["-g"]).output() {
            let text = String::from_utf8(output.stdout).unwrap_or_default();
            let low_power = text
                .lines()
                .any(|l| l.contains("lowpowermode") && l.contains("1"));
            self.active_profile = if low_power {
                PowerProfile::PowerSaver
            } else {
                PowerProfile::Balanced
            };
        }

        // Check AC power
        if let Ok(output) = std::process::Command::new("pmset")
            .args(["-g", "ps"])
            .output()
        {
            let text = String::from_utf8(output.stdout).unwrap_or_default();
            self.on_ac_power = Some(text.contains("AC Power"));
        }
        Ok(())
    }
}

impl Default for PowerProfileMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            active_profile: PowerProfile::Unknown,
            power_plans: Vec::new(),
            cpu_freq: None,
            charge_policy: ChargePolicy::Unknown,
            inferred: None,
            on_ac_power: None,
            display_brightness: None,
            sleep_timeout_secs: None,
            disk_standby_secs: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructing the monitor either enumerates or says why it could not.
    ///
    /// This asserted `is_ok()`, which was true by construction until 6.0.0
    /// because `refresh` could not fail. Now that it can, the contract is
    /// weaker and more useful: **whatever happens, the caller can tell which
    /// happened.** A failure has to carry a reason, because a reason is the
    /// whole difference between "this machine has none" and "nobody looked".
    #[test]
    fn test_power_profile_creation() {
        match PowerProfileMonitor::new() {
            Ok(_monitor) => {}
            Err(e) => {
                let why = e.to_string();
                assert!(
                    why.len() > 10,
                    "enumeration failed without saying why: {why:?}"
                );
            }
        }
    }

    #[test]
    fn test_power_profile_default() {
        let monitor = PowerProfileMonitor::default();
        let _ = monitor.active_profile();
        let _ = monitor.inferred_behavior();
        let _ = monitor.charge_policy();
    }

    #[test]
    fn test_inference_performance() {
        let monitor = PowerProfileMonitor {
            active_profile: PowerProfile::Performance,
            power_plans: Vec::new(),
            cpu_freq: Some(CpuFreqConfig {
                governor: CpuGovernor::Performance,
                current_freq_mhz: 4500,
                min_freq_mhz: 4000,
                max_freq_mhz: 5000,
                base_freq_mhz: Some(3700),
                boost_enabled: true,
                energy_perf_preference: "performance".into(),
            }),
            charge_policy: ChargePolicy::Full,
            inferred: None,
            on_ac_power: Some(false), // On battery with performance = bad
            display_brightness: Some(100),
            sleep_timeout_secs: None,
            disk_standby_secs: None,
        };
        let behavior = monitor.infer_behavior();
        assert!(behavior.efficiency_score < 30); // Should be low efficiency
        assert!(!behavior.recommendations.is_empty());
    }

    #[test]
    fn test_inference_powersaver() {
        let monitor = PowerProfileMonitor {
            active_profile: PowerProfile::PowerSaver,
            power_plans: Vec::new(),
            cpu_freq: Some(CpuFreqConfig {
                governor: CpuGovernor::Powersave,
                current_freq_mhz: 800,
                min_freq_mhz: 400,
                max_freq_mhz: 3600,
                base_freq_mhz: Some(2400),
                boost_enabled: false,
                energy_perf_preference: "power".into(),
            }),
            charge_policy: ChargePolicy::Threshold(80),
            inferred: None,
            on_ac_power: Some(false),
            display_brightness: Some(40),
            sleep_timeout_secs: Some(300),
            disk_standby_secs: Some(600),
        };
        let behavior = monitor.infer_behavior();
        assert!(behavior.efficiency_score > 60); // Should be high efficiency
    }

    #[test]
    fn test_serialization() {
        let plan = PowerPlanInfo {
            guid: "381b4222-f694-41f0-9685-ff5bb260df2e".into(),
            name: "Balanced".into(),
            active: true,
        };
        let json = serde_json::to_string(&plan).unwrap();
        let _: PowerPlanInfo = serde_json::from_str(&json).unwrap();
    }
}
