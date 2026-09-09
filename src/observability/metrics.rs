//! Metric Collection and Aggregation
//!
//! This module provides:
//! - Unified metric types
//! - Time-series metric collection
//! - Aggregation (min, max, avg, percentiles)
//! - Metric export formats

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A single metric value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    /// Metric name
    pub name: String,
    /// Metric value
    pub value: f64,
    /// Unit (e.g., "percent", "bytes", "celsius")
    pub unit: String,
    /// Timestamp (unix epoch)
    pub timestamp: u64,
    /// Labels/tags
    pub labels: HashMap<String, String>,
}

impl MetricValue {
    pub fn new(name: impl Into<String>, value: f64, unit: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value,
            unit: unit.into(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            labels: HashMap::new(),
        }
    }

    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }
}

/// Metric types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    /// Instantaneous value (gauge)
    Gauge,
    /// Monotonically increasing counter
    Counter,
    /// Distribution of values
    Histogram,
    /// Summary statistics
    Summary,
}

/// Metric definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDefinition {
    /// Metric name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Metric type
    pub metric_type: MetricType,
    /// Unit
    pub unit: String,
    /// Expected labels
    pub labels: Vec<String>,
}

/// Standard system metrics
pub mod standard {
    pub const CPU_USAGE_PERCENT: &str = "cpu_usage_percent";
    pub const CPU_TEMPERATURE_CELSIUS: &str = "cpu_temperature_celsius";
    pub const CPU_FREQUENCY_MHZ: &str = "cpu_frequency_mhz";

    pub const MEMORY_USED_BYTES: &str = "memory_used_bytes";
    pub const MEMORY_FREE_BYTES: &str = "memory_free_bytes";
    pub const MEMORY_USAGE_PERCENT: &str = "memory_usage_percent";
    pub const SWAP_USED_BYTES: &str = "swap_used_bytes";
    pub const SWAP_USAGE_PERCENT: &str = "swap_usage_percent";

    pub const GPU_USAGE_PERCENT: &str = "gpu_usage_percent";
    pub const GPU_MEMORY_USED_BYTES: &str = "gpu_memory_used_bytes";
    pub const GPU_MEMORY_USAGE_PERCENT: &str = "gpu_memory_usage_percent";
    pub const GPU_TEMPERATURE_CELSIUS: &str = "gpu_temperature_celsius";
    pub const GPU_POWER_WATTS: &str = "gpu_power_watts";
    pub const GPU_FAN_SPEED_PERCENT: &str = "gpu_fan_speed_percent";

    pub const DISK_USED_BYTES: &str = "disk_used_bytes";
    pub const DISK_FREE_BYTES: &str = "disk_free_bytes";
    pub const DISK_USAGE_PERCENT: &str = "disk_usage_percent";
    pub const DISK_READ_BYTES_TOTAL: &str = "disk_read_bytes_total";
    pub const DISK_WRITE_BYTES_TOTAL: &str = "disk_write_bytes_total";
    pub const DISK_READ_BPS: &str = "disk_read_bps";
    pub const DISK_WRITE_BPS: &str = "disk_write_bps";

    pub const NETWORK_RX_BYTES_TOTAL: &str = "network_rx_bytes_total";
    pub const NETWORK_TX_BYTES_TOTAL: &str = "network_tx_bytes_total";
    pub const NETWORK_RX_BPS: &str = "network_rx_bps";
    pub const NETWORK_TX_BPS: &str = "network_tx_bps";
    pub const NETWORK_ERRORS_TOTAL: &str = "network_errors_total";
    pub const NETWORK_DROPPED_TOTAL: &str = "network_dropped_total";

    pub const PROCESS_COUNT: &str = "process_count";
    pub const PROCESS_CPU_PERCENT: &str = "process_cpu_percent";
    pub const PROCESS_MEMORY_BYTES: &str = "process_memory_bytes";

    pub const SYSTEM_LOAD_1: &str = "system_load_1";
    pub const SYSTEM_LOAD_5: &str = "system_load_5";
    pub const SYSTEM_LOAD_15: &str = "system_load_15";
    pub const SYSTEM_UPTIME_SECONDS: &str = "system_uptime_seconds";

    pub const BATTERY_PERCENT: &str = "battery_percent";
    pub const POWER_DRAW_WATTS: &str = "power_draw_watts";

    pub const FAN_SPEED_RPM: &str = "fan_speed_rpm";
    pub const FAN_SPEED_PERCENT: &str = "fan_speed_percent";
}

/// Aggregated statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricStats {
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Average value
    pub avg: f64,
    /// Current (last) value
    pub current: f64,
    /// Sample count
    pub count: usize,
    /// Start timestamp
    pub start_time: u64,
    /// End timestamp
    pub end_time: u64,
    /// Percentiles (p50, p90, p95, p99)
    pub percentiles: Option<Percentiles>,
}

/// Percentile values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Percentiles {
    pub p50: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
}

