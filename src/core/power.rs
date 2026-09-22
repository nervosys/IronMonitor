//! Power monitoring

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Power rail information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerRail {
    /// Rail online status
    pub online: bool,
    /// Sensor type (e.g., INA3221)
    pub sensor_type: String,
    /// Voltage in millivolts, or `None` where the rail reported none.
    ///
    /// **Bare beside `warn`/`crit`, which were already `Option`** -- the same
    /// tell that found the motherboard voltage rails, one module over. The
    /// Linux INA3221 reader filled this and `current` with
    /// `read_file_u32(..).unwrap_or(0)`, and the Windows battery path wrote
    /// `current: 0, // Not exposed by this class.` -- a comment stating the
    /// truth into a field asserting 0 mA.
    pub voltage: Option<u32>,
    /// Current in milliamperes, or `None` where the rail reported none.
    pub current: Option<u32>,
    /// Power in milliwatts, or `None` where it could not be established.
    ///
    /// On the INA3221 path this is **derived**, as `voltage * current`, and so
    /// is only as good as its inputs. It was computed from two `unwrap_or(0)`
    /// values, which meant one unreadable file produced a rail drawing exactly
    /// 0 W -- indistinguishable from a rail that is genuinely off, and summed
    /// into the system total as though it were a measurement.
    pub power: Option<u32>,
    /// Average power in milliwatts, or `None` before a sample establishes one.
    pub average: Option<u32>,
    /// Warning current limit in milliamperes (optional)
    pub warn: Option<u32>,
    /// Critical current limit in milliamperes (optional)
    pub crit: Option<u32>,
}

/// Total power information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotalPower {
    /// Total power in milliwatts, or `None` where it could not be totalled.
    ///
    /// Summed over the online rails, and `None` if any of them has no reading.
    /// Skipping an unreadable rail instead would understate system draw by an
    /// unknown amount while still looking like a measurement -- the worse of
    /// the two errors for anything sizing a power budget.
    pub power: Option<u32>,
    /// Average total power in milliwatts, or `None` before a sample
    /// establishes one.
    pub average: Option<u32>,
}

/// Exponential Moving Average calculator for power readings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerEma {
    /// Current EMA value
    value: f64,
    /// Smoothing factor (0.0 - 1.0, higher = more responsive)
    alpha: f64,
    /// Number of samples seen
    samples: u64,
}

impl PowerEma {
    /// Create a new EMA calculator with given alpha (smoothing factor)
    /// Alpha of 0.1 gives ~10 sample smoothing, 0.2 gives ~5 sample smoothing
    pub fn new(alpha: f64) -> Self {
        Self {
            value: 0.0,
            alpha: alpha.clamp(0.01, 1.0),
            samples: 0,
        }
    }

    /// Create EMA with default smoothing (alpha = 0.1 for ~10 sample window)
    pub fn default_smoothing() -> Self {
        Self::new(0.1)
    }

    /// Update with new sample and return new average
    pub fn update(&mut self, sample: u32) -> u32 {
        let sample_f = sample as f64;
        if self.samples == 0 {
            self.value = sample_f;
        } else {
            self.value = self.alpha * sample_f + (1.0 - self.alpha) * self.value;
        }
        self.samples += 1;
        self.value.round() as u32
    }

    /// Get current average value
    pub fn average(&self) -> u32 {
        self.value.round() as u32
    }

    /// Get number of samples processed
    pub fn sample_count(&self) -> u64 {
        self.samples
    }

    /// Reset the EMA calculator
    pub fn reset(&mut self) {
        self.value = 0.0;
        self.samples = 0;
    }
}

impl Default for PowerEma {
    fn default() -> Self {
        Self::default_smoothing()
    }
}

/// Power average tracker for multiple rails
#[derive(Debug, Clone, Default)]
pub struct PowerAverageTracker {
    /// EMA calculators for each rail
    rail_emas: HashMap<String, PowerEma>,
    /// EMA calculator for total power
    total_ema: PowerEma,
}

impl PowerAverageTracker {
    /// Create a new power average tracker
    pub fn new() -> Self {
        Self {
            rail_emas: HashMap::new(),
            total_ema: PowerEma::default_smoothing(),
        }
    }

    /// Create tracker with custom smoothing factor
    pub fn with_alpha(alpha: f64) -> Self {
        Self {
            rail_emas: HashMap::new(),
            total_ema: PowerEma::new(alpha),
        }
    }

    /// Update a rail's power reading and return the new average.
    pub fn update_rail(&mut self, name: &str, power: u32) -> u32 {
        self.rail_emas
            .entry(name.to_string())
            .or_insert_with(PowerEma::default_smoothing)
            .update(power)
    }

