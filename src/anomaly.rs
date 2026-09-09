//! Anomaly Detection and System Optimization
//!
//! Statistical anomaly detection for hardware metrics with actionable optimization
//! recommendations. Uses moving averages, z-score analysis, and threshold-based
//! alerting to identify unusual system behavior.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::anomaly::{AnomalyDetector, AnomalyConfig};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut detector = AnomalyDetector::new(AnomalyConfig::default());
//!
//! // Feed metrics over time
//! detector.record_cpu(45.0);
//! detector.record_cpu(47.0);
//! detector.record_cpu(98.0); // Spike!
//!
//! // Check for anomalies
//! for anomaly in detector.detect() {
//!     println!("[{}] {}: {}", anomaly.severity, anomaly.metric, anomaly.message);
//!     for rec in &anomaly.recommendations {
//!         println!("  → {}", rec);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Configuration for anomaly detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyConfig {
    /// Number of samples to keep for moving average
    pub window_size: usize,
    /// Z-score threshold for anomaly detection (default: 2.5)
    pub z_score_threshold: f64,
    /// CPU usage warning threshold (percentage)
    pub cpu_warning: f64,
    /// CPU usage critical threshold (percentage)
    pub cpu_critical: f64,
    /// Memory usage warning threshold (percentage)
    pub memory_warning: f64,
    /// Memory usage critical threshold (percentage)
    pub memory_critical: f64,
    /// GPU temperature warning threshold (°C)
    pub gpu_temp_warning: f64,
    /// GPU temperature critical threshold (°C)
    pub gpu_temp_critical: f64,
    /// Disk usage warning threshold (percentage)
    pub disk_warning: f64,
    /// Disk I/O latency warning (milliseconds)
    pub disk_latency_warning: f64,
    /// Minimum samples before anomaly detection activates
    pub min_samples: usize,
    /// Cooldown between same-type alerts
    pub alert_cooldown: Duration,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            window_size: 60,
            z_score_threshold: 2.5,
            cpu_warning: 85.0,
            cpu_critical: 95.0,
            memory_warning: 85.0,
            memory_critical: 95.0,
            gpu_temp_warning: 80.0,
            gpu_temp_critical: 90.0,
            disk_warning: 90.0,
            disk_latency_warning: 100.0,
            min_samples: 10,
            alert_cooldown: Duration::from_secs(60),
        }
    }
}

/// Severity level of a detected anomaly
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AnomalySeverity {
    /// Informational — noteworthy but not concerning
    Info,
    /// Warning — approaching limits or unusual behavior
    Warning,
    /// Critical — immediate attention needed
    Critical,
}

impl std::fmt::Display for AnomalySeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARN"),
            Self::Critical => write!(f, "CRIT"),
        }
    }
}

/// A detected anomaly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    /// Which metric triggered the anomaly
    pub metric: String,
    /// Severity level
    pub severity: AnomalySeverity,
    /// Human-readable description
    pub message: String,
    /// Current value that triggered the anomaly
    pub current_value: f64,
    /// Expected/normal range
    pub expected_range: Option<(f64, f64)>,
    /// Z-score if statistical detection was used
    pub z_score: Option<f64>,
    /// Actionable recommendations
    pub recommendations: Vec<String>,
    /// Timestamp (seconds since detector start)
    pub timestamp_secs: f64,
}

/// Optimization recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Category of optimization
    pub category: String,
    /// Priority (1 = highest)
    pub priority: u8,
    /// Description of the recommendation
    pub description: String,
    /// Expected impact
    pub impact: String,
}

/// Sliding window for metric time series
struct MetricWindow {
    values: VecDeque<f64>,
    max_size: usize,
    sum: f64,
    sum_sq: f64,
}

impl MetricWindow {
    fn new(max_size: usize) -> Self {
        Self {
            values: VecDeque::with_capacity(max_size),
            max_size,
            sum: 0.0,
            sum_sq: 0.0,
        }
    }

