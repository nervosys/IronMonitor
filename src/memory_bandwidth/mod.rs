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
//! - **macOS**: none. The memory configuration is not exposed, so
//!   [`MemoryBandwidthMonitor::new`] returns an error with the reason.

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
    /// Bus width in bits for this technology, or `None` for `Unknown`, which
    /// has no technology to take a width from.
    pub fn bus_width_bits(&self) -> Option<u32> {
        match self {
            Self::DDR3 | Self::DDR4 | Self::DDR5 => Some(64),
            Self::LPDDR4 | Self::LPDDR4X => Some(32), // per channel
            Self::LPDDR5 | Self::LPDDR5X => Some(32),
            Self::HBM2 => Some(128), // per channel, 8 channels per stack
            Self::HBM2E => Some(128),
            Self::HBM3 => Some(128),
            Self::HBM3E => Some(128),
            Self::GDDR6 => Some(32),
            Self::GDDR6X => Some(32),
            Self::Unknown => None,
        }
    }

    /// Typical peak speed in MT/s for each generation, or `None` for `Unknown`.
    /// A typical figure is a table entry, not a reading of this machine.
    pub fn typical_speed_mts(&self) -> Option<u32> {
        match self {
            Self::DDR3 => Some(1600),
            Self::DDR4 => Some(3200),
            Self::DDR5 => Some(5600),
            Self::LPDDR4 => Some(4267),
            Self::LPDDR4X => Some(4267),
            Self::LPDDR5 => Some(6400),
            Self::LPDDR5X => Some(8533),
            Self::HBM2 => Some(2000),
            Self::HBM2E => Some(3200),
            Self::HBM3 => Some(6400),
            Self::HBM3E => Some(9600),
            Self::GDDR6 => Some(16000),
            Self::GDDR6X => Some(21000),
            Self::Unknown => None,
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
///
/// Every field is `None` when the platform did not supply the evidence for it.
/// The active count is read from DIMM locator names (`ChannelA-DIMM0`, `DIMM_B1`
/// and the like). The maximum is not read on any platform: the installed module
/// count used to stand in for it, which assumed one module per channel slot and
/// published that assumption as the board's limit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// Number of active channels, or `None` where no locator names one.
    pub active_channels: Option<u32>,
    /// Maximum supported channels, or `None`: no platform reader establishes it.
    pub max_channels: Option<u32>,
    /// Whether running in interleaved mode, or `None` without an active count.
    pub interleaved: Option<bool>,
    /// Channel mode description, or `None` without an active count.
    pub mode: Option<String>,
}

/// Bandwidth estimation from DIMM configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthEstimate {
    /// Memory generation.
    pub generation: MemoryGeneration,
    /// Speed in MT/s, or `None` where no DIMM speed was read.
    ///
    /// **This was filled from `MemoryGeneration::typical_speed_mts()` whenever
    /// the real speed was missing** -- a lookup table keyed on the generation,
    /// published by the ontology as a `specification` of *this* machine's
    /// memory. A DDR5 system whose SMBIOS speed field was unreadable reported
    /// 5600 MT/s because that is what DDR5 typically runs at. That is the exact
    /// defect class `AGENTS.md` cites: a plausible constant arriving through
    /// the same field as a reading.
    pub speed_mts: Option<u32>,
    /// Channel configuration.
    pub channels: ChannelConfig,
    /// Theoretical peak bandwidth (GB/s), or `None` without a speed to compute it from.
    pub peak_bandwidth_gbs: Option<f64>,
    /// Estimated achievable bandwidth (GB/s). Always `None`: it used to be the
    /// peak times a fixed per-generation efficiency factor, which is not a
    /// reading of this machine.
    pub achievable_bandwidth_gbs: Option<f64>,
    /// Estimated read bandwidth (GB/s). Always `None`: it was derived from the
    /// achievable figure, which is withheld (see `achievable_bandwidth_gbs`).
    pub estimated_read_gbs: Option<f64>,
    /// Estimated write bandwidth (GB/s). Always `None`, for the same reason.
    pub estimated_write_gbs: Option<f64>,
    /// STREAM Triad estimate (GB/s). Always `None`: it used to be the peak
    /// times a fixed ratio, and no benchmark was run to produce it.
    pub stream_triad_estimate_gbs: Option<f64>,
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
    /// Typical latency in nanoseconds for this memory generation, from a
    /// per-generation table in this module. It is not a measurement: the live
    /// reading, when the hardware counters allow one, is `measurement`. `None`
    /// for a generation the table does not list.
    pub estimated_latency_ns: Option<f64>,
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

    /// The estimate built from what was read. Only the peak is computed. The
    /// achievable, read/write and STREAM Triad figures used to be the peak
    /// multiplied by a per-generation efficiency factor (DDR5 0.82, DDR4 0.78,
    /// HBM3 0.92, 0.75 otherwise), a read ratio of 0.65 and a Triad ratio of 0.72.
    /// None of those constants is read from this machine or published as a
    /// specification, so each was a rule of thumb presented as a figure for this
    /// hardware. They are withheld; the ontology publishes them as unavailable
    /// with that reason.
    fn bandwidth_estimate(
        generation: MemoryGeneration,
        speed_mts: Option<u32>,
        channels: ChannelConfig,
    ) -> BandwidthEstimate {
        // The peak needs the speed and the active channel count. Without either
        // there is nothing to compute from, and a typical speed or a guessed
        // channel count would make it an estimate of a different machine.
        let peak = match (speed_mts, channels.active_channels) {
            (Some(speed), Some(active)) => Self::compute_peak_bandwidth(speed, active, &generation),
            _ => None,
        };

        BandwidthEstimate {
            generation,
            speed_mts,
            channels,
            peak_bandwidth_gbs: peak,
            achievable_bandwidth_gbs: None,
            estimated_read_gbs: None,
            estimated_write_gbs: None,
            stream_triad_estimate_gbs: None,
        }
    }

    fn analyze() -> Result<BandwidthAnalysis, IronError> {
        let (generation, speed_mts, channels) = Self::detect_memory_config()?;

        let estimate = Self::bandwidth_estimate(generation, speed_mts, channels);

        // Try live measurement
        let measurement = Self::try_measure(&estimate);

        // Latency estimate
        // Typical latency for the generation, not a measurement. Generations the
        // table does not list (GDDR, Unknown) have none, rather than the 70 ns
        // that used to stand in for every one of them.
        let latency_ns = match generation {
            MemoryGeneration::DDR5 => Some(85.0), // CAS latency offset by higher speed
            MemoryGeneration::DDR4 => Some(70.0),
            MemoryGeneration::DDR3 => Some(55.0),
            MemoryGeneration::LPDDR5 | MemoryGeneration::LPDDR5X => Some(90.0),
            MemoryGeneration::LPDDR4 | MemoryGeneration::LPDDR4X => Some(80.0),
            MemoryGeneration::HBM2 | MemoryGeneration::HBM2E => Some(30.0),
            MemoryGeneration::HBM3 | MemoryGeneration::HBM3E => Some(25.0),
            _ => None,
        };

        // Score: bandwidth per core. Absent when either side of the quotient
        // is: the core count, or -- now that it can be missing -- the
        // achievable bandwidth. A per-core figure from a typical speed would
        // put a bottleneck recommendation in front of someone on the strength
        // of a lookup table.
        // Achievable bandwidth is withheld above, so there is no numerator and
        // the per-core figure is absent on every platform.
        let bw_per_core: Option<f64> = None;
        let score = bw_per_core.map(|b| (b * 10.0).min(100.0) as u32);

        // < 3 GB/s per core is concerning
        let potential_bottleneck = bw_per_core.map(|b| b < 3.0);

        let mut recommendations = Vec::new();

        // Advise only when both counts were read; no platform reader sets the
        // maximum today, so this stays silent rather than comparing to a guess.
        if let (Some(active), Some(max)) = (
            estimate.channels.active_channels,
            estimate.channels.max_channels,
        ) {
            if active < max {
                recommendations.push(format!(
                    "Running {}-channel; {} channels available. Adding DIMMs could increase bandwidth {:.0}%",
                    active,
                    max,
                    (max as f64 / active as f64 - 1.0) * 100.0
                ));
            }
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

    // `None` for an Unknown generation: the bus width is not known, so a peak
    // computed from a 64-bit assumption would describe no measured machine.
    fn compute_peak_bandwidth(
        speed_mts: u32,
        channels: u32,
        gen: &MemoryGeneration,
    ) -> Option<f64> {
        let bus_width = gen.bus_width_bits()?;
        // Peak BW = speed_mts × bus_width_bytes × channels / 1000 (to GB/s)
        let bus_width_bytes = bus_width as f64 / 8.0;
        Some(speed_mts as f64 * bus_width_bytes * channels as f64 / 1000.0)
    }

    #[cfg(target_os = "linux")]
    fn detect_memory_config() -> Result<(MemoryGeneration, Option<u32>, ChannelConfig), IronError> {
        // Try dmidecode first
        let output = std::process::Command::new("dmidecode")
            .args(["-t", "memory"])
            .output();

        // A run that exited non-zero (dmidecode needs root for /dev/mem) can
        // still write something to stdout; that text is not a memory table.
        if let Ok(out) = output {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                return Self::parse_dmidecode(&text);
            }
        }

        // Nothing read.
        Self::memory_config_unreadable()
    }

    #[cfg(target_os = "linux")]
    fn parse_dmidecode(
        text: &str,
    ) -> Result<(MemoryGeneration, Option<u32>, ChannelConfig), IronError> {
        let mut generation = MemoryGeneration::Unknown;
        let mut max_speed = 0u32;
        let mut total_slots = 0u32;

        // Locators of the modules that reported a speed. Channels come from
        // these names; the module count alone does not say how they are wired.
        let mut populated_locators: Vec<String> = Vec::new();
        let mut in_device = false;
        let mut locator = String::new();
        let mut device_populated = false;

        for line in text.lines() {
            let trimmed = line.trim();

            // A blank line ends a record, and so does the next `Memory Device`
            // header. Only the exact header counts: `Memory Device Mapped
            // Addresses` is a different SMBIOS record, not a slot.
            if trimmed.is_empty() || trimmed == "Memory Device" {
                if in_device && device_populated {
                    populated_locators.push(std::mem::take(&mut locator));
                }
                locator.clear();
                device_populated = false;
                in_device = trimmed == "Memory Device";
                if in_device {
                    total_slots += 1;
                }
                continue;
            }

            if !in_device {
                continue;
            }

            if let Some(val) = trimmed.strip_prefix("Locator:") {
                locator = val.trim().to_string();
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
                            device_populated = true;
                        }
                    }
                }
            }
        }
        // The last record has no trailing blank line when the output ends on it.
        if in_device && device_populated {
            populated_locators.push(locator);
        }

        // No `Memory Device` record at all means the table was not read, however
        // cleanly dmidecode exited. Reporting one channel for it was the defect.
        if total_slots == 0 {
            return Self::memory_config_unreadable();
        }

        // No substitution. `max_speed == 0` means no DIMM reported a speed,
        // and a typical speed for the generation is not this machine's.
        let speed = (max_speed > 0).then_some(max_speed);

        let locators: Vec<&str> = populated_locators.iter().map(String::as_str).collect();
        Ok((
            generation,
            speed,
            Self::channel_config(Self::channels_from_locators(&locators)),
        ))
    }

    /// The channel configuration for an active count that was read, or none.
    #[cfg_attr(not(any(target_os = "linux", target_os = "windows")), allow(dead_code))]
    fn channel_config(active: Option<u32>) -> ChannelConfig {
        ChannelConfig {
            active_channels: active,
            max_channels: None,
            interleaved: active.map(|a| a > 1),
            mode: active.map(|a| format!("{a}-channel")),
        }
    }

    #[cfg(target_os = "windows")]
    fn detect_memory_config() -> Result<(MemoryGeneration, Option<u32>, ChannelConfig), IronError> {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_PhysicalMemory | Select-Object Speed, SMBIOSMemoryType, DeviceLocator | ConvertTo-Json",
            ])
            .output();

        if let Some(out) = output.as_ref().ok().filter(|out| out.status.success()) {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                let items = if val.is_array() {
                    val.as_array().cloned().unwrap_or_default()
                } else {
                    vec![val]
                };
                if items.is_empty() {
                    return Self::memory_config_unreadable();
                }

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

                // No substitution; see the Linux reader.
                let speed = (max_speed > 0).then_some(max_speed);

                // Channels from the DeviceLocator names, as on Linux. The
                // installed module count was used here before, which read every
                // module as its own channel.
                let locators: Vec<&str> = items
                    .iter()
                    .map(|item| item["DeviceLocator"].as_str().unwrap_or(""))
                    .collect();

                return Ok((
                    gen,
                    speed,
                    Self::channel_config(Self::channels_from_locators(&locators)),
                ));
            }
        }

        Self::memory_config_unreadable()
    }

    #[cfg(target_os = "macos")]
    fn detect_memory_config() -> Result<(MemoryGeneration, Option<u32>, ChannelConfig), IronError> {
        // This matched the CPU brand string -- `M4` to LPDDR5X-8533 on eight
        // channels, `M3` and `M2` to LPDDR5-6400 on eight, anything else to
        // LPDDR5-6400 on four -- and the resolver published the result as a
        // `specification` of this machine. It read no memory. It was also
        // wrong where it could be checked: `M3` matched the M3, M3 Pro and M3
        // Max, whose memory buses differ, and the last arm gave the original M1
        // a memory generation it did not ship with.
        //
        // It is not rebuilt as a per-chip table because Apple does not publish
        // what one would need: its specifications give a bandwidth per chip,
        // not a generation, transfer rate and channel count. Reconstructing
        // those three from recall is the defect this replaced, with better
        // numbers. Declining leaves `memory.bandwidth.<none>` saying why.
        Err(IronError::FeatureNotAvailable(
            concat!(
                "macOS exposes no memory generation, transfer rate or channel ",
                "count to read; Apple publishes a bandwidth per chip, but not ",
                "the configuration an estimate is built from"
            )
            .into(),
        ))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn detect_memory_config() -> Result<(MemoryGeneration, Option<u32>, ChannelConfig), IronError> {
        Self::memory_config_unreadable()
    }

    /// The memory configuration could not be read on this path.
    ///
    /// **This was `infer_from_cpu`, and it inferred nothing.** It returned
    /// `DDR4`, 3200 MT/s and a dual-channel layout as constants whenever SMBIOS
    /// could not be read -- and because it said `DDR4` rather than `Unknown`,
    /// it defeated the resolver guard written specifically to withhold figures
    /// built on an unidentified generation. A machine whose memory was not read
    /// at all published DDR4-3200 dual-channel as a specification of itself.
    ///
    /// An error is the honest answer, and the resolver already has the right
    /// path for one: it reports the configuration as unreadable and publishes
    /// nothing built on it.
    ///
    /// Not compiled on macOS, which declines with its own reason: there is no
    /// SMBIOS path there to have failed.
    #[cfg(not(target_os = "macos"))]
    fn memory_config_unreadable(
    ) -> Result<(MemoryGeneration, Option<u32>, ChannelConfig), IronError> {
        Err(IronError::FeatureNotAvailable(
            "the memory configuration (generation, speed, channels) could not be read from SMBIOS"
                .into(),
        ))
    }

    /// Active channel count from the DIMM locator names of the populated modules.
    ///
    /// This is the rule `memory_topology` applies (`infer_channels` there), kept
    /// in step with it by hand because that function takes its own record type.
    /// A locator naming a channel letter (`ChannelA-DIMM0`, `Channel B`, `DIMM_B1`,
    /// `BANK 0_A0`) contributes that letter, and the distinct letters are the
    /// channels. With no such locator, a lone module occupies one channel and
    /// more than one module does not say how it is wired, so `None`.
    ///
    /// The heuristic this replaced was "two modules per channel, capped at four
    /// or eight", which gave a plausible count for any module count.
    #[cfg_attr(not(any(target_os = "linux", target_os = "windows")), allow(dead_code))]
    fn channels_from_locators(locators: &[&str]) -> Option<u32> {
        if locators.is_empty() {
            return None;
        }

        let mut channels = std::collections::BTreeSet::new();
        for locator in locators {
            let loc = locator.to_uppercase();
            if loc.contains("CHANNEL") || loc.contains("DIMM_") {
                for c in ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H'] {
                    if loc.contains(&format!("CHANNEL{c}"))
                        || loc.contains(&format!("CHANNEL {c}"))
                        || loc.contains(&format!("DIMM_{c}"))
                        || loc.contains(&format!("_{c}0"))
                        || loc.contains(&format!("_{c}1"))
                    {
                        channels.insert(c);
                    }
                }
            }
        }

        if channels.is_empty() {
            (locators.len() == 1).then_some(1)
        } else {
            Some(channels.len() as u32)
        }
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

    /// No live measurement. The uncore IMC PMUs under `/sys/devices` are only
    /// readable through `perf_event_open()`, which this module does not call.
    /// The stub this returned whenever an `uncore_imc*` directory existed
    /// carried 0.0 GB/s for read, write and total, with a zero duration and a
    /// zero utilisation: a measurement of nothing, published as one. Until the
    /// counters are read, there is no reading to return.
    #[cfg(target_os = "linux")]
    fn read_imc_counters(_estimate: &BandwidthEstimate) -> Option<BandwidthMeasurement> {
        None
    }
}

