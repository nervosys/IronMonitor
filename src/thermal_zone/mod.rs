//! Thermal zone monitoring.
//!
//! Enumerates all thermal zones, trip points, cooling devices, and thermal
//! policies. Provides comprehensive thermal state beyond what hwmon offers.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/class/thermal/thermal_zone*`, `/sys/class/thermal/cooling_device*`
//! - **Windows**: WMI `Win32_TemperatureProbe`
//! - **macOS**: IOKit thermal sensors

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// Most trip points read from one zone. Real zones expose a handful; this bound
/// only stops a runaway tree. Reaching it sets `trip_points_truncated`.
#[cfg(target_os = "linux")]
const MAX_TRIP_POINTS: u32 = 256;

/// Thermal zone type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThermalZoneType {
    /// ACPI thermal zone.
    Acpi,
    /// x86 package thermal.
    X86Pkg,
    /// Intel PCH (Platform Controller Hub).
    Pch,
    /// SoC internal sensor.
    Soc,
    /// GPU thermal.
    Gpu,
    /// NVMe drive.
    Nvme,
    /// iwlwifi (Wi-Fi card).
    Iwlwifi,
    /// Other / unknown.
    Other(String),
}

impl std::fmt::Display for ThermalZoneType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Acpi => write!(f, "ACPI"),
            Self::X86Pkg => write!(f, "x86_pkg"),
            Self::Pch => write!(f, "PCH"),
            Self::Soc => write!(f, "SoC"),
            Self::Gpu => write!(f, "GPU"),
            Self::Nvme => write!(f, "NVMe"),
            Self::Iwlwifi => write!(f, "iwlwifi"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

/// Trip point type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TripPointType {
    /// Active cooling trip (fan ramp).
    Active,
    /// Passive cooling trip (throttle).
    Passive,
    /// Hot — OS-level alert.
    Hot,
    /// Critical — hardware shutdown.
    Critical,
}

impl std::fmt::Display for TripPointType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Passive => write!(f, "passive"),
            Self::Hot => write!(f, "hot"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// Trip point information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripPoint {
    /// Trip point index.
    pub index: u32,
    /// Trip type, or `None` where the `type` file was not read or named no
    /// type this reader knows. The trip itself is kept, because its temperature
    /// may still have been read.
    pub trip_type: Option<TripPointType>,
    /// Trip temperature in millidegrees Celsius, where it was read.
    pub temp_mc: Option<i64>,
    /// Hysteresis in millidegrees Celsius, where it was read.
    pub hysteresis_mc: Option<i64>,
}

impl TripPoint {
    /// Temperature in degrees Celsius, where it was read.
    pub fn temp_c(&self) -> Option<f64> {
        self.temp_mc.map(|mc| mc as f64 / 1000.0)
    }
}

/// Cooling device information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoolingDeviceInfo {
    /// Device name (e.g. "cooling_device0").
    pub name: String,
    /// Cooling type (e.g. "Processor", "intel_powerclamp", "Fan").
    pub cooling_type: String,
    /// Current cooling state, or `None` where `cur_state` was not read.
    ///
    /// **`0` is a real and reassuring state here: not throttling.** For a
    /// `Processor` cooling device it means the CPU is running at full speed.
    /// `unwrap_or(0)` therefore reported "no thermal throttling" for any
    /// device whose state could not be read -- the comfortable answer to the
    /// question someone chasing an unexplained slowdown is asking. Same shape
    /// as the watchdog pre-timeout, where `0` means disabled.
    pub cur_state: Option<u32>,
    /// Maximum cooling state, or `None` where not read.
    pub max_state: Option<u32>,
}

impl CoolingDeviceInfo {
    /// How far into its range the device is throttling, as a percentage, or
    /// `None` where either state is unread or the device reports no range.
    pub fn utilization_pct(&self) -> Option<f64> {
        let (cur, max) = (self.cur_state?, self.max_state?);
        if max == 0 {
            return None;
        }
        Some((cur as f64 / max as f64) * 100.0)
    }
}