    fn push(&mut self, value: f64) {
        if self.values.len() >= self.max_size {
            if let Some(old) = self.values.pop_front() {
                self.sum -= old;
                self.sum_sq -= old * old;
            }
        }
        self.sum += value;
        self.sum_sq += value * value;
        self.values.push_back(value);
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn mean(&self) -> f64 {
        if self.values.is_empty() {
            0.0
        } else {
            self.sum / self.values.len() as f64
        }
    }

    fn std_dev(&self) -> f64 {
        let n = self.values.len() as f64;
        if n < 2.0 {
            return 0.0;
        }
        let variance = (self.sum_sq - (self.sum * self.sum) / n) / (n - 1.0);
        variance.max(0.0).sqrt()
    }

    fn z_score(&self, value: f64) -> f64 {
        let sd = self.std_dev();
        if sd < 1e-10 {
            // When std_dev is ~0 (constant series), a deviation is highly anomalous
            let diff = (value - self.mean()).abs();
            if diff < 1e-10 {
                0.0
            } else {
                diff.signum() * 100.0
            }
        } else {
            (value - self.mean()) / sd
        }
    }

    fn last(&self) -> Option<f64> {
        self.values.back().copied()
    }

    fn trend(&self) -> f64 {
        // Simple linear trend: positive = increasing, negative = decreasing
        if self.values.len() < 3 {
            return 0.0;
        }
        let n = self.values.len();
        let half = n / 2;
        let first_half_avg: f64 = self.values.iter().take(half).sum::<f64>() / half as f64;
        let second_half_avg: f64 = self.values.iter().skip(half).sum::<f64>() / (n - half) as f64;
        second_half_avg - first_half_avg
    }
}

/// Anomaly detector with rolling window statistics
pub struct AnomalyDetector {
    config: AnomalyConfig,
    cpu_window: MetricWindow,
    memory_window: MetricWindow,
    gpu_temp_window: MetricWindow,
    gpu_util_window: MetricWindow,
    disk_usage_window: MetricWindow,
    network_rx_window: MetricWindow,
    network_tx_window: MetricWindow,
    start_time: Instant,
    last_alerts: std::collections::HashMap<String, Instant>,
}

impl AnomalyDetector {
    /// Create a new anomaly detector with the given configuration
    pub fn new(config: AnomalyConfig) -> Self {
        let ws = config.window_size;
        Self {
            config,
            cpu_window: MetricWindow::new(ws),
            memory_window: MetricWindow::new(ws),
            gpu_temp_window: MetricWindow::new(ws),
            gpu_util_window: MetricWindow::new(ws),
            disk_usage_window: MetricWindow::new(ws),
            network_rx_window: MetricWindow::new(ws),
            network_tx_window: MetricWindow::new(ws),
            start_time: Instant::now(),
            last_alerts: std::collections::HashMap::new(),
        }
    }

    /// Record a CPU utilization sample (0–100%)
    pub fn record_cpu(&mut self, percent: f64) {
        self.cpu_window.push(percent);
    }

    /// Record a memory utilization sample (0–100%)
    pub fn record_memory(&mut self, percent: f64) {
        self.memory_window.push(percent);
    }

    /// Record a GPU temperature sample (°C)
    pub fn record_gpu_temp(&mut self, celsius: f64) {
        self.gpu_temp_window.push(celsius);
    }

    /// Record GPU utilization (0–100%)
    pub fn record_gpu_util(&mut self, percent: f64) {
        self.gpu_util_window.push(percent);
    }

    /// Record disk usage percentage
    pub fn record_disk_usage(&mut self, percent: f64) {
        self.disk_usage_window.push(percent);
    }

    /// Record network receive rate (bytes/sec)
    pub fn record_network_rx(&mut self, bytes_per_sec: f64) {
        self.network_rx_window.push(bytes_per_sec);
    }

    /// Record network transmit rate (bytes/sec)
    pub fn record_network_tx(&mut self, bytes_per_sec: f64) {
        self.network_tx_window.push(bytes_per_sec);
    }