impl Default for MemoryBandwidthMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            analysis: BandwidthAnalysis {
                estimate: BandwidthEstimate {
                    generation: MemoryGeneration::Unknown,
                    speed_mts: None,
                    channels: ChannelConfig {
                        active_channels: None,
                        max_channels: None,
                        interleaved: None,
                        mode: None,
                    },
                    peak_bandwidth_gbs: None,
                    achievable_bandwidth_gbs: None,
                    estimated_read_gbs: None,
                    estimated_write_gbs: None,
                    stream_triad_estimate_gbs: None,
                },
                measurement: None,
                estimated_latency_ns: None,
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

    /// macOS exposes no memory configuration, so there is nothing to estimate
    /// from. This used to succeed with a configuration looked up from the CPU
    /// brand string, which the resolver then published as a specification.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_declines_rather_than_looking_up_a_configuration() {
        match MemoryBandwidthMonitor::new() {
            Err(IronError::FeatureNotAvailable(why)) => {
                assert!(why.contains("macOS"), "{why}")
            }
            Err(e) => panic!("expected FeatureNotAvailable, got {e}"),
            Ok(m) => panic!(
                "macOS produced a memory configuration it cannot read: {:?}",
                m.analysis()
            ),
        }
    }

    #[test]
    fn test_peak_bandwidth_ddr4() {
        // DDR4-3200 dual channel: 3200 × 8 × 2 / 1000 = 51.2 GB/s
        let bw = MemoryBandwidthMonitor::compute_peak_bandwidth(3200, 2, &MemoryGeneration::DDR4);
        assert!(bw.is_some_and(|bw| (bw - 51.2).abs() < 0.1));
    }

    #[test]
    fn test_peak_bandwidth_ddr5() {
        // DDR5-5600 dual channel: 5600 × 8 × 2 / 1000 = 89.6 GB/s
        let bw = MemoryBandwidthMonitor::compute_peak_bandwidth(5600, 2, &MemoryGeneration::DDR5);
        assert!(bw.is_some_and(|bw| (bw - 89.6).abs() < 0.1));
    }

    #[test]
    fn only_the_peak_is_computed_and_the_rest_is_withheld() {
        let est = MemoryBandwidthMonitor::bandwidth_estimate(
            MemoryGeneration::DDR5,
            Some(5600),
            MemoryBandwidthMonitor::channel_config(Some(2)),
        );
        assert!(est
            .peak_bandwidth_gbs
            .is_some_and(|p| (p - 89.6).abs() < 0.1));
        assert_eq!(est.achievable_bandwidth_gbs, None);
        assert_eq!(est.estimated_read_gbs, None);
        assert_eq!(est.estimated_write_gbs, None);
        assert_eq!(est.stream_triad_estimate_gbs, None);
    }

    #[test]
    fn no_speed_means_no_peak_and_no_derived_figures() {
        let est = MemoryBandwidthMonitor::bandwidth_estimate(
            MemoryGeneration::DDR5,
            None,
            MemoryBandwidthMonitor::channel_config(Some(2)),
        );
        assert_eq!(est.peak_bandwidth_gbs, None);
        assert_eq!(est.achievable_bandwidth_gbs, None);
    }

    #[test]
    fn unknown_generation_has_no_peak_bandwidth() {
        let bw =
            MemoryBandwidthMonitor::compute_peak_bandwidth(3200, 2, &MemoryGeneration::Unknown);
        assert_eq!(bw, None);
    }

    #[test]
    fn test_generation_display() {
        assert_eq!(MemoryGeneration::DDR5.to_string(), "DDR5");
        assert_eq!(MemoryGeneration::LPDDR5X.to_string(), "LPDDR5X");
        assert_eq!(MemoryGeneration::HBM3E.to_string(), "HBM3E");
    }

    #[test]
    fn channels_come_from_locator_names_not_module_counts() {
        // Two channels named by their locators.
        assert_eq!(
            MemoryBandwidthMonitor::channels_from_locators(&["ChannelA-DIMM0", "ChannelB-DIMM0"]),
            Some(2)
        );
        // Two modules on the same channel are one channel, not two.
        assert_eq!(
            MemoryBandwidthMonitor::channels_from_locators(&["DIMM_A1", "DIMM_A2"]),
            Some(1)
        );
        // Eight modules, four channels: the count of modules does not decide it.
        assert_eq!(
            MemoryBandwidthMonitor::channels_from_locators(&[
                "DIMM_A1", "DIMM_A2", "DIMM_B1", "DIMM_B2", "DIMM_C1", "DIMM_C2", "DIMM_D1",
                "DIMM_D2",
            ]),
            Some(4)
        );
    }

    #[test]
    fn channels_are_unknown_without_locators_naming_them() {
        // Several modules with no channel in their names: not a guessed count.
        assert_eq!(
            MemoryBandwidthMonitor::channels_from_locators(&["DIMM 0", "DIMM 1"]),
            None
        );
        // A lone module occupies one channel.
        assert_eq!(
            MemoryBandwidthMonitor::channels_from_locators(&["DIMM 0"]),
            Some(1)
        );
        assert_eq!(MemoryBandwidthMonitor::channels_from_locators(&[]), None);
    }

    /// A captured `dmidecode -t memory` run, trimmed to the fields read here.
    /// Two populated DDR4 modules on channels A and B, and one empty slot.
    #[cfg(target_os = "linux")]
    const DMIDECODE_TWO_CHANNELS: &str = "\
# dmidecode 3.5
Getting SMBIOS data from sysfs.

