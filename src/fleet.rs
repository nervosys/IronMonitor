// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 nervosys

//! Fleet-level multi-host monitoring and aggregation
//!
//! Aggregates metrics from multiple hosts, provides fleet health scoring,
//! tag-based grouping, and threshold-based alerting.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Host status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostStatus {
    Online,
    Degraded,
    Offline,
    Unknown,
}

/// Alert severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Alert category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertCategory {
    CpuOverload,
    MemoryPressure,
    DiskFull,
    GpuOverheat,
    HostOffline,
    HealthDegraded,
    NetworkSaturation,
}

/// Fleet alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetAlert {
    pub host_id: String,
    pub severity: AlertSeverity,
    pub category: AlertCategory,
    pub message: String,
    pub timestamp: u64,
}

/// Per-host metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_percent: f64,
    pub gpu_temperature_max: Option<f64>,
    pub gpu_utilization_max: Option<f64>,
    pub disk_usage_percent: f64,
    pub network_rx_bytes_sec: f64,
    pub network_tx_bytes_sec: f64,
    pub process_count: u32,
    pub uptime_seconds: u64,
    pub timestamp: u64,
}

/// Host info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub host_id: String,
    pub hostname: String,
    pub address: Option<String>,
    pub tags: HashMap<String, String>,
    pub status: HostStatus,
    pub last_seen: u64,
    pub latest_metrics: Option<HostMetrics>,
}

/// Fleet health summary per host
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostSummary {
    pub host_id: String,
    pub hostname: String,
    pub status: HostStatus,
    pub health_score: f64,
    pub alert_count: usize,
}

/// Metrics aggregated by tag group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagGroupMetrics {
    pub tag_key: String,
    pub tag_value: String,
    pub host_count: usize,
    pub avg_cpu: f64,
    pub avg_memory: f64,
    pub avg_gpu_temp: Option<f64>,
    pub total_alerts: usize,
}

/// Fleet configuration thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetThresholds {
    pub cpu_warning: f64,
    pub cpu_critical: f64,
    pub memory_warning: f64,
    pub memory_critical: f64,
    pub disk_warning: f64,
    pub disk_critical: f64,
    pub gpu_temp_warning: f64,
    pub gpu_temp_critical: f64,
    pub offline_timeout_secs: u64,
}

impl Default for FleetThresholds {
    fn default() -> Self {
        Self {
            cpu_warning: 80.0,
            cpu_critical: 95.0,
            memory_warning: 85.0,
            memory_critical: 95.0,
            disk_warning: 85.0,
            disk_critical: 95.0,
            gpu_temp_warning: 80.0,
            gpu_temp_critical: 95.0,
            offline_timeout_secs: 120,
        }
    }
}

/// Fleet configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetConfig {
    pub fleet_name: String,
    pub thresholds: FleetThresholds,
}

/// Fleet snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetSnapshot {
    pub fleet_name: String,
    pub host_count: usize,
    pub online_count: usize,
    pub health_score: f64,
    pub hosts: Vec<HostSummary>,
    pub alerts: Vec<FleetAlert>,
    pub tag_groups: Vec<TagGroupMetrics>,
    pub timestamp: u64,
}

/// Fleet manager
pub struct FleetManager {
    config: FleetConfig,
    hosts: HashMap<String, HostInfo>,
    alerts: Vec<FleetAlert>,
}

impl FleetManager {
    pub fn new(config: FleetConfig) -> Self {
        Self {
            config,
            hosts: HashMap::new(),
            alerts: Vec::new(),
        }
    }

    /// Register a host
    pub fn register_host(
        &mut self,
        host_id: impl Into<String>,
        hostname: impl Into<String>,
    ) -> &mut HostInfo {
        let id = host_id.into();
        let now = now_secs();
        self.hosts.entry(id.clone()).or_insert_with(|| HostInfo {
            host_id: id,
            hostname: hostname.into(),
            address: None,
            tags: HashMap::new(),
            status: HostStatus::Unknown,
            last_seen: now,
            latest_metrics: None,
        })
    }

    /// Record metrics for a host
    pub fn record_metrics(&mut self, host_id: &str, metrics: HostMetrics) {
        let now = now_secs();
        if let Some(host) = self.hosts.get_mut(host_id) {
            host.status = HostStatus::Online;
            host.last_seen = now;
            // Check thresholds and generate alerts
            Self::check_thresholds(&mut self.alerts, &self.config.thresholds, host_id, &metrics);
            host.latest_metrics = Some(metrics);
        }
    }

