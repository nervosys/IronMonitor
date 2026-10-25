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
    /// Watchdog has triggered at least once. Always `None`: no sysfs attribute
    /// reports this, so no reader can obtain it.
    ///
    /// This was a literal `false` under a doc admitting nothing reports it,
    /// so every device answered "has not triggered". The only trigger evidence
    /// sysfs offers is `bootstatus`, reported as `boot_triggered`.
    pub triggered: Option<bool>,
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
    /// Device identity (driver name, e.g. "iTCO_wdt"), or `None` where the
    /// `identity` attribute was not read or was empty.
    ///
    /// This was `"unknown"` when unread, and that literal then classified the
    /// device's type by string comparison.
    pub identity: Option<String>,
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
    /// Pre-timeout governor, or `None` where the `pretimeout_governor`
    /// attribute was not read. An empty attribute is `Some(PreTimeoutGovernor::None)`.
    ///
    /// An unread attribute used to become `PreTimeoutGovernor::None`, which
    /// reads as a configuration ("no pre-timeout") that no one set.
    pub pretimeout_governor: Option<PreTimeoutGovernor>,
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
        Self::scan_root(std::path::Path::new("/sys/class/watchdog"))
    }

    /// Read the watchdog devices under `wdt_path`, normally `/sys/class/watchdog`.
    #[cfg(target_os = "linux")]
    fn scan_root(wdt_path: &std::path::Path) -> Result<WatchdogOverview, IronError> {
        // The softdog driver registers exactly this identity. Matched in full:
        // a substring match would also catch any hardware driver whose name
        // happens to contain "soft".
        const SOFTDOG_IDENTITY: &str = "Software Watchdog";

        // A missing class directory means the watchdog subsystem is not exposed,
        // which is not the same as having no watchdog devices. It used to return
        // an empty overview, which read as "no watchdogs here".
        if !wdt_path.exists() {
            return Err(IronError::NotFound(format!(
                "{} does not exist, so watchdog devices cannot be read",
                wdt_path.display()
            )));
        }

        let entries = std::fs::read_dir(wdt_path).map_err(IronError::Io)?;
        let mut devices = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // An empty attribute is no identity, so it is `None` like an
            // unreadable one. Neither is classified as hardware.
            let identity = Self::read_sysfs(&path.join("identity")).filter(|s| !s.is_empty());

            // Anything read that is not softdog is a hardware driver. No
            // identity means no claim about the device's type.
            let watchdog_type = match identity.as_deref() {
                Some(SOFTDOG_IDENTITY) => WatchdogType::Software,
                Some(_) => WatchdogType::Hardware,
                None => WatchdogType::Unknown,
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

            // An unread attribute stays `None`. An empty one is the kernel's own
            // "no governor", a real `PreTimeoutGovernor::None`.
            let pretimeout_governor =
                Self::read_sysfs(&path.join("pretimeout_governor")).map(|g| match g.as_str() {
                    "noop" => PreTimeoutGovernor::Noop,
                    "panic" => PreTimeoutGovernor::Panic,
                    "" => PreTimeoutGovernor::None,
                    other => PreTimeoutGovernor::Custom(other.to_string()),
                });

            let available_governors =
                Self::read_sysfs(&path.join("pretimeout_available_governors"))
                    .map(|s| s.split_whitespace().map(String::from).collect())
                    .unwrap_or_default();

            // Status from state file. Both of these are `Option` now: the
            // helpers already returned one and both call sites threw it away,
            // three lines below four sibling reads that did the same.
            let active = Self::read_sysfs(&path.join("state")).map(|s| s == "active");
            let boot_triggered = Self::read_sysfs_u32(&path.join("bootstatus")).map(|b| b != 0);

            // No sysfs attribute reports a trigger, so this is never obtained.
            let status = WatchdogStatus {
                active,
                triggered: None,
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
        // Claimed only when every device's type is known to be software. A
        // device with an unread identity is not evidence of "no hardware".
        if total > 0
            && devices
                .iter()
                .all(|d| d.watchdog_type == WatchdogType::Software)
        {
            recs.push("No hardware watchdog detected; software watchdog only".into());
        }
        for dev in devices.iter().filter(|d| d.identity.is_none()) {
            recs.push(format!(
                "{}: identity could not be read, so its watchdog type is unknown",
                dev.name
            ));
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
            identity: Some("iTCO_wdt".into()),
            watchdog_type: WatchdogType::Hardware,
            timeout_secs: Some(30),
            pretimeout_secs: Some(0),
            pretimeout_governor: Some(PreTimeoutGovernor::None),
            min_timeout_secs: Some(2),
            max_timeout_secs: Some(614),
            firmware_version: Some(0),
            status: WatchdogStatus {
                active: Some(false),
                triggered: None,
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

    /// A missing `/sys/class/watchdog` is a failed read, not a machine with no
    /// watchdog devices.
    #[cfg(target_os = "linux")]
    #[test]
    fn missing_watchdog_class_is_an_error_not_zero_devices() {
        let root =
            std::env::temp_dir().join(format!("ironmon-watchdog-absent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(WatchdogMonitor::scan_root(&root).is_err());
    }

    #[test]
    fn test_serialization() {
        let info = WatchdogInfo {
            name: "watchdog0".into(),
            identity: Some("Software Watchdog".into()),
            watchdog_type: WatchdogType::Software,
            timeout_secs: Some(60),
            pretimeout_secs: Some(10),
            pretimeout_governor: Some(PreTimeoutGovernor::Panic),
            min_timeout_secs: Some(1),
            max_timeout_secs: Some(65535),
            firmware_version: Some(0),
            status: WatchdogStatus {
                active: Some(true),
                triggered: None,
                boot_triggered: Some(false),
            },
            available_governors: vec!["noop".into(), "panic".into()],
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("Software Watchdog"));
        let _: WatchdogInfo = serde_json::from_str(&json).unwrap();
    }

    /// A throwaway `/sys/class/watchdog` tree. Each device is a directory
    /// holding the given attribute files. Callers remove it.
    #[cfg(target_os = "linux")]
    fn watchdog_root(tag: &str, devices: &[(&str, &[(&str, &str)])]) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("ironmon-watchdog-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for (dev, files) in devices {
            let dir = root.join(dev);
            std::fs::create_dir_all(&dir).unwrap();
            for (file, body) in files.iter() {
                std::fs::write(dir.join(file), body).unwrap();
            }
        }
        root
    }

    /// An attribute set for a device that exposes every file a real driver
    /// would, so only the attributes a test removes are missing.
    #[cfg(target_os = "linux")]
    const FULL_DEVICE: &[(&str, &str)] = &[
        ("identity", "iTCO_wdt\n"),
        ("state", "inactive\n"),
        ("timeout", "30\n"),
        ("pretimeout", "0\n"),
        ("min_timeout", "2\n"),
        ("max_timeout", "614\n"),
        ("fw_version", "2\n"),
        ("bootstatus", "0\n"),
        ("pretimeout_governor", "noop\n"),
        ("pretimeout_available_governors", "noop panic\n"),
    ];

    #[cfg(target_os = "linux")]
    fn without(
        attrs: &[(&'static str, &'static str)],
        drop: &str,
    ) -> Vec<(&'static str, &'static str)> {
        attrs.iter().copied().filter(|(f, _)| *f != drop).collect()
    }

    /// Scan a fixture tree and remove it afterwards.
    #[cfg(target_os = "linux")]
    fn scan_fixture(root: &std::path::Path) -> WatchdogOverview {
        let overview = WatchdogMonitor::scan_root(root).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        overview
    }

    /// An unreadable identity is no identity, and the device's type is not
    /// claimed. The old reader turned it into "unknown", and the software-only
    /// recommendation then fired on a device nobody had identified.
    #[cfg(target_os = "linux")]
    #[test]
    fn unreadable_identity_claims_no_device_type() {
        let attrs = without(FULL_DEVICE, "identity");
        let root = watchdog_root("no-identity", &[("watchdog0", &attrs)]);
        let overview = scan_fixture(&root);

        let dev = &overview.devices[0];
        assert_eq!(dev.identity, None);
        assert_eq!(dev.watchdog_type, WatchdogType::Unknown);
        assert_eq!(overview.hardware_count, 0);
        assert!(
            !overview
                .recommendations
                .iter()
                .any(|r| r.contains("software watchdog only")),
            "an unread identity must not produce a software-only claim: {:?}",
            overview.recommendations
        );
        assert!(overview
            .recommendations
            .iter()
            .any(|r| r.starts_with("watchdog0: identity could not be read")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn softdog_identity_is_software_and_only_then_claimed_software_only() {
        let mut attrs = without(FULL_DEVICE, "identity");
        attrs.push(("identity", "Software Watchdog\n"));
        let root = watchdog_root("softdog", &[("watchdog0", &attrs)]);
        let overview = scan_fixture(&root);

        assert_eq!(
            overview.devices[0].identity.as_deref(),
            Some("Software Watchdog")
        );
        assert_eq!(overview.devices[0].watchdog_type, WatchdogType::Software);
        assert!(overview
            .recommendations
            .iter()
            .any(|r| r == "No hardware watchdog detected; software watchdog only"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn hardware_identity_is_hardware_and_not_software_only() {
        let root = watchdog_root("hardware", &[("watchdog0", FULL_DEVICE)]);
        let overview = scan_fixture(&root);

        assert_eq!(overview.devices[0].watchdog_type, WatchdogType::Hardware);
        assert_eq!(overview.hardware_count, 1);
        assert!(!overview
            .recommendations
            .iter()
            .any(|r| r.contains("software watchdog only")));
    }

    /// An identity that merely contains "soft" is not the softdog driver.
    #[cfg(target_os = "linux")]
    #[test]
    fn identity_matching_is_strict_not_substring() {
        let mut attrs = without(FULL_DEVICE, "identity");
        attrs.push(("identity", "softlock_wdt\n"));
        let root = watchdog_root("strict", &[("watchdog0", &attrs)]);
        let overview = scan_fixture(&root);

        assert_eq!(overview.devices[0].watchdog_type, WatchdogType::Hardware);
    }

    /// A missing `pretimeout_governor` is not "no pre-timeout"; an empty one is.
    #[cfg(target_os = "linux")]
    #[test]
    fn unread_pretimeout_governor_is_none_and_empty_is_no_governor() {
        let unread = without(FULL_DEVICE, "pretimeout_governor");
        let mut empty = without(FULL_DEVICE, "pretimeout_governor");
        empty.push(("pretimeout_governor", "\n"));
        let root = watchdog_root("governor", &[("watchdog0", &unread), ("watchdog1", &empty)]);
        let overview = scan_fixture(&root);

        assert_eq!(overview.devices[0].pretimeout_governor, None);
        assert_eq!(
            overview.devices[1].pretimeout_governor,
            Some(PreTimeoutGovernor::None)
        );
    }

    /// No sysfs attribute reports a trigger, so `triggered` is never a value.
    #[cfg(target_os = "linux")]
    #[test]
    fn triggered_is_never_reported_as_false() {
        let root = watchdog_root("triggered", &[("watchdog0", FULL_DEVICE)]);
        let overview = scan_fixture(&root);

        assert_eq!(overview.devices[0].status.triggered, None);
        assert_eq!(overview.devices[0].status.boot_triggered, Some(false));
    }
}
