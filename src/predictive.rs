//! Predictive Maintenance Alerts
//!
//! Analyzes hardware metric trends to predict potential failures before they
//! occur. Uses linear regression, moving averages, and degradation rate
//! analysis to generate maintenance alerts.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::predictive::{MaintenanceEngine, PredictionConfig};
//!
//! let mut engine = MaintenanceEngine::new(PredictionConfig::default());
//!
//! // Feed historical data
//! for temp in [65.0, 66.0, 67.5, 68.0, 70.0, 72.0, 74.0, 76.0] {
//!     engine.record_gpu_temp(0, temp);
//! }
//! engine.record_disk_health(0, 95.0); // 95% SMART health
//! engine.record_fan_rpm(0, 1200.0);
//!
//! // Get predictions
//! for alert in engine.predict() {
//!     println!("[{}] {}: {}", alert.urgency, alert.component, alert.message);
//!     if let Some(eta) = alert.eta_hours {
//!         println!("  Estimated time to issue: {:.0} hours", eta);
//!     }
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for predictive maintenance analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionConfig {
    /// Minimum data points for trend analysis
    pub min_data_points: usize,
    /// GPU temperature threshold for thermal alert (°C)
    pub gpu_thermal_limit: f64,
    /// Disk health warning threshold (%)
    pub disk_health_warning: f64,
    /// Fan RPM minimum before failure prediction
    pub fan_min_rpm: f64,
    /// Fan degradation rate threshold (RPM/hour decline)
    pub fan_degradation_rate: f64,
    /// Memory error rate threshold (errors/hour)
    pub memory_error_threshold: f64,
    /// GPU clock degradation threshold (MHz decline over window)
    pub clock_degradation_mhz: f64,
    /// Number of samples in sliding window
    pub window_size: usize,
}

impl Default for PredictionConfig {
    fn default() -> Self {
        Self {
            min_data_points: 10,
            gpu_thermal_limit: 95.0,
            disk_health_warning: 80.0,
            fan_min_rpm: 500.0,
            fan_degradation_rate: 50.0,
            memory_error_threshold: 1.0,
            clock_degradation_mhz: 100.0,
            window_size: 100,
        }
    }
}

/// Urgency level for maintenance alerts
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Urgency {
    /// Informational — no action needed yet
    Low,
    /// Plan maintenance within weeks
    Medium,
    /// Schedule maintenance within days
    High,
    /// Immediate attention required
    Critical,
}

impl std::fmt::Display for Urgency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "LOW"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::High => write!(f, "HIGH"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// A predictive maintenance alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceAlert {
    /// Component that may need maintenance
    pub component: String,
    /// Type of issue predicted
    pub issue_type: IssueType,
    /// Urgency level
    pub urgency: Urgency,
    /// Human-readable message
    pub message: String,
    /// Estimated time to issue in hours (None if cannot predict)
    pub eta_hours: Option<f64>,
    /// Current degradation rate
    pub degradation_rate: Option<f64>,
    /// Current value
    pub current_value: f64,
    /// Threshold value
    pub threshold: f64,
    /// Recommended action
    pub action: String,
    /// Confidence level (0.0–1.0)
    pub confidence: f64,
}

/// Type of predicted issue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueType {
    /// Component approaching thermal limits
    ThermalDegradation,
    /// Storage device health declining
    DiskFailure,
    /// Fan speed declining (bearing wear)
    FanFailure,
    /// GPU clocks declining (chip degradation)
    GpuDegradation,
    /// Memory errors increasing
    MemoryFailure,
    /// Power supply degradation
    PowerDegradation,
    /// Capacitor aging (increasing voltage ripple)
    CapacitorAging,
}

impl std::fmt::Display for IssueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ThermalDegradation => write!(f, "Thermal Degradation"),
            Self::DiskFailure => write!(f, "Disk Failure Risk"),
            Self::FanFailure => write!(f, "Fan Failure Risk"),
            Self::GpuDegradation => write!(f, "GPU Degradation"),
            Self::MemoryFailure => write!(f, "Memory Failure Risk"),
            Self::PowerDegradation => write!(f, "Power Degradation"),
            Self::CapacitorAging => write!(f, "Capacitor Aging"),
        }
    }
}

/// Time-series data for a single metric
struct TimeSeries {
    values: Vec<f64>,
    max_size: usize,
}

