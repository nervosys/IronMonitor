//! Anomaly Detection and System Optimization
//!
//! Statistical anomaly detection for hardware metrics with actionable optimization
//! recommendations. Uses moving averages, z-score analysis, and threshold-based
//! alerting to identify unusual system behavior.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonitor::anomaly::{AnomalyDetector, AnomalyConfig};
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
//! let report = detector.detect();
//! for anomaly in &report.anomalies {
//!     println!("[{}] {}: {}", anomaly.severity, anomaly.metric, anomaly.message);
//!     for rec in &anomaly.recommendations {
//!         println!("  → {}", rec);
//!     }
//! }
//! // Checks that could not run are not "no anomaly".
//! for gap in &report.not_checked {
//!     println!("{gap}");
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

/// Why a check did not run. A check that did not run says nothing about the
/// system, so it must never be read as "no anomaly" or "healthy".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckGap {
    /// No sample has been recorded for the metric.
    NoSamples,
    /// Samples exist, but the rule needs `need` and only `have` were recorded.
    InsufficientSamples { have: usize, need: usize },
    /// The metric was not read in the latest cycle, so there is no current value
    /// to test.
    MetricUnavailable,
}

impl std::fmt::Display for CheckGap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSamples => write!(f, "no samples recorded"),
            Self::InsufficientSamples { have, need } => {
                write!(f, "{have} of the {need} samples needed recorded")
            }
            Self::MetricUnavailable => write!(f, "not read in the latest cycle"),
        }
    }
}

/// A check that could not run, and why. Shared with [`crate::predictive`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotChecked {
    /// The check that did not run, such as "cpu thresholds".
    pub check: String,
    /// Why it did not run.
    pub reason: CheckGap,
}

impl std::fmt::Display for NotChecked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} not checked: {}", self.check, self.reason)
    }
}

/// What a report can say about the system as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// At least one finding. This holds whatever else was not checked.
    Findings,
    /// Every check ran and none found anything.
    Clear,
    /// Nothing was found, but some checks did not run. This is not a clean result.
    NotFullyChecked,
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Findings => write!(f, "findings"),
            Self::Clear => write!(f, "clear"),
            Self::NotFullyChecked => write!(f, "not fully checked"),
        }
    }
}

/// The verdict for a report with these findings and gaps.
pub(crate) fn verdict_for(has_findings: bool, not_checked: &[NotChecked]) -> Verdict {
    if has_findings {
        Verdict::Findings
    } else if not_checked.is_empty() {
        Verdict::Clear
    } else {
        Verdict::NotFullyChecked
    }
}

/// The gap for a rule that needs `need` samples, or `None` when it has them.
pub(crate) fn sample_gap(have: usize, need: usize) -> Option<CheckGap> {
    if have >= need {
        None
    } else if have == 0 {
        Some(CheckGap::NoSamples)
    } else {
        Some(CheckGap::InsufficientSamples { have, need })
    }
}

/// Result of one [`AnomalyDetector::detect`] call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyReport {
    /// Anomalies found this cycle.
    pub anomalies: Vec<Anomaly>,
    /// Checks that could not run this cycle, with the reason for each.
    pub not_checked: Vec<NotChecked>,
}

impl AnomalyReport {
    /// What this report says overall. An empty `anomalies` list is not a verdict
    /// of health on its own: it is only that, if `not_checked` is empty too.
    pub fn verdict(&self) -> Verdict {
        verdict_for(!self.anomalies.is_empty(), &self.not_checked)
    }
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

    /// Mean of the window, or `None` when it holds no samples. `mean()` returns
    /// 0.0 for an empty window, which is a value, not a reading.
    fn known_mean(&self) -> Option<f64> {
        (!self.values.is_empty()).then(|| self.sum / self.values.len() as f64)
    }

    fn std_dev(&self) -> f64 {
        let n = self.values.len() as f64;
        if n < 2.0 {
            return 0.0;
        }
        let variance = (self.sum_sq - (self.sum * self.sum) / n) / (n - 1.0);
        variance.max(0.0).sqrt()
    }

