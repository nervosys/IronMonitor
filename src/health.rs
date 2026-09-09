//! System Health Scoring and Alerts
//!
//! Provides overall system health metrics and threshold-based alerting.
//! Inspired by monitoring tools that provide quick health overviews.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::health::{SystemHealth, HealthCheck, HealthStatus};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let health = SystemHealth::check()?;
//!
//! println!("Overall Health: {} ({}/100)", health.status, health.score);
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
    /// Score contribution (0-100)
    pub score: u8,
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
            score: 0,
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
            HealthStatus::Healthy => 100,
            HealthStatus::Good => 85,
            HealthStatus::Warning => 50,
            HealthStatus::Critical => 10,
            HealthStatus::Unknown => 0,
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
    /// Overall health score (0-100)
    pub score: u8,
    /// Individual health checks
    pub checks: Vec<HealthCheck>,
    /// Number of healthy checks
    pub healthy_count: usize,
    /// Number of warning checks
    pub warning_count: usize,
    /// Number of critical checks
    pub critical_count: usize,
    /// Timestamp of check
    pub timestamp: std::time::SystemTime,
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
        if let Ok(cpu) = crate::stats::platform_cpu_stats() {
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
        if let Ok(mem) = crate::stats::platform_memory_stats() {
            let mem_usage = mem.ram_usage_percent();
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
        if let Ok(gpus) = GpuCollection::auto_detect() {
            for (idx, gpu) in gpus.gpus().iter().enumerate() {
                if let Ok(dynamic) = gpu.dynamic_info() {
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
                    }

                    // GPU Utilization (informational, not a health concern
                    // usually). No reading, no check -- rather than reporting an
                    // unreadable device as a comfortable 0%.
                    let Some(util) = dynamic.utilization.map(f64::from) else {
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
        if let Ok(disks) = crate::disk::enumerate_disks() {
            for disk in disks {
                // Get disk health status
                if let Ok(health) = disk.health() {
                    let disk_name = disk.name();
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
                        crate::disk::traits::DiskHealth::Unknown => continue,
                    };
                    checks.push(
                        HealthCheck::new(&format!("Disk {} Health", disk_name), "Storage")
                            .with_status(status, &message),
                    );
                }

                // Get filesystem space usage
                if let Ok(fs_infos) = disk.filesystem_info() {
                    for fs in fs_infos {
                        if fs.total_size > 1_000_000_000 {
                            // Only check disks > 1GB
                            let usage_pct = fs.usage_percent() as f64;
                            let mount_name = fs.mount_point.display().to_string();

                            let (status, message) = if usage_pct >= thresholds.disk_critical as f64
                            {
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
                                    format!("Disk {} usage high: {:.1}%", mount_name, usage_pct),
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
                                    format!("Disk {} usage normal: {:.1}%", mount_name, usage_pct),
                                )
                            };

                            checks.push(
                                HealthCheck::new(&format!("Mount {}", mount_name), "Storage")
                                    .with_status(status, &message)
                                    .with_value(usage_pct, Some(thresholds.disk_warning as f64)),
                            );
                        }
                    }
                }
            }
        }

        // Calculate overall health
        let healthy_count = checks
            .iter()
            .filter(|c| c.status == HealthStatus::Healthy)
            .count();
        let good_count = checks
            .iter()
            .filter(|c| c.status == HealthStatus::Good)
            .count();
        let warning_count = checks
            .iter()
            .filter(|c| c.status == HealthStatus::Warning)
            .count();
        let critical_count = checks
            .iter()
            .filter(|c| c.status == HealthStatus::Critical)
            .count();

        // Calculate weighted score
        let total_score: u32 = checks.iter().map(|c| c.score as u32).sum();
        let avg_score = if !checks.is_empty() {
            (total_score / checks.len() as u32) as u8
        } else {
            0
        };

        // Determine overall status
        let overall_status = if critical_count > 0 {
            HealthStatus::Critical
        } else if warning_count > 0 {
            HealthStatus::Warning
        } else if good_count > healthy_count {
            HealthStatus::Good
        } else if !checks.is_empty() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unknown
        };

        Ok(SystemHealth {
            status: overall_status,
            score: avg_score,
            checks,
            healthy_count: healthy_count + good_count,
            warning_count,
            critical_count,
            timestamp: std::time::SystemTime::now(),
        })
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
        format!(
            "{} - Score: {}/100 ({} healthy, {} warning, {} critical)",
            self.status, self.score, self.healthy_count, self.warning_count, self.critical_count
        )
    }
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
    SystemHealth::check().map(|h| h.score).ok()
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
        assert!(health.score <= 100);
    }

    #[test]
    fn test_thresholds() {
        let thresholds = HealthThresholds::default();
        assert!(thresholds.cpu_warning < thresholds.cpu_critical);
        assert!(thresholds.memory_warning < thresholds.memory_critical);
    }
}
