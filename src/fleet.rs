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
    /// `None` when the host did not report CPU usage, as for `memory_usage_percent`.
    pub cpu_usage_percent: Option<f64>,
    /// `None` when the host did not report memory. A caller must not pass 0.0:
    /// that scores as a host with all its memory free.
    pub memory_usage_percent: Option<f64>,
    pub gpu_temperature_max: Option<f64>,
    pub gpu_utilization_max: Option<f64>,
    /// `None` when the host did not report disk usage, as for `memory_usage_percent`.
    pub disk_usage_percent: Option<f64>,
    pub network_rx_bytes_sec: Option<f64>,
    pub network_tx_bytes_sec: Option<f64>,
    pub process_count: Option<u32>,
    /// `None` when the host did not report uptime. A caller must not pass 0: an
    /// unread uptime is not a host that has just booted.
    pub uptime_seconds: Option<u64>,
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
    /// `None` for a host whose status is unknown or that has sent no metrics.
    pub health_score: Option<f64>,
    pub alert_count: usize,
}

/// Metrics aggregated by tag group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagGroupMetrics {
    pub tag_key: String,
    pub tag_value: String,
    pub host_count: usize,
    /// Mean over the group's hosts that have reported metrics, `None` if none
    /// has. This divided by every host in the group, so a host that never
    /// reported counted as idle.
    pub avg_cpu: Option<f64>,
    /// As `avg_cpu`, for memory.
    pub avg_memory: Option<f64>,
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
    /// See [`FleetManager::fleet_health`].
    pub health_score: Option<f64>,
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

        if let Some(cpu) = m.cpu_usage_percent.filter(|&p| p >= t.cpu_critical) {
            alerts.push(FleetAlert {
                host_id: host_id.into(),
                severity: AlertSeverity::Critical,
                category: AlertCategory::CpuOverload,
                message: format!("CPU at {:.1}%", cpu),
                timestamp: now_secs(),
            });
        } else if let Some(cpu) = m.cpu_usage_percent.filter(|&p| p >= t.cpu_warning) {
            alerts.push(FleetAlert {
                host_id: host_id.into(),
                severity: AlertSeverity::Warning,
                category: AlertCategory::CpuOverload,
                message: format!("CPU at {:.1}%", cpu),
                timestamp: now_secs(),
            });
        }

        // An unreported value raises no alert, the same as one below threshold.
        if let Some(mem) = m.memory_usage_percent.filter(|&p| p >= t.memory_critical) {
            alerts.push(FleetAlert {
                host_id: host_id.into(),
                severity: AlertSeverity::Critical,
                category: AlertCategory::MemoryPressure,
                message: format!("Memory at {:.1}%", mem),
                timestamp: now_secs(),
            });
        }

        if let Some(disk) = m.disk_usage_percent.filter(|&p| p >= t.disk_critical) {
            alerts.push(FleetAlert {
                host_id: host_id.into(),
                severity: AlertSeverity::Critical,
                category: AlertCategory::DiskFull,
                message: format!("Disk at {:.1}%", disk),
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

    /// Calculate health score for a host (0-100).
    ///
    /// `None` for a host whose status is unknown or that has sent no metrics.
    /// Both scored 50, a midpoint that described no host -- and averaged into
    /// the fleet's score as though it had been measured.
    fn host_health_score(host: &HostInfo) -> Option<f64> {
        match host.status {
            // A host that went quiet after reporting is scored 0 as offline. One
            // that was registered and never heard from is `Offline` too, once the
            // timeout passes, but nothing about it was ever observed: it used to
            // score 0 and drag the fleet mean down, where the rule for an unheard
            // host is no score at all.
            HostStatus::Offline if host.latest_metrics.is_none() => None,
            HostStatus::Offline => Some(0.0),
            HostStatus::Unknown => None,
            _ => {
                let m = host.latest_metrics.as_ref()?;
                // Each term is weighted in only when the host reported it, and the
                // mean is taken over the weight actually used.
                //
                // `unwrap_or(100.0)` gave a host with no GPU reading the full
                // twenty points for being cool -- so an unread sensor and a
                // perfectly cool card scored identically, and a host whose
                // thermal probe had failed looked *better* than one reporting
                // 85 C. Memory and disk had the same fault in the other
                // direction: an unreported 0.0 usage scored as fully free. Rescaling
                // by the weight actually used scores a host on what is known about
                // it rather than crediting it for what is not.
                // CPU too: a bare 0.0 for an unreported CPU scored as idle.
                let mut weighted = 0.0;
                let mut weight = 0.0;
                if let Some(cpu) = m.cpu_usage_percent {
                    weighted += (100.0 - cpu) * 0.3;
                    weight += 0.3;
                }
                if let Some(mem) = m.memory_usage_percent {
                    weighted += (100.0 - mem) * 0.3;
                    weight += 0.3;
                }
                if let Some(disk) = m.disk_usage_percent {
                    weighted += (100.0 - disk) * 0.2;
                    weight += 0.2;
                }
                if let Some(t) = m.gpu_temperature_max {
                    let gpu_score = if t < 80.0 {
                        100.0
                    } else {
                        100.0 - (t - 80.0) * 5.0
                    };
                    weighted += gpu_score * 0.2;
                    weight += 0.2;
                }
                // Nothing reported is no score, not a perfect one.
                (weight > 0.0).then(|| (weighted / weight).clamp(0.0, 100.0))
            }
        }
    }

    /// Fleet-level health score: the mean over every host, or `None` when the
    /// fleet is empty or any host could not be scored -- the mean of the rest
    /// would not be the fleet's. An empty fleet scored 100, perfectly healthy.
    pub fn fleet_health(&self) -> Option<f64> {
        if self.hosts.is_empty() {
            return None;
        }
        let scores: Vec<f64> = self
            .hosts
            .values()
            .map(Self::host_health_score)
            .collect::<Option<_>>()?;
        Some(scores.iter().sum::<f64>() / scores.len() as f64)
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
                // Averages only the hosts that reported the metric: an unreported
                // memory figure is not a 0% host.
                let mean = |pick: fn(&HostMetrics) -> Option<f64>| {
                    let values: Vec<f64> = hosts
                        .iter()
                        .filter_map(|h| h.latest_metrics.as_ref().and_then(pick))
                        .collect();
                    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
                };
                let avg_cpu = mean(|m| m.cpu_usage_percent);
                let avg_mem = mean(|m| m.memory_usage_percent);
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

        // Every tag key any host carries, each grouped the way `hosts_by_tag`
        // groups it. Sorted so the snapshot is stable across calls.
        let tag_keys: std::collections::BTreeSet<&str> = self
            .hosts
            .values()
            .flat_map(|h| h.tags.keys().map(String::as_str))
            .collect();
        let mut tag_groups: Vec<TagGroupMetrics> = tag_keys
            .into_iter()
            .flat_map(|key| self.hosts_by_tag(key))
            .collect();
        tag_groups.sort_by(|a, b| (&a.tag_key, &a.tag_value).cmp(&(&b.tag_key, &b.tag_value)));

        FleetSnapshot {
            fleet_name: self.config.fleet_name.clone(),
            host_count: self.hosts.len(),
            online_count: online,
            health_score: self.fleet_health(),
            hosts,
            alerts: self.alerts.clone(),
            tag_groups,
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
                cpu_usage_percent: Some(20.0),
                memory_usage_percent: Some(20.0),
                gpu_temperature_max,
                gpu_utilization_max: None,
                disk_usage_percent: Some(20.0),
                network_rx_bytes_sec: Some(0.0),
                network_tx_bytes_sec: Some(0.0),
                process_count: Some(0),
                uptime_seconds: None,
                timestamp: 0,
            }),
        }
    }

    /// A host with CPU and GPU data but no memory or disk report. The score must
    /// come from the terms that were reported, not from zeroed ones.
    #[test]
    fn missing_memory_and_disk_are_rescaled_out_not_scored_as_zero() {
        let mut h = host(Some(40.0));
        if let Some(m) = h.latest_metrics.as_mut() {
            m.memory_usage_percent = None;
            m.disk_usage_percent = None;
        }
        // CPU at 20% (score 80, weight 0.3) and a cool GPU (100, weight 0.2):
        // (80*0.3 + 100*0.2) / 0.5 = 88.
        let score = FleetManager::host_health_score(&h).expect("scored");
        assert!((score - 88.0).abs() < 0.001, "expected 88, got {score}");
    }

    /// A host reporting memory at 100% scores lower than one reporting none. An
    /// unreported memory figure is not fully free memory.
    #[test]
    fn unreported_memory_is_not_scored_as_free_memory() {
        let mut unreported = host(None);
        if let Some(m) = unreported.latest_metrics.as_mut() {
            m.memory_usage_percent = None;
        }
        let mut full = host(None);
        if let Some(m) = full.latest_metrics.as_mut() {
            m.memory_usage_percent = Some(100.0);
        }
        let none = FleetManager::host_health_score(&unreported).expect("scored");
        let at_full = FleetManager::host_health_score(&full).expect("scored");
        assert!(
            at_full < none,
            "a full host must not outscore an unreported one by treating it as 0%: {at_full} vs {none}"
        );
    }

    /// Tag groups are computed, not left empty, and the mean of a metric skips
    /// hosts that did not report it.
    #[test]
    fn snapshot_groups_hosts_by_tag_and_skips_unreported_memory() {
        let mut fleet = FleetManager::new(FleetConfig {
            fleet_name: "f".into(),
            thresholds: FleetThresholds::default(),
        });
        fleet
            .register_host("a", "a")
            .tags
            .insert("role".into(), "web".into());
        fleet
            .register_host("b", "b")
            .tags
            .insert("role".into(), "web".into());
        fleet.record_metrics("a", host(None).latest_metrics.unwrap());
        let mut b = host(None).latest_metrics.unwrap();
        b.memory_usage_percent = None;
        fleet.record_metrics("b", b);

        let snap = fleet.snapshot();
        assert_eq!(snap.tag_groups.len(), 1);
        let group = &snap.tag_groups[0];
        assert_eq!(group.tag_key, "role");
        assert_eq!(group.tag_value, "web");
        assert_eq!(group.host_count, 2);
        // Memory was reported by one host only: its 20% is the mean, not 10%.
        assert_eq!(group.avg_memory, Some(20.0));
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
        let score = |h: HostInfo| FleetManager::host_health_score(&h).expect("scored");
        let cool = score(host(Some(40.0)));
        let hot = score(host(Some(90.0)));
        let unread = score(host(None));

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

    /// A host nobody has heard from is unscored, not a 50, and a fleet with one
    /// is not given the mean of the others.
    #[test]
    fn an_unheard_host_is_unscored_and_so_is_its_fleet() {
        let silent = HostInfo {
            latest_metrics: None,
            ..host(None)
        };
        assert_eq!(FleetManager::host_health_score(&silent), None);
        // Registered, never heard from, then timed out: still unscored, not 0.
        let timed_out_unheard = HostInfo {
            latest_metrics: None,
            status: HostStatus::Offline,
            ..host(None)
        };
        assert_eq!(FleetManager::host_health_score(&timed_out_unheard), None);
        // A host that reported and then went quiet is genuinely offline: 0.
        let went_quiet = HostInfo {
            status: HostStatus::Offline,
            ..host(Some(40.0))
        };
        assert_eq!(FleetManager::host_health_score(&went_quiet), Some(0.0));
        let unknown = HostInfo {
            status: HostStatus::Unknown,
            ..host(Some(40.0))
        };
        assert_eq!(FleetManager::host_health_score(&unknown), None);
    }

    /// The weights still sum to one when every component is present.
    #[test]
    fn a_host_with_every_component_is_scored_on_all_of_them() {
        // 80 + 80 + 80 across cpu/memory/disk, and a cool GPU at 100.
        let score = FleetManager::host_health_score(&host(Some(40.0))).unwrap();
        assert!(
            (score - 84.0).abs() < 0.001,
            "0.3*80 + 0.3*80 + 0.2*80 + 0.2*100 = 84, got {score}"
        );
    }
}