    /// Sample standard deviation, or `None` with fewer than two samples. The
    /// `std_dev()` helper answers 0.0 there, which says "no spread" when the
    /// truth is "not measured".
    fn known_std_dev(&self) -> Option<f64> {
        (self.values.len() >= 2).then(|| self.std_dev())
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
    /// Latest GPU temperature, `None` when the last call reported it unread. Kept
    /// apart from the window so an unread cycle does not reuse an older sample.
    gpu_temp_latest: Option<f64>,
    /// Latest GPU utilization, with the same meaning as `gpu_temp_latest`.
    gpu_util_latest: Option<f64>,
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
            gpu_temp_latest: None,
            gpu_util_latest: None,
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

    /// Record a GPU temperature sample (°C). `None` means the temperature was not
    /// read this cycle: nothing is added to the window, and no GPU thermal rule
    /// runs on an older value. A caller must not pass 0.0 for an unread sensor.
    pub fn record_gpu_temp(&mut self, celsius: Option<f64>) {
        self.gpu_temp_latest = celsius;
        if let Some(c) = celsius {
            self.gpu_temp_window.push(c);
        }
    }

    /// Record GPU utilization (0–100%). `None` means not read this cycle, with the
    /// same handling as [`Self::record_gpu_temp`].
    pub fn record_gpu_util(&mut self, percent: Option<f64>) {
        self.gpu_util_latest = percent;
        if let Some(p) = percent {
            self.gpu_util_window.push(p);
        }
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

    /// Detect anomalies across all recorded metrics.
    ///
    /// The report also lists every check that could not run on the samples held,
    /// so an empty `anomalies` list is never mistaken for "healthy". See
    /// [`AnomalyReport::verdict`].
    pub fn detect(&mut self) -> AnomalyReport {
        let not_checked = self.check_gaps();
        let anomalies = self.find_anomalies();
        AnomalyReport {
            anomalies,
            not_checked,
        }
    }

    /// Checks that cannot run on the samples held now. The conditions mirror the
    /// ones `find_anomalies` tests before it evaluates a rule.
    fn check_gaps(&self) -> Vec<NotChecked> {
        let need = self.config.min_samples;
        // Unread is not the same as never recorded: the GPU latest is `None` in
        // both cases, and only the window tells them apart.
        let gpu_temp_gap = match self.gpu_temp_latest {
            Some(_) => None,
            None if self.gpu_temp_window.len() == 0 => Some(CheckGap::NoSamples),
            None => Some(CheckGap::MetricUnavailable),
        };
        let mut gaps = Vec::new();
        let mut note = |check: &str, reason: Option<CheckGap>| {
            if let Some(reason) = reason {
                gaps.push(NotChecked {
                    check: check.to_string(),
                    reason,
                });
            }
        };
        note("cpu thresholds", sample_gap(self.cpu_window.len(), 1));
        note(
            "cpu spike and trend",
            sample_gap(self.cpu_window.len(), need),
        );
        note("memory thresholds", sample_gap(self.memory_window.len(), 1));
        note("gpu temperature thresholds", gpu_temp_gap);
        note("disk usage", sample_gap(self.disk_usage_window.len(), 1));
        note(
            "network receive spike",
            sample_gap(self.network_rx_window.len(), need),
        );
        gaps
    }

    /// Run every rule over the current samples and return the anomalies found.
    fn find_anomalies(&mut self) -> Vec<Anomaly> {
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
        if let Some(temp) = self.gpu_temp_latest {
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
        if let Some(temp) = self.gpu_temp_latest {
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
        if let (Some(gpu_util), Some(gpu_temp)) = (self.gpu_util_latest, self.gpu_temp_latest) {
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
            cpu_mean: self.cpu_window.known_mean(),
            cpu_std: self.cpu_window.known_std_dev(),
            memory_mean: self.memory_window.known_mean(),
            memory_std: self.memory_window.known_std_dev(),
            gpu_temp_mean: self.gpu_temp_window.known_mean(),
            gpu_temp_std: self.gpu_temp_window.known_std_dev(),
            samples_collected: self.cpu_window.len(),
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }
}

/// Summary of anomaly detector statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalySummary {
    /// Mean CPU utilization, `None` with no samples
    pub cpu_mean: Option<f64>,
    /// CPU standard deviation, `None` with fewer than two samples
    pub cpu_std: Option<f64>,
    /// Mean memory utilization, `None` with no samples
    pub memory_mean: Option<f64>,
    /// Memory standard deviation, `None` with fewer than two samples
    pub memory_std: Option<f64>,
    /// Mean GPU temperature, `None` with no samples
    pub gpu_temp_mean: Option<f64>,
    /// GPU temperature standard deviation, `None` with fewer than two samples
    pub gpu_temp_std: Option<f64>,
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
        let anomalies = detector.detect().anomalies;
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
        let report = detector.detect();
        assert!(
            report.anomalies.is_empty(),
            "Should not detect anomalies for normal values"
        );
    }

    #[test]
    fn test_recommendations() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        detector.record_cpu(92.0);
        detector.record_memory(88.0);
        detector.record_gpu_temp(Some(85.0));
        let recs = detector.recommendations();
        assert!(
            recs.len() >= 2,
            "Should have recommendations for high usage"
        );
    }

    /// An unread GPU sensor must not fire the GPU rules. Recording it as 0% util
    /// with a warm card used to produce "warm but underutilized".
    #[test]
    fn an_unread_gpu_does_not_fire_gpu_rules() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        detector.record_gpu_temp(Some(70.0));
        detector.record_gpu_util(Some(10.0));
        assert!(detector
            .recommendations()
            .iter()
            .any(|r| r.category == "GPU Power"));

        // The utilization read failed this cycle: the old 10% must not be reused,
        // and no 0% is invented in its place.
        detector.record_gpu_util(None);
        assert!(!detector
            .recommendations()
            .iter()
            .any(|r| r.category == "GPU Power"));
        assert_eq!(detector.gpu_util_window.len(), 1, "unread is not a sample");
    }

    /// Unread GPU temperature is not a 0 C reading and is not the last sample.
    #[test]
    fn an_unread_gpu_temperature_is_not_a_sample() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        detector.record_gpu_temp(Some(90.0));
        detector.record_gpu_temp(None);
        assert!(!detector
            .recommendations()
            .iter()
            .any(|r| r.category == "GPU Thermal"));
        assert_eq!(detector.gpu_temp_window.len(), 1);
    }