    fn check_thresholds(
        alerts: &mut Vec<FleetAlert>,
        thresholds: &FleetThresholds,
        host_id: &str,
        m: &HostMetrics,
    ) {
        let t = thresholds;

        if m.cpu_usage_percent >= t.cpu_critical {
            alerts.push(FleetAlert {
                host_id: host_id.into(),
                severity: AlertSeverity::Critical,
                category: AlertCategory::CpuOverload,
                message: format!("CPU at {:.1}%", m.cpu_usage_percent),
                timestamp: now_secs(),
            });
        } else if m.cpu_usage_percent >= t.cpu_warning {
            alerts.push(FleetAlert {
                host_id: host_id.into(),
                severity: AlertSeverity::Warning,
                category: AlertCategory::CpuOverload,
                message: format!("CPU at {:.1}%", m.cpu_usage_percent),
                timestamp: now_secs(),
            });
        }

        if m.memory_usage_percent >= t.memory_critical {
            alerts.push(FleetAlert {
                host_id: host_id.into(),
                severity: AlertSeverity::Critical,
                category: AlertCategory::MemoryPressure,
                message: format!("Memory at {:.1}%", m.memory_usage_percent),
                timestamp: now_secs(),
            });
        }

        if m.disk_usage_percent >= t.disk_critical {
            alerts.push(FleetAlert {
                host_id: host_id.into(),
                severity: AlertSeverity::Critical,
                category: AlertCategory::DiskFull,
                message: format!("Disk at {:.1}%", m.disk_usage_percent),
                timestamp: now_secs(),
            });
        }

        if let Some(temp) = m.gpu_temperature_max {
            if temp >= t.gpu_temp_critical {
                alerts.push(FleetAlert {
                    host_id: host_id.into(),
                    severity: AlertSeverity::Critical,
                    category: AlertCategory::GpuOverheat,
                    message: format!("GPU temp at {:.0}C", temp),
                    timestamp: now_secs(),
                });
            }
        }
    }

    /// Update host statuses (mark offline if not seen recently)
    pub fn update_statuses(&mut self) {
        let now = now_secs();
        let timeout = self.config.thresholds.offline_timeout_secs;
        for host in self.hosts.values_mut() {
            if now - host.last_seen > timeout && host.status != HostStatus::Offline {
                host.status = HostStatus::Offline;
            }
        }
    }

    /// Calculate health score for a host (0-100)
    fn host_health_score(host: &HostInfo) -> f64 {
        match host.status {
            HostStatus::Offline => 0.0,
            HostStatus::Unknown => 50.0,
            _ => {
                let Some(m) = &host.latest_metrics else {
                    return 50.0;
                };
                let cpu_score = 100.0 - m.cpu_usage_percent;
                let mem_score = 100.0 - m.memory_usage_percent;
                let disk_score = 100.0 - m.disk_usage_percent;

                // The GPU term is weighted in only when there is a temperature
                // to weigh.
                //
                // `unwrap_or(100.0)` gave a host with no GPU reading the full
                // twenty points for being cool -- so an unread sensor and a
                // perfectly cool card scored identically, and a host whose
                // thermal probe had failed looked *better* than one reporting
                // 85 C. Rescaling by the weight actually used scores a host on
                // what is known about it rather than crediting it for what is
                // not.
                let mut weighted = cpu_score * 0.3 + mem_score * 0.3 + disk_score * 0.2;
                let mut weight = 0.8;
                if let Some(t) = m.gpu_temperature_max {
                    let gpu_score = if t < 80.0 {
                        100.0
                    } else {
                        100.0 - (t - 80.0) * 5.0
                    };
                    weighted += gpu_score * 0.2;
                    weight += 0.2;
                }
                (weighted / weight).clamp(0.0, 100.0)
            }
        }
    }

    /// Fleet-level health score
    pub fn fleet_health(&self) -> f64 {
        if self.hosts.is_empty() {
            return 100.0;
        }
        let sum: f64 = self.hosts.values().map(Self::host_health_score).sum();
        sum / self.hosts.len() as f64
    }

