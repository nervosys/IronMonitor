//! Intel RAPL and AMD power capping energy monitoring.
//!
//! Reads per-domain energy counters (package, core, uncore, DRAM) for
//! real-time power draw measurement. Supports Intel RAPL via powercap sysfs,
//! AMD RAPL via the same interface, and Windows via LibreHardwareMonitor/RAPL MSRs.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/class/powercap/intel-rapl:*/` (works for both Intel and AMD)
//! - **Windows**: Inferred from CPU TDP and utilization
//! - **macOS**: `powermetrics` or `sudo powermetrics` for per-domain power

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::error::IronError;

/// Power domain type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PowerDomain {
    /// Full CPU package (socket).
    Package,
    /// CPU cores only.
    Core,
    /// Uncore (memory controller, caches, interconnect).
    Uncore,
    /// DRAM / memory subsystem.
    Dram,
    /// Platform-level (PSys on Intel).
    Platform,
    /// GPU (integrated).
    Gpu,
    /// Other/unknown sub-domain.
    Other,
}

impl std::fmt::Display for PowerDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Package => write!(f, "Package"),
            Self::Core => write!(f, "Core"),
            Self::Uncore => write!(f, "Uncore"),
            Self::Dram => write!(f, "DRAM"),
            Self::Platform => write!(f, "Platform"),
            Self::Gpu => write!(f, "GPU"),
            Self::Other => write!(f, "Other"),
        }
    }
}

/// Energy reading for a single domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyReading {
    /// Domain type.
    pub domain: PowerDomain,
    /// Domain name as reported by the system.
    pub name: String,
    /// Socket / package index, parsed from the `intel-rapl:<N>` directory name.
    pub socket: u32,
    /// Current energy counter in microjoules.
    pub energy_uj: u64,
    /// Counter range in microjoules -- the counter wraps at this value -- or
    /// `None` where `max_energy_range_uj` was not read.
    ///
    /// **This fell back to `u64::MAX`, and was published as `measured`.** An
    /// unreadable range reached agents as `power.rapl.N.max_energy_range =
    /// 18446744073709551615`, a type's maximum stated as an observation. It
    /// also fed the wrap arithmetic in `compute_power`, where a counter that
    /// genuinely wrapped produced a delta near 1.8e19 uJ -- a power reading in
    /// the terawatts for that interval.
    pub max_energy_range_uj: Option<u64>,
    /// Current power limit (constraint) in microwatts, if available.
    pub power_limit_uw: Option<u64>,
    /// Whether this domain is enabled, or `None` where its `enabled` attribute
    /// was not read or did not hold `0` or `1`.
    ///
    /// **This was `bool`, and an unread attribute became `true` through
    /// `unwrap_or(true)`.** The resolver then published that as a measurement.
    pub enabled: Option<bool>,
}

/// Power snapshot with computed wattage between two readings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerSnapshot {
    /// Per-domain power in watts.
    pub domain_watts: HashMap<String, f64>,
    /// Total package power in watts (sum of package domains), or `None` when
    /// no package domain produced a delta this interval.
    ///
    /// The three totals were bare `f64` accumulators starting at `0.0`, so a
    /// machine drawing no power, a platform with no such domain, and an
    /// interval in which every counter wrapped without a known range all read
    /// `0.0`. `Some(0.0)` now means domains were read and summed to zero.
    pub total_package_watts: Option<f64>,
    /// Total core power in watts, `None` as for the package total.
    pub total_core_watts: Option<f64>,
    /// Total DRAM power in watts, `None` as for the package total.
    pub total_dram_watts: Option<f64>,
    /// Platform (PSys) power in watts, `None` as for the package total.
    ///
    /// Kept apart from `total_package_watts`: PSys is whole-platform power and
    /// already includes the package, so adding it to the package total counted
    /// the package twice and inflated `efficiency()`.
    pub total_platform_watts: Option<f64>,
    /// Duration of measurement in seconds.
    pub measurement_duration_secs: f64,
    /// Per-domain energy delta in microjoules.
    pub energy_delta_uj: HashMap<String, u64>,
}

/// Power efficiency analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerEfficiency {
    /// Average package power in watts.
    pub avg_package_watts: f64,
    /// Peak package power observed (in watts).
    pub peak_package_watts: f64,
    /// Fraction of the caller-supplied TDP not drawn by the package:
    /// `1 - min(package_watts / tdp, 1)`, in `0.0..=1.0`, or `0.0` when the TDP
    /// is not positive.
    ///
    /// This is headroom against a TDP the caller supplied. It is not useful work
    /// per watt. The field used to be named `efficiency_ratio` and documented as
    /// useful work over total power, which the computation never measured.
    pub tdp_headroom_fraction: f64,
    /// Whether the system is within its TDP envelope.
    pub within_tdp: bool,
    /// Estimated TDP in watts.
    pub estimated_tdp: f64,
    /// Power headroom (TDP - current power) in watts.
    pub headroom_watts: f64,
    /// Recommendations for power optimization.
    pub recommendations: Vec<String>,
}