impl MetricStats {
    /// Calculate percentiles from a sorted slice of values
    fn calculate_percentiles(sorted_values: &[f64]) -> Percentiles {
        let len = sorted_values.len();
        if len == 0 {
            return Percentiles {
                p50: 0.0,
                p90: 0.0,
                p95: 0.0,
                p99: 0.0,
            };
        }

        let percentile = |p: f64| -> f64 {
            let index = (p * (len - 1) as f64).round() as usize;
            sorted_values[index.min(len - 1)]
        };

        Percentiles {
            p50: percentile(0.50),
            p90: percentile(0.90),
            p95: percentile(0.95),
            p99: percentile(0.99),
        }
    }
}

/// Time-series data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    pub timestamp: u64,
    pub value: f64,
}

/// Time-series metric storage
pub struct MetricTimeSeries {
    /// Metric name
    #[allow(dead_code)]
    name: String,
    /// Max data points to keep
    max_points: usize,
    /// Retention period
    retention: Duration,
    /// Data points
    data: RwLock<VecDeque<TimeSeriesPoint>>,
}

impl MetricTimeSeries {
    pub fn new(name: impl Into<String>, max_points: usize, retention: Duration) -> Self {
        Self {
            name: name.into(),
            max_points,
            retention,
            data: RwLock::new(VecDeque::with_capacity(max_points)),
        }
    }

    /// Record a new value
    pub fn record(&self, value: f64) {
        let point = TimeSeriesPoint {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            value,
        };

        if let Ok(mut data) = self.data.write() {
            // Remove old points
            let cutoff = point.timestamp.saturating_sub(self.retention.as_secs());
            while data.front().map(|p| p.timestamp < cutoff).unwrap_or(false) {
                data.pop_front();
            }

            // Add new point
            if data.len() >= self.max_points {
                data.pop_front();
            }
            data.push_back(point);
        }
    }

