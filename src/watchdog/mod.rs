//! Hardware and software watchdog timer monitoring.
//!
//! Enumerates watchdog devices, reports timeout configuration, pre-timeout
//! governors, identity, firmware version, and status flags.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/class/watchdog/`, `/dev/watchdog*`
//! - **Windows / macOS**: Basic detection only

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// Watchdog device type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WatchdogType {
    /// Hardware watchdog (e.g. iTCO, SP5100).
    Hardware,
    /// Software watchdog (softdog).
    Software,
    Unknown,
}

impl std::fmt::Display for WatchdogType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hardware => write!(f, "hardware"),
            Self::Software => write!(f, "software"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Watchdog status flags.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WatchdogStatus {
    /// Watchdog is active (currently ticking), or `None` where `state` was not
    /// read.
    ///
    /// `read_sysfs(..).unwrap_or_default()` gave an empty string, which is not
    /// `"active"`, so an unreadable `state` reported a watchdog that is not
    /// running -- the reassuring answer, asserted from nothing.
    pub active: Option<bool>,
    /// Watchdog has triggered at least once.
    ///
    /// Always `false`: no sysfs attribute reports this, and the field has
    /// never been fed by a reader. It is `bool` rather than `Option` because
    /// that is a fact about this crate, not about the device -- see the
    /// constructor, which is the only place it is set.
    pub triggered: bool,
    /// Did the watchdog cause the last reboot? `None` where `bootstatus` was
    /// not read.
    ///
    /// **A safety-relevant claim, and it defaulted to the comfortable one.**
    /// `unwrap_or(0)` then `!= 0` reported "the system was not reset by the
    /// watchdog" for any device whose `bootstatus` could not be read -- which
    /// is exactly the question someone investigating an unexplained reboot is
    /// asking.
    pub boot_triggered: Option<bool>,
}

/// Pre-timeout governor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreTimeoutGovernor {
    /// No pre-timeout action.
    Noop,
    /// Panic on pre-timeout.
    Panic,
    /// Custom governor.
    Custom(String),
    /// No pre-timeout configured.
    None,
}

impl std::fmt::Display for PreTimeoutGovernor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Noop => write!(f, "noop"),
            Self::Panic => write!(f, "panic"),
            Self::Custom(s) => write!(f, "{}", s),
            Self::None => write!(f, "none"),
        }
    }
}

/// Information about a single watchdog device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogInfo {
    /// Device name (e.g. "watchdog0").
    pub name: String,
    /// Device identity (driver name, e.g. "iTCO_wdt").
    pub identity: String,
    /// Watchdog type.
    pub watchdog_type: WatchdogType,
    /// Timeout in seconds.
    /// Configured timeout in seconds, where it was read.
    ///
    /// `unwrap_or(0)` published a watchdog with a zero-second timeout, which
    /// would fire immediately and is not a configuration any device holds.
    pub timeout_secs: Option<u32>,
    /// Pre-timeout in seconds, where it was read. `Some(0)` means disabled.
    ///
    /// **The sharpest of the four below `timeout_secs`**, because here `0` is a
    /// meaningful configuration rather than an impossible one: a pre-timeout of
    /// zero *is* how the kernel reports "disabled". So `unwrap_or(0)` did not
    /// merely invent a number, it invented a specific and plausible claim --
    /// that the administrator had turned the pre-timeout off -- about a device
    /// whose `pretimeout` attribute was never read.
    pub pretimeout_secs: Option<u32>,
    /// Pre-timeout governor.
    pub pretimeout_governor: PreTimeoutGovernor,
    /// Minimum timeout in seconds, where it was read.
    ///
    /// Not every driver exposes `min_timeout`/`max_timeout`; those that do not
    /// have no bound to report, which is different from a bound of zero.
    pub min_timeout_secs: Option<u32>,
    /// Maximum timeout in seconds, where it was read.
    pub max_timeout_secs: Option<u32>,
    /// Firmware version, where the driver exposes `fw_version`.
    pub firmware_version: Option<u32>,
    /// Status.
    pub status: WatchdogStatus,
    /// Available pre-timeout governors.
    pub available_governors: Vec<String>,
}