    /// Update total power and return the new average
    pub fn update_total(&mut self, power: u32) -> u32 {
        self.total_ema.update(power)
    }

    /// Get rail average without updating
    pub fn get_rail_average(&self, name: &str) -> Option<u32> {
        self.rail_emas.get(name).map(|ema| ema.average())
    }

    /// Get total average without updating
    pub fn get_total_average(&self) -> u32 {
        self.total_ema.average()
    }

    /// Update PowerStats with tracked averages
    /// Rails and totals with no reading this cycle are **skipped, not fed a
    /// zero**. An EMA is a weighted history, so one `0` for an unread rail
    /// does not just produce one wrong sample -- it pulls every subsequent
    /// average down and takes several cycles to decay back out. The rail keeps
    /// whatever average it had, which is the last thing actually observed.
    pub fn update_stats(&mut self, stats: &mut PowerStats) {
        // Update each rail's average
        for (name, rail) in stats.rails.iter_mut() {
            if let Some(power) = rail.power {
                rail.average = Some(self.update_rail(name, power));
            }
        }

        // Update total average
        if let Some(total) = stats.total.power {
            stats.total.average = Some(self.update_total(total));
        }
    }

    /// Reset all tracking
    pub fn reset(&mut self) {
        self.rail_emas.clear();
        self.total_ema.reset();
    }
}

/// Power statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerStats {
    /// Individual power rails
    pub rails: HashMap<String, PowerRail>,
    /// Total power
    pub total: TotalPower,
}

impl PowerStats {
    /// A zeroed struct for a platform reader to fill in.
    ///
    /// **This reads nothing.** It returns zero draw on every rail, and it was called `new()`
    /// until 6.0.0 — a name that reads like a constructor that gathers
    /// data. Two GUI defects, one in the HTTP server and one in the
    /// library's own `snapshot_power` came from exactly that misreading,
    /// found over three separate occasions.
    ///
    /// The real values come from the per-platform readers. Use those
    /// unless you are a reader building your own starting struct.
    pub fn empty() -> Self {
        Self {
            rails: HashMap::new(),
            // `None`, not `0`. This function's own documentation above
            // complains that it "returns zero draw on every rail" and that
            // three separate defects came from reading it as a constructor
            // that gathers data. The type can now say what the doc comment
            // had to.
            total: TotalPower {
                power: None,
                average: None,
            },
        }
    }

    /// Get power rail by name
    pub fn get_rail(&self, name: &str) -> Option<&PowerRail> {
        self.rails.get(name)
    }

    /// Get total power in watts, or `None` where no total was established.
    pub fn total_watts(&self) -> Option<f32> {
        Some(self.total.power? as f32 / 1000.0)
    }
}

impl Default for PowerStats {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === PowerEma tests ===