    /// Get all data points
    pub fn get_all(&self) -> Vec<TimeSeriesPoint> {
        self.data
            .read()
            .map(|d| d.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get data points in a time range
    pub fn get_range(&self, start: u64, end: u64) -> Vec<TimeSeriesPoint> {
        self.data
            .read()
            .map(|d| {
                d.iter()
                    .filter(|p| p.timestamp >= start && p.timestamp <= end)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get statistics for the time series
    pub fn get_stats(&self) -> Option<MetricStats> {
        let data = self.data.read().ok()?;
        if data.is_empty() {
            return None;
        }

        let mut values: Vec<f64> = data.iter().map(|p| p.value).collect();
        let sum: f64 = values.iter().sum();
        let count = values.len();

        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        Some(MetricStats {
            min: values.first().copied().unwrap_or(0.0),
            max: values.last().copied().unwrap_or(0.0),
            avg: sum / count as f64,
            current: data.back().map(|p| p.value).unwrap_or(0.0),
            count,
            start_time: data.front().map(|p| p.timestamp).unwrap_or(0),
            end_time: data.back().map(|p| p.timestamp).unwrap_or(0),
            percentiles: Some(MetricStats::calculate_percentiles(&values)),
        })
    }

    /// Get the latest value
    pub fn latest(&self) -> Option<f64> {
        self.data.read().ok()?.back().map(|p| p.value)
    }
}

/// Metric collector that manages multiple metrics
pub struct MetricCollector {
    /// Metric time series by name
    metrics: RwLock<HashMap<String, MetricTimeSeries>>,
    /// Default retention period
    default_retention: Duration,
    /// Default max points
    default_max_points: usize,
}

impl MetricCollector {
    pub fn new() -> Self {
        Self {
            metrics: RwLock::new(HashMap::new()),
            default_retention: Duration::from_secs(3600), // 1 hour
            default_max_points: 3600,                     // 1 sample per second for 1 hour
        }
    }

    pub fn with_retention(mut self, retention: Duration) -> Self {
        self.default_retention = retention;
        self
    }

    pub fn with_max_points(mut self, max_points: usize) -> Self {
        self.default_max_points = max_points;
        self
    }

    /// Record a metric value
    pub fn record(&self, name: &str, value: f64) {
        if let Ok(mut metrics) = self.metrics.write() {
            let series = metrics.entry(name.to_string()).or_insert_with(|| {
                MetricTimeSeries::new(name, self.default_max_points, self.default_retention)
            });
            series.record(value);
        }
    }

    /// Record a metric with labels
    pub fn record_with_labels(&self, name: &str, value: f64, labels: &[(&str, &str)]) {
        // Create a unique key from name and labels
        let label_str: String = labels
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",");
        let key = if label_str.is_empty() {
            name.to_string()
        } else {
            format!("{}:{{{}}}", name, label_str)
        };

        self.record(&key, value);
    }

    /// Get statistics for a metric
    pub fn get_stats(&self, name: &str) -> Option<MetricStats> {
        self.metrics.read().ok()?.get(name)?.get_stats()
    }

    /// Get all metric names
    pub fn list_metrics(&self) -> Vec<String> {
        self.metrics
            .read()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Get time series data
    pub fn get_time_series(&self, name: &str) -> Vec<TimeSeriesPoint> {
        self.metrics
            .read()
            .ok()
            .and_then(|m| m.get(name).map(|s| s.get_all()))
            .unwrap_or_default()
    }

    /// Get time series data in a range
    pub fn get_time_series_range(&self, name: &str, start: u64, end: u64) -> Vec<TimeSeriesPoint> {
        self.metrics
            .read()
            .ok()
            .and_then(|m| m.get(name).map(|s| s.get_range(start, end)))
            .unwrap_or_default()
    }

    /// Get latest values for all metrics
    pub fn get_latest_all(&self) -> HashMap<String, f64> {
        self.metrics
            .read()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.latest().map(|val| (k.clone(), val)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Export metrics in Prometheus text exposition format.
    ///
    /// This is what `/api/v1/metrics/prometheus` serves, so it is the output a
    /// real scrape sees -- and it emitted the storage key verbatim.
    /// `record_with_labels` encodes a labelled series as
    /// `name:{gpu=0,vendor=NVIDIA}`, which is a lookup key and not a metric:
    /// Prometheus wants `name{gpu="0",vendor="NVIDIA"}`, with the values quoted
    /// and no colon. Nothing calls `record_with_labels` on the serving path
    /// today, so no malformed line has been served yet -- but the encoding is
    /// the reason the collection loop can only record whole-machine metrics,
    /// and that is why the bundled dashboards see ten of the twenty-four names
    /// they query.
    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();

        if let Ok(metrics) = self.metrics.read() {
            for (key, series) in metrics.iter() {
                let Some(value) = series.latest() else {
                    continue;
                };
                let (name, labels) = split_label_key(key);
                let prom_name = name.replace(['.', '-'], "_");
                if labels.is_empty() {
                    output.push_str(&format!("{prom_name} {value}\n"));
                } else {
                    output.push_str(&format!("{prom_name}{{{labels}}} {value}\n"));
                }
            }
        }

        output
    }

    /// Export metrics as JSON
    pub fn export_json(&self) -> String {
        let latest = self.get_latest_all();
        serde_json::to_string_pretty(&latest).unwrap_or_default()
    }
}

impl Default for MetricCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Metric snapshot - all current values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSnapshot {
    /// Timestamp
    pub timestamp: u64,
    /// CPU metrics
    pub cpu: Option<CpuMetricSnapshot>,
    /// GPU metrics (per GPU)
    pub gpus: Vec<GpuMetricSnapshot>,
    /// Memory metrics
    pub memory: Option<MemoryMetricSnapshot>,
    /// Disk metrics (per disk)
    pub disks: Vec<DiskMetricSnapshot>,
    /// Network metrics (per interface)
    pub network: Vec<NetworkMetricSnapshot>,
    /// System metrics
    pub system: Option<SystemMetricSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuMetricSnapshot {
    pub usage_percent: f32,
    pub temperature_celsius: Option<f32>,
    pub frequency_mhz: Option<u32>,
    /// Per-core utilization, `None` for a core whose idle time was not read.
    ///
    /// Was `Vec<f32>`, filled with `100.0 - c.idle.unwrap_or(100.0)` -- so an
    /// unread core arrived as a measured 0% busy. The type could not say
    /// "not read", which is the fault the queued section of the handoff is
    /// about, and it had eight copies.
    pub per_core_usage: Vec<Option<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMetricSnapshot {
    pub index: usize,
    pub name: String,
    pub usage_percent: f32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub temperature_celsius: Option<f32>,
    pub power_watts: Option<f32>,
    pub fan_speed_percent: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetricSnapshot {
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub usage_percent: f32,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetricSnapshot {
    pub device: String,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub usage_percent: f32,
    pub read_bps: Option<u64>,
    pub write_bps: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetricSnapshot {
    pub interface: String,
    pub rx_bytes_total: u64,
    pub tx_bytes_total: u64,
    pub rx_bps: f64,
    pub tx_bps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetricSnapshot {
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
    pub uptime_seconds: u64,
    pub process_count: u32,
}

/// Split a storage key back into a metric name and Prometheus label syntax.
///
/// [`MetricCollector::record_with_labels`] joins labels into the key as
/// `name:{k=v,k=v}` so that each label set gets its own time series. That form
/// is a map key, not exposition syntax, so rendering needs the inverse: the
/// values get quoted and escaped, and the colon goes away.
///
/// A key with no label section comes back unchanged with an empty label string.
fn split_label_key(key: &str) -> (&str, String) {
    let Some((name, rest)) = key.split_once(":{") else {
        return (key, String::new());
    };
    let Some(inner) = rest.strip_suffix('}') else {
        // Not the shape `record_with_labels` produces; treat the whole thing as
        // a name rather than guess at where the labels end.
        return (key, String::new());
    };

    let rendered: Vec<String> = inner
        .split(',')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => format!("{}=\"{}\"", k, escape_label(v)),
            // A label with no `=` cannot be rendered as a pair; name it so the
            // value is not silently dropped.
            None => format!("malformed=\"{}\"", escape_label(pair)),
        })
        .collect();

    (name, rendered.join(","))
}

/// Escape a label value for the exposition format.
fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