    /// Detect anomalies across all recorded metrics
    pub fn detect(&mut self) -> Vec<Anomaly> {
        let mut anomalies = Vec::new();
        let now = self.start_time.elapsed().as_secs_f64();

        // CPU anomalies
        if let Some(cpu) = self.cpu_window.last() {
            if cpu >= self.config.cpu_critical {
                self.maybe_alert(
                    &mut anomalies,
                    "cpu",
                    AnomalySeverity::Critical,
                    cpu,
                    now,
                    "CPU utilization is critically high",
                    vec![
                        "Identify and terminate CPU-intensive processes".into(),
                        "Check for runaway processes or infinite loops".into(),
                        "Consider scaling up CPU resources".into(),
                    ],
                );
            } else if cpu >= self.config.cpu_warning {
                self.maybe_alert(
                    &mut anomalies,
                    "cpu",
                    AnomalySeverity::Warning,
                    cpu,
                    now,
                    "CPU utilization is elevated",
                    vec![
                        "Monitor for sustained high usage".into(),
                        "Review process priorities with `nice`/`renice`".into(),
                    ],
                );
            } else if self.cpu_window.len() >= self.config.min_samples {
                let z = self.cpu_window.z_score(cpu);
                if z.abs() > self.config.z_score_threshold {
                    self.maybe_alert(
                        &mut anomalies,
                        "cpu_spike",
                        AnomalySeverity::Info,
                        cpu,
                        now,
                        &format!("Unusual CPU activity (z-score: {:.1})", z),
                        vec!["Check for newly started processes".into()],
                    );
                }
            }
            // Trend detection
            let trend = self.cpu_window.trend();
            if trend > 15.0 && self.cpu_window.len() >= self.config.min_samples {
                self.maybe_alert(
                    &mut anomalies,
                    "cpu_trend",
                    AnomalySeverity::Warning,
                    cpu,
                    now,
                    &format!("CPU usage trending upward (+{:.1}% avg)", trend),
                    vec![
                        "Possible memory leak causing swap thrashing".into(),
                        "Check for accumulating background tasks".into(),
                    ],
                );
            }
        }

        // Memory anomalies
        if let Some(mem) = self.memory_window.last() {
            if mem >= self.config.memory_critical {
                self.maybe_alert(
                    &mut anomalies,
                    "memory",
                    AnomalySeverity::Critical,
                    mem,
                    now,
                    "Memory usage is critically high — OOM risk",
                    vec![
                        "Identify memory-hungry processes with `ironmon process --sort memory`"
                            .into(),
                        "Check for memory leaks in long-running services".into(),
                        "Consider adding swap or increasing RAM".into(),
                    ],
                );
            } else if mem >= self.config.memory_warning {
                self.maybe_alert(
                    &mut anomalies,
                    "memory",
                    AnomalySeverity::Warning,
                    mem,
                    now,
                    "Memory usage is elevated",
                    vec![
                        "Close unused applications".into(),
                        "Check browser tab count and extensions".into(),
                    ],
                );
            }
        }

        // GPU temperature anomalies
        if let Some(temp) = self.gpu_temp_window.last() {
            if temp >= self.config.gpu_temp_critical {
                self.maybe_alert(
                    &mut anomalies,
                    "gpu_temp",
                    AnomalySeverity::Critical,
                    temp,
                    now,
                    "GPU temperature is critically high — throttling likely",
                    vec![
                        "Check GPU fan operation and airflow".into(),
                        "Reduce GPU workload or power limit".into(),
                        "Clean dust from heatsink and fans".into(),
                        "Consider improving case ventilation".into(),
                    ],
                );
            } else if temp >= self.config.gpu_temp_warning {
                self.maybe_alert(
                    &mut anomalies,
                    "gpu_temp",
                    AnomalySeverity::Warning,
                    temp,
                    now,
                    "GPU temperature is elevated",
                    vec![
                        "Monitor for sustained high temperatures".into(),
                        "Adjust fan curve for better cooling".into(),
                    ],
                );
            }
        }

        // Disk usage anomalies
        if let Some(disk) = self.disk_usage_window.last() {
            if disk >= self.config.disk_warning {
                self.maybe_alert(
                    &mut anomalies,
                    "disk",
                    AnomalySeverity::Warning,
                    disk,
                    now,
                    "Disk usage is high",
                    vec![
                        "Clean temporary files and caches".into(),
                        "Review and remove unused packages/data".into(),
                        "Check log file sizes in /var/log".into(),
                    ],
                );
            }
        }

        // Network anomalies (statistical only)
        if self.network_rx_window.len() >= self.config.min_samples {
            if let Some(rx) = self.network_rx_window.last() {
                let z = self.network_rx_window.z_score(rx);
                if z > self.config.z_score_threshold * 1.5 {
                    self.maybe_alert(
                        &mut anomalies,
                        "network_rx",
                        AnomalySeverity::Info,
                        rx / 1_000_000.0,
                        now,
                        &format!(
                            "Unusually high network receive rate ({:.1} MB/s)",
                            rx / 1_000_000.0
                        ),
                        vec![
                            "Check for large downloads or updates".into(),
                            "Review active network connections".into(),
                        ],
                    );
                }
            }
        }

        anomalies
    }