    /// Get hosts grouped by tag
    pub fn hosts_by_tag(&self, tag_key: &str) -> Vec<TagGroupMetrics> {
        let mut groups: HashMap<String, Vec<&HostInfo>> = HashMap::new();
        for host in self.hosts.values() {
            if let Some(val) = host.tags.get(tag_key) {
                groups.entry(val.clone()).or_default().push(host);
            }
        }

        groups
            .into_iter()
            .map(|(val, hosts)| {
                let count = hosts.len();
                let avg_cpu = hosts
                    .iter()
                    .filter_map(|h| h.latest_metrics.as_ref().map(|m| m.cpu_usage_percent))
                    .sum::<f64>()
                    / count.max(1) as f64;
                let avg_mem = hosts
                    .iter()
                    .filter_map(|h| h.latest_metrics.as_ref().map(|m| m.memory_usage_percent))
                    .sum::<f64>()
                    / count.max(1) as f64;
                let gpu_temps: Vec<f64> = hosts
                    .iter()
                    .filter_map(|h| {
                        h.latest_metrics
                            .as_ref()
                            .and_then(|m| m.gpu_temperature_max)
                    })
                    .collect();
                let avg_gpu = if gpu_temps.is_empty() {
                    None
                } else {
                    Some(gpu_temps.iter().sum::<f64>() / gpu_temps.len() as f64)
                };
                let alert_count = self
                    .alerts
                    .iter()
                    .filter(|a| hosts.iter().any(|h| h.host_id == a.host_id))
                    .count();

                TagGroupMetrics {
                    tag_key: tag_key.into(),
                    tag_value: val,
                    host_count: count,
                    avg_cpu,
                    avg_memory: avg_mem,
                    avg_gpu_temp: avg_gpu,
                    total_alerts: alert_count,
                }
            })
            .collect()
    }

    /// Full fleet snapshot
    pub fn snapshot(&self) -> FleetSnapshot {
        let now = now_secs();
        let hosts: Vec<HostSummary> = self
            .hosts
            .values()
            .map(|h| {
                let alerts = self
                    .alerts
                    .iter()
                    .filter(|a| a.host_id == h.host_id)
                    .count();
                HostSummary {
                    host_id: h.host_id.clone(),
                    hostname: h.hostname.clone(),
                    status: h.status,
                    health_score: Self::host_health_score(h),
                    alert_count: alerts,
                }
            })
            .collect();

        let online = hosts
            .iter()
            .filter(|h| h.status == HostStatus::Online)
            .count();

        FleetSnapshot {
            fleet_name: self.config.fleet_name.clone(),
            host_count: self.hosts.len(),
            online_count: online,
            health_score: self.fleet_health(),
            hosts,
            alerts: self.alerts.clone(),
            tag_groups: Vec::new(),
            timestamp: now,
        }
    }

    /// Clear all alerts
    pub fn clear_alerts(&mut self) {
        self.alerts.clear();
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(gpu_temperature_max: Option<f64>) -> HostInfo {
        HostInfo {
            host_id: "h".into(),
            hostname: "h".into(),
            address: None,
            tags: HashMap::new(),
            status: HostStatus::Online,
            last_seen: 0,
            latest_metrics: Some(HostMetrics {
                cpu_usage_percent: 20.0,
                memory_usage_percent: 20.0,
                gpu_temperature_max,
                gpu_utilization_max: None,
                disk_usage_percent: 20.0,
                network_rx_bytes_sec: 0.0,
                network_tx_bytes_sec: 0.0,
                process_count: 0,
                uptime_seconds: 0,
                timestamp: 0,
            }),
        }
    }

    /// An unread GPU temperature must not earn the points a cool one does.
    ///
    /// The GPU term was `unwrap_or(100.0)`, so a host with no temperature
    /// collected the full twenty points for being cool. That made an unread
    /// sensor and a perfectly cool card score identically, and left a host
    /// whose probe had failed scoring **higher** than one honestly reporting
    /// 85 C.
    #[test]
    fn a_missing_gpu_temperature_is_not_a_cool_gpu() {
        let cool = FleetManager::host_health_score(&host(Some(40.0)));
        let hot = FleetManager::host_health_score(&host(Some(90.0)));
        let unread = FleetManager::host_health_score(&host(None));

        assert!(
            hot < cool,
            "a hotter GPU must score lower: {hot} is not below {cool}"
        );
        // An unread GPU neither earns the cool card's bonus nor takes the hot
        // card's penalty: the host is scored on the three components that were
        // measured, which at 20% usage each is 80.
        assert!(
            (unread - 80.0).abs() < 0.001,
            "an unread GPU term should leave the weighted mean of the measured \
             components, 80, and gave {unread}"
        );
        assert!(
            hot < unread && unread < cool,
            "an unread temperature belongs between a hot and a cool one, not \
             level with either: {hot} < {unread} < {cool}"
        );
    }

    /// The weights still sum to one when every component is present.
    #[test]
    fn a_host_with_every_component_is_scored_on_all_of_them() {
        // 80 + 80 + 80 across cpu/memory/disk, and a cool GPU at 100.
        let score = FleetManager::host_health_score(&host(Some(40.0)));
        assert!(
            (score - 84.0).abs() < 0.001,
            "0.3*80 + 0.3*80 + 0.2*80 + 0.2*100 = 84, got {score}"
        );
    }
}