impl TimeSeries {
    fn new(max_size: usize) -> Self {
        Self {
            values: Vec::with_capacity(max_size),
            max_size,
        }
    }

    fn push(&mut self, value: f64) {
        if self.values.len() >= self.max_size {
            self.values.remove(0);
        }
        self.values.push(value);
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn last(&self) -> Option<f64> {
        self.values.last().copied()
    }

    fn mean(&self) -> f64 {
        if self.values.is_empty() {
            return 0.0;
        }
        self.values.iter().sum::<f64>() / self.values.len() as f64
    }

    /// Simple linear regression: returns (slope, intercept)
    fn linear_regression(&self) -> Option<(f64, f64)> {
        let n = self.values.len() as f64;
        if n < 3.0 {
            return None;
        }

        let x_mean = (n - 1.0) / 2.0;
        let y_mean = self.mean();

        let mut ss_xy = 0.0;
        let mut ss_xx = 0.0;

        for (i, y) in self.values.iter().enumerate() {
            let x = i as f64;
            ss_xy += (x - x_mean) * (y - y_mean);
            ss_xx += (x - x_mean) * (x - x_mean);
        }

        if ss_xx.abs() < 1e-10 {
            return None;
        }

        let slope = ss_xy / ss_xx;
        let intercept = y_mean - slope * x_mean;

        Some((slope, intercept))
    }

    /// Predict value at future index (from current last position)
    #[allow(dead_code)]
    fn predict_at(&self, steps_ahead: usize) -> Option<f64> {
        let (slope, intercept) = self.linear_regression()?;
        let future_x = (self.values.len() + steps_ahead - 1) as f64;
        Some(slope * future_x + intercept)
    }

    /// Estimate steps until value crosses threshold
    fn steps_until_threshold(&self, threshold: f64) -> Option<f64> {
        let (slope, intercept) = self.linear_regression()?;
        if slope.abs() < 1e-10 {
            return None;
        }
        let current_x = (self.values.len() - 1) as f64;
        let threshold_x = (threshold - intercept) / slope;
        let steps = threshold_x - current_x;
        if steps > 0.0 {
            Some(steps)
        } else {
            None
        }
    }

    /// R² coefficient of determination (goodness of fit)
    fn r_squared(&self) -> f64 {
        if self.values.len() < 3 {
            return 0.0;
        }
        let (slope, intercept) = match self.linear_regression() {
            Some(v) => v,
            None => return 0.0,
        };
        let y_mean = self.mean();
        let mut ss_res = 0.0;
        let mut ss_tot = 0.0;
        for (i, y) in self.values.iter().enumerate() {
            let predicted = slope * i as f64 + intercept;
            ss_res += (y - predicted).powi(2);
            ss_tot += (y - y_mean).powi(2);
        }
        if ss_tot.abs() < 1e-10 {
            return 0.0;
        }
        1.0 - (ss_res / ss_tot)
    }
}

/// Predictive maintenance engine
pub struct MaintenanceEngine {
    config: PredictionConfig,
    gpu_temps: HashMap<usize, TimeSeries>,
    gpu_clocks: HashMap<usize, TimeSeries>,
    disk_health: HashMap<usize, TimeSeries>,
    fan_rpms: HashMap<usize, TimeSeries>,
    memory_errors: TimeSeries,
}

impl MaintenanceEngine {
    /// Create a new maintenance engine
    pub fn new(config: PredictionConfig) -> Self {
        let ws = config.window_size;
        Self {
            config,
            gpu_temps: HashMap::new(),
            gpu_clocks: HashMap::new(),
            disk_health: HashMap::new(),
            fan_rpms: HashMap::new(),
            memory_errors: TimeSeries::new(ws),
        }
    }

    /// Record GPU temperature measurement
    pub fn record_gpu_temp(&mut self, gpu_idx: usize, celsius: f64) {
        self.gpu_temps
            .entry(gpu_idx)
            .or_insert_with(|| TimeSeries::new(self.config.window_size))
            .push(celsius);
    }

    /// Record GPU clock speed measurement
    pub fn record_gpu_clock(&mut self, gpu_idx: usize, mhz: f64) {
        self.gpu_clocks
            .entry(gpu_idx)
            .or_insert_with(|| TimeSeries::new(self.config.window_size))
            .push(mhz);
    }

