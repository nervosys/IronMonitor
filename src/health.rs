//! System Health Scoring and Alerts
//!
//! Provides overall system health metrics and threshold-based alerting.
//! Inspired by monitoring tools that provide quick health overviews.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonitor::health::{SystemHealth, HealthCheck, HealthStatus};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let health = SystemHealth::check()?;
//!
//! println!("Overall Health: {}", health.summary());
//!
//! for check in &health.checks {
//!     println!("  {}: {:?} - {}", check.name, check.status, check.message);
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::Result;
use crate::gpu::GpuCollection;
use serde::{Deserialize, Serialize};

/// Health status level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Everything is fine
    Healthy,
    /// Minor issues, system functioning normally
    Good,
    /// Some concerns, may need attention
    Warning,
    /// Significant issues that need addressing
    Critical,
    /// Unable to determine status
    Unknown,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "✓ Healthy"),
            HealthStatus::Good => write!(f, "● Good"),
            HealthStatus::Warning => write!(f, "⚠ Warning"),
            HealthStatus::Critical => write!(f, "✗ Critical"),
            HealthStatus::Unknown => write!(f, "? Unknown"),
        }
    }
}

/// Individual health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Name of the check
    pub name: String,
    /// Category of the check
    pub category: String,
    /// Status of this check
    pub status: HealthStatus,
    /// Score contribution (0-100), or `None` for an `Unknown` check. An unknown
    /// check is not a zero: scored 0, it dragged every average it joined down.
    pub score: Option<u8>,
    /// Human-readable message
    pub message: String,
    /// Current value (for metrics)
    pub value: Option<f64>,
    /// Threshold that triggered warning/critical
    pub threshold: Option<f64>,
}

impl HealthCheck {
    /// Create a new health check
    pub fn new(name: &str, category: &str) -> Self {
        Self {
            name: name.to_string(),
            category: category.to_string(),
            status: HealthStatus::Unknown,
            score: None,
            message: String::new(),
            value: None,
            threshold: None,
        }
    }

    /// Set status and generate score
    pub fn with_status(mut self, status: HealthStatus, message: &str) -> Self {
        self.status = status;
        self.message = message.to_string();
        self.score = match status {
            HealthStatus::Healthy => Some(100),
            HealthStatus::Good => Some(85),
            HealthStatus::Warning => Some(50),
            HealthStatus::Critical => Some(10),
            HealthStatus::Unknown => None,
        };
        self
    }

    /// Set value and threshold
    pub fn with_value(mut self, value: f64, threshold: Option<f64>) -> Self {
        self.value = Some(value);
        self.threshold = threshold;
        self
    }
}

/// System health thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthThresholds {
    /// CPU usage warning threshold (%)
    pub cpu_warning: f32,
    /// CPU usage critical threshold (%)
    pub cpu_critical: f32,
    /// Memory usage warning threshold (%)
    pub memory_warning: f32,
    /// Memory usage critical threshold (%)
    pub memory_critical: f32,
    /// GPU temperature warning threshold (°C)
    pub gpu_temp_warning: u32,
    /// GPU temperature critical threshold (°C)
    pub gpu_temp_critical: u32,
    /// GPU memory warning threshold (%)
    pub gpu_memory_warning: f32,
    /// GPU memory critical threshold (%)
    pub gpu_memory_critical: f32,
    /// Disk usage warning threshold (%)
    pub disk_warning: f32,
    /// Disk usage critical threshold (%)
    pub disk_critical: f32,
    /// Swap usage warning threshold (%)
    pub swap_warning: f32,
    /// Swap usage critical threshold (%)
    pub swap_critical: f32,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            cpu_warning: 80.0,
            cpu_critical: 95.0,
            memory_warning: 80.0,
            memory_critical: 95.0,
            gpu_temp_warning: 80,
            gpu_temp_critical: 95,
            gpu_memory_warning: 85.0,
            gpu_memory_critical: 95.0,
            disk_warning: 85.0,
            disk_critical: 95.0,
            swap_warning: 50.0,
            swap_critical: 80.0,
        }
    }
}

