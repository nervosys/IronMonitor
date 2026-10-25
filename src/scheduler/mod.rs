//! Linux process scheduler monitoring and tuning analysis.
//!
//! Exposes CFS/EEVDF scheduler parameters, real-time scheduling classes,
//! CPU pressure stall information (PSI), scheduler latency statistics,
//! and runqueue depth per CPU.
//!
//! ## Platform Support
//!
//! - **Linux**: `/proc/schedstat`, `/proc/pressure/`, `/proc/sys/kernel/sched_*`
//! - **Other platforms**: none. [`SchedulerMonitor::new`] returns
//!   [`IronError::UnsupportedPlatform`] rather than an empty analysis, so
//!   "cannot be read here" stays distinguishable from "nothing is queued".

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// Scheduler policy / class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchedPolicy {
    /// Completely Fair Scheduler (normal).
    Normal,
    /// EEVDF (Earliest Eligible Virtual Deadline First) — Linux 6.6+.
    EEVDF,
    /// FIFO real-time.
    Fifo,
    /// Round-robin real-time.
    RoundRobin,
    /// Batch (non-interactive).
    Batch,
    /// Idle (very low priority).
    Idle,
    /// Deadline scheduling (SCHED_DEADLINE).
    Deadline,
    Unknown,
}

impl std::fmt::Display for SchedPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "CFS/Normal"),
            Self::EEVDF => write!(f, "EEVDF"),
            Self::Fifo => write!(f, "SCHED_FIFO"),
            Self::RoundRobin => write!(f, "SCHED_RR"),
            Self::Batch => write!(f, "SCHED_BATCH"),
            Self::Idle => write!(f, "SCHED_IDLE"),
            Self::Deadline => write!(f, "SCHED_DEADLINE"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Pressure Stall Information for a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PressureInfo {
    /// Resource name (cpu, memory, io).
    pub resource: String,
    /// "some" avg10 (% of time at least one task stalled).
    pub some_avg10: f64,
    /// "some" avg60.
    pub some_avg60: f64,
    /// "some" avg300.
    pub some_avg300: f64,
    /// "some" total microseconds.
    pub some_total_us: u64,
    /// "full" avg10 (% of time all tasks stalled — not for CPU).
    pub full_avg10: Option<f64>,
    /// "full" avg60.
    pub full_avg60: Option<f64>,
    /// "full" avg300.
    pub full_avg300: Option<f64>,
    /// "full" total microseconds.
    pub full_total_us: Option<u64>,
}

/// Per-CPU scheduler statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuSchedStats {
    /// CPU index.
    pub cpu: u32,
    /// Total time spent running (nanoseconds).
    pub running_ns: u64,
    /// Total time spent waiting on runqueue (nanoseconds).
    pub waiting_ns: u64,
    /// Number of timeslices run.
    pub timeslices: u64,
    /// Current runqueue depth (number of runnable tasks), or `None`.
    ///
    /// Always `None` on mainline Linux: there is no per-CPU `runqueue` file
    /// under `/sys/devices/system/cpu/cpuN`, and the read used to fall back to
    /// 0, an idle queue, on every machine.
    pub runqueue_depth: Option<u32>,
}

/// Global scheduler tuning parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedTuning {
    /// sched_min_granularity_ns (minimum CFS timeslice).
    pub min_granularity_ns: Option<u64>,
    /// sched_latency_ns (target CFS latency).
    pub latency_ns: Option<u64>,
    /// sched_wakeup_granularity_ns.
    pub wakeup_granularity_ns: Option<u64>,
    /// sched_migration_cost_ns.
    pub migration_cost_ns: Option<u64>,
    /// sched_nr_migrate.
    pub nr_migrate: Option<u32>,
    /// kernel.sched_autogroup_enabled.
    pub autogroup_enabled: Option<bool>,
    /// Detected scheduler type.
    pub scheduler_type: SchedPolicy,
}

/// Scheduler analysis and recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerAnalysis {
    /// Per-CPU stats.
    pub cpu_stats: Vec<CpuSchedStats>,
    /// PSI data.
    pub pressure: Vec<PressureInfo>,
    /// Tuning parameters.
    pub tuning: SchedTuning,
    /// Average runqueue depth; `None` when `/proc/schedstat` gave no per-CPU
    /// rows, rather than an idle-looking 0.
    pub avg_runqueue_depth: Option<f64>,
    /// Maximum runqueue depth; `None` under the same condition.
    pub max_runqueue_depth: Option<u32>,
    /// CPU with the highest wait time; `None` under the same condition, rather
    /// than naming CPU 0.
    pub busiest_cpu: Option<u32>,
    /// Whether scheduling latency is concerning, or `None` when the CPU pressure
    /// file could not be read. It used to be `false` then, which read as "no
    /// concern" on a machine that was not measured.
    pub latency_concern: Option<bool>,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// Scheduler monitor.