Handle 0x0010, DMI type 16, 23 bytes
Physical Memory Array
	Location: System Board Or Motherboard

Handle 0x0011, DMI type 17, 84 bytes
Memory Device
	Array Handle: 0x0010
	Locator: ChannelA-DIMM0
	Bank Locator: BANK 0
	Type: DDR4
	Size: 16 GB
	Speed: 3200 MT/s
	Configured Memory Speed: 3200 MT/s

Handle 0x0012, DMI type 17, 84 bytes
Memory Device
	Array Handle: 0x0010
	Locator: ChannelB-DIMM0
	Bank Locator: BANK 2
	Type: DDR4
	Size: 16 GB
	Speed: 3200 MT/s

Handle 0x0013, DMI type 17, 84 bytes
Memory Device
	Array Handle: 0x0010
	Locator: ChannelA-DIMM1
	Bank Locator: BANK 1
	Type: Unknown
	Size: No Module Installed
	Speed: Unknown
";

    #[cfg(target_os = "linux")]
    #[test]
    fn dmidecode_reads_generation_speed_and_channels_from_locators() {
        let (generation, speed, channels) =
            MemoryBandwidthMonitor::parse_dmidecode(DMIDECODE_TWO_CHANNELS).unwrap();
        assert_eq!(generation, MemoryGeneration::DDR4);
        assert_eq!(speed, Some(3200));
        assert_eq!(channels.active_channels, Some(2));
        assert_eq!(channels.interleaved, Some(true));
        assert_eq!(channels.mode.as_deref(), Some("2-channel"));
        // Nothing establishes a board maximum, so none is published.
        assert_eq!(channels.max_channels, None);
    }

    /// dmidecode can exit cleanly and still hold no memory table (a stray
    /// message, or a run that never reached the table). With no `Memory Device`
    /// record the reader must say it read nothing, not report one channel.
    #[cfg(target_os = "linux")]
    #[test]
    fn dmidecode_without_a_memory_device_record_is_unreadable() {
        let text = "# dmidecode 3.5\nPermission denied\n";
        assert!(MemoryBandwidthMonitor::parse_dmidecode(text).is_err());
    }

    /// Slots that are all empty read as unknown speed and unknown channels,
    /// not as one channel.
    #[cfg(target_os = "linux")]
    #[test]
    fn dmidecode_with_only_empty_slots_reports_no_channel_count() {
        let text =
            "Memory Device\n\tLocator: DIMM 0\n\tSpeed: Unknown\n\tSize: No Module Installed\n";
        let (_, speed, channels) = MemoryBandwidthMonitor::parse_dmidecode(text).unwrap();
        assert_eq!(speed, None);
        assert_eq!(channels.active_channels, None);
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
            speed_mts: Some(5600),
            channels: ChannelConfig {
                active_channels: Some(2),
                max_channels: Some(4),
                interleaved: Some(true),
                mode: Some("dual-channel".into()),
            },
            peak_bandwidth_gbs: Some(89.6),
            achievable_bandwidth_gbs: Some(73.5),
            estimated_read_gbs: Some(47.8),
            estimated_write_gbs: Some(25.7),
            stream_triad_estimate_gbs: Some(64.5),
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