    /// Record disk SMART health percentage
    pub fn record_disk_health(&mut self, disk_idx: usize, percent: f64) {
        self.disk_health
            .entry(disk_idx)
            .or_insert_with(|| TimeSeries::new(self.config.window_size))
            .push(percent);
    }

    /// Record fan RPM measurement
    pub fn record_fan_rpm(&mut self, fan_idx: usize, rpm: f64) {
        self.fan_rpms
            .entry(fan_idx)
            .or_insert_with(|| TimeSeries::new(self.config.window_size))
            .push(rpm);
    }

    /// Record memory error count
    pub fn record_memory_errors(&mut self, errors: f64) {
        self.memory_errors.push(errors);
    }

    /// Run prediction analysis and return alerts
    pub fn predict(&self) -> Vec<MaintenanceAlert> {
        let mut alerts = Vec::new();

        self.predict_gpu_thermal(&mut alerts);
        self.predict_gpu_degradation(&mut alerts);
        self.predict_disk_failure(&mut alerts);
        self.predict_fan_failure(&mut alerts);
        self.predict_memory_failure(&mut alerts);

        alerts.sort_by_key(|p| std::cmp::Reverse(p.urgency));
        alerts
    }

    fn predict_gpu_thermal(&self, alerts: &mut Vec<MaintenanceAlert>) {
        for (&gpu_idx, ts) in &self.gpu_temps {
            if ts.len() < self.config.min_data_points {
                continue;
            }
            let current = match ts.last() {
                Some(v) => v,
                None => continue,
            };

            // Check if trending toward thermal limit
            if let Some(eta) = ts.steps_until_threshold(self.config.gpu_thermal_limit) {
                let (slope, _) = ts.linear_regression().unwrap_or((0.0, 0.0));
                if slope > 0.1 {
                    // Temperature is rising
                    let confidence = ts.r_squared().clamp(0.0, 1.0);
                    let urgency = if eta < 10.0 {
                        Urgency::Critical
                    } else if eta < 50.0 {
                        Urgency::High
                    } else if eta < 200.0 {
                        Urgency::Medium
                    } else {
                        Urgency::Low
                    };

                    alerts.push(MaintenanceAlert {
                        component: format!("GPU {}", gpu_idx),
                        issue_type: IssueType::ThermalDegradation,
                        urgency,
                        message: format!(
                            "GPU {} temperature trending upward ({:.1}°C → predicted {:.1}°C). \
                             Rate: +{:.2}°C/sample",
                            gpu_idx, current, self.config.gpu_thermal_limit, slope
                        ),
                        eta_hours: Some(eta),
                        degradation_rate: Some(slope),
                        current_value: current,
                        threshold: self.config.gpu_thermal_limit,
                        action: "Check cooling system, clean heatsinks, adjust fan curve".into(),
                        confidence,
                    });
                }
            }
        }
    }

    fn predict_gpu_degradation(&self, alerts: &mut Vec<MaintenanceAlert>) {
        for (&gpu_idx, ts) in &self.gpu_clocks {
            if ts.len() < self.config.min_data_points {
                continue;
            }
            let current = match ts.last() {
                Some(v) => v,
                None => continue,
            };

            if let Some((slope, _)) = ts.linear_regression() {
                // Negative slope means clocks are declining
                if slope < -1.0 {
                    let decline = slope.abs() * ts.len() as f64;
                    if decline > self.config.clock_degradation_mhz {
                        let confidence = ts.r_squared().clamp(0.0, 1.0);
                        alerts.push(MaintenanceAlert {
                            component: format!("GPU {}", gpu_idx),
                            issue_type: IssueType::GpuDegradation,
                            urgency: if decline > 200.0 {
                                Urgency::High
                            } else {
                                Urgency::Medium
                            },
                            message: format!(
                                "GPU {} clocks declining ({:.0} MHz loss over {} samples)",
                                gpu_idx,
                                decline,
                                ts.len()
                            ),
                            eta_hours: None,
                            degradation_rate: Some(slope),
                            current_value: current,
                            threshold: 0.0,
                            action:
                                "May indicate chip degradation. Monitor power limits and thermals."
                                    .into(),
                            confidence,
                        });
                    }
                }
            }
        }
    }