/// Overall system health assessment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    /// Overall health status
    pub status: HealthStatus,
    /// Overall health score (0-100), or `None` when no check produced a score.
    pub score: Option<u8>,
    /// Individual health checks
    pub checks: Vec<HealthCheck>,
    /// Number of healthy checks
    pub healthy_count: usize,
    /// Number of warning checks
    pub warning_count: usize,
    /// Number of critical checks
    pub critical_count: usize,
    /// Number of checks whose reading could not be taken. Each is present in
    /// `checks` with status `Unknown` and no score. The overall status is never
    /// `Healthy` or `Good` while this is non-zero, and `score` is the mean over
    /// the checks that ran only.
    #[serde(default)]
    pub unknown_count: usize,
    /// Timestamp of check
    pub timestamp: std::time::SystemTime,
}

/// A check for a reading that was attempted and failed. It carries no score
/// and is counted in `SystemHealth::unknown_count`, so a failed read can never
/// look like a passed one.
fn unreadable(name: &str, category: &str, reason: &str) -> HealthCheck {
    HealthCheck::new(name, category).with_status(HealthStatus::Unknown, reason)
}

impl SystemHealth {
    /// Perform a full system health check with default thresholds
    pub fn check() -> Result<Self> {
        Self::check_with_thresholds(&HealthThresholds::default())
    }