/// RAPL / power capping monitor.
pub struct RaplMonitor {
    /// Current energy readings.
    readings: Vec<EnergyReading>,
    /// Previous readings for delta computation.
    prev_readings: Vec<EnergyReading>,
    /// Timestamp of previous reading.
    prev_time: Option<Instant>,
    /// Latest power snapshot.
    snapshot: Option<PowerSnapshot>,
    /// Historical peak power.
    peak_watts: f64,
    /// Running average power.
    avg_watts: f64,
    /// Number of samples for running average.
    sample_count: u64,
}

impl RaplMonitor {
    /// Create a new RAPL monitor.
    pub fn new() -> Result<Self, IronError> {
        let readings = Self::read_energy()?;
        Ok(Self {
            readings: readings.clone(),
            prev_readings: readings,
            prev_time: Some(Instant::now()),
            snapshot: None,
            peak_watts: 0.0,
            avg_watts: 0.0,
            sample_count: 0,
        })
    }

    /// Refresh readings and compute power deltas.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        let now = Instant::now();
        self.prev_readings = self.readings.clone();
        self.readings = Self::read_energy()?;

        if let Some(prev_time) = self.prev_time {
            let elapsed = now.duration_since(prev_time);
            if elapsed > Duration::from_millis(10) {
                self.snapshot = Some(Self::compute_power(
                    &self.prev_readings,
                    &self.readings,
                    elapsed,
                ));

                // Only an interval that measured package power moves the peak
                // and the running average; one that measured nothing used to
                // pull the average toward zero.
                if let Some(package) = self.snapshot.as_ref().and_then(|s| s.total_package_watts) {
                    if package > self.peak_watts {
                        self.peak_watts = package;
                    }
                    self.sample_count += 1;
                    self.avg_watts += (package - self.avg_watts) / self.sample_count as f64;
                }
            }
        }