    fn predict_disk_failure(&self, alerts: &mut Vec<MaintenanceAlert>) {
        for (&disk_idx, ts) in &self.disk_health {
            if ts.len() < self.config.min_data_points {
                continue;
            }
            let current = match ts.last() {
                Some(v) => v,
                None => continue,
            };

            if current < self.config.disk_health_warning {
                let urgency = if current < 50.0 {
                    Urgency::Critical
                } else if current < 70.0 {
                    Urgency::High
                } else {
                    Urgency::Medium
                };

                let eta = ts.steps_until_threshold(0.0);

                alerts.push(MaintenanceAlert {
                    component: format!("Disk {}", disk_idx),
                    issue_type: IssueType::DiskFailure,
                    urgency,
                    message: format!(
                        "Disk {} SMART health at {:.0}% (threshold: {:.0}%)",
                        disk_idx, current, self.config.disk_health_warning
                    ),
                    eta_hours: eta,
                    degradation_rate: ts.linear_regression().map(|(s, _)| s),
                    current_value: current,
                    threshold: self.config.disk_health_warning,
                    action: "Back up important data immediately. Plan disk replacement.".into(),
                    confidence: 0.9,
                });
            }
        }
    }

    fn predict_fan_failure(&self, alerts: &mut Vec<MaintenanceAlert>) {
        for (&fan_idx, ts) in &self.fan_rpms {
            if ts.len() < self.config.min_data_points {
                continue;
            }
            let current = match ts.last() {
                Some(v) => v,
                None => continue,
            };

            // Check for declining RPM
            if let Some((slope, _)) = ts.linear_regression() {
                if slope < -self.config.fan_degradation_rate / ts.len() as f64 {
                    let eta = ts.steps_until_threshold(self.config.fan_min_rpm);
                    let confidence = ts.r_squared().clamp(0.0, 1.0);

                    let urgency = if current < self.config.fan_min_rpm {
                        Urgency::Critical
                    } else if let Some(eta_val) = eta {
                        if eta_val < 50.0 {
                            Urgency::High
                        } else {
                            Urgency::Medium
                        }
                    } else {
                        Urgency::Low
                    };

                    alerts.push(MaintenanceAlert {
                        component: format!("Fan {}", fan_idx),
                        issue_type: IssueType::FanFailure,
                        urgency,
                        message: format!(
                            "Fan {} RPM declining (currently {:.0} RPM, rate: {:.1} RPM/sample)",
                            fan_idx, current, slope
                        ),
                        eta_hours: eta,
                        degradation_rate: Some(slope),
                        current_value: current,
                        threshold: self.config.fan_min_rpm,
                        action: "Inspect fan bearings, clean dust, plan replacement.".into(),
                        confidence,
                    });
                }
            }
        }
    }

    fn predict_memory_failure(&self, alerts: &mut Vec<MaintenanceAlert>) {
        if self.memory_errors.len() < self.config.min_data_points {
            return;
        }
        let current = match self.memory_errors.last() {
            Some(v) => v,
            None => return,
        };

        if current > 0.0 {
            if let Some((slope, _)) = self.memory_errors.linear_regression() {
                if slope > 0.0 {
                    let urgency = if current > self.config.memory_error_threshold * 10.0 {
                        Urgency::Critical
                    } else if current > self.config.memory_error_threshold {
                        Urgency::High
                    } else {
                        Urgency::Medium
                    };

                    alerts.push(MaintenanceAlert {
                        component: "Memory".into(),
                        issue_type: IssueType::MemoryFailure,
                        urgency,
                        message: format!(
                            "Memory errors increasing ({:.0} errors, rate: +{:.2}/sample)",
                            current, slope
                        ),
                        eta_hours: None,
                        degradation_rate: Some(slope),
                        current_value: current,
                        threshold: self.config.memory_error_threshold,
                        action: "Run memory diagnostics (memtest86+). Consider RAM replacement."
                            .into(),
                        confidence: 0.8,
                    });
                }
            }
        }
    }

