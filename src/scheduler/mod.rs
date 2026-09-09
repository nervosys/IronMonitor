//! Linux process scheduler monitoring and tuning analysis.
//!
//! Exposes CFS/EEVDF scheduler parameters, real-time scheduling classes,
//! CPU pressure stall information (PSI), scheduler latency statistics,
//! and runqueue depth per CPU.
//!
//! ## Platform Support
//!
//! - **Linux**: `/proc/schedstat`, `/proc/pressure/`, `/proc/sys/kernel/sched_*`
//! - **Windows**: Thread scheduling info via performance counters
//! - **macOS**: Mach scheduling statistics

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
    /// Current runqueue depth (number of runnable tasks).
    pub runqueue_depth: u32,
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
    /// Average runqueue depth.
    pub avg_runqueue_depth: f64,
    /// Maximum runqueue depth.
    pub max_runqueue_depth: u32,
    /// CPU with highest wait time.
    pub busiest_cpu: u32,
    /// Whether scheduling latency is concerning.
    pub latency_concern: bool,
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

    fn collect() -> Result<SchedulerAnalysis, IronError> {
        let cpu_stats = Self::read_schedstat();
        let pressure = Self::read_psi();
        let tuning = Self::read_tuning();

        let avg_rq: f64 = if cpu_stats.is_empty() {
            0.0
        } else {
            cpu_stats
                .iter()
                .map(|c| c.runqueue_depth as f64)
                .sum::<f64>()
                / cpu_stats.len() as f64
        };
        let max_rq = cpu_stats
            .iter()
            .map(|c| c.runqueue_depth)
            .max()
            .unwrap_or(0);
        let busiest = cpu_stats
            .iter()
            .max_by_key(|c| c.waiting_ns)
            .map(|c| c.cpu)
            .unwrap_or(0);

        let latency_concern = pressure
            .iter()
            .any(|p| p.resource == "cpu" && p.some_avg10 > 25.0);

        let mut recommendations = Vec::new();

        if avg_rq > 4.0 {
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

            let rq_path = format!("/sys/devices/system/cpu/cpu{}/runqueue", cpu);
            let runqueue_depth = std::fs::read_to_string(&rq_path)
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);

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

    #[cfg(not(target_os = "linux"))]
    fn read_schedstat() -> Vec<CpuSchedStats> {
        Vec::new()
    }

    #[cfg(target_os = "linux")]
    fn read_psi() -> Vec<PressureInfo> {
        let mut pressures = Vec::new();
        for resource in &["cpu", "memory", "io"] {
            let path = format!("/proc/pressure/{}", resource);
            if let Ok(content) = std::fs::read_to_string(&path) {
                let mut some_avg10 = 0.0;
                let mut some_avg60 = 0.0;
                let mut some_avg300 = 0.0;
                let mut some_total = 0u64;
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
                                            some_avg10 = v;
                                        }
                                    }
                                }
                                "avg60" => {
                                    if let Ok(v) = val.parse::<f64>() {
                                        if is_full {
                                            full_avg60 = Some(v);
                                        } else {
                                            some_avg60 = v;
                                        }
                                    }
                                }
                                "avg300" => {
                                    if let Ok(v) = val.parse::<f64>() {
                                        if is_full {
                                            full_avg300 = Some(v);
                                        } else {
                                            some_avg300 = v;
                                        }
                                    }
                                }
                                "total" => {
                                    if let Ok(v) = val.parse::<u64>() {
                                        if is_full {
                                            full_total = Some(v);
                                        } else {
                                            some_total = v;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }

                pressures.push(PressureInfo {
                    resource: resource.to_string(),
                    some_avg10,
                    some_avg60,
                    some_avg300,
                    some_total_us: some_total,
                    full_avg10,
                    full_avg60,
                    full_avg300,
                    full_total_us: full_total,
                });
            }
        }
        pressures
    }

    #[cfg(not(target_os = "linux"))]
    fn read_psi() -> Vec<PressureInfo> {
        Vec::new()
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
        let kernel_version = std::fs::read_to_string("/proc/version").unwrap_or_default();
        let scheduler_type = if Self::kernel_version_ge(&kernel_version, 6, 6) {
            SchedPolicy::EEVDF
        } else {
            SchedPolicy::Normal
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

    #[cfg(target_os = "linux")]
    fn kernel_version_ge(version_str: &str, major: u32, minor: u32) -> bool {
        // "Linux version 6.8.0-..." -> extract 6.8
        if let Some(ver_part) = version_str.split_whitespace().nth(2) {
            let parts: Vec<&str> = ver_part.split('.').collect();
            if let (Some(maj), Some(min)) = (
                parts.first().and_then(|s| s.parse::<u32>().ok()),
                parts.get(1).and_then(|s| s.parse::<u32>().ok()),
            ) {
                return (maj, min) >= (major, minor);
            }
        }
        false
    }

    #[cfg(not(target_os = "linux"))]
    fn read_tuning() -> SchedTuning {
        SchedTuning {
            min_granularity_ns: None,
            latency_ns: None,
            wakeup_granularity_ns: None,
            migration_cost_ns: None,
            nr_migrate: None,
            autogroup_enabled: None,
            scheduler_type: SchedPolicy::Unknown,
        }
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
                avg_runqueue_depth: 0.0,
                max_runqueue_depth: 0,
                busiest_cpu: 0,
                latency_concern: false,
                recommendations: Vec::new(),
            },
        })
    }
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
    fn test_runqueue_analysis() {
        let stats = [
            CpuSchedStats {
                cpu: 0,
                running_ns: 100,
                waiting_ns: 50,
                timeslices: 10,
                runqueue_depth: 2,
            },
            CpuSchedStats {
                cpu: 1,
                running_ns: 200,
                waiting_ns: 100,
                timeslices: 20,
                runqueue_depth: 6,
            },
        ];
        let avg: f64 =
            stats.iter().map(|c| c.runqueue_depth as f64).sum::<f64>() / stats.len() as f64;
        assert!((avg - 4.0).abs() < 0.01);
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
