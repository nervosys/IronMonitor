//! Voltage regulator monitoring.
//!
//! Enumerates voltage regulators exposed by the Linux regulator framework,
//! reporting rail name, state (enabled/disabled), voltage/current limits,
//! and consumer associations.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/class/regulator/`
//! - **Windows / macOS**: Not available (no comparable subsystem)

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// Regulator state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegulatorState {
    Enabled,
    Disabled,
    Unknown,
}

impl std::fmt::Display for RegulatorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enabled => write!(f, "enabled"),
            Self::Disabled => write!(f, "disabled"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Regulator operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegulatorMode {
    Fast,
    Normal,
    Idle,
    Standby,
    Unknown,
}

impl std::fmt::Display for RegulatorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fast => write!(f, "fast"),
            Self::Normal => write!(f, "normal"),
            Self::Idle => write!(f, "idle"),
            Self::Standby => write!(f, "standby"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Regulator type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegulatorType {
    /// Fixed voltage.
    Voltage,
    /// Adjustable voltage.
    Adjustable,
    /// Current regulator.
    Current,
    Unknown,
}

/// Information about a single voltage regulator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoltageRegulatorInfo {
    /// Regulator identifier (e.g. "regulator.0").
    pub id: String,
    /// Regulator supply name (the `name` attribute), or `None` where it was not
    /// read. The sysfs id used to be substituted here, so an unread name looked
    /// like a supply called `regulator.0`.
    pub name: Option<String>,
    /// Current state.
    pub state: RegulatorState,
    /// Operating mode.
    pub mode: RegulatorMode,
    /// Type.
    pub regulator_type: RegulatorType,
    /// Current voltage in microvolts (if readable).
    pub voltage_uv: Option<u64>,
    /// Minimum voltage in microvolts.
    pub min_voltage_uv: Option<u64>,
    /// Maximum voltage in microvolts.
    pub max_voltage_uv: Option<u64>,
    /// Current in microamps (if readable).
    pub current_ua: Option<u64>,
    /// Number of consumers using this regulator, or `None` where `num_users`
    /// was not read.
    ///
    /// The four reads above this one in the reader -- `microvolts`,
    /// `min_microvolts`, `max_microvolts`, `microamps` -- each keep the `Option`
    /// that `read_sysfs_*` returns. This was the fifth, on the next line, and
    /// alone discarded it with `.unwrap_or(0)`. Zero consumers is a real state
    /// for a regulator, so an unread count read as an idle one.
    pub num_users: Option<u32>,
    /// Whether the regulator is always on. Always `None` today.
    ///
    /// This used to be read from `suspend_mem_state == "enabled"`, which says
    /// what the regulator is set to during suspend-to-memory, not whether it
    /// stays on. The regulator sysfs ABI has no always-on attribute (the
    /// `regulator-always-on` property stays in the device tree), so nothing
    /// here can read it.
    pub always_on: Option<bool>,
    /// The regulator's `suspend_mem_state` attribute, verbatim, or `None`
    /// where the attribute is absent.
    pub suspend_mem_state: Option<String>,
    /// Power in microwatts (voltage * current if both available).
    pub power_uw: Option<u64>,
}

impl VoltageRegulatorInfo {
    /// Voltage in volts.
    pub fn voltage_v(&self) -> Option<f64> {
        self.voltage_uv.map(|uv| uv as f64 / 1_000_000.0)
    }

    /// Current in amps.
    pub fn current_a(&self) -> Option<f64> {
        self.current_ua.map(|ua| ua as f64 / 1_000_000.0)
    }

    /// Power in watts.
    pub fn power_w(&self) -> Option<f64> {
        self.power_uw.map(|uw| uw as f64 / 1_000_000.0)
    }

    /// Whether voltage is within the specified min/max limits.
    pub fn voltage_in_range(&self) -> Option<bool> {
        match (self.voltage_uv, self.min_voltage_uv, self.max_voltage_uv) {
            (Some(v), Some(min), Some(max)) => Some(v >= min && v <= max),
            _ => None,
        }
    }
}

/// Summary of all regulators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoltageRegulatorOverview {
    /// All regulators.
    pub regulators: Vec<VoltageRegulatorInfo>,
    /// Total count.
    pub total_count: u32,
    /// Enabled count.
    pub enabled_count: u32,
    /// Regulators with out-of-range voltage.
    pub out_of_range_count: u32,
    /// Estimated power in microwatts: the sum of `power_uw` over enabled
    /// regulators whose voltage and current were both read. A lower bound, since
    /// an enabled regulator missing either reading contributes nothing. `None`
    /// when no regulator contributed.
    ///
    /// This was named `total_power_uw` and summed every regulator regardless of
    /// state, so a disabled rail's last-set voltage counted as draw.
    pub estimated_power_uw: Option<u64>,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// Voltage regulator monitor.
pub struct VoltageRegulatorMonitor {
    overview: VoltageRegulatorOverview,
}

impl VoltageRegulatorMonitor {
    /// Create a new voltage regulator monitor.
    pub fn new() -> Result<Self, IronError> {
        let overview = Self::scan()?;
        Ok(Self { overview })
    }

    /// Refresh.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.overview = Self::scan()?;
        Ok(())
    }

    /// Get overview.
    pub fn overview(&self) -> &VoltageRegulatorOverview {
        &self.overview
    }

    /// Get regulators.
    pub fn regulators(&self) -> &[VoltageRegulatorInfo] {
        &self.overview.regulators
    }

    /// Get regulator by name.
    pub fn regulator_by_name(&self, name: &str) -> Option<&VoltageRegulatorInfo> {
        self.overview
            .regulators
            .iter()
            .find(|r| r.name.as_deref() == Some(name))
    }

    /// Get enabled regulators.
    pub fn enabled_regulators(&self) -> Vec<&VoltageRegulatorInfo> {
        self.overview
            .regulators
            .iter()
            .filter(|r| r.state == RegulatorState::Enabled)
            .collect()
    }

    #[cfg(target_os = "linux")]
    fn scan() -> Result<VoltageRegulatorOverview, IronError> {
        Self::scan_root(std::path::Path::new("/sys/class/regulator"))
    }

    /// Read the regulators under `reg_path`, normally `/sys/class/regulator`.
    #[cfg(target_os = "linux")]
    fn scan_root(reg_path: &std::path::Path) -> Result<VoltageRegulatorOverview, IronError> {
        // A missing directory means the framework exposes nothing to read. That
        // is a failed read, not a machine with zero regulators.
        if !reg_path.exists() {
            return Err(IronError::NotFound(format!(
                "{} does not exist, so voltage regulators cannot be read",
                reg_path.display()
            )));
        }

        let entries = std::fs::read_dir(reg_path).map_err(IronError::Io)?;
        let mut regulators = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let id = entry.file_name().to_string_lossy().to_string();

            // An unread name stays `None`; the sysfs id is not a supply name.
            let name = Self::read_sysfs_string(&path.join("name")).filter(|s| !s.is_empty());

            let state = match Self::read_sysfs_string(&path.join("state")).as_deref() {
                Some("enabled") => RegulatorState::Enabled,
                Some("disabled") => RegulatorState::Disabled,
                _ => RegulatorState::Unknown,
            };

            let mode = match Self::read_sysfs_string(&path.join("opmode")).as_deref() {
                Some("fast") => RegulatorMode::Fast,
                Some("normal") => RegulatorMode::Normal,
                Some("idle") => RegulatorMode::Idle,
                Some("standby") => RegulatorMode::Standby,
                _ => RegulatorMode::Unknown,
            };

            let voltage_uv = Self::read_sysfs_u64(&path.join("microvolts"));
            let min_voltage_uv = Self::read_sysfs_u64(&path.join("min_microvolts"));
            let max_voltage_uv = Self::read_sysfs_u64(&path.join("max_microvolts"));
            let current_ua = Self::read_sysfs_u64(&path.join("microamps"));
            let num_users = Self::read_sysfs_u32(&path.join("num_users"));

            let regulator_type = if min_voltage_uv.is_some() && max_voltage_uv.is_some() {
                if min_voltage_uv == max_voltage_uv {
                    RegulatorType::Voltage
                } else {
                    RegulatorType::Adjustable
                }
            } else if current_ua.is_some() {
                RegulatorType::Current
            } else {
                RegulatorType::Unknown
            };

            let suspend_mem_state = Self::read_sysfs_string(&path.join("suspend_mem_state"));

            let power_uw = match (voltage_uv, current_ua) {
                (Some(v), Some(c)) => Some(v * c / 1_000_000),
                _ => None,
            };

            regulators.push(VoltageRegulatorInfo {
                id,
                name,
                state,
                mode,
                regulator_type,
                voltage_uv,
                min_voltage_uv,
                max_voltage_uv,
                current_ua,
                num_users,
                always_on: None,
                suspend_mem_state,
                power_uw,
            });
        }

        regulators.sort_by(|a, b| a.id.cmp(&b.id));

        let total = regulators.len() as u32;
        let enabled = regulators
            .iter()
            .filter(|r| r.state == RegulatorState::Enabled)
            .count() as u32;
        let out_of_range = regulators
            .iter()
            .filter(|r| r.voltage_in_range() == Some(false))
            .count() as u32;
        // Only enabled regulators draw power. A disabled rail's voltage is the
        // level it would use if switched on, not a draw.
        let contributing: Vec<u64> = regulators
            .iter()
            .filter(|r| r.state == RegulatorState::Enabled)
            .filter_map(|r| r.power_uw)
            .collect();
        let estimated_power = (!contributing.is_empty()).then(|| contributing.iter().sum());

        let mut recommendations = Vec::new();
        if out_of_range > 0 {
            recommendations.push(format!(
                "{} regulator(s) have voltage outside specified range",
                out_of_range
            ));
        }

        Ok(VoltageRegulatorOverview {
            regulators,
            total_count: total,
            enabled_count: enabled,
            out_of_range_count: out_of_range,
            estimated_power_uw: estimated_power,
            recommendations,
        })
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_string(path: &std::path::Path) -> Option<String> {
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_u64(path: &std::path::Path) -> Option<u64> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_u32(path: &std::path::Path) -> Option<u32> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    #[cfg(not(target_os = "linux"))]
    fn scan() -> Result<VoltageRegulatorOverview, IronError> {
        // Returning an empty overview here made "this platform cannot
        // answer" indistinguishable from "this machine has none", which
        // is the same defect RAPL shipped once. The reason travels with
        // the error so a caller can report it.
        Err(IronError::UnsupportedPlatform(
            "voltage regulators are read from `/sys/class/regulator`, which this platform does not expose"
                .into(),
        ))
    }

    fn empty_overview() -> VoltageRegulatorOverview {
        VoltageRegulatorOverview {
            regulators: Vec::new(),
            total_count: 0,
            enabled_count: 0,
            out_of_range_count: 0,
            estimated_power_uw: None,
            recommendations: Vec::new(),
        }
    }
}

impl Default for VoltageRegulatorMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            overview: Self::empty_overview(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_display() {
        assert_eq!(RegulatorState::Enabled.to_string(), "enabled");
        assert_eq!(RegulatorState::Disabled.to_string(), "disabled");
    }

    #[test]
    fn test_voltage_conversion() {
        let info = VoltageRegulatorInfo {
            id: "regulator.0".into(),
            name: Some("vdd_cpu".into()),
            state: RegulatorState::Enabled,
            mode: RegulatorMode::Normal,
            regulator_type: RegulatorType::Adjustable,
            voltage_uv: Some(1_100_000),
            min_voltage_uv: Some(800_000),
            max_voltage_uv: Some(1_300_000),
            current_ua: Some(5_000_000),
            num_users: Some(1),
            always_on: None,
            suspend_mem_state: None,
            power_uw: Some(5_500_000),
        };
        assert!((info.voltage_v().unwrap() - 1.1).abs() < 0.001);
        assert!((info.current_a().unwrap() - 5.0).abs() < 0.001);
        assert!(info.voltage_in_range() == Some(true));
    }

    #[test]
    fn test_out_of_range() {
        let info = VoltageRegulatorInfo {
            id: "regulator.1".into(),
            name: Some("vdd_mem".into()),
            state: RegulatorState::Enabled,
            mode: RegulatorMode::Normal,
            regulator_type: RegulatorType::Adjustable,
            voltage_uv: Some(500_000),
            min_voltage_uv: Some(800_000),
            max_voltage_uv: Some(1_300_000),
            current_ua: None,
            num_users: Some(0),
            always_on: None,
            suspend_mem_state: None,
            power_uw: None,
        };
        assert_eq!(info.voltage_in_range(), Some(false));
    }

    #[test]
    fn test_monitor_default() {
        let monitor = VoltageRegulatorMonitor::default();
        let _overview = monitor.overview();
    }

    #[test]
    fn test_serialization() {
        let info = VoltageRegulatorInfo {
            id: "regulator.0".into(),
            name: Some("vdd_soc".into()),
            state: RegulatorState::Enabled,
            mode: RegulatorMode::Fast,
            regulator_type: RegulatorType::Voltage,
            voltage_uv: Some(900_000),
            min_voltage_uv: Some(900_000),
            max_voltage_uv: Some(900_000),
            current_ua: None,
            num_users: Some(2),
            always_on: None,
            suspend_mem_state: Some("disabled".into()),
            power_uw: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("vdd_soc"));
        let _: VoltageRegulatorInfo = serde_json::from_str(&json).unwrap();
    }

    /// A throwaway `/sys/class/regulator` tree. Each regulator is a directory
    /// holding the given attribute files. Callers remove it.
    #[cfg(target_os = "linux")]
    fn regulator_root(tag: &str, regs: &[(&str, &[(&str, &str)])]) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("ironmon-regulator-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for (id, files) in regs {
            let dir = root.join(id);
            std::fs::create_dir_all(&dir).unwrap();
            for (file, body) in files.iter() {
                std::fs::write(dir.join(file), body).unwrap();
            }
        }
        root
    }

    /// Scan a fixture tree and remove it afterwards.
    #[cfg(target_os = "linux")]
    fn scan_fixture(root: &std::path::Path) -> VoltageRegulatorOverview {
        let overview = VoltageRegulatorMonitor::scan_root(root).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        overview
    }

    /// A missing regulator tree is a failed read, not an empty list.
    #[cfg(target_os = "linux")]
    #[test]
    fn missing_regulator_tree_is_an_error_not_zero_regulators() {
        let root =
            std::env::temp_dir().join(format!("ironmon-regulator-absent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(VoltageRegulatorMonitor::scan_root(&root).is_err());
    }

    /// An unread `name` is `None`, not the sysfs id `regulator.0`.
    #[cfg(target_os = "linux")]
    #[test]
    fn unread_name_is_none_not_the_sysfs_id() {
        let root = regulator_root(
            "name",
            &[
                ("regulator.0", &[("state", "enabled\n")]),
                (
                    "regulator.1",
                    &[("name", "vdd_cpu\n"), ("state", "enabled\n")],
                ),
            ],
        );
        let overview = scan_fixture(&root);

        let unnamed = overview
            .regulators
            .iter()
            .find(|r| r.id == "regulator.0")
            .unwrap();
        assert_eq!(unnamed.name, None);
        let named = overview
            .regulators
            .iter()
            .find(|r| r.id == "regulator.1")
            .unwrap();
        assert_eq!(named.name.as_deref(), Some("vdd_cpu"));
    }

    /// Power is summed over enabled regulators only. The disabled regulator's
    /// readings and the enabled one missing a current contribute nothing.
    #[cfg(target_os = "linux")]
    #[test]
    fn estimated_power_counts_only_enabled_regulators() {
        let root = regulator_root(
            "power",
            &[
                // Enabled, both readings: 1.1 V * 5 A = 5.5 W.
                (
                    "regulator.0",
                    &[
                        ("name", "vdd_cpu\n"),
                        ("state", "enabled\n"),
                        ("microvolts", "1100000\n"),
                        ("microamps", "5000000\n"),
                    ],
                ),
                // Disabled: its last-set voltage and a current must not count.
                (
                    "regulator.1",
                    &[
                        ("name", "vdd_mem\n"),
                        ("state", "disabled\n"),
                        ("microvolts", "1200000\n"),
                        ("microamps", "3000000\n"),
                    ],
                ),
                // Enabled but no current read: contributes nothing.
                (
                    "regulator.2",
                    &[
                        ("name", "vdd_io\n"),
                        ("state", "enabled\n"),
                        ("microvolts", "1800000\n"),
                    ],
                ),
            ],
        );
        let overview = scan_fixture(&root);

        assert_eq!(overview.estimated_power_uw, Some(5_500_000));
    }

    /// No contributing regulator is no estimate, not a zero draw.
    #[cfg(target_os = "linux")]
    #[test]
    fn estimated_power_is_none_when_no_regulator_contributes() {
        let root = regulator_root(
            "power-none",
            &[(
                "regulator.0",
                &[
                    ("name", "vdd_mem\n"),
                    ("state", "disabled\n"),
                    ("microvolts", "1200000\n"),
                    ("microamps", "3000000\n"),
                ],
            )],
        );
        let overview = scan_fixture(&root);

        assert_eq!(overview.estimated_power_uw, None);
    }
}
