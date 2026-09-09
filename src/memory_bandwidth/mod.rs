//! Memory bandwidth monitoring and estimation.
//!
//! Monitors memory bus utilization via hardware performance counters
//! (Intel IMC / AMD Data Fabric) when available, and provides bandwidth
//! estimation from DIMM configuration parameters (speed, channels, width).
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/devices/uncore_imc_*/`, perf events, DIMM config inference
//! - **Windows**: Performance counters, DIMM config inference
//! - **macOS**: `sysctl`, DIMM config inference

use serde::{Deserialize, Serialize};

use crate::error::IronError;

/// Memory technology generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryGeneration {
    DDR3,
    DDR4,
    DDR5,
    LPDDR4,
    LPDDR4X,
    LPDDR5,
    LPDDR5X,
    HBM2,
    HBM2E,
    HBM3,
    HBM3E,
    GDDR6,
    GDDR6X,
    Unknown,
}

impl MemoryGeneration {
    /// Bus width in bits for this technology.
    pub fn bus_width_bits(&self) -> u32 {
        match self {
            Self::DDR3 | Self::DDR4 | Self::DDR5 => 64,
            Self::LPDDR4 | Self::LPDDR4X => 32, // per channel
            Self::LPDDR5 | Self::LPDDR5X => 32,
            Self::HBM2 => 128, // per channel, 8 channels per stack
            Self::HBM2E => 128,
            Self::HBM3 => 128,
            Self::HBM3E => 128,
            Self::GDDR6 => 32,
            Self::GDDR6X => 32,
            Self::Unknown => 64,
        }
    }

    /// Typical peak speed in MT/s for each generation.
    pub fn typical_speed_mts(&self) -> u32 {
        match self {
            Self::DDR3 => 1600,
            Self::DDR4 => 3200,
            Self::DDR5 => 5600,
            Self::LPDDR4 => 4267,
            Self::LPDDR4X => 4267,
            Self::LPDDR5 => 6400,
            Self::LPDDR5X => 8533,
            Self::HBM2 => 2000,
            Self::HBM2E => 3200,
            Self::HBM3 => 6400,
            Self::HBM3E => 9600,
            Self::GDDR6 => 16000,
            Self::GDDR6X => 21000,
            Self::Unknown => 3200,
        }
    }
}

impl std::fmt::Display for MemoryGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DDR3 => write!(f, "DDR3"),
            Self::DDR4 => write!(f, "DDR4"),
            Self::DDR5 => write!(f, "DDR5"),
            Self::LPDDR4 => write!(f, "LPDDR4"),
            Self::LPDDR4X => write!(f, "LPDDR4X"),
            Self::LPDDR5 => write!(f, "LPDDR5"),
            Self::LPDDR5X => write!(f, "LPDDR5X"),
            Self::HBM2 => write!(f, "HBM2"),
            Self::HBM2E => write!(f, "HBM2E"),
            Self::HBM3 => write!(f, "HBM3"),
            Self::HBM3E => write!(f, "HBM3E"),
            Self::GDDR6 => write!(f, "GDDR6"),
            Self::GDDR6X => write!(f, "GDDR6X"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Memory channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// Number of active channels.
    pub active_channels: u32,
    /// Maximum supported channels.
    pub max_channels: u32,
    /// Whether running in interleaved mode.
    pub interleaved: bool,
    /// Channel mode description.
    pub mode: String,
}

/// Bandwidth estimation from DIMM configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthEstimate {
    /// Memory generation.
    pub generation: MemoryGeneration,
    /// Speed in MT/s.
    pub speed_mts: u32,
    /// Channel configuration.
    pub channels: ChannelConfig,
    /// Theoretical peak bandwidth (GB/s).
    pub peak_bandwidth_gbs: f64,
    /// Estimated achievable bandwidth (GB/s) — typically 70-85% of peak.
    pub achievable_bandwidth_gbs: f64,
    /// Estimated read bandwidth (GB/s).
    pub estimated_read_gbs: f64,
    /// Estimated write bandwidth (GB/s).
    pub estimated_write_gbs: f64,
    /// STREAM Triad estimate (GB/s).
    pub stream_triad_estimate_gbs: f64,
}

/// Live bandwidth measurement (if hardware counters available).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthMeasurement {
    /// Measured read bandwidth (GB/s).
    pub read_gbs: f64,
    /// Measured write bandwidth (GB/s).
    pub write_gbs: f64,
    /// Total measured bandwidth (GB/s).
    pub total_gbs: f64,
    /// Measurement source.
    pub source: String,
    /// Measurement duration (microseconds).
    pub duration_us: u64,
    /// Bus utilization percentage.
    pub utilization_pct: f64,
}

/// Bandwidth analysis and recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthAnalysis {
    /// Bandwidth estimate (always available).
    pub estimate: BandwidthEstimate,
    /// Live measurement (if hardware counters available).
    pub measurement: Option<BandwidthMeasurement>,
    /// Latency estimate in nanoseconds.
    pub estimated_latency_ns: f64,
    /// Score (0-100) based on bandwidth per core, or `None` when the core count
    /// could not be read.
    ///
    /// Both this and `potential_bottleneck` are quotients over
    /// `available_parallelism()`, which used to fall back to `4.0`. Defined
    /// arithmetic is not the same as a defined result: a machine whose core
    /// count is unreadable got a score and a bottleneck verdict computed from an
    /// invented denominator, and neither carried any sign of it. Neither field is
    /// published to the ontology — scores are a judgement rather than a reading,
    /// the same call made for `single_thread_score` and `acceleration_score`.
    pub bandwidth_score: Option<u32>,
    /// Whether memory is likely a bottleneck, or `None` when the core count could
    /// not be read.
    pub potential_bottleneck: Option<bool>,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// Memory bandwidth monitor.
pub struct MemoryBandwidthMonitor {
    analysis: BandwidthAnalysis,
}

impl MemoryBandwidthMonitor {
    /// Create a new memory bandwidth monitor.
    pub fn new() -> Result<Self, IronError> {
        let analysis = Self::analyze()?;
        Ok(Self { analysis })
    }

    /// Refresh.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.analysis = Self::analyze()?;
        Ok(())
    }

    /// Get analysis.
    pub fn analysis(&self) -> &BandwidthAnalysis {
        &self.analysis
    }

    /// Get the bandwidth estimate.
    pub fn estimate(&self) -> &BandwidthEstimate {
        &self.analysis.estimate
    }

    fn analyze() -> Result<BandwidthAnalysis, IronError> {
        let (generation, speed_mts, channels) = Self::detect_memory_config()?;

        let peak = Self::compute_peak_bandwidth(speed_mts, channels.active_channels, &generation);

        // Achievable is typically 70-85% of peak depending on technology
        let efficiency = match generation {
            MemoryGeneration::DDR5 => 0.82,
            MemoryGeneration::DDR4 => 0.78,
            MemoryGeneration::DDR3 => 0.72,
            MemoryGeneration::LPDDR5 | MemoryGeneration::LPDDR5X => 0.80,
            MemoryGeneration::LPDDR4 | MemoryGeneration::LPDDR4X => 0.75,
            MemoryGeneration::HBM2 | MemoryGeneration::HBM2E => 0.90,
            MemoryGeneration::HBM3 | MemoryGeneration::HBM3E => 0.92,
            _ => 0.75,
        };

        let achievable = peak * efficiency;
        let read_ratio = 0.65; // Typical workloads are ~65% reads
        let stream_triad_ratio = 0.72; // STREAM Triad typically achieves ~72% of peak

        let estimate = BandwidthEstimate {
            generation,
            speed_mts,
            channels,
            peak_bandwidth_gbs: peak,
            achievable_bandwidth_gbs: achievable,
            estimated_read_gbs: achievable * read_ratio,
            estimated_write_gbs: achievable * (1.0 - read_ratio),
            stream_triad_estimate_gbs: peak * stream_triad_ratio,
        };

        // Try live measurement
        let measurement = Self::try_measure(&estimate);

        // Latency estimate
        let latency_ns = match generation {
            MemoryGeneration::DDR5 => 85.0, // CAS latency offset by higher speed
            MemoryGeneration::DDR4 => 70.0,
            MemoryGeneration::DDR3 => 55.0,
            MemoryGeneration::LPDDR5 | MemoryGeneration::LPDDR5X => 90.0,
            MemoryGeneration::LPDDR4 | MemoryGeneration::LPDDR4X => 80.0,
            MemoryGeneration::HBM2 | MemoryGeneration::HBM2E => 30.0,
            MemoryGeneration::HBM3 | MemoryGeneration::HBM3E => 25.0,
            _ => 70.0,
        };

        // Score: bandwidth per core. Absent when the core count is, because the
        // quotient is only meaningful over a denominator that was read.
        let bw_per_core = std::thread::available_parallelism()
            .ok()
            .map(|n| achievable / n.get() as f64);
        let score = bw_per_core.map(|b| (b * 10.0).min(100.0) as u32);

        // < 3 GB/s per core is concerning
        let potential_bottleneck = bw_per_core.map(|b| b < 3.0);

        let mut recommendations = Vec::new();

        if estimate.channels.active_channels < estimate.channels.max_channels {
            recommendations.push(format!(
                "Running {}-channel; {} channels available. Adding DIMMs could increase bandwidth {:.0}%",
                estimate.channels.active_channels,
                estimate.channels.max_channels,
                (estimate.channels.max_channels as f64 / estimate.channels.active_channels as f64 - 1.0) * 100.0
            ));
        }

        // Only advise on a per-core figure that was actually computed.
        if let (Some(true), Some(per_core)) = (potential_bottleneck, bw_per_core) {
            recommendations.push(format!(
                "Low bandwidth per core ({:.1} GB/s). Consider faster memory or more channels",
                per_core
            ));
        }

        if generation == MemoryGeneration::DDR4 {
            recommendations.push("DDR5 upgrade could provide ~40-75% more bandwidth".into());
        }

        Ok(BandwidthAnalysis {
            estimate,
            measurement,
            estimated_latency_ns: latency_ns,
            bandwidth_score: score,
            potential_bottleneck,
            recommendations,
        })
    }

    fn compute_peak_bandwidth(speed_mts: u32, channels: u32, gen: &MemoryGeneration) -> f64 {
        let bus_width = gen.bus_width_bits();
        // Peak BW = speed_mts × bus_width_bytes × channels / 1000 (to GB/s)
        let bus_width_bytes = bus_width as f64 / 8.0;
        speed_mts as f64 * bus_width_bytes * channels as f64 / 1000.0
    }

    #[cfg(target_os = "linux")]
    fn detect_memory_config() -> Result<(MemoryGeneration, u32, ChannelConfig), IronError> {
        // Try dmidecode first
        let output = std::process::Command::new("dmidecode")
            .args(["-t", "memory"])
            .output();

        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            return Self::parse_dmidecode(&text);
        }

        // Fallback: infer from CPU model
        Self::infer_from_cpu()
    }

    #[cfg(target_os = "linux")]
    fn parse_dmidecode(text: &str) -> Result<(MemoryGeneration, u32, ChannelConfig), IronError> {
        let mut generation = MemoryGeneration::Unknown;
        let mut max_speed = 0u32;
        let mut populated_count = 0u32;
        let mut total_slots = 0u32;

        let mut in_device = false;

        for line in text.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("Memory Device") {
                in_device = true;
                total_slots += 1;
                continue;
            }

            if !in_device {
                continue;
            }

            if trimmed.starts_with("Type:") {
                let val = trimmed.strip_prefix("Type:").unwrap_or("").trim();
                match val {
                    "DDR3" => generation = MemoryGeneration::DDR3,
                    "DDR4" => generation = MemoryGeneration::DDR4,
                    "DDR5" => generation = MemoryGeneration::DDR5,
                    "LPDDR4" => generation = MemoryGeneration::LPDDR4,
                    "LPDDR5" => generation = MemoryGeneration::LPDDR5,
                    _ => {}
                }
            }

            if trimmed.starts_with("Speed:") {
                if let Some(speed_str) = trimmed.strip_prefix("Speed:").and_then(|s| {
                    s.trim()
                        .strip_suffix("MT/s")
                        .or_else(|| s.trim().strip_suffix("MHz"))
                }) {
                    if let Ok(speed) = speed_str.trim().parse::<u32>() {
                        if speed > max_speed {
                            max_speed = speed;
                        }
                        if speed > 0 {
                            populated_count += 1;
                        }
                    }
                }
            }

            if trimmed.is_empty() {
                in_device = false;
            }
        }

        if max_speed == 0 {
            max_speed = generation.typical_speed_mts();
        }

        let (active_channels, max_channels) =
            Self::infer_channels(populated_count, total_slots, &generation);

        Ok((
            generation,
            max_speed,
            ChannelConfig {
                active_channels,
                max_channels,
                interleaved: active_channels > 1 && populated_count >= active_channels,
                mode: format!("{}-channel", active_channels),
            },
        ))
    }

    #[cfg(target_os = "windows")]
    fn detect_memory_config() -> Result<(MemoryGeneration, u32, ChannelConfig), IronError> {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_PhysicalMemory | Select-Object Speed, SMBIOSMemoryType, DeviceLocator | ConvertTo-Json",
            ])
            .output();

        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                let items = if val.is_array() {
                    val.as_array().cloned().unwrap_or_default()
                } else {
                    vec![val]
                };

                let populated = items.len() as u32;
                let mut max_speed = 0u32;
                let mut gen = MemoryGeneration::Unknown;

                for item in &items {
                    if let Some(speed) = item["Speed"].as_u64() {
                        max_speed = max_speed.max(speed as u32);
                    }
                    if let Some(smbios_type) = item["SMBIOSMemoryType"].as_u64() {
                        gen = match smbios_type {
                            24 => MemoryGeneration::DDR3,
                            26 => MemoryGeneration::DDR4,
                            34 => MemoryGeneration::DDR5,
                            _ => MemoryGeneration::Unknown,
                        };
                    }
                }

                if max_speed == 0 {
                    max_speed = gen.typical_speed_mts();
                }

                let (active, max) = Self::infer_channels(populated, populated, &gen);

                return Ok((
                    gen,
                    max_speed,
                    ChannelConfig {
                        active_channels: active,
                        max_channels: max,
                        interleaved: active > 1,
                        mode: format!("{}-channel", active),
                    },
                ));
            }
        }

        Self::infer_from_cpu()
    }

    #[cfg(target_os = "macos")]
    fn detect_memory_config() -> Result<(MemoryGeneration, u32, ChannelConfig), IronError> {
        // Apple Silicon uses unified LPDDR
        let output = std::process::Command::new("sysctl")
            .arg("-n")
            .arg("hw.memsize")
            .output();

        let _total_bytes = output
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .unwrap_or(0);

        // Detect Apple Silicon
        let brand = std::process::Command::new("sysctl")
            .arg("-n")
            .arg("machdep.cpu.brand_string")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

        if brand.contains("Apple") {
            let (gen, speed, channels) = if brand.contains("M4") {
                (MemoryGeneration::LPDDR5X, 8533, 8u32)
            } else if brand.contains("M3") {
                (MemoryGeneration::LPDDR5, 6400, 8)
            } else if brand.contains("M2") {
                (MemoryGeneration::LPDDR5, 6400, 8)
            } else {
                (MemoryGeneration::LPDDR5, 6400, 4)
            };

            return Ok((
                gen,
                speed,
                ChannelConfig {
                    active_channels: channels,
                    max_channels: channels,
                    interleaved: true,
                    mode: format!("{}-channel unified", channels),
                },
            ));
        }

        Self::infer_from_cpu()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn detect_memory_config() -> Result<(MemoryGeneration, u32, ChannelConfig), IronError> {
        Self::infer_from_cpu()
    }

    fn infer_from_cpu() -> Result<(MemoryGeneration, u32, ChannelConfig), IronError> {
        // Baseline fallback
        Ok((
            MemoryGeneration::DDR4,
            3200,
            ChannelConfig {
                active_channels: 2,
                max_channels: 2,
                interleaved: true,
                mode: "dual-channel (inferred)".into(),
            },
        ))
    }

    fn infer_channels(populated: u32, total_slots: u32, _gen: &MemoryGeneration) -> (u32, u32) {
        // Heuristic: 2 DIMMs per channel, max 4 channels for desktop, 8 for server
        let max = if total_slots >= 8 {
            (total_slots / 2).min(8)
        } else if total_slots >= 4 {
            (total_slots / 2).min(4)
        } else {
            total_slots.max(1)
        };

        let active = if populated > 0 { populated.min(max) } else { 1 };

        (active, max)
    }

    fn try_measure(estimate: &BandwidthEstimate) -> Option<BandwidthMeasurement> {
        #[cfg(target_os = "linux")]
        {
            // Try reading IMC counters
            Self::read_imc_counters(estimate)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = estimate;
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn read_imc_counters(estimate: &BandwidthEstimate) -> Option<BandwidthMeasurement> {
        // Check for uncore IMC perf events
        let imc_path = std::path::Path::new("/sys/devices");
        if !imc_path.exists() {
            return None;
        }

        // Look for uncore_imc_* directories
        let entries = std::fs::read_dir(imc_path).ok()?;
        let has_imc = entries
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with("uncore_imc"));

        if has_imc {
            // IMC counters found but would need perf_event_open() to read
            // Return a stub indicating capability exists
            Some(BandwidthMeasurement {
                read_gbs: 0.0,
                write_gbs: 0.0,
                total_gbs: 0.0,
                source: "uncore_imc (counters detected, perf_event needed)".into(),
                duration_us: 0,
                utilization_pct: 0.0,
            })
        } else {
            // No hardware counters, just use estimate
            let _ = estimate;
            None
        }
    }
}

impl Default for MemoryBandwidthMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            analysis: BandwidthAnalysis {
                estimate: BandwidthEstimate {
                    generation: MemoryGeneration::Unknown,
                    speed_mts: 0,
                    channels: ChannelConfig {
                        active_channels: 0,
                        max_channels: 0,
                        interleaved: false,
                        mode: String::new(),
                    },
                    peak_bandwidth_gbs: 0.0,
                    achievable_bandwidth_gbs: 0.0,
                    estimated_read_gbs: 0.0,
                    estimated_write_gbs: 0.0,
                    stream_triad_estimate_gbs: 0.0,
                },
                measurement: None,
                estimated_latency_ns: 0.0,
                bandwidth_score: None,
                potential_bottleneck: None,
                recommendations: Vec::new(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peak_bandwidth_ddr4() {
        // DDR4-3200 dual channel: 3200 × 8 × 2 / 1000 = 51.2 GB/s
        let bw = MemoryBandwidthMonitor::compute_peak_bandwidth(3200, 2, &MemoryGeneration::DDR4);
        assert!((bw - 51.2).abs() < 0.1);
    }

    #[test]
    fn test_peak_bandwidth_ddr5() {
        // DDR5-5600 dual channel: 5600 × 8 × 2 / 1000 = 89.6 GB/s
        let bw = MemoryBandwidthMonitor::compute_peak_bandwidth(5600, 2, &MemoryGeneration::DDR5);
        assert!((bw - 89.6).abs() < 0.1);
    }

    #[test]
    fn test_generation_display() {
        assert_eq!(MemoryGeneration::DDR5.to_string(), "DDR5");
        assert_eq!(MemoryGeneration::LPDDR5X.to_string(), "LPDDR5X");
        assert_eq!(MemoryGeneration::HBM3E.to_string(), "HBM3E");
    }

    #[test]
    fn test_channel_inference() {
        // 2 populated out of 4 slots
        let (active, max) = MemoryBandwidthMonitor::infer_channels(2, 4, &MemoryGeneration::DDR4);
        assert_eq!(active, 2);
        assert_eq!(max, 2);

        // Server: 8 populated out of 16 slots
        let (active, max) = MemoryBandwidthMonitor::infer_channels(8, 16, &MemoryGeneration::DDR5);
        assert_eq!(active, 8);
        assert_eq!(max, 8);
    }

    #[test]
    fn test_monitor_default() {
        let monitor = MemoryBandwidthMonitor::default();
        let _analysis = monitor.analysis();
    }

    #[test]
    fn test_serialization() {
        let estimate = BandwidthEstimate {
            generation: MemoryGeneration::DDR5,
            speed_mts: 5600,
            channels: ChannelConfig {
                active_channels: 2,
                max_channels: 4,
                interleaved: true,
                mode: "dual-channel".into(),
            },
            peak_bandwidth_gbs: 89.6,
            achievable_bandwidth_gbs: 73.5,
            estimated_read_gbs: 47.8,
            estimated_write_gbs: 25.7,
            stream_triad_estimate_gbs: 64.5,
        };
        let json = serde_json::to_string(&estimate).unwrap();
        assert!(json.contains("DDR5"));
        let _: BandwidthEstimate = serde_json::from_str(&json).unwrap();
    }
}

#[cfg(test)]
mod per_core_absence_tests {
    use super::*;

    /// The per-core figures are a quotient, and a quotient over an unread
    /// denominator is not a reading. `available_parallelism()` answers on every
    /// platform this runs on, so the absent arm cannot be reached from a test —
    /// what is asserted here is the invariant that ties the three fields
    /// together, which holds whichever arm was taken.
    #[test]
    fn the_score_and_the_bottleneck_verdict_agree_on_whether_they_were_computed() {
        let analysis = MemoryBandwidthMonitor::new()
            .map(|m| m.analysis().clone())
            .unwrap_or_else(|_| return_empty());

        assert_eq!(
            analysis.bandwidth_score.is_some(),
            analysis.potential_bottleneck.is_some(),
            "both derive from the same core count, so one cannot be present \
             while the other is absent"
        );

        if let Some(score) = analysis.bandwidth_score {
            assert!(score <= 100, "the score is a 0-100 scale, got {score}");
        }
    }

    /// A bottleneck recommendation quotes a per-core figure, so it may only
    /// appear when that figure exists.
    #[test]
    fn no_per_core_advice_is_given_without_a_per_core_figure() {
        let analysis = MemoryBandwidthMonitor::new()
            .map(|m| m.analysis().clone())
            .unwrap_or_else(|_| return_empty());

        if analysis.potential_bottleneck.is_none() {
            assert!(
                !analysis
                    .recommendations
                    .iter()
                    .any(|r| r.contains("bandwidth per core")),
                "advised on a per-core figure that was never computed"
            );
        }
    }

    fn return_empty() -> BandwidthAnalysis {
        MemoryBandwidthMonitor::default().analysis().clone()
    }
}