    #[test]
    fn test_ema_new_clamps_alpha() {
        let low = PowerEma::new(0.0);
        assert_eq!(low.alpha, 0.01);
        let high = PowerEma::new(2.0);
        assert_eq!(high.alpha, 1.0);
        let normal = PowerEma::new(0.5);
        assert!((normal.alpha - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_ema_default_smoothing() {
        let ema = PowerEma::default_smoothing();
        assert!((ema.alpha - 0.1).abs() < f64::EPSILON);
        assert_eq!(ema.samples, 0);
        assert_eq!(ema.average(), 0);
    }

    #[test]
    fn test_ema_first_sample_sets_value() {
        let mut ema = PowerEma::new(0.2);
        let result = ema.update(1000);
        assert_eq!(result, 1000);
        assert_eq!(ema.average(), 1000);
        assert_eq!(ema.sample_count(), 1);
    }

    #[test]
    fn test_ema_smoothing_convergence() {
        let mut ema = PowerEma::new(0.5);
        ema.update(100);
        // second sample: 0.5*200 + 0.5*100 = 150
        let r2 = ema.update(200);
        assert_eq!(r2, 150);
        // third: 0.5*200 + 0.5*150 = 175
        let r3 = ema.update(200);
        assert_eq!(r3, 175);
    }

    #[test]
    fn test_ema_constant_input_converges() {
        let mut ema = PowerEma::new(0.1);
        for _ in 0..100 {
            ema.update(500);
        }
        assert_eq!(ema.average(), 500);
    }

    #[test]
    fn test_ema_reset() {
        let mut ema = PowerEma::new(0.2);
        ema.update(100);
        ema.update(200);
        ema.reset();
        assert_eq!(ema.average(), 0);
        assert_eq!(ema.sample_count(), 0);
        // After reset, first sample should set value directly
        let r = ema.update(500);
        assert_eq!(r, 500);
    }

    #[test]
    fn test_ema_default_trait() {
        let ema = PowerEma::default();
        assert!((ema.alpha - 0.1).abs() < f64::EPSILON);
    }

    // === PowerAverageTracker tests ===

    #[test]
    fn test_tracker_new() {
        let tracker = PowerAverageTracker::new();
        assert_eq!(tracker.get_total_average(), 0);
    }

    #[test]
    fn test_tracker_update_rail() {
        let mut tracker = PowerAverageTracker::new();
        let avg = tracker.update_rail("VDD_CPU", 5000);
        assert_eq!(avg, 5000); // First sample
        assert_eq!(tracker.get_rail_average("VDD_CPU"), Some(5000));
    }

    #[test]
    fn test_tracker_unknown_rail() {
        let tracker = PowerAverageTracker::new();
        assert_eq!(tracker.get_rail_average("nonexistent"), None);
    }

    #[test]
    fn test_tracker_update_total() {
        let mut tracker = PowerAverageTracker::new();
        let avg = tracker.update_total(10000);
        assert_eq!(avg, 10000);
        assert_eq!(tracker.get_total_average(), 10000);
    }

    #[test]
    fn test_tracker_multiple_rails() {
        let mut tracker = PowerAverageTracker::new();
        tracker.update_rail("VDD_CPU", 5000);
        tracker.update_rail("VDD_GPU", 8000);
        assert_eq!(tracker.get_rail_average("VDD_CPU"), Some(5000));
        assert_eq!(tracker.get_rail_average("VDD_GPU"), Some(8000));
    }

    #[test]
    fn test_tracker_reset() {
        let mut tracker = PowerAverageTracker::new();
        tracker.update_rail("VDD_CPU", 5000);
        tracker.update_total(10000);
        tracker.reset();
        assert_eq!(tracker.get_rail_average("VDD_CPU"), None);
        assert_eq!(tracker.get_total_average(), 0);
    }

    #[test]
    fn test_tracker_with_alpha() {
        let mut tracker = PowerAverageTracker::with_alpha(0.5);
        tracker.update_total(100);
        let avg = tracker.update_total(200);
        assert_eq!(avg, 150); // 0.5*200 + 0.5*100
    }

    #[test]
    fn test_tracker_update_stats() {
        let mut tracker = PowerAverageTracker::new();
        let mut stats = PowerStats::default();
        stats.total.power = Some(15000);
        stats.rails.insert(
            "VDD_CPU".to_string(),
            PowerRail {
                online: true,
                sensor_type: "INA3221".to_string(),
                voltage: Some(5000),
                current: Some(1000),
                power: Some(5000),
                average: None,
                warn: None,
                crit: None,
            },
        );
        tracker.update_stats(&mut stats);
        assert_eq!(stats.total.average, Some(15000));
        assert_eq!(stats.rails["VDD_CPU"].average, Some(5000));
    }

    // === PowerStats tests ===

    #[test]
    fn test_power_stats_total_watts() {
        let mut stats = PowerStats::default();
        stats.total.power = Some(15500);
        assert!((stats.total_watts().expect("a total was set") - 15.5).abs() < 0.01);
    }

    #[test]
    fn test_power_stats_get_rail() {
        let mut stats = PowerStats::default();
        assert!(stats.get_rail("CPU").is_none());
        stats.rails.insert(
            "CPU".to_string(),
            PowerRail {
                online: true,
                sensor_type: "INA3221".to_string(),
                voltage: Some(5000),
                current: Some(1000),
                power: Some(5000),
                average: Some(5000),
                warn: Some(3000),
                crit: None,
            },
        );
        assert!(stats.get_rail("CPU").is_some());
    }

    /// An unreadable rail must not be averaged as a rail drawing nothing.
    ///
    /// The EMA is the reason this matters more than a single wrong sample: a
    /// `0` fed in for an unread cycle pulls every later average down and takes
    /// several cycles to decay back out, so one missed read distorts a window.
    #[test]
    fn an_unread_rail_is_not_averaged_as_zero() {
        let mut tracker = PowerAverageTracker::new();
        let mut stats = PowerStats::empty();
        stats.rails.insert(
            "VDD_CPU".to_string(),
            PowerRail {
                online: true,
                sensor_type: "INA3221".to_string(),
                voltage: None,
                current: None,
                power: None,
                average: None,
                warn: None,
                crit: None,
            },
        );

        tracker.update_stats(&mut stats);

        assert_eq!(
            stats.rails["VDD_CPU"].average, None,
            "a rail with no reading has no average, and must not seed one with 0"
        );
        assert_eq!(
            tracker.get_rail_average("VDD_CPU"),
            None,
            "nothing may enter the EMA's history for a cycle that read nothing"
        );
        assert_eq!(stats.total.power, None);
        assert_eq!(stats.total_watts(), None);
    }
}