    /// An empty window has no mean, and one sample has no spread. Neither is
    /// reported as 0.0.
    #[test]
    fn summary_reports_no_mean_or_spread_without_samples() {
        let detector = AnomalyDetector::new(AnomalyConfig::default());
        let empty = detector.summary();
        assert_eq!(empty.cpu_mean, None);
        assert_eq!(empty.gpu_temp_mean, None);
        assert_eq!(empty.memory_std, None);

        let mut one = AnomalyDetector::new(AnomalyConfig::default());
        one.record_cpu(40.0);
        let s = one.summary();
        assert_eq!(s.cpu_mean, Some(40.0));
        assert_eq!(s.cpu_std, None, "one sample has no spread");
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
        assert_eq!(summary.memory_mean, Some(60.0));
    }

    /// An empty detector ran no checks, so its empty list is "not checked", not
    /// "clear".
    #[test]
    fn an_empty_detector_reports_its_checks_as_not_run() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        let report = detector.detect();
        assert!(report.anomalies.is_empty());
        assert_ne!(report.verdict(), Verdict::Clear);
        assert!(report.not_checked.contains(&NotChecked {
            check: "cpu thresholds".into(),
            reason: CheckGap::NoSamples,
        }));
        assert!(report.not_checked.contains(&NotChecked {
            check: "gpu temperature thresholds".into(),
            reason: CheckGap::NoSamples,
        }));
    }

    /// Five samples satisfy the thresholds but not the statistical rules, which
    /// must be reported as not run rather than as passed.
    #[test]
    fn too_few_samples_leave_the_statistical_rules_not_run() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        for _ in 0..5 {
            detector.record_cpu(30.0);
        }
        let report = detector.detect();
        assert_eq!(report.verdict(), Verdict::NotFullyChecked);
        assert!(report.not_checked.contains(&NotChecked {
            check: "cpu spike and trend".into(),
            reason: CheckGap::InsufficientSamples { have: 5, need: 10 },
        }));
        assert!(!report
            .not_checked
            .iter()
            .any(|c| c.check == "cpu thresholds"));
    }

    /// Every rule ran on enough samples and none fired: that is a clear verdict.
    #[test]
    fn a_fully_checked_quiet_system_is_clear() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        for _ in 0..15 {
            detector.record_cpu(30.0);
            detector.record_memory(40.0);
            detector.record_gpu_temp(Some(55.0));
            detector.record_disk_usage(50.0);
            detector.record_network_rx(1000.0);
        }
        let report = detector.detect();
        assert!(report.not_checked.is_empty(), "{:?}", report.not_checked);
        assert!(report.anomalies.is_empty());
        assert_eq!(report.verdict(), Verdict::Clear);
    }

    /// A finding stands even when other checks did not run.
    #[test]
    fn findings_stand_even_when_other_checks_did_not_run() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        detector.record_cpu(97.0);
        let report = detector.detect();
        assert_eq!(report.verdict(), Verdict::Findings);
        assert!(!report.not_checked.is_empty());
    }

    /// A GPU sensor that was read once and not in the latest cycle is an
    /// unavailable metric, not a quiet one.
    #[test]
    fn an_unread_gpu_temperature_is_reported_unavailable() {
        let mut detector = AnomalyDetector::new(AnomalyConfig::default());
        detector.record_gpu_temp(Some(60.0));
        detector.record_gpu_temp(None);
        let report = detector.detect();
        assert!(report.not_checked.contains(&NotChecked {
            check: "gpu temperature thresholds".into(),
            reason: CheckGap::MetricUnavailable,
        }));
    }
}