/// Thermal zone information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalZoneInfo {
    /// Zone name (e.g. "thermal_zone0").
    pub name: String,
    /// Zone type, or `None` where the zone's `type` file was unreadable or empty.
    ///
    /// This was `ThermalZoneType::Other("unknown")` in that case, a type the
    /// driver never declared.
    pub zone_type: Option<ThermalZoneType>,
    /// Raw type string, verbatim, or `None` where the `type` file was unreadable
    /// or empty. This is the evidence `zone_type` was classified from.
    pub type_string: Option<String>,
    /// Current temperature in millidegrees Celsius, or `None` where the
    /// zone's `temp` file could not be read.
    ///
    /// This was `i64` with `unwrap_or(0)`, so an unreadable zone reported
    /// **0 °C** — and `is_throttling` then compared `0 >= passive_trip` and
    /// answered `false`. An unread thermal zone said it was cold and not
    /// throttling, which is the reassuring direction.
    pub temp_mc: Option<i64>,
    /// Thermal policy/governor (e.g. "step_wise", "user_space"), or `None`
    /// where the zone's `policy` file was not read.
    pub policy: Option<String>,
    /// Available policies, or `None` where the zone's `available_policies`
    /// file was not read. An empty list is a read that found none.
    pub available_policies: Option<Vec<String>>,
    /// Trip points.
    pub trip_points: Vec<TripPoint>,
    /// True when the zone has more trip points than `trip_points` holds, because
    /// the read stopped at the safety bound. The list is then not the zone's
    /// full set. Read from the file tree, so it is never a guess.
    pub trip_points_truncated: bool,
    /// Mode (enabled/disabled), or `None` where the zone has no readable
    /// `mode`. A missing `mode` used to read as enabled.
    pub enabled: Option<bool>,
    /// Whether the zone is in passive cooling mode, or `None` where the zone
    /// temperature or the passive trip point was not read.
    pub passive_active: Option<bool>,
}

impl ThermalZoneInfo {
    /// Temperature in degrees Celsius, where it was read.
    pub fn temp_c(&self) -> Option<f64> {
        self.temp_mc.map(|mc| mc as f64 / 1000.0)
    }

    /// Distance to the critical trip point in degrees, where both are known.
    pub fn headroom_to_critical_c(&self) -> Option<f64> {
        let here = self.temp_c()?;
        self.trip_points
            .iter()
            .filter(|tp| tp.trip_type == Some(TripPointType::Critical))
            .find_map(|tp| tp.temp_c())
            .map(|critical| critical - here)
    }

    /// Whether the temperature is above a passive trip point, or `None` when
    /// the zone's temperature is not known.
    ///
    /// This returned `bool`, and an unread zone (`temp_mc` defaulting to 0)
    /// compared `0 >= passive_trip` and answered "not throttling".
    pub fn is_throttling(&self) -> Option<bool> {
        let here = self.temp_mc?;
        let passive: Vec<i64> = self
            .trip_points
            .iter()
            .filter(|tp| tp.trip_type == Some(TripPointType::Passive))
            .filter_map(|tp| tp.temp_mc)
            .collect();
        // No passive trip point read is not the same as being below one.
        (!passive.is_empty()).then(|| passive.iter().any(|t| here >= *t))
    }
}

/// Thermal overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalZoneOverview {
    /// All thermal zones.
    pub zones: Vec<ThermalZoneInfo>,
    /// Cooling devices.
    pub cooling_devices: Vec<CoolingDeviceInfo>,
    /// Total zone count.
    pub zone_count: u32,
    /// Hottest zone name, or `None` when no zone reported a temperature.
    pub hottest_zone: Option<String>,
    /// Hottest temperature in C, or `None` when no zone reported one.
    ///
    /// `unwrap_or_default()` made this 0.0 with no zones readable, which reads
    /// as a machine at freezing rather than one nothing was read from.
    pub hottest_temp_c: Option<f64>,
    /// Number of zones in passive/throttle state.
    pub throttling_count: u32,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// Thermal zone monitor.