    /// Get a summary of maintenance status
    pub fn summary(&self) -> MaintenanceSummary {
        let alerts = self.predict();
        MaintenanceSummary {
            total_alerts: alerts.len(),
            critical: alerts
                .iter()
                .filter(|a| a.urgency == Urgency::Critical)
                .count(),
            high: alerts.iter().filter(|a| a.urgency == Urgency::High).count(),
            medium: alerts
                .iter()
                .filter(|a| a.urgency == Urgency::Medium)
                .count(),
            low: alerts.iter().filter(|a| a.urgency == Urgency::Low).count(),
            components_monitored: self.gpu_temps.len()
                + self.disk_health.len()
                + self.fan_rpms.len()
                + if self.memory_errors.len() > 0 { 1 } else { 0 },
            data_points: self.gpu_temps.values().map(|t| t.len()).sum::<usize>()
                + self.disk_health.values().map(|t| t.len()).sum::<usize>()
                + self.fan_rpms.values().map(|t| t.len()).sum::<usize>()
                + self.memory_errors.len(),
        }
    }
}

/// Summary of predictive maintenance status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceSummary {
    /// Total number of active alerts
    pub total_alerts: usize,
    /// Number of critical alerts
    pub critical: usize,
    /// Number of high-urgency alerts
    pub high: usize,
    /// Number of medium-urgency alerts
    pub medium: usize,
    /// Number of low-urgency alerts
    pub low: usize,
    /// Number of components being monitored
    pub components_monitored: usize,
    /// Total data points collected
    pub data_points: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_regression() {
        let mut ts = TimeSeries::new(100);
        for i in 0..20 {
            ts.push(i as f64 * 2.0 + 10.0);
        }
        let (slope, intercept) = ts.linear_regression().unwrap();
        assert!((slope - 2.0).abs() < 0.01);
        assert!((intercept - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_steps_until_threshold() {
        let mut ts = TimeSeries::new(100);
        // Linear increase: 50, 55, 60, 65, 70...
        for i in 0..10 {
            ts.push(50.0 + i as f64 * 5.0);
        }
        // Current at 95, threshold at 100
        let steps = ts.steps_until_threshold(120.0);
        assert!(steps.is_some());
        assert!(steps.unwrap() > 0.0);
    }

    #[test]
    fn test_r_squared_perfect_fit() {
        let mut ts = TimeSeries::new(100);
        for i in 0..20 {
            ts.push(i as f64 * 3.0);
        }
        let r2 = ts.r_squared();
        assert!(
            r2 > 0.99,
            "R² should be near 1.0 for perfect linear data, got {}",
            r2
        );
    }

    #[test]
    fn test_thermal_prediction() {
        let mut engine = MaintenanceEngine::new(PredictionConfig::default());
        // Simulated rising GPU temperatures
        for i in 0..20 {
            engine.record_gpu_temp(0, 60.0 + i as f64 * 1.5);
        }
        let alerts = engine.predict();
        assert!(!alerts.is_empty(), "Should predict thermal issue");
        assert_eq!(alerts[0].issue_type, IssueType::ThermalDegradation);
    }

    #[test]
    fn test_disk_health_alert() {
        let mut engine = MaintenanceEngine::new(PredictionConfig::default());
        for i in 0..15 {
            engine.record_disk_health(0, 90.0 - i as f64 * 2.0);
        }
        let alerts = engine.predict();
        assert!(
            alerts
                .iter()
                .any(|a| a.issue_type == IssueType::DiskFailure),
            "Should detect disk health decline"
        );
    }

    #[test]
    fn test_fan_failure_prediction() {
        let mut engine = MaintenanceEngine::new(PredictionConfig::default());
        for i in 0..20 {
            engine.record_fan_rpm(0, 1800.0 - i as f64 * 60.0);
        }
        let alerts = engine.predict();
        assert!(
            alerts.iter().any(|a| a.issue_type == IssueType::FanFailure),
            "Should predict fan failure"
        );
    }

    #[test]
    fn test_no_alerts_stable_system() {
        let mut engine = MaintenanceEngine::new(PredictionConfig::default());
        for _ in 0..20 {
            engine.record_gpu_temp(0, 65.0);
            engine.record_disk_health(0, 98.0);
            engine.record_fan_rpm(0, 1800.0);
        }
        let alerts = engine.predict();
        assert!(alerts.is_empty(), "Stable system should produce no alerts");
    }

    #[test]
    fn test_maintenance_summary() {
        let mut engine = MaintenanceEngine::new(PredictionConfig::default());
        for i in 0..20 {
            engine.record_gpu_temp(0, 60.0 + i as f64 * 1.5);
        }
        let summary = engine.summary();
        assert!(summary.components_monitored > 0);
        assert!(summary.data_points > 0);
    }
}