pub struct SchedulerMonitor {
    analysis: SchedulerAnalysis,
}

impl SchedulerMonitor {
    /// Create a new scheduler monitor.
    pub fn new() -> Result<Self, IronError> {
        let analysis = Self::collect()?;
        Ok(Self { analysis })
    }

    /// Refresh.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.analysis = Self::collect()?;
        Ok(())
    }

    /// Get analysis.
    pub fn analysis(&self) -> &SchedulerAnalysis {
        &self.analysis
    }

    /// Get PSI data.
    pub fn pressure(&self) -> &[PressureInfo] {
        &self.analysis.pressure
    }

    #[cfg(not(target_os = "linux"))]
    fn collect() -> Result<SchedulerAnalysis, IronError> {
        // An empty analysis here read as a machine with nothing queued and no
        // pressure, on a platform that exposes neither. The reason travels
        // with the error, as the watchdog reader's does.
        Err(IronError::UnsupportedPlatform(
            "scheduler statistics are read from `/proc/schedstat` and `/proc/pressure`, which this platform does not expose"
                .into(),
        ))
    }

    #[cfg(target_os = "linux")]
    fn collect() -> Result<SchedulerAnalysis, IronError> {
        let cpu_stats = Self::read_schedstat();
        let pressure = Self::read_psi();
        let tuning = Self::read_tuning();

        // Depths that were read. Today none is, so these are `None`.
        let depths: Vec<u32> = cpu_stats.iter().filter_map(|c| c.runqueue_depth).collect();
        let avg_rq: Option<f64> = (!depths.is_empty())
            .then(|| depths.iter().map(|&d| d as f64).sum::<f64>() / depths.len() as f64);
        let max_rq = depths.iter().copied().max();
        let busiest = cpu_stats.iter().max_by_key(|c| c.waiting_ns).map(|c| c.cpu);

        // `None` without the CPU pressure file, not "no concern".
        let latency_concern = pressure
            .iter()
            .find(|p| p.resource == "cpu")
            .map(|p| p.some_avg10 > 25.0);

        let mut recommendations = Vec::new();

        if let Some(avg_rq) = avg_rq.filter(|&d| d > 4.0) {
            recommendations.push(format!(
                "High average runqueue depth ({:.1}); system may be CPU-overcommitted",
                avg_rq
            ));
        }

        if let Some(cpu_psi) = pressure.iter().find(|p| p.resource == "cpu") {
            if cpu_psi.some_avg10 > 50.0 {
                recommendations
                    .push("Severe CPU pressure (>50% stall); add cores or reduce load".into());
            } else if cpu_psi.some_avg10 > 25.0 {
                recommendations
                    .push("Moderate CPU pressure; consider workload redistribution".into());
            }
        }

        if let Some(mem_psi) = pressure.iter().find(|p| p.resource == "memory") {
            if let Some(full) = mem_psi.full_avg10 {
                if full > 10.0 {
                    recommendations
                        .push("Memory pressure detected; add RAM or reduce memory usage".into());
                }
            }
        }

        if let Some(io_psi) = pressure.iter().find(|p| p.resource == "io") {
            if let Some(full) = io_psi.full_avg10 {
                if full > 20.0 {
                    recommendations.push(
                        "I/O pressure detected; consider faster storage or I/O scheduling".into(),
                    );
                }
            }
        }

        Ok(SchedulerAnalysis {
            cpu_stats,
            pressure,
            tuning,
            avg_runqueue_depth: avg_rq,
            max_runqueue_depth: max_rq,
            busiest_cpu: busiest,
            latency_concern,
            recommendations,
        })
    }

    #[cfg(target_os = "linux")]
    fn read_schedstat() -> Vec<CpuSchedStats> {
        let content = match std::fs::read_to_string("/proc/schedstat") {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut stats = Vec::new();
        for line in content.lines() {
            if !line.starts_with("cpu") {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }
            let cpu_str = parts[0].strip_prefix("cpu").unwrap_or("0");
            let cpu: u32 = cpu_str.parse().unwrap_or(0);
            let running_ns: u64 = parts.get(7).and_then(|s| s.parse().ok()).unwrap_or(0);
            let waiting_ns: u64 = parts.get(8).and_then(|s| s.parse().ok()).unwrap_or(0);
            let timeslices: u64 = parts.get(9).and_then(|s| s.parse().ok()).unwrap_or(0);

            // No runqueue depth. The per-CPU `runqueue` file this used to read
            // is not in mainline Linux, and the fallback of 0 published an idle
            // queue on every machine.
            let runqueue_depth = None;

            stats.push(CpuSchedStats {
                cpu,
                running_ns,
                waiting_ns,
                timeslices,
                runqueue_depth,
            });
        }
        stats
    }

    #[cfg(target_os = "linux")]
    fn read_psi() -> Vec<PressureInfo> {
        let mut pressures = Vec::new();
        for resource in &["cpu", "memory", "io"] {
            let path = format!("/proc/pressure/{}", resource);
            if let Ok(content) = std::fs::read_to_string(&path) {
                // A file whose "some" line does not parse is skipped, not
                // reported as zero pressure.
                if let Some(psi) = parse_psi(resource, &content) {
                    pressures.push(psi);
                }
            }
        }
        pressures
    }

    #[cfg(target_os = "linux")]
    fn read_tuning() -> SchedTuning {
        let read_ns = |name: &str| -> Option<u64> {
            std::fs::read_to_string(format!("/proc/sys/kernel/{}", name))
                .ok()
                .and_then(|s| s.trim().parse().ok())
        };
        let read_u32 = |name: &str| -> Option<u32> {
            std::fs::read_to_string(format!("/proc/sys/kernel/{}", name))
                .ok()
                .and_then(|s| s.trim().parse().ok())
        };

        // Detect EEVDF vs CFS — EEVDF was merged in Linux 6.6
        // Unknown when `/proc/version` is unread or unparsed. It used to be
        // `Normal` then, which named a scheduler nobody had checked.
        let scheduler_type = match std::fs::read_to_string("/proc/version")
            .ok()
            .and_then(|v| Self::kernel_version(&v))
        {
            Some((major, minor)) if (major, minor) >= (6, 6) => SchedPolicy::EEVDF,
            Some(_) => SchedPolicy::Normal,
            None => SchedPolicy::Unknown,
        };

        SchedTuning {
            min_granularity_ns: read_ns("sched_min_granularity_ns"),
            latency_ns: read_ns("sched_latency_ns"),
            wakeup_granularity_ns: read_ns("sched_wakeup_granularity_ns"),
            migration_cost_ns: read_ns("sched_migration_cost_ns"),
            nr_migrate: read_u32("sched_nr_migrate"),
            autogroup_enabled: read_u32("sched_autogroup_enabled").map(|v| v != 0),
            scheduler_type,
        }
    }

    /// `(major, minor)` from a `/proc/version` line, or `None` if it does not
    /// parse. "Linux version 6.8.0-..." gives (6, 8).
    #[cfg(target_os = "linux")]
    fn kernel_version(version_str: &str) -> Option<(u32, u32)> {
        let ver_part = version_str.split_whitespace().nth(2)?;
        let mut parts = ver_part.split('.');
        let maj = parts.next()?.parse::<u32>().ok()?;
        let min = parts.next()?.parse::<u32>().ok()?;
        Some((maj, min))
    }
}