    // Threshold, severity, and message plumbing for one alert; splitting into a
    // struct here would only move the same eight values behind a constructor.
    #[allow(clippy::too_many_arguments)]
    fn maybe_alert(
        &mut self,
        anomalies: &mut Vec<Anomaly>,
        key: &str,
        severity: AnomalySeverity,
        value: f64,
        now: f64,
        message: &str,
        recommendations: Vec<String>,
    ) {
        // Check cooldown
        if let Some(&last) = self.last_alerts.get(key) {
            if last.elapsed() < self.config.alert_cooldown {
                return;
            }
        }
        self.last_alerts.insert(key.to_string(), Instant::now());

        anomalies.push(Anomaly {
            metric: key.to_string(),
            severity,
            message: message.to_string(),
            current_value: value,
            expected_range: None,
            z_score: None,
            recommendations,
            timestamp_secs: now,
        });
    }

    /// Generate optimization recommendations based on current system state
    pub fn recommendations(&self) -> Vec<Recommendation> {
        let mut recs = Vec::new();

        // CPU recommendations
        if let Some(cpu) = self.cpu_window.last() {
            if cpu > 80.0 {
                recs.push(Recommendation {
                    category: "CPU".into(),
                    priority: 1,
                    description: "High CPU usage detected. Consider process prioritization.".into(),
                    impact: "Reduce latency and improve responsiveness".into(),
                });
            }
        }

        // Memory recommendations
        if let Some(mem) = self.memory_window.last() {
            if mem > 80.0 {
                recs.push(Recommendation {
                    category: "Memory".into(),
                    priority: if mem > 90.0 { 1 } else { 2 },
                    description: format!(
                        "Memory usage at {:.0}%. Consider closing unused applications.",
                        mem
                    ),
                    impact: "Prevent OOM kills and swap thrashing".into(),
                });
            }
        }

        // GPU thermal recommendations
        if let Some(temp) = self.gpu_temp_window.last() {
            if temp > 75.0 {
                recs.push(Recommendation {
                    category: "GPU Thermal".into(),
                    priority: if temp > 85.0 { 1 } else { 3 },
                    description: format!(
                        "GPU at {:.0}°C. Consider adjusting fan curve or reducing power limit.",
                        temp
                    ),
                    impact: "Prevent thermal throttling, extend GPU lifespan".into(),
                });
            }
        }

        // GPU utilization recommendations
        if let (Some(gpu_util), Some(gpu_temp)) =
            (self.gpu_util_window.last(), self.gpu_temp_window.last())
        {
            if gpu_util < 30.0 && gpu_temp > 60.0 {
                recs.push(Recommendation {
                    category: "GPU Power".into(),
                    priority: 4,
                    description: "GPU is warm but underutilized. Power limit could be reduced."
                        .into(),
                    impact: "Reduce power consumption and heat output".into(),
                });
            }
        }

        // Disk recommendations
        if let Some(disk) = self.disk_usage_window.last() {
            if disk > 85.0 {
                recs.push(Recommendation {
                    category: "Storage".into(),
                    priority: 2,
                    description: format!(
                        "Disk at {:.0}% capacity. Performance degrades above 90%.",
                        disk
                    ),
                    impact: "Maintain filesystem performance and prevent write failures".into(),
                });
            }
        }

        recs.sort_by_key(|r| r.priority);
        recs
    }