pub struct ThermalZoneMonitor {
    overview: ThermalZoneOverview,
}

impl ThermalZoneMonitor {
    /// Create a new thermal zone monitor.
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
    pub fn overview(&self) -> &ThermalZoneOverview {
        &self.overview
    }

    /// Get zones.
    pub fn zones(&self) -> &[ThermalZoneInfo] {
        &self.overview.zones
    }

    /// Get cooling devices.
    pub fn cooling_devices(&self) -> &[CoolingDeviceInfo] {
        &self.overview.cooling_devices
    }

    /// Get the hottest zone among those with a temperature reading.
    ///
    /// `max_by_key` over `Option` ranks `None` below every `Some`, which is
    /// harmless for a maximum but would silently return an unread zone when
    /// none has a reading. Filtered explicitly instead.
    pub fn hottest(&self) -> Option<&ThermalZoneInfo> {
        self.overview
            .zones
            .iter()
            .filter(|z| z.temp_mc.is_some())
            .max_by_key(|z| z.temp_mc)
    }

    /// Get zones that are throttling.
    pub fn throttling_zones(&self) -> Vec<&ThermalZoneInfo> {
        self.overview
            .zones
            .iter()
            .filter(|z| z.is_throttling() == Some(true))
            .collect()
    }

    #[cfg(target_os = "linux")]
    fn scan() -> Result<ThermalZoneOverview, IronError> {
        Self::scan_root(std::path::Path::new("/sys/class/thermal"))
    }

    /// Read the thermal zones and cooling devices under `thermal_path`,
    /// normally `/sys/class/thermal`.
    #[cfg(target_os = "linux")]
    fn scan_root(thermal_path: &std::path::Path) -> Result<ThermalZoneOverview, IronError> {
        // A missing directory means the kernel exposes no thermal zones to
        // read. That is a failed read of the source, not a machine with none.
        if !thermal_path.exists() {
            return Err(IronError::NotFound(format!(
                "{} does not exist, so thermal zones cannot be read",
                thermal_path.display()
            )));
        }

        let mut zones = Vec::new();
        let mut cooling = Vec::new();

        let entries = std::fs::read_dir(thermal_path).map_err(IronError::Io)?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();

            if name.starts_with("thermal_zone") {
                if let Some(zone) = Self::read_zone(&path, &name) {
                    zones.push(zone);
                }
            } else if name.starts_with("cooling_device") {
                if let Some(cd) = Self::read_cooling_device(&path, &name) {
                    cooling.push(cd);
                }
            }
        }

        zones.sort_by(|a, b| a.name.cmp(&b.name));
        cooling.sort_by(|a, b| a.name.cmp(&b.name));

        let zone_count = zones.len() as u32;
        // Only zones known to be throttling. A zone whose temperature was not
        // read is not counted either way.
        let throttling = zones
            .iter()
            .filter(|z| z.is_throttling() == Some(true))
            .count() as u32;

        let (hottest_zone, hottest_temp) = zones
            .iter()
            .filter(|z| z.temp_mc.is_some())
            .max_by_key(|z| z.temp_mc)
            .map_or((None, None), |z| (Some(z.name.clone()), z.temp_c()));

        let mut recs = Vec::new();
        if throttling > 0 {
            recs.push(format!(
                "{} zone(s) in thermal throttling state",
                throttling
            ));
        }
        for zone in &zones {
            if let Some(headroom) = zone.headroom_to_critical_c() {
                if headroom < 10.0 {
                    recs.push(format!(
                        "{}: only {:.1}°C from critical shutdown temperature",
                        zone.name, headroom
                    ));
                }
            }
        }