impl WatchdogInfo {
    /// Whether the timeout is at a common default (30 or 60 seconds), or
    /// `None` when the timeout was not read.
    pub fn is_default_timeout(&self) -> Option<bool> {
        self.timeout_secs.map(|t| t == 30 || t == 60)
    }
}

/// Overview of all watchdog devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogOverview {
    /// All watchdog devices.
    pub devices: Vec<WatchdogInfo>,
    /// Total count.
    pub total_count: u32,
    /// Active count.
    pub active_count: u32,
    /// Hardware watchdog count.
    pub hardware_count: u32,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// Watchdog monitor.
pub struct WatchdogMonitor {
    overview: WatchdogOverview,
}

impl WatchdogMonitor {
    /// Create a new watchdog monitor.
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
    pub fn overview(&self) -> &WatchdogOverview {
        &self.overview
    }

    /// Get devices.
    pub fn devices(&self) -> &[WatchdogInfo] {
        &self.overview.devices
    }

    /// Find device by name.
    pub fn device(&self, name: &str) -> Option<&WatchdogInfo> {
        self.overview.devices.iter().find(|d| d.name == name)
    }

    #[cfg(target_os = "linux")]
    fn scan() -> Result<WatchdogOverview, IronError> {
        let wdt_path = std::path::Path::new("/sys/class/watchdog");

        if !wdt_path.exists() {
            return Ok(Self::empty_overview());
        }

        let entries = std::fs::read_dir(wdt_path).map_err(IronError::Io)?;
        let mut devices = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            let identity =
                Self::read_sysfs(&path.join("identity")).unwrap_or_else(|| "unknown".into());

            let watchdog_type = if identity.contains("soft") || identity.contains("Soft") {
                WatchdogType::Software
            } else if identity != "unknown" {
                WatchdogType::Hardware
            } else {
                WatchdogType::Unknown
            };

            let timeout_secs = Self::read_sysfs_u32(&path.join("timeout"));
            // `read_sysfs_u32` already returns `Option`; these four threw it
            // away four lines below the `timeout_secs` doc explaining why that
            // is wrong. A driver exposing no `min_timeout` is not a driver with
            // a minimum of zero.
            let pretimeout_secs = Self::read_sysfs_u32(&path.join("pretimeout"));
            let min_timeout_secs = Self::read_sysfs_u32(&path.join("min_timeout"));
            let max_timeout_secs = Self::read_sysfs_u32(&path.join("max_timeout"));
            let firmware_version = Self::read_sysfs_u32(&path.join("fw_version"));

            let pretimeout_governor =
                match Self::read_sysfs(&path.join("pretimeout_governor")).as_deref() {
                    Some("noop") => PreTimeoutGovernor::Noop,
                    Some("panic") => PreTimeoutGovernor::Panic,
                    Some("") | None => PreTimeoutGovernor::None,
                    Some(other) => PreTimeoutGovernor::Custom(other.to_string()),
                };

            let available_governors =
                Self::read_sysfs(&path.join("pretimeout_available_governors"))
                    .map(|s| s.split_whitespace().map(String::from).collect())
                    .unwrap_or_default();

            // Status from state file. Both of these are `Option` now: the
            // helpers already returned one and both call sites threw it away,
            // three lines below four sibling reads that did the same.
            let active = Self::read_sysfs(&path.join("state")).map(|s| s == "active");
            let boot_triggered = Self::read_sysfs_u32(&path.join("bootstatus")).map(|b| b != 0);

            let status = WatchdogStatus {
                active,
                triggered: false,
                boot_triggered,
            };

            devices.push(WatchdogInfo {
                name,
                identity,
                watchdog_type,
                timeout_secs,
                pretimeout_secs,
                pretimeout_governor,
                min_timeout_secs,
                max_timeout_secs,
                firmware_version,
                status,
                available_governors,
            });
        }

        devices.sort_by(|a, b| a.name.cmp(&b.name));

        let total = devices.len() as u32;
        // Counted only where the state was read. A device whose `state` is
        // unreadable is not counted as inactive; `active_count` is a count of
        // devices known to be ticking, which is what its name claims.
        let active = devices
            .iter()
            .filter(|d| d.status.active == Some(true))
            .count() as u32;
        let hw = devices
            .iter()
            .filter(|d| d.watchdog_type == WatchdogType::Hardware)
            .count() as u32;