        self.prev_time = Some(now);
        Ok(())
    }

    /// Get current energy readings.
    pub fn readings(&self) -> &[EnergyReading] {
        &self.readings
    }

    /// Get latest power snapshot (requires at least one `refresh()`).
    pub fn snapshot(&self) -> Option<&PowerSnapshot> {
        self.snapshot.as_ref()
    }

    /// Power efficiency analysis, or `None` until an interval has measured
    /// package power.
    ///
    /// Without one this used `0.0` as the current draw, and so reported the
    /// machine within its TDP, with all of it as headroom and an efficiency
    /// ratio of 1.0 -- a complete analysis of a reading that was never taken.
    pub fn efficiency(&self, estimated_tdp: f64) -> Option<PowerEfficiency> {
        let current_watts = self.snapshot.as_ref()?.total_package_watts?;

        let within_tdp = current_watts <= estimated_tdp;
        let headroom = (estimated_tdp - current_watts).max(0.0);

        let tdp_headroom_fraction = if estimated_tdp > 0.0 {
            (1.0 - (current_watts / estimated_tdp).min(1.0)).max(0.0)
        } else {
            0.0
        };

        let mut recommendations = Vec::new();
        if current_watts > estimated_tdp * 0.95 {
            recommendations.push("Running near TDP limit; may thermal throttle".into());
        }
        if self.peak_watts > estimated_tdp * 1.2 {
            recommendations.push(format!(
                "Peak power ({:.1}W) exceeded TDP by {:.0}%",
                self.peak_watts,
                ((self.peak_watts / estimated_tdp) - 1.0) * 100.0
            ));
        }
        if current_watts < estimated_tdp * 0.1 && current_watts > 0.0 {
            recommendations.push("Very low power draw; system may be mostly idle".into());
        }
        if headroom > estimated_tdp * 0.5 {
            recommendations.push("Significant power headroom available for turbo boost".into());
        }

        Some(PowerEfficiency {
            avg_package_watts: self.avg_watts,
            peak_package_watts: self.peak_watts,
            tdp_headroom_fraction,
            within_tdp,
            estimated_tdp,
            headroom_watts: headroom,
            recommendations,
        })
    }

    fn compute_power(
        prev: &[EnergyReading],
        curr: &[EnergyReading],
        elapsed: Duration,
    ) -> PowerSnapshot {
        let secs = elapsed.as_secs_f64();
        let mut domain_watts = HashMap::new();
        let mut energy_delta_uj = HashMap::new();
        let mut total_package: Option<f64> = None;
        let mut total_core: Option<f64> = None;
        let mut total_dram: Option<f64> = None;
        let mut total_platform: Option<f64> = None;

        for c in curr {
            // Find matching previous reading
            if let Some(p) = prev
                .iter()
                .find(|p| p.name == c.name && p.socket == c.socket)
            {
                let delta = if c.energy_uj >= p.energy_uj {
                    c.energy_uj - p.energy_uj
                } else {
                    // Counter wrapped. Recovering the delta needs the range it
                    // wrapped at; without one there is no delta for this
                    // interval, and the domain is skipped rather than given
                    // one computed against `u64::MAX`.
                    let Some(range) = c.max_energy_range_uj else {
                        continue;
                    };
                    (range - p.energy_uj) + c.energy_uj
                };

                let watts = delta as f64 / (secs * 1_000_000.0);
                let key = format!("socket{}:{}", c.socket, c.name);
                domain_watts.insert(key.clone(), watts);
                energy_delta_uj.insert(key, delta);

                let total = match c.domain {
                    PowerDomain::Package => &mut total_package,
                    // PSys includes the package, so it is kept out of the
                    // package total rather than added to it.
                    PowerDomain::Platform => &mut total_platform,
                    PowerDomain::Core => &mut total_core,
                    PowerDomain::Dram => &mut total_dram,
                    _ => continue,
                };
                *total = Some(total.unwrap_or(0.0) + watts);
            }
        }

        PowerSnapshot {
            domain_watts,
            total_package_watts: total_package,
            total_core_watts: total_core,
            total_dram_watts: total_dram,
            total_platform_watts: total_platform,
            measurement_duration_secs: secs,
            energy_delta_uj,
        }
    }

    #[cfg(target_os = "linux")]
    fn read_energy() -> Result<Vec<EnergyReading>, IronError> {
        let powercap = std::path::Path::new("/sys/class/powercap");

        // A missing powercap class means this kernel exposes no RAPL interface.
        // That is an unsupported host, not one with zero energy domains, so it is
        // an error and the resolver reports it as such.
        if !powercap.exists() {
            return Err(IronError::UnsupportedPlatform(
                "/sys/class/powercap does not exist: this kernel exposes no powercap (RAPL) interface"
                    .into(),
            ));
        }

        // A directory that exists but cannot be listed is a failed read, not an
        // empty interface.
        let entries = std::fs::read_dir(powercap)
            .map_err(|e| IronError::System(format!("failed to read /sys/class/powercap: {e}")))?;

        let mut readings = Vec::new();

        // Read top-level RAPL domains (intel-rapl:0, intel-rapl:1, ...)
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("intel-rapl:") {
                continue;
            }

            let path = entry.path();
            // A domain whose suffix does not parse cannot be attributed to a
            // socket. It is skipped with its sub-domains, which share its
            // socket, rather than reported as socket 0.
            let Some(socket) = name
                .strip_prefix("intel-rapl:")
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };

            // Read the top-level domain
            if let Some(reading) = Self::read_domain(&path, socket) {
                readings.push(reading);
            }

            // Read sub-domains (intel-rapl:0:0, intel-rapl:0:1, ...)
            if let Ok(sub_entries) = std::fs::read_dir(&path) {
                for sub in sub_entries.flatten() {
                    let sub_name = sub.file_name().to_string_lossy().to_string();
                    if sub_name.starts_with("intel-rapl:") {
                        if let Some(reading) = Self::read_domain(&sub.path(), socket) {
                            readings.push(reading);
                        }
                    }
                }
            }
        }

        Ok(readings)
    }

    /// The sysfs `enabled` attribute holds `0` or `1`. Anything else is not a
    /// reading of it, so it is `None`. This used to read as `true` both when the
    /// file was missing and when it held something unexpected.
    #[cfg(any(target_os = "linux", test))]
    fn parse_enabled(raw: &str) -> Option<bool> {
        match raw.trim() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        }
    }

    #[cfg(target_os = "linux")]
    fn read_domain(path: &std::path::Path, socket: u32) -> Option<EnergyReading> {
        let name = std::fs::read_to_string(path.join("name"))
            .ok()?
            .trim()
            .to_string();

        let energy_uj = std::fs::read_to_string(path.join("energy_uj"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())?;

        let max_energy_range_uj = std::fs::read_to_string(path.join("max_energy_range_uj"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok());

        let enabled = std::fs::read_to_string(path.join("enabled"))
            .ok()
            .and_then(|s| Self::parse_enabled(&s));

        // Try to read power limit
        let power_limit_uw = std::fs::read_to_string(path.join("constraint_0_power_limit_uw"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok());

        let domain = match name.as_str() {
            "package-0" | "package-1" | "package-2" | "package-3" => PowerDomain::Package,
            "core" => PowerDomain::Core,
            "uncore" => PowerDomain::Uncore,
            "dram" => PowerDomain::Dram,
            "psys" => PowerDomain::Platform,
            _ => PowerDomain::Other,
        };

        Some(EnergyReading {
            domain,
            name,
            socket,
            energy_uj,
            max_energy_range_uj,
            power_limit_uw,
            enabled,
        })
    }

    // The two platforms below return an error carrying the reason rather than an
    // empty list. An empty list is what a Linux box with no RAPL zones returns,
    // and collapsing "this platform has no interface" into it means a caller
    // cannot tell an unsupported host from an unpowered one.
    #[cfg(target_os = "windows")]
    fn read_energy() -> Result<Vec<EnergyReading>, IronError> {
        Err(IronError::UnsupportedPlatform(
            "Windows exposes no RAPL interface to unprivileged code: the MSRs behind it need a kernel driver, and there is no sysfs equivalent"
                .into(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn read_energy() -> Result<Vec<EnergyReading>, IronError> {
        Err(IronError::PermissionDenied(
            "macOS exposes package energy only through powermetrics, which requires root".into(),
        ))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn read_energy() -> Result<Vec<EnergyReading>, IronError> {
        Err(IronError::UnsupportedPlatform(
            "RAPL energy monitoring is not implemented for this platform".into(),
        ))
    }
}

impl Default for RaplMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            readings: Vec::new(),
            prev_readings: Vec::new(),
            prev_time: None,
            snapshot: None,
            peak_watts: 0.0,
            avg_watts: 0.0,
            sample_count: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_computation() {
        let prev = vec![EnergyReading {
            domain: PowerDomain::Package,
            name: "package-0".into(),
            socket: 0,
            energy_uj: 1_000_000, // 1 joule
            max_energy_range_uj: Some(u64::MAX),
            power_limit_uw: Some(125_000_000),
            enabled: Some(true),
        }];
        let curr = vec![EnergyReading {
            domain: PowerDomain::Package,
            name: "package-0".into(),
            socket: 0,
            energy_uj: 11_000_000, // 11 joules (10J delta)
            max_energy_range_uj: Some(u64::MAX),
            power_limit_uw: Some(125_000_000),
            enabled: Some(true),
        }];
        let elapsed = Duration::from_secs(1);
        let snap = RaplMonitor::compute_power(&prev, &curr, elapsed);
        // 10_000_000 uJ / (1s * 1_000_000) = 10W
        assert!((snap.total_package_watts.unwrap() - 10.0).abs() < 0.1);
    }

    #[test]
    fn test_counter_wrap() {
        let prev = vec![EnergyReading {
            domain: PowerDomain::Core,
            name: "core".into(),
            socket: 0,
            energy_uj: 90_000_000,
            max_energy_range_uj: Some(100_000_000),
            power_limit_uw: None,
            enabled: Some(true),
        }];
        let curr = vec![EnergyReading {
            domain: PowerDomain::Core,
            name: "core".into(),
            socket: 0,
            energy_uj: 5_000_000, // Wrapped around
            max_energy_range_uj: Some(100_000_000),
            power_limit_uw: None,
            enabled: Some(true),
        }];
        let elapsed = Duration::from_secs(1);
        let snap = RaplMonitor::compute_power(&prev, &curr, elapsed);
        // Delta = (100M - 90M) + 5M = 15M uJ = 15W
        assert!((snap.total_core_watts.unwrap() - 15.0).abs() < 0.1);
    }

    /// A wrap with no known range has no recoverable delta, so the domain is
    /// left out of the interval rather than given one computed against
    /// `u64::MAX`.
    ///
    /// Before, an unreadable `max_energy_range_uj` fell back to `u64::MAX`, and
    /// this exact input produced a delta of about 1.8e19 uJ: a core drawing
    /// some eighteen terawatts for one second.
    #[test]
    fn a_wrap_with_no_known_range_reports_no_power_rather_than_terawatts() {
        let reading = |energy_uj| EnergyReading {
            domain: PowerDomain::Core,
            name: "core".into(),
            socket: 0,
            energy_uj,
            max_energy_range_uj: None,
            power_limit_uw: None,
            enabled: Some(true),
        };
        let prev = vec![reading(90_000_000)];
        let curr = vec![reading(5_000_000)]; // wrapped, range unknown

        let snap = RaplMonitor::compute_power(&prev, &curr, Duration::from_secs(1));

        assert!(
            snap.domain_watts.is_empty(),
            "no delta is recoverable, so the domain must be absent: {:?}",
            snap.domain_watts
        );
        // Nothing was summed, and the total now says so. This used to assert
        // only `< 1.0`, because a bare `f64` could not distinguish "no domain
        // produced a delta" from "the cores drew no power".
        assert_eq!(
            snap.total_core_watts, None,
            "no core domain produced a delta, so there is no core total"
        );
    }

    /// PSys is whole-platform power and already includes the package, so it must
    /// not be added to the package total. Before, it was, and a 60 W package
    /// under a 90 W PSys read as 150 W of package power.
    #[test]
    fn platform_domain_is_kept_out_of_the_package_total() {
        let reading = |domain, name: &str, energy_uj| EnergyReading {
            domain,
            name: name.into(),
            socket: 0,
            energy_uj,
            max_energy_range_uj: Some(u64::MAX),
            power_limit_uw: None,
            enabled: Some(true),
        };
        let prev = vec![
            reading(PowerDomain::Package, "package-0", 0),
            reading(PowerDomain::Platform, "psys", 0),
        ];
        let curr = vec![
            reading(PowerDomain::Package, "package-0", 60_000_000),
            reading(PowerDomain::Platform, "psys", 90_000_000),
        ];

        let snap = RaplMonitor::compute_power(&prev, &curr, Duration::from_secs(1));

        assert!((snap.total_package_watts.unwrap() - 60.0).abs() < 0.1);
        assert!((snap.total_platform_watts.unwrap() - 90.0).abs() < 0.1);
    }

    #[test]
    fn enabled_attribute_is_none_unless_it_reads_zero_or_one() {
        assert_eq!(RaplMonitor::parse_enabled("1\n"), Some(true));
        assert_eq!(RaplMonitor::parse_enabled("0"), Some(false));
        assert_eq!(RaplMonitor::parse_enabled(""), None);
        assert_eq!(RaplMonitor::parse_enabled("yes"), None);
    }

    #[test]
    fn test_efficiency_analysis() {
        let monitor = RaplMonitor {
            readings: Vec::new(),
            prev_readings: Vec::new(),
            prev_time: None,
            snapshot: Some(PowerSnapshot {
                domain_watts: HashMap::new(),
                total_package_watts: Some(85.0),
                total_core_watts: Some(60.0),
                total_dram_watts: Some(8.0),
                total_platform_watts: None,
                measurement_duration_secs: 1.0,
                energy_delta_uj: HashMap::new(),
            }),
            peak_watts: 135.0,
            avg_watts: 78.0,
            sample_count: 100,
        };

        let eff = monitor
            .efficiency(125.0)
            .expect("a package total was measured");
        assert!(!eff.within_tdp || eff.headroom_watts >= 0.0);
        assert!(eff.avg_package_watts > 0.0);
        assert!(eff.peak_package_watts > 100.0);
    }

    /// No measured package power, no analysis -- rather than one that treats
    /// the missing reading as zero watts and finds all of the TDP free.
    #[test]
    fn no_measured_package_power_means_no_efficiency_analysis() {
        let monitor = RaplMonitor {
            readings: Vec::new(),
            prev_readings: Vec::new(),
            prev_time: None,
            snapshot: None,
            peak_watts: 0.0,
            avg_watts: 0.0,
            sample_count: 0,
        };
        assert!(monitor.efficiency(125.0).is_none());
    }

    #[test]
    fn test_power_domain_display() {
        assert_eq!(PowerDomain::Package.to_string(), "Package");
        assert_eq!(PowerDomain::Core.to_string(), "Core");
        assert_eq!(PowerDomain::Dram.to_string(), "DRAM");
    }

    #[test]
    fn test_monitor_default() {
        let monitor = RaplMonitor::default();
        let _readings = monitor.readings();
    }

    #[test]
    fn test_serialization() {
        let reading = EnergyReading {
            domain: PowerDomain::Package,
            name: "package-0".into(),
            socket: 0,
            energy_uj: 12345678,
            max_energy_range_uj: Some(u64::MAX),
            power_limit_uw: Some(125_000_000),
            enabled: Some(true),
        };
        let json = serde_json::to_string(&reading).unwrap();
        assert!(json.contains("package-0"));
        let _: EnergyReading = serde_json::from_str(&json).unwrap();
    }
}