        Ok(ThermalZoneOverview {
            zones,
            cooling_devices: cooling,
            zone_count,
            hottest_zone,
            hottest_temp_c: hottest_temp,
            throttling_count: throttling,
            recommendations: recs,
        })
    }

    #[cfg(target_os = "linux")]
    fn read_zone(path: &std::path::Path, name: &str) -> Option<ThermalZoneInfo> {
        // An unreadable or empty `type` is no type. The raw string is kept as
        // evidence, and the classification is `None` rather than a made-up
        // `Other("unknown")`.
        let type_string = Self::read_sysfs(&path.join("type")).filter(|s| !s.is_empty());

        let zone_type = type_string.as_deref().map(|s| match s {
            "acpitz" | "ACPI\\_THM" => ThermalZoneType::Acpi,
            s if s.starts_with("x86_pkg") => ThermalZoneType::X86Pkg,
            "pch_cannonlake" | "pch_skylake" | "pch_alderlake" | "pch_raptorlake" => {
                ThermalZoneType::Pch
            }
            s if s.contains("pch") => ThermalZoneType::Pch,
            s if s.contains("soc") || s.contains("SoC") => ThermalZoneType::Soc,
            s if s.contains("gpu") || s.contains("GPU") => ThermalZoneType::Gpu,
            s if s.starts_with("nvme") => ThermalZoneType::Nvme,
            "iwlwifi_1" | "iwlwifi" => ThermalZoneType::Iwlwifi,
            s if s.contains("iwlwifi") => ThermalZoneType::Iwlwifi,
            other => ThermalZoneType::Other(other.to_string()),
        });

        // A zone whose `temp` file cannot be read has no temperature, not a
        // temperature of zero.
        let temp_mc = Self::read_sysfs_i64(&path.join("temp"));

        let policy = Self::read_sysfs(&path.join("policy"));
        let available_policies = Self::read_sysfs(&path.join("available_policies")).map(|s| {
            s.split_whitespace()
                .map(String::from)
                .collect::<Vec<String>>()
        });

        // No `mode` file is no answer about enablement, not an enabled zone.
        let enabled = Self::read_sysfs(&path.join("mode")).map(|mode| mode != "disabled");

        // Read trip points until `trip_point_{i}_temp` is absent. The bound is a
        // safety net, not the list's length: a zone with more trips than it
        // holds is reported as truncated instead of quietly cut at 20.
        let mut trip_points = Vec::new();
        for i in 0..MAX_TRIP_POINTS {
            let tp_temp_path = path.join(format!("trip_point_{}_temp", i));
            let tp_type_path = path.join(format!("trip_point_{}_type", i));

            if !tp_temp_path.exists() {
                break;
            }

            let tp_temp = Self::read_sysfs_i64(&tp_temp_path);
            // An unread `type` used to be `""`, which matched no arm and dropped
            // the whole trip along with its temperature. The trip now stays, with
            // its type `None`.
            let tp_type = Self::read_sysfs(&tp_type_path).and_then(|s| match s.as_str() {
                "active" => Some(TripPointType::Active),
                "passive" => Some(TripPointType::Passive),
                "hot" => Some(TripPointType::Hot),
                "critical" => Some(TripPointType::Critical),
                _ => None,
            });

            let hyst = Self::read_sysfs_i64(&path.join(format!("trip_point_{}_hyst", i)));

            trip_points.push(TripPoint {
                index: i,
                trip_type: tp_type,
                temp_mc: tp_temp,
                hysteresis_mc: hyst,
            });
        }

        // Compared only where both the zone temperature and a passive trip
        // point were read. `Option` orders `None` below `Some`, so comparing
        // the raw values reported passive cooling for any zone whose trip
        // point was unread.
        let passive: Vec<i64> = trip_points
            .iter()
            .filter(|tp| tp.trip_type == Some(TripPointType::Passive))
            .filter_map(|tp| tp.temp_mc)
            .collect();
        let passive_active = match temp_mc {
            Some(here) if !passive.is_empty() => Some(passive.iter().any(|t| here >= *t)),
            _ => None,
        };

        // The loop above stops at the first absent trip or at the bound. If the
        // trip at the bound still exists, the list was cut short there.
        let trip_points_truncated = path
            .join(format!("trip_point_{}_temp", MAX_TRIP_POINTS))
            .exists();

        Some(ThermalZoneInfo {
            name: name.to_string(),
            zone_type,
            type_string,
            temp_mc,
            policy,
            available_policies,
            trip_points,
            trip_points_truncated,
            enabled,
            passive_active,
        })
    }

    #[cfg(target_os = "linux")]
    fn read_cooling_device(path: &std::path::Path, name: &str) -> Option<CoolingDeviceInfo> {
        let cooling_type = Self::read_sysfs(&path.join("type")).unwrap_or_default();
        // `read_sysfs_u32` returns `Option`; these discarded it.
        let cur_state = Self::read_sysfs_u32(&path.join("cur_state"));
        let max_state = Self::read_sysfs_u32(&path.join("max_state"));

        Some(CoolingDeviceInfo {
            name: name.to_string(),
            cooling_type,
            cur_state,
            max_state,
        })
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs(path: &std::path::Path) -> Option<String> {
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_i64(path: &std::path::Path) -> Option<i64> {
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
    fn scan() -> Result<ThermalZoneOverview, IronError> {
        // Returning an empty overview here made "this platform cannot
        // answer" indistinguishable from "this machine has none", which
        // is the same defect RAPL shipped once. The reason travels with
        // the error so a caller can report it.
        Err(IronError::UnsupportedPlatform(
            "thermal zones are read from `/sys/class/thermal`, which this platform does not expose"
                .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_type_display() {
        assert_eq!(ThermalZoneType::X86Pkg.to_string(), "x86_pkg");
        assert_eq!(ThermalZoneType::Pch.to_string(), "PCH");
        assert_eq!(ThermalZoneType::Nvme.to_string(), "NVMe");
    }

    #[test]
    fn test_trip_point_temp() {
        let tp = TripPoint {
            index: 0,
            trip_type: Some(TripPointType::Critical),
            temp_mc: Some(105_000),
            hysteresis_mc: Some(0),
        };
        assert!((tp.temp_c().expect("a read trip point") - 105.0).abs() < 0.01);

        // A trip point whose file could not be read has no temperature.
        let unread = TripPoint {
            index: 1,
            trip_type: Some(TripPointType::Critical),
            temp_mc: None,
            hysteresis_mc: None,
        };
        assert_eq!(unread.temp_c(), None);
    }

    #[test]
    fn test_throttle_detection() {
        let zone = ThermalZoneInfo {
            name: "thermal_zone0".into(),
            zone_type: Some(ThermalZoneType::X86Pkg),
            type_string: Some("x86_pkg_temp".into()),
            temp_mc: Some(95_000),
            policy: Some("step_wise".into()),
            available_policies: Some(vec!["step_wise".into()]),
            trip_points: vec![
                TripPoint {
                    index: 0,
                    trip_type: Some(TripPointType::Passive),
                    temp_mc: Some(90_000),
                    hysteresis_mc: Some(2_000),
                },
                TripPoint {
                    index: 1,
                    trip_type: Some(TripPointType::Critical),
                    temp_mc: Some(105_000),
                    hysteresis_mc: Some(0),
                },
            ],
            trip_points_truncated: false,
            enabled: Some(true),
            passive_active: Some(true),
        };
        assert_eq!(zone.is_throttling(), Some(true));
        assert!((zone.headroom_to_critical_c().unwrap() - 10.0).abs() < 0.01);
    }

    /// A zone whose temperature was not read is not a zone that is cool.
    ///
    /// `temp_mc` was `i64` with `unwrap_or(0)`, so an unreadable zone held 0
    /// and `is_throttling` compared `0 >= passive_trip` — answering "not
    /// throttling" about a zone nothing had measured, which is the reassuring
    /// direction.
    #[test]
    fn an_unread_zone_does_not_report_that_it_is_not_throttling() {
        let zone = ThermalZoneInfo {
            name: "thermal_zone0".into(),
            zone_type: Some(ThermalZoneType::X86Pkg),
            type_string: Some("x86_pkg_temp".into()),
            temp_mc: None,
            policy: Some("step_wise".into()),
            available_policies: Some(vec!["step_wise".into()]),
            trip_points: vec![TripPoint {
                index: 0,
                trip_type: Some(TripPointType::Passive),
                temp_mc: Some(90_000),
                hysteresis_mc: Some(2_000),
            }],
            trip_points_truncated: false,
            enabled: Some(true),
            passive_active: None,
        };

        assert_eq!(
            zone.is_throttling(),
            None,
            concat!(
                "with no temperature there is no answer, and `false` is ",
                "the answer that gets acted on"
            )
        );
        assert_eq!(zone.temp_c(), None);
        assert_eq!(
            zone.headroom_to_critical_c(),
            None,
            "headroom from an unknown temperature is unknown"
        );
    }

    #[test]
    fn test_cooling_utilization() {
        let cd = CoolingDeviceInfo {
            name: "cooling_device0".into(),
            cooling_type: "Processor".into(),
            cur_state: Some(5),
            max_state: Some(10),
        };
        assert!((cd.utilization_pct().expect("both states read") - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_serialization() {
        let zone = ThermalZoneInfo {
            name: "thermal_zone0".into(),
            zone_type: Some(ThermalZoneType::Acpi),
            type_string: Some("acpitz".into()),
            temp_mc: Some(45_000),
            policy: Some("step_wise".into()),
            available_policies: Some(Vec::new()),
            trip_points: Vec::new(),
            trip_points_truncated: false,
            enabled: Some(true),
            passive_active: Some(false),
        };
        let json = serde_json::to_string(&zone).unwrap();
        assert!(json.contains("acpitz"));
        let _: ThermalZoneInfo = serde_json::from_str(&json).unwrap();
    }

    /// A throwaway `/sys/class/thermal` tree holding one `thermal_zone0`
    /// directory, for the caller to fill. Callers remove it.
    #[cfg(target_os = "linux")]
    fn thermal_tree(tag: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("ironmon-thermal-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("thermal_zone0")).unwrap();
        root
    }

    /// Write `count` passive trip points, each a temperature and a type file.
    #[cfg(target_os = "linux")]
    fn write_passive_trips(zone: &std::path::Path, count: u32) {
        for i in 0..count {
            std::fs::write(
                zone.join(format!("trip_point_{}_temp", i)),
                format!("{}\n", 40_000 + i * 1_000),
            )
            .unwrap();
            std::fs::write(zone.join(format!("trip_point_{}_type", i)), "passive\n").unwrap();
        }
    }

    /// A missing thermal tree is a failed read, not an empty list.
    #[cfg(target_os = "linux")]
    #[test]
    fn missing_thermal_tree_is_an_error_not_zero_zones() {
        let root =
            std::env::temp_dir().join(format!("ironmon-thermal-absent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(ThermalZoneMonitor::scan_root(&root).is_err());
    }

    /// An unreadable `type` is no zone type, and the raw string is absent too.
    /// It used to be `Other("unknown")`, a type the driver never declared.
    #[cfg(target_os = "linux")]
    #[test]
    fn unread_zone_type_is_none_not_other_unknown() {
        let root = thermal_tree("no-type");
        std::fs::write(root.join("thermal_zone0/temp"), "40000\n").unwrap();
        let overview = ThermalZoneMonitor::scan_root(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        let zone = &overview.zones[0];
        assert_eq!(zone.zone_type, None);
        assert_eq!(zone.type_string, None);
        assert_eq!(zone.temp_mc, Some(40_000));
    }

    /// An empty `type` file is no type either, not a type called "".
    #[cfg(target_os = "linux")]
    #[test]
    fn empty_zone_type_is_none() {
        let root = thermal_tree("empty-type");
        std::fs::write(root.join("thermal_zone0/type"), "\n").unwrap();
        let overview = ThermalZoneMonitor::scan_root(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        assert_eq!(overview.zones[0].zone_type, None);
        assert_eq!(overview.zones[0].type_string, None);
    }

    /// A read `type` is classified and its raw string kept verbatim.
    #[cfg(target_os = "linux")]
    #[test]
    fn read_zone_type_is_classified_with_raw_string_kept() {
        let root = thermal_tree("x86");
        std::fs::write(root.join("thermal_zone0/type"), "x86_pkg_temp\n").unwrap();
        let overview = ThermalZoneMonitor::scan_root(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        assert_eq!(overview.zones[0].zone_type, Some(ThermalZoneType::X86Pkg));
        assert_eq!(
            overview.zones[0].type_string.as_deref(),
            Some("x86_pkg_temp")
        );
    }

    /// The old reader stopped at 20 trip points without saying so. A zone with
    /// 25 is read in full and is not marked truncated.
    #[cfg(target_os = "linux")]
    #[test]
    fn trip_points_beyond_twenty_are_read_not_dropped() {
        let root = thermal_tree("trips-25");
        write_passive_trips(&root.join("thermal_zone0"), 25);
        let overview = ThermalZoneMonitor::scan_root(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        let zone = &overview.zones[0];
        assert_eq!(zone.trip_points.len(), 25);
        assert_eq!(zone.trip_points[24].index, 24);
        assert!(!zone.trip_points_truncated);
    }

    /// More trip points than the safety bound is reported as truncation, not
    /// as a complete list of exactly the bound's length.
    #[cfg(target_os = "linux")]
    #[test]
    fn trip_points_past_the_bound_are_marked_truncated() {
        let root = thermal_tree("trips-over");
        write_passive_trips(&root.join("thermal_zone0"), MAX_TRIP_POINTS + 1);
        let overview = ThermalZoneMonitor::scan_root(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        let zone = &overview.zones[0];
        assert_eq!(zone.trip_points.len(), MAX_TRIP_POINTS as usize);
        assert!(zone.trip_points_truncated);
    }

    /// A trip whose `type` is unreadable is kept with its temperature and no
    /// type. It used to be dropped, which also removed its temperature from
    /// the passive and critical comparisons.
    #[cfg(target_os = "linux")]
    #[test]
    fn trip_with_unread_type_is_kept_with_type_none() {
        let root = thermal_tree("trip-no-type");
        let zone = root.join("thermal_zone0");
        std::fs::write(zone.join("trip_point_0_temp"), "90000\n").unwrap();
        let overview = ThermalZoneMonitor::scan_root(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        let trips = &overview.zones[0].trip_points;
        assert_eq!(trips.len(), 1);
        assert_eq!(trips[0].trip_type, None);
        assert_eq!(trips[0].temp_mc, Some(90_000));
    }

    /// Exactly the bound's worth of trip points is complete, not truncated.
    #[cfg(target_os = "linux")]
    #[test]
    fn trip_points_at_the_bound_are_not_truncated() {
        let root = thermal_tree("trips-exact");
        write_passive_trips(&root.join("thermal_zone0"), MAX_TRIP_POINTS);
        let overview = ThermalZoneMonitor::scan_root(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        let zone = &overview.zones[0];
        assert_eq!(zone.trip_points.len(), MAX_TRIP_POINTS as usize);
        assert!(!zone.trip_points_truncated);
    }
}