    /// Perform a full system health check with custom thresholds
    pub fn check_with_thresholds(thresholds: &HealthThresholds) -> Result<Self> {
        let mut checks = Vec::new();

        // CPU Health Check
        // Reads the platform. This called `CpuStats::new()`, a zero-constructor,
        // so every CPU health check saw 100% idle and could never report high
        // usage — the check existed and could not fire.
        let cpu_read = crate::stats::platform_cpu_stats();
        if let Err(e) = &cpu_read {
            checks.push(unreadable(
                "CPU Usage",
                "CPU",
                &format!("CPU statistics could not be read: {e}"),
            ));
        }
        if let Ok(cpu) = cpu_read {
            let cpu_usage = 100.0 - cpu.total.idle;
            let (status, message) = if cpu_usage >= thresholds.cpu_critical {
                (
                    HealthStatus::Critical,
                    format!("CPU usage critically high: {:.1}%", cpu_usage),
                )
            } else if cpu_usage >= thresholds.cpu_warning {
                (
                    HealthStatus::Warning,
                    format!("CPU usage elevated: {:.1}%", cpu_usage),
                )
            } else if cpu_usage >= 50.0 {
                (
                    HealthStatus::Good,
                    format!("CPU usage moderate: {:.1}%", cpu_usage),
                )
            } else {
                (
                    HealthStatus::Healthy,
                    format!("CPU usage normal: {:.1}%", cpu_usage),
                )
            };

            checks.push(
                HealthCheck::new("CPU Usage", "CPU")
                    .with_status(status, &message)
                    .with_value(cpu_usage as f64, Some(thresholds.cpu_warning as f64)),
            );
        }

        // Memory Health Check
        // Reads the platform, for the reason above.
        let mem_read = crate::stats::platform_memory_stats();
        if let Err(e) = &mem_read {
            let reason = format!("memory statistics could not be read: {e}");
            checks.push(unreadable("Memory Usage", "Memory", &reason));
            checks.push(unreadable("Swap Usage", "Memory", &reason));
        }
        if let Ok(mem) = mem_read {
            // No RAM total was read, so there is no usage to judge. The check is
            // reported Unknown rather than passed or failed on a guess.
            if let Some(mem_usage) = mem.ram_usage_percent() {
                let (status, message) = if mem_usage >= thresholds.memory_critical {
                    (
                        HealthStatus::Critical,
                        format!("Memory usage critically high: {:.1}%", mem_usage),
                    )
                } else if mem_usage >= thresholds.memory_warning {
                    (
                        HealthStatus::Warning,
                        format!("Memory usage elevated: {:.1}%", mem_usage),
                    )
                } else if mem_usage >= 60.0 {
                    (
                        HealthStatus::Good,
                        format!("Memory usage moderate: {:.1}%", mem_usage),
                    )
                } else {
                    (
                        HealthStatus::Healthy,
                        format!("Memory usage normal: {:.1}%", mem_usage),
                    )
                };

                checks.push(
                    HealthCheck::new("Memory Usage", "Memory")
                        .with_status(status, &message)
                        .with_value(mem_usage as f64, Some(thresholds.memory_warning as f64)),
                );
            } else {
                checks.push(unreadable(
                    "Memory Usage",
                    "Memory",
                    "RAM total was not reported by this platform",
                ));
            }

            // Swap check.
            //
            // Three cases, and `total_or_zero() > 0` collapsed two of them: a
            // pagefile in use, a machine with none configured, and a pagefile
            // this platform did not report. The last one silently produced no
            // check at all -- and a health report with a check missing reads
            // as one where the check passed. `HealthStatus::Unknown` exists
            // for precisely this and was not being used.
            if mem.swap.total.is_none() {
                checks.push(HealthCheck::new("Swap Usage", "Memory").with_status(
                    HealthStatus::Unknown,
                    "swap was not reported by this platform",
                ));
            } else if mem.swap.total_or_zero() > 0 && mem.swap.used.is_none() {
                // The total was read and the used figure was not. Taking the used
                // figure as zero reported "Swap usage minimal: 0.0%" as a passed
                // check for swap whose use was never read.
                checks.push(unreadable(
                    "Swap Usage",
                    "Memory",
                    "swap usage was not reported",
                ));
            } else if mem.swap.total_or_zero() > 0 {
                let swap_usage =
                    (mem.swap.used_or_zero() as f64 / mem.swap.total_or_zero() as f64) * 100.0;
                let (status, message) = if swap_usage >= thresholds.swap_critical as f64 {
                    (
                        HealthStatus::Critical,
                        format!("Swap usage critically high: {:.1}%", swap_usage),
                    )
                } else if swap_usage >= thresholds.swap_warning as f64 {
                    (
                        HealthStatus::Warning,
                        format!("Swap usage elevated: {:.1}%", swap_usage),
                    )
                } else if swap_usage > 10.0 {
                    (
                        HealthStatus::Good,
                        format!("Swap in use: {:.1}%", swap_usage),
                    )
                } else {
                    (
                        HealthStatus::Healthy,
                        format!("Swap usage minimal: {:.1}%", swap_usage),
                    )
                };

                checks.push(
                    HealthCheck::new("Swap Usage", "Memory")
                        .with_status(status, &message)
                        .with_value(swap_usage, Some(thresholds.swap_warning as f64)),
                );
            }
        }

        // GPU Health Checks
        let gpu_read = GpuCollection::auto_detect();
        if let Err(e) = &gpu_read {
            checks.push(unreadable(
                "GPU Detection",
                "GPU",
                &format!("GPU enumeration failed: {e}"),
            ));
        }
        if let Ok(gpus) = gpu_read {
            // An enumeration that found no adapters is reported, not left out: a
            // report with no GPU row would read as a GPU check that passed.
            if gpus.gpus().is_empty() {
                checks.push(unreadable(
                    "GPU Detection",
                    "GPU",
                    "no GPU adapters were enumerated",
                ));
            }
            for (idx, gpu) in gpus.gpus().iter().enumerate() {
                let dynamic_read = gpu.dynamic_info();
                if let Err(e) = &dynamic_read {
                    checks.push(unreadable(
                        &format!("GPU{} Telemetry", idx),
                        "GPU",
                        &format!("GPU telemetry could not be read: {e}"),
                    ));
                }
                if let Ok(dynamic) = dynamic_read {
                    // GPU Temperature
                    if let Some(temp) = dynamic.thermal.temperature {
                        let (status, message) = if temp >= thresholds.gpu_temp_critical as i32 {
                            (
                                HealthStatus::Critical,
                                format!("GPU{} temperature critical: {}°C", idx, temp),
                            )
                        } else if temp >= thresholds.gpu_temp_warning as i32 {
                            (
                                HealthStatus::Warning,
                                format!("GPU{} temperature elevated: {}°C", idx, temp),
                            )
                        } else if temp >= 70 {
                            (
                                HealthStatus::Good,
                                format!("GPU{} temperature warm: {}°C", idx, temp),
                            )
                        } else {
                            (
                                HealthStatus::Healthy,
                                format!("GPU{} temperature normal: {}°C", idx, temp),
                            )
                        };

                        checks.push(
                            HealthCheck::new(&format!("GPU{} Temperature", idx), "GPU")
                                .with_status(status, &message)
                                .with_value(temp as f64, Some(thresholds.gpu_temp_warning as f64)),
                        );
                    } else {
                        checks.push(unreadable(
                            &format!("GPU{} Temperature", idx),
                            "GPU",
                            "temperature was not reported",
                        ));
                    }

                    // GPU Memory
                    // Derived on `GpuMemory`, so a device that reported no
                    // memory has no percentage and no check runs -- rather than
                    // computing 0% and quietly passing every threshold.
                    if let Some(mem_pct) = dynamic.memory.utilization.map(f64::from) {
                        let (status, message) = if mem_pct >= thresholds.gpu_memory_critical as f64
                        {
                            (
                                HealthStatus::Critical,
                                format!("GPU{} memory critically high: {:.1}%", idx, mem_pct),
                            )
                        } else if mem_pct >= thresholds.gpu_memory_warning as f64 {
                            (
                                HealthStatus::Warning,
                                format!("GPU{} memory elevated: {:.1}%", idx, mem_pct),
                            )
                        } else if mem_pct >= 50.0 {
                            (
                                HealthStatus::Good,
                                format!("GPU{} memory moderate: {:.1}%", idx, mem_pct),
                            )
                        } else {
                            (
                                HealthStatus::Healthy,
                                format!("GPU{} memory normal: {:.1}%", idx, mem_pct),
                            )
                        };

                        checks.push(
                            HealthCheck::new(&format!("GPU{} Memory", idx), "GPU")
                                .with_status(status, &message)
                                .with_value(mem_pct, Some(thresholds.gpu_memory_warning as f64)),
                        );
                    } else {
                        checks.push(unreadable(
                            &format!("GPU{} Memory", idx),
                            "GPU",
                            "memory utilization was not reported",
                        ));
                    }

                    // GPU Utilization (informational, not a health concern
                    // usually). No reading is reported as Unknown, not as a
                    // comfortable 0%.
                    let Some(util) = dynamic.utilization.map(f64::from) else {
                        checks.push(unreadable(
                            &format!("GPU{} Utilization", idx),
                            "GPU",
                            "utilization was not reported",
                        ));
                        continue;
                    };
                    let (status, message) = if util >= 95.0 {
                        (
                            HealthStatus::Good,
                            format!("GPU{} fully utilized: {:.0}%", idx, util),
                        )
                    } else if util >= 50.0 {
                        (
                            HealthStatus::Healthy,
                            format!("GPU{} active: {:.0}%", idx, util),
                        )
                    } else {
                        (
                            HealthStatus::Healthy,
                            format!("GPU{} idle/low usage: {:.0}%", idx, util),
                        )
                    };

                    checks.push(
                        HealthCheck::new(&format!("GPU{} Utilization", idx), "GPU")
                            .with_status(status, &message)
                            .with_value(util, None),
                    );
                }
            }
        }

        // Disk Health Checks - use filesystem info for space usage
        let disk_read = crate::disk::enumerate_disks();
        if let Err(e) = &disk_read {
            checks.push(unreadable(
                "Disks",
                "Storage",
                &format!("block devices could not be enumerated: {e}"),
            ));
        }
        if let Ok(disks) = disk_read {
            // Same as the GPU case: no block devices is a row, not an absence.
            if disks.is_empty() {
                checks.push(unreadable(
                    "Disks",
                    "Storage",
                    "no block devices were enumerated",
                ));
            }
            for disk in disks {
                let disk_name = disk.name();
                // Get disk health status
                match disk.health() {
                    Ok(health) => {
                        let (status, message) = match health {
                            crate::disk::traits::DiskHealth::Healthy => (
                                HealthStatus::Healthy,
                                format!("Disk {} health: OK", disk_name),
                            ),
                            crate::disk::traits::DiskHealth::Warning => (
                                HealthStatus::Warning,
                                format!("Disk {} health: Warning", disk_name),
                            ),
                            crate::disk::traits::DiskHealth::Critical => (
                                HealthStatus::Critical,
                                format!("Disk {} health: Critical!", disk_name),
                            ),
                            crate::disk::traits::DiskHealth::Failed => (
                                HealthStatus::Critical,
                                format!("Disk {} health: FAILED!", disk_name),
                            ),
                            crate::disk::traits::DiskHealth::Unknown => (
                                HealthStatus::Unknown,
                                format!("Disk {} health: not reported", disk_name),
                            ),
                        };
                        checks.push(
                            HealthCheck::new(&format!("Disk {} Health", disk_name), "Storage")
                                .with_status(status, &message),
                        );
                    }
                    Err(e) => checks.push(unreadable(
                        &format!("Disk {} Health", disk_name),
                        "Storage",
                        &format!("health could not be read: {e}"),
                    )),
                }

                // Get filesystem space usage
                match disk.filesystem_info() {
                    Err(e) => checks.push(unreadable(
                        &format!("Disk {} Filesystems", disk_name),
                        "Storage",
                        &format!("filesystems could not be read: {e}"),
                    )),
                    Ok(fs_infos) => {
                        for fs in fs_infos {
                            // A filesystem whose size was not read cannot be
                            // judged against the 1 GB floor, so it is reported
                            // Unknown rather than dropped.
                            let mount_name = fs.mount_point.display().to_string();
                            let Some(total) = fs.total_size else {
                                checks.push(unreadable(
                                    &format!("Mount {}", mount_name),
                                    "Storage",
                                    "size was not reported",
                                ));
                                continue;
                            };
                            let Some(usage_pct) = fs.usage_percent().map(f64::from) else {
                                if total > 1_000_000_000 {
                                    checks.push(unreadable(
                                        &format!("Mount {}", mount_name),
                                        "Storage",
                                        "usage was not reported",
                                    ));
                                }
                                continue;
                            };
                            if total > 1_000_000_000 {
                                // Only check disks > 1GB
                                let (status, message) =
                                    if usage_pct >= thresholds.disk_critical as f64 {
                                        (
                                            HealthStatus::Critical,
                                            format!(
                                                "Disk {} critically full: {:.1}%",
                                                mount_name, usage_pct
                                            ),
                                        )
                                    } else if usage_pct >= thresholds.disk_warning as f64 {
                                        (
                                            HealthStatus::Warning,
                                            format!(
                                                "Disk {} usage high: {:.1}%",
                                                mount_name, usage_pct
                                            ),
                                        )
                                    } else if usage_pct >= 60.0 {
                                        (
                                            HealthStatus::Good,
                                            format!(
                                                "Disk {} usage moderate: {:.1}%",
                                                mount_name, usage_pct
                                            ),
                                        )
                                    } else {
                                        (
                                            HealthStatus::Healthy,
                                            format!(
                                                "Disk {} usage normal: {:.1}%",
                                                mount_name, usage_pct
                                            ),
                                        )
                                    };

                                checks.push(
                                    HealthCheck::new(&format!("Mount {}", mount_name), "Storage")
                                        .with_status(status, &message)
                                        .with_value(
                                            usage_pct,
                                            Some(thresholds.disk_warning as f64),
                                        ),
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(assemble(checks))
    }

    /// Get only checks with issues (warning or critical)
    pub fn issues(&self) -> Vec<&HealthCheck> {
        self.checks
            .iter()
            .filter(|c| c.status == HealthStatus::Warning || c.status == HealthStatus::Critical)
            .collect()
    }

    /// Get checks by category
    pub fn by_category(&self, category: &str) -> Vec<&HealthCheck> {
        self.checks
            .iter()
            .filter(|c| c.category == category)
            .collect()
    }

    /// Check if system has any critical issues
    pub fn has_critical(&self) -> bool {
        self.critical_count > 0
    }

    /// Check if system has any warnings
    pub fn has_warnings(&self) -> bool {
        self.warning_count > 0
    }

    /// Get a quick summary string
    pub fn summary(&self) -> String {
        let score = self
            .score
            .map_or_else(|| "n/a".to_string(), |s| format!("{}/100", s));
        // Said outright, so a score over the checks that ran is not read as the
        // score of the whole system.
        let unread = if self.unknown_count > 0 {
            format!(", {} could not be read", self.unknown_count)
        } else {
            String::new()
        };
        format!(
            "{} - Score: {} ({} healthy, {} warning, {} critical{})",
            self.status, score, self.healthy_count, self.warning_count, self.critical_count, unread
        )
    }
}

/// Build the report from the checks that were attempted.
///
/// A check that could not be read is `Unknown` and has no score. It is counted
/// in `unknown_count` and keeps the overall status from reading `Healthy` or
/// `Good`: a failed read is not a passed one. Critical and Warning findings
/// still take precedence, since they were measured.
fn assemble(checks: Vec<HealthCheck>) -> SystemHealth {
    let count = |status| checks.iter().filter(|c| c.status == status).count();
    let healthy_count = count(HealthStatus::Healthy);
    let good_count = count(HealthStatus::Good);
    let warning_count = count(HealthStatus::Warning);
    let critical_count = count(HealthStatus::Critical);
    let unknown_count = count(HealthStatus::Unknown);

    // An `Unknown` check has no score to average. Counting it as zero would
    // grade the whole report down for a reading that was never taken.
    let avg_score = aggregate_score(&checks);

    let overall_status = if critical_count > 0 {
        HealthStatus::Critical
    } else if warning_count > 0 {
        HealthStatus::Warning
    } else if unknown_count > 0 || checks.is_empty() {
        // Something was not read, or nothing was: neither is Healthy.
        HealthStatus::Unknown
    } else if good_count > healthy_count {
        HealthStatus::Good
    } else {
        HealthStatus::Healthy
    };

    SystemHealth {
        status: overall_status,
        score: avg_score,
        checks,
        healthy_count: healthy_count + good_count,
        warning_count,
        critical_count,
        unknown_count,
        timestamp: std::time::SystemTime::now(),
    }
}

/// Mean of the scores that were produced, or `None` if no check produced one.
fn aggregate_score(checks: &[HealthCheck]) -> Option<u8> {
    let scores: Vec<u32> = checks
        .iter()
        .filter_map(|c| c.score)
        .map(u32::from)
        .collect();
    if scores.is_empty() {
        return None;
    }
    Some((scores.iter().sum::<u32>() / scores.len() as u32) as u8)
}

/// Quick health check - returns overall status
pub fn quick_health_check() -> HealthStatus {
    SystemHealth::check()
        .map(|h| h.status)
        .unwrap_or(HealthStatus::Unknown)
}

/// Health score (0-100), or `None` when the check itself failed.
///
/// This returned `u8` with `unwrap_or(0)`, so a failed check reported a score
/// of zero — the worst possible health — rather than no score. That errs
/// alarming rather than reassuring, which is the safer direction, but zero is
/// a real score and a caller cannot tell the two apart.
pub fn health_score() -> Option<u8> {
    SystemHealth::check().ok().and_then(|h| h.score)
}

/// Whether the system has critical issues, or `None` when the check failed.
///
/// `unwrap_or(false)` reported **no critical issues** when nothing had been
/// checked. `health_status` beside it already returned `HealthStatus::Unknown`
/// for the same failure, so the module had the right answer and two of its
/// three helpers did not use it.
pub fn has_critical_issues() -> Option<bool> {
    SystemHealth::check().map(|h| h.has_critical()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check() {
        let health = SystemHealth::check();
        assert!(health.is_ok());
        let health = health.unwrap();
        assert!(health.score.is_none_or(|s| s <= 100));
    }

    #[test]
    fn an_unknown_check_is_not_averaged_in_as_zero() {
        let check = |status| HealthCheck::new("cpu", "CPU").with_status(status, "");
        assert_eq!(aggregate_score(&[]), None);
        assert_eq!(aggregate_score(&[HealthCheck::new("cpu", "CPU")]), None);
        let mixed = [
            check(HealthStatus::Healthy),
            HealthCheck::new("disk", "Storage"),
        ];
        assert_eq!(aggregate_score(&mixed), Some(100));
    }

    fn healthy(name: &str) -> HealthCheck {
        HealthCheck::new(name, "CPU").with_status(HealthStatus::Healthy, "ok")
    }

    /// A failed read is reported, and it keeps the verdict off Healthy. The
    /// memory check passed, but the CPU read failed: the report must not say
    /// Healthy about a system where half of what it looks at was never read.
    #[test]
    fn a_failed_read_is_not_a_passed_verdict() {
        let cpu_failed = unreadable("CPU Usage", "CPU", "statistics could not be read");
        let report = assemble(vec![healthy("Memory Usage"), cpu_failed]);

        assert_eq!(report.status, HealthStatus::Unknown);
        assert_eq!(report.unknown_count, 1);
        assert_eq!(
            report.checks.len(),
            2,
            "the failed read stays in the report"
        );
        assert_eq!(
            report.score,
            Some(100),
            "score covers only the check that ran"
        );
        assert!(report.summary().contains("1 could not be read"));
    }

    /// A measured Warning outranks an unread check: the finding was observed.
    #[test]
    fn a_measured_warning_still_wins_over_an_unread_check() {
        let warn = HealthCheck::new("Swap Usage", "Memory")
            .with_status(HealthStatus::Warning, "swap elevated");
        let report = assemble(vec![warn, unreadable("Disks", "Storage", "no")]);
        assert_eq!(report.status, HealthStatus::Warning);
        assert_eq!(report.unknown_count, 1);
    }

    /// With every read succeeding the verdict is unchanged.
    #[test]
    fn an_all_read_report_is_healthy_with_no_unknowns() {
        let report = assemble(vec![healthy("CPU Usage"), healthy("Memory Usage")]);
        assert_eq!(report.status, HealthStatus::Healthy);
        assert_eq!(report.unknown_count, 0);
        assert!(!report.summary().contains("could not be read"));
    }

    /// Nothing was read at all: unknown, with no score rather than a zero.
    #[test]
    fn a_report_of_only_failed_reads_has_no_score() {
        let report = assemble(vec![unreadable("CPU Usage", "CPU", "no")]);
        assert_eq!(report.status, HealthStatus::Unknown);
        assert_eq!(report.score, None);
        assert_eq!(report.unknown_count, 1);
    }

    #[test]
    fn test_thresholds() {
        let thresholds = HealthThresholds::default();
        assert!(thresholds.cpu_warning < thresholds.cpu_critical);
        assert!(thresholds.memory_warning < thresholds.memory_critical);
    }
}