impl Default for SchedulerMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            analysis: SchedulerAnalysis {
                cpu_stats: Vec::new(),
                pressure: Vec::new(),
                tuning: SchedTuning {
                    min_granularity_ns: None,
                    latency_ns: None,
                    wakeup_granularity_ns: None,
                    migration_cost_ns: None,
                    nr_migrate: None,
                    autogroup_enabled: None,
                    scheduler_type: SchedPolicy::Unknown,
                },
                avg_runqueue_depth: None,
                max_runqueue_depth: None,
                busiest_cpu: None,
                latency_concern: None,
                recommendations: Vec::new(),
            },
        })
    }
}

/// Parses one `/proc/pressure/<resource>` file. The "some" line is required:
/// a file without it, or with any of its four fields unreadable, yields `None`
/// rather than a record of zero pressure. The "full" line is optional, since
/// cpu has none on older kernels.
#[cfg(any(target_os = "linux", test))]
fn parse_psi(resource: &str, content: &str) -> Option<PressureInfo> {
    let mut some_avg10 = None;
    let mut some_avg60 = None;
    let mut some_avg300 = None;
    let mut some_total = None;
    let mut full_avg10 = None;
    let mut full_avg60 = None;
    let mut full_avg300 = None;
    let mut full_total = None;

    for line in content.lines() {
        let is_full = line.starts_with("full");
        let is_some = line.starts_with("some");
        if !is_full && !is_some {
            continue;
        }

        for part in line.split_whitespace().skip(1) {
            if let Some((key, val)) = part.split_once('=') {
                match key {
                    "avg10" => {
                        if let Ok(v) = val.parse::<f64>() {
                            if is_full {
                                full_avg10 = Some(v);
                            } else {
                                some_avg10 = Some(v);
                            }
                        }
                    }
                    "avg60" => {
                        if let Ok(v) = val.parse::<f64>() {
                            if is_full {
                                full_avg60 = Some(v);
                            } else {
                                some_avg60 = Some(v);
                            }
                        }
                    }
                    "avg300" => {
                        if let Ok(v) = val.parse::<f64>() {
                            if is_full {
                                full_avg300 = Some(v);
                            } else {
                                some_avg300 = Some(v);
                            }
                        }
                    }
                    "total" => {
                        if let Ok(v) = val.parse::<u64>() {
                            if is_full {
                                full_total = Some(v);
                            } else {
                                some_total = Some(v);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let (Some(some_avg10), Some(some_avg60), Some(some_avg300), Some(some_total_us)) =
        (some_avg10, some_avg60, some_avg300, some_total)
    else {
        return None;
    };
    Some(PressureInfo {
        resource: resource.to_string(),
        some_avg10,
        some_avg60,
        some_avg300,
        some_total_us,
        full_avg10,
        full_avg60,
        full_avg300,
        full_total_us: full_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sched_policy_display() {
        assert_eq!(SchedPolicy::Normal.to_string(), "CFS/Normal");
        assert_eq!(SchedPolicy::EEVDF.to_string(), "EEVDF");
        assert_eq!(SchedPolicy::Deadline.to_string(), "SCHED_DEADLINE");
    }

    #[test]
    fn test_pressure_analysis() {
        let pressure = [PressureInfo {
            resource: "cpu".into(),
            some_avg10: 55.0,
            some_avg60: 30.0,
            some_avg300: 20.0,
            some_total_us: 1000000,
            full_avg10: None,
            full_avg60: None,
            full_avg300: None,
            full_total_us: None,
        }];
        // With high CPU pressure, concern should be true
        let concern = pressure
            .iter()
            .any(|p| p.resource == "cpu" && p.some_avg10 > 25.0);
        assert!(concern);
    }

    #[test]
    fn a_psi_file_without_a_readable_some_line_reports_no_pressure() {
        // No "some" line at all: nothing to report, not zero pressure.
        let full_only = "full avg10=1.00 avg60=1.00 avg300=1.00 total=5\n";
        assert!(parse_psi("memory", full_only).is_none());

        // A "some" line with one unreadable field is rejected as a whole.
        let malformed = "some avg10=n/a avg60=0.00 avg300=0.00 total=0\n";
        assert!(parse_psi("cpu", malformed).is_none());

        // A complete file parses, and the absent "full" line stays None.
        let good = "some avg10=1.50 avg60=2.00 avg300=3.00 total=4000\n";
        let psi = parse_psi("cpu", good).unwrap();
        assert_eq!(psi.some_total_us, 4000);
        assert_eq!(psi.full_avg10, None);
    }

    #[test]
    fn test_runqueue_analysis() {
        let stats = [
            CpuSchedStats {
                cpu: 0,
                running_ns: 100,
                waiting_ns: 50,
                timeslices: 10,
                runqueue_depth: Some(2),
            },
            CpuSchedStats {
                cpu: 1,
                running_ns: 200,
                waiting_ns: 100,
                timeslices: 20,
                runqueue_depth: Some(6),
            },
        ];
        let avg: f64 = stats
            .iter()
            .filter_map(|c| c.runqueue_depth)
            .map(|d| d as f64)
            .sum::<f64>()
            / stats.len() as f64;
        assert!((avg - 4.0).abs() < 0.01);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn a_platform_without_proc_declines_rather_than_reporting_an_idle_scheduler() {
        match SchedulerMonitor::new() {
            Err(IronError::UnsupportedPlatform(why)) => assert!(why.contains("/proc/schedstat")),
            Err(e) => panic!("expected UnsupportedPlatform, got {e}"),
            Ok(m) => panic!(
                "a platform with no /proc produced an analysis: {:?}",
                m.analysis()
            ),
        }
    }

    #[test]
    fn test_monitor_default() {
        let monitor = SchedulerMonitor::default();
        let _analysis = monitor.analysis();
    }

    #[test]
    fn test_serialization() {
        let psi = PressureInfo {
            resource: "cpu".into(),
            some_avg10: 5.0,
            some_avg60: 3.0,
            some_avg300: 2.0,
            some_total_us: 100000,
            full_avg10: None,
            full_avg60: None,
            full_avg300: None,
            full_total_us: None,
        };
        let json = serde_json::to_string(&psi).unwrap();
        assert!(json.contains("cpu"));
        let _: PressureInfo = serde_json::from_str(&json).unwrap();
    }
}