    /// Get a summary string of detector state
    pub fn summary(&self) -> AnomalySummary {
        AnomalySummary {
            cpu_mean: self.cpu_window.mean(),
            cpu_std: self.cpu_window.std_dev(),
            memory_mean: self.memory_window.mean(),
            memory_std: self.memory_window.std_dev(),
            gpu_temp_mean: self.gpu_temp_window.mean(),
            gpu_temp_std: self.gpu_temp_window.std_dev(),
            samples_collected: self.cpu_window.len(),
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }
}

/// Summary of anomaly detector statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalySummary {
    /// Mean CPU utilization
    pub cpu_mean: f64,
    /// CPU standard deviation
    pub cpu_std: f64,
    /// Mean memory utilization
    pub memory_mean: f64,
    /// Memory standard deviation
    pub memory_std: f64,
    /// Mean GPU temperature
    pub gpu_temp_mean: f64,
    /// GPU temperature standard deviation
    pub gpu_temp_std: f64,
    /// Number of samples collected
    pub samples_collected: usize,
    /// Seconds since detector started
    pub uptime_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_window_stats() {
        let mut w = MetricWindow::new(10);
        for v in [10.0, 20.0, 30.0, 40.0, 50.0] {
            w.push(v);
        }
        assert!((w.mean() - 30.0).abs() < 0.01);
        assert!(w.std_dev() > 0.0);
    }

    #[test]
    fn test_z_score_spike() {
        let mut w = MetricWindow::new(20);
        for _ in 0..15 {
            w.push(50.0);
        }
        // A spike should produce high z-score
        let z = w.z_score(99.0);
        assert!(z > 2.0, "z-score for spike should be high, got {}", z);
    }

    #[test]
    fn test_anomaly_detection_critical_cpu() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        for _ in 0..15 {
            detector.record_cpu(50.0);
        }
        detector.record_cpu(97.0);
        let anomalies = detector.detect();
        assert!(!anomalies.is_empty(), "Should detect critical CPU");
        assert_eq!(anomalies[0].severity, AnomalySeverity::Critical);
    }

    #[test]
    fn test_anomaly_detection_normal() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        for _ in 0..15 {
            detector.record_cpu(30.0);
            detector.record_memory(40.0);
        }
        let anomalies = detector.detect();
        assert!(
            anomalies.is_empty(),
            "Should not detect anomalies for normal values"
        );
    }

    #[test]
    fn test_recommendations() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        detector.record_cpu(92.0);
        detector.record_memory(88.0);
        detector.record_gpu_temp(85.0);
        let recs = detector.recommendations();
        assert!(
            recs.len() >= 2,
            "Should have recommendations for high usage"
        );
    }

    #[test]
    fn test_trend_detection() {
        let mut w = MetricWindow::new(20);
        // Increasing trend
        for i in 0..20 {
            w.push(30.0 + i as f64 * 2.0);
        }
        assert!(w.trend() > 0.0, "Should detect upward trend");
    }

    #[test]
    fn test_summary() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        detector.record_cpu(50.0);
        detector.record_memory(60.0);
        let summary = detector.summary();
        assert_eq!(summary.samples_collected, 1);
    }
}