        let mut recs = Vec::new();
        if hw == 0 && total > 0 {
            recs.push("No hardware watchdog detected; software watchdog only".into());
        }
        for dev in &devices {
            match dev.status.boot_triggered {
                Some(true) => recs.push(format!(
                    "{}: watchdog-triggered reboot detected in boot status",
                    dev.name
                )),
                Some(false) => {}
                // Saying nothing here would be the old behaviour: silence that
                // reads as "no watchdog reboot". Someone investigating an
                // unexplained reset needs to know the question went unanswered.
                None => recs.push(format!(
                    concat!(
                        "{}: boot status could not be read, so a ",
                        "watchdog-triggered reboot can be neither ",
                        "confirmed nor ruled out"
                    ),
                    dev.name
                )),
            }
        }

        Ok(WatchdogOverview {
            devices,
            total_count: total,
            active_count: active,
            hardware_count: hw,
            recommendations: recs,
        })
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs(path: &std::path::Path) -> Option<String> {
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_u32(path: &std::path::Path) -> Option<u32> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    #[cfg(not(target_os = "linux"))]
    fn scan() -> Result<WatchdogOverview, IronError> {
        // Returning an empty overview here made "this platform cannot
        // answer" indistinguishable from "this machine has none", which
        // is the same defect RAPL shipped once. The reason travels with
        // the error so a caller can report it.
        Err(IronError::UnsupportedPlatform(
            "watchdog devices are read from `/sys/class/watchdog`, which this platform does not expose"
                .into(),
        ))
    }

    fn empty_overview() -> WatchdogOverview {
        WatchdogOverview {
            devices: Vec::new(),
            total_count: 0,
            active_count: 0,
            hardware_count: 0,
            recommendations: Vec::new(),
        }
    }
}

impl Default for WatchdogMonitor {
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
    fn test_type_display() {
        assert_eq!(WatchdogType::Hardware.to_string(), "hardware");
        assert_eq!(WatchdogType::Software.to_string(), "software");
    }

    #[test]
    fn test_governor_display() {
        assert_eq!(PreTimeoutGovernor::Panic.to_string(), "panic");
        assert_eq!(PreTimeoutGovernor::Noop.to_string(), "noop");
        assert_eq!(PreTimeoutGovernor::None.to_string(), "none");
    }

    #[test]
    fn test_default_timeout() {
        let info = WatchdogInfo {
            name: "watchdog0".into(),
            identity: "iTCO_wdt".into(),
            watchdog_type: WatchdogType::Hardware,
            timeout_secs: Some(30),
            pretimeout_secs: Some(0),
            pretimeout_governor: PreTimeoutGovernor::None,
            min_timeout_secs: Some(2),
            max_timeout_secs: Some(614),
            firmware_version: Some(0),
            status: WatchdogStatus {
                active: Some(false),
                triggered: false,
                boot_triggered: Some(false),
            },
            available_governors: Vec::new(),
        };
        assert_eq!(info.is_default_timeout(), Some(true));

        // A timeout that was not read is not a non-default timeout. It used to
        // hold `0` and answer `false`.
        let unread = WatchdogInfo {
            timeout_secs: None,
            ..info
        };
        assert_eq!(unread.is_default_timeout(), None);
    }

    #[test]
    fn test_monitor_default() {
        let monitor = WatchdogMonitor::default();
        let _overview = monitor.overview();
    }

    #[test]
    fn test_serialization() {
        let info = WatchdogInfo {
            name: "watchdog0".into(),
            identity: "softdog".into(),
            watchdog_type: WatchdogType::Software,
            timeout_secs: Some(60),
            pretimeout_secs: Some(10),
            pretimeout_governor: PreTimeoutGovernor::Panic,
            min_timeout_secs: Some(1),
            max_timeout_secs: Some(65535),
            firmware_version: Some(0),
            status: WatchdogStatus {
                active: Some(true),
                triggered: false,
                boot_triggered: Some(false),
            },
            available_governors: vec!["noop".into(), "panic".into()],
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("softdog"));
        let _: WatchdogInfo = serde_json::from_str(&json).unwrap();
    }
}
