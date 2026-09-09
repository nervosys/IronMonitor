//! Physical memory topology monitoring.
//!
//! Enumerates DIMM slots, memory speeds, timings, ECC status, ranks, and
//! manufacturer information. Provides inference-based bandwidth estimation
//! and upgrade recommendations based on empty slots and configuration.
//!
//! ## Platform Support
//!
//! - **Linux**: DMI/SMBIOS via `/sys/class/dmi/id/` and `dmidecode` output parsing
//! - **Windows**: `Win32_PhysicalMemory` and `Win32_PhysicalMemoryArray` via WMI
//! - **macOS**: `system_profiler SPMemoryDataType`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::IronError;

/// Memory technology type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryType {
    DDR2,
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
    ECC,
    Unknown,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DDR2 => write!(f, "DDR2"),
            Self::DDR3 => write!(f, "DDR3"),
            Self::DDR4 => write!(f, "DDR4"),
            Self::DDR5 => write!(f, "DDR5"),
            Self::LPDDR4 => write!(f, "LPDDR4"),
            Self::LPDDR4X => write!(f, "LPDDR4X"),
            Self::LPDDR5 => write!(f, "LPDDR5"),
            Self::LPDDR5X => write!(f, "LPDDR5X"),
            Self::HBM2 => write!(f, "HBM2"),
            Self::HBM2E => write!(f, "HBM2e"),
            Self::HBM3 => write!(f, "HBM3"),
            Self::HBM3E => write!(f, "HBM3e"),
            Self::GDDR6 => write!(f, "GDDR6"),
            Self::GDDR6X => write!(f, "GDDR6X"),
            Self::ECC => write!(f, "ECC"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Form factor of the memory module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormFactor {
    DIMM,
    SODIMM,
    LRDIMM,
    RDIMM,
    UDIMM,
    NVDIMM,
    OnBoard,
    Unknown,
}

impl std::fmt::Display for FormFactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::DIMM => "DIMM",
            Self::SODIMM => "SO-DIMM",
            Self::LRDIMM => "LR-DIMM",
            Self::RDIMM => "R-DIMM",
            Self::UDIMM => "U-DIMM",
            Self::NVDIMM => "NV-DIMM",
            Self::OnBoard => "On-Board",
            Self::Unknown => "Unknown",
        };
        write!(f, "{}", s)
    }
}

/// Information about a single DIMM / memory module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimmInfo {
    /// Slot/bank locator string (e.g. "DIMM_A1", "ChannelA-DIMM0").
    pub locator: String,
    /// Bank locator.
    pub bank: String,
    /// Capacity in bytes (0 means empty slot).
    pub capacity_bytes: u64,
    /// Speed in MT/s (megatransfers per second).
    pub speed_mts: u32,
    /// Configured speed in MT/s (may differ from rated).
    pub configured_speed_mts: u32,
    /// Memory type.
    pub memory_type: MemoryType,
    /// Form factor.
    pub form_factor: FormFactor,
    /// Data width in bits (typically 64).
    /// Data width in bits, where it was read. See [`Self::is_ecc`].
    pub data_width_bits: Option<u32>,
    /// Total width in bits (72 = ECC, 64 = non-ECC).
    /// Total width in bits, where it was read. See [`Self::is_ecc`].
    pub total_width_bits: Option<u32>,
    /// Number of ranks.
    pub ranks: u32,
    /// Manufacturer name.
    pub manufacturer: String,
    /// Part number.
    pub part_number: String,
    /// Serial number.
    pub serial_number: String,
    /// Whether this slot is populated.
    pub populated: bool,
    /// Operating voltage in volts, or `None` where the firmware reported
    /// none.
    ///
    /// This was `f64`, and the Windows reader set it to `0.0` under the comment
    /// "WMI doesn't provide this easily". WMI provides it directly:
    /// `Win32_PhysicalMemory.ConfiguredVoltage`, in millivolts. The query
    /// simply did not select the column. On this crate's development machine it
    /// reads 1100 mV, and the ontology had been publishing
    /// "SMBIOS reported no operating voltage" for it.
    pub voltage: Option<f64>,
}

impl DimmInfo {
    /// Capacity in GiB.
    pub fn capacity_gib(&self) -> f64 {
        self.capacity_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    /// Whether this DIMM has ECC, or `None` when either width is unknown.
    ///
    /// ECC is inferred from the total width exceeding the data width — 72 bits
    /// carrying 64. Both widths defaulted: Windows and Linux set `data_width`
    /// to 64 and `total_width` to whatever `data_width` came out as, and macOS
    /// hardcoded both to 64. Equal widths mean no ECC, so **a machine with ECC
    /// memory reported that it had none** whenever the widths were not read,
    /// and always on macOS.
    pub fn is_ecc(&self) -> Option<bool> {
        Some(self.total_width_bits? > self.data_width_bits?)
    }
}

/// Inferred memory configuration analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAnalysis {
    /// Total installed capacity in bytes.
    pub total_capacity_bytes: u64,
    /// Maximum supported capacity in bytes (from array info).
    pub max_capacity_bytes: u64,
    /// Number of populated slots.
    pub populated_slots: usize,
    /// Total number of slots.
    pub total_slots: usize,
    /// Whether running in dual-channel mode (inferred from slot population).
    pub dual_channel: bool,
    /// Whether running in quad-channel mode.
    pub quad_channel: bool,
    /// Whether all DIMMs are the same speed.
    pub matched_speeds: bool,
    /// Whether all DIMMs are the same capacity (optimal for interleaving).
    pub matched_capacities: bool,
    /// Whether ECC is active.
    pub ecc_active: bool,
    /// Estimated peak bandwidth in GB/s.
    pub estimated_bandwidth_gbs: f64,
    /// Inferred memory channel count.
    pub channel_count: u32,
    /// Upgrade recommendations.
    pub recommendations: Vec<String>,
    /// Efficiency score 0-100 (how well the memory is configured).
    pub efficiency_score: u32,
}

/// Full memory topology for the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTopology {
    /// All DIMM slots (populated and empty).
    pub dimms: Vec<DimmInfo>,
    /// Memory analysis with inference.
    pub analysis: MemoryAnalysis,
}

/// Memory topology monitor.
pub struct MemoryTopologyMonitor {
    topology: MemoryTopology,
}

impl MemoryTopologyMonitor {
    /// Create a new memory topology monitor.
    pub fn new() -> Result<Self, IronError> {
        let dimms = Self::enumerate_dimms()?;
        let analysis = Self::analyze(&dimms);
        Ok(Self {
            topology: MemoryTopology { dimms, analysis },
        })
    }

    /// Refresh data.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.topology.dimms = Self::enumerate_dimms()?;
        self.topology.analysis = Self::analyze(&self.topology.dimms);
        Ok(())
    }

    /// Get the topology.
    pub fn topology(&self) -> &MemoryTopology {
        &self.topology
    }

    /// Get populated DIMMs only.
    pub fn populated_dimms(&self) -> Vec<&DimmInfo> {
        self.topology.dimms.iter().filter(|d| d.populated).collect()
    }

    /// Get empty slots.
    pub fn empty_slots(&self) -> Vec<&DimmInfo> {
        self.topology
            .dimms
            .iter()
            .filter(|d| !d.populated)
            .collect()
    }

    /// Analyze memory configuration.
    fn analyze(dimms: &[DimmInfo]) -> MemoryAnalysis {
        let populated: Vec<&DimmInfo> = dimms.iter().filter(|d| d.populated).collect();
        let total_capacity: u64 = populated.iter().map(|d| d.capacity_bytes).sum();
        let populated_count = populated.len();
        let total_slots = dimms.len();

        // Check matched speeds and capacities
        let speeds: Vec<u32> = populated.iter().map(|d| d.speed_mts).collect();
        let capacities: Vec<u64> = populated.iter().map(|d| d.capacity_bytes).collect();
        let matched_speeds = speeds.windows(2).all(|w| w[0] == w[1]) || speeds.len() <= 1;
        let matched_capacities =
            capacities.windows(2).all(|w| w[0] == w[1]) || capacities.len() <= 1;

        // ECC detection. `any` over `Option` must ask for `Some(true)`: a DIMM
        // whose widths were not read is not a DIMM known to lack ECC.
        let ecc_active = populated.iter().any(|d| d.is_ecc() == Some(true));

        // Channel inference from locator naming patterns
        let channel_count = Self::infer_channels(&populated);
        let dual_channel = channel_count >= 2;
        let quad_channel = channel_count >= 4;

        // Bandwidth estimation
        let max_speed = speeds.iter().copied().max().unwrap_or(0);
        let estimated_bandwidth_gbs = Self::estimate_bandwidth(max_speed, channel_count);

        // Max capacity: assume 2x current or 128GB per slot, whichever is more reasonable
        let max_per_slot: u64 = if populated
            .iter()
            .any(|d| matches!(d.memory_type, MemoryType::DDR5))
        {
            64 * 1024 * 1024 * 1024 // 64 GB per slot for DDR5
        } else {
            32 * 1024 * 1024 * 1024 // 32 GB per slot for DDR4
        };
        let max_capacity_bytes = total_slots as u64 * max_per_slot;

        // Recommendations
        let mut recommendations = Vec::new();
        let efficiency = Self::compute_efficiency(
            &populated,
            total_slots,
            matched_speeds,
            matched_capacities,
            dual_channel,
            &mut recommendations,
        );

        MemoryAnalysis {
            total_capacity_bytes: total_capacity,
            max_capacity_bytes,
            populated_slots: populated_count,
            total_slots,
            dual_channel,
            quad_channel,
            matched_speeds,
            matched_capacities,
            ecc_active,
            estimated_bandwidth_gbs,
            channel_count,
            recommendations,
            efficiency_score: efficiency,
        }
    }

    fn infer_channels(populated: &[&DimmInfo]) -> u32 {
        if populated.is_empty() {
            return 0;
        }

        // Try to detect channels from locator strings
        let mut channels: std::collections::HashSet<String> = std::collections::HashSet::new();
        for d in populated {
            let loc = d.locator.to_uppercase();
            // Common patterns: "DIMM_A1", "ChannelA-DIMM0", "BANK 0", "Channel A"
            if loc.contains("CHANNEL") || loc.contains("DIMM_") {
                // Extract channel letter
                for c in ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H'] {
                    if loc.contains(c)
                        && (loc.contains(&format!("CHANNEL{}", c))
                            || loc.contains(&format!("CHANNEL {}", c))
                            || loc.contains(&format!("DIMM_{}", c))
                            || loc.contains(&format!("_{}{}", c, '0'))
                            || loc.contains(&format!("_{}{}", c, '1')))
                    {
                        channels.insert(c.to_string());
                    }
                }
            }
        }

        if channels.is_empty() {
            // Fallback: infer from populated count
            match populated.len() {
                1 => 1,
                2 => 2,
                3 => 2, // likely 2-channel with one extra
                4 => 2, // commonly dual-channel with 2 DIMMs each
                6 => 3, // triple channel
                8 => 4, // quad channel
                _ => 2, // default assumption
            }
        } else {
            channels.len() as u32
        }
    }

    /// Estimate peak memory bandwidth in GB/s.
    fn estimate_bandwidth(speed_mts: u32, channels: u32) -> f64 {
        // Bandwidth (GB/s) = speed (MT/s) × 8 bytes × channels / 1000
        let bw = speed_mts as f64 * 8.0 * channels as f64 / 1000.0;
        (bw * 100.0).round() / 100.0
    }

    fn compute_efficiency(
        populated: &[&DimmInfo],
        total_slots: usize,
        matched_speeds: bool,
        matched_capacities: bool,
        dual_channel: bool,
        recommendations: &mut Vec<String>,
    ) -> u32 {
        let mut score: i32 = 70; // Base score

        if populated.is_empty() {
            return 0;
        }

        // Dual-channel bonus
        if dual_channel {
            score += 10;
        } else if total_slots >= 2 {
            score -= 15;
            recommendations.push("Enable dual-channel by populating matching DIMM pairs".into());
        }

        // Matched speeds
        if matched_speeds {
            score += 5;
        } else {
            score -= 10;
            recommendations.push("Mismatched DIMM speeds—all modules run at slowest speed".into());
        }

        // Matched capacities
        if matched_capacities {
            score += 5;
        } else {
            score -= 5;
            recommendations
                .push("Mismatched DIMM capacities reduce interleaving efficiency".into());
        }

        // Empty slots = upgrade potential
        let empty = total_slots - populated.len();
        if empty > 0 {
            score += 5; // Upgrade headroom is a positive
            recommendations.push(format!(
                "{} empty DIMM slot(s) available for expansion",
                empty
            ));
        }

        // ECC bonus for workstations/servers. Only credited where ECC was
        // actually observed.
        if populated.iter().any(|d| d.is_ecc() == Some(true)) {
            score += 5;
        }

        score.clamp(0, 100) as u32
    }

    #[cfg(target_os = "linux")]
    fn enumerate_dimms() -> Result<Vec<DimmInfo>, IronError> {
        // Try dmidecode parsing
        let output = std::process::Command::new("dmidecode")
            .args(["-t", "memory"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                Ok(Self::parse_dmidecode(&text))
            }
            _ => {
                // Fallback: /proc/meminfo for total only
                Ok(Vec::new())
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn enumerate_dimms() -> Result<Vec<DimmInfo>, IronError> {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_PhysicalMemory | Select-Object BankLabel,DeviceLocator,Capacity,Speed,ConfiguredClockSpeed,ConfiguredVoltage,MemoryType,SMBIOSMemoryType,FormFactor,DataWidth,TotalWidth,Manufacturer,PartNumber,SerialNumber | ConvertTo-Json",
            ])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                Ok(Self::parse_windows_wmi(&text))
            }
            _ => Ok(Vec::new()),
        }
    }

    #[cfg(target_os = "macos")]
    fn enumerate_dimms() -> Result<Vec<DimmInfo>, IronError> {
        let output = std::process::Command::new("system_profiler")
            .args(["SPMemoryDataType", "-json"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                Ok(Self::parse_macos_profiler(&text))
            }
            _ => Ok(Vec::new()),
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn enumerate_dimms() -> Result<Vec<DimmInfo>, IronError> {
        Ok(Vec::new())
    }

    #[allow(dead_code)]
    fn parse_dmidecode(text: &str) -> Vec<DimmInfo> {
        let mut dimms = Vec::new();
        let mut in_device = false;
        let mut fields: HashMap<String, String> = HashMap::new();

        for line in text.lines() {
            let trimmed = line.trim();

            if trimmed == "Memory Device" {
                if in_device && !fields.is_empty() {
                    if let Some(dimm) = Self::dimm_from_fields(&fields) {
                        dimms.push(dimm);
                    }
                }
                in_device = true;
                fields.clear();
                continue;
            }

            if in_device {
                if let Some((key, val)) = trimmed.split_once(':') {
                    fields.insert(key.trim().to_string(), val.trim().to_string());
                }
            }
        }

        // Don't forget the last one
        if in_device && !fields.is_empty() {
            if let Some(dimm) = Self::dimm_from_fields(&fields) {
                dimms.push(dimm);
            }
        }

        dimms
    }

    fn dimm_from_fields(fields: &HashMap<String, String>) -> Option<DimmInfo> {
        let locator = fields.get("Locator").cloned().unwrap_or_default();
        let bank = fields.get("Bank Locator").cloned().unwrap_or_default();

        let size_str = fields.get("Size").cloned().unwrap_or_default();
        let populated = !size_str.contains("No Module Installed")
            && !size_str.is_empty()
            && size_str != "Unknown";

        let capacity_bytes = if populated {
            Self::parse_size_to_bytes(&size_str)
        } else {
            0
        };

        let speed_str = fields.get("Speed").cloned().unwrap_or_default();
        let speed_mts = Self::parse_mts(&speed_str);

        let conf_speed_str = fields
            .get("Configured Memory Speed")
            .or(fields.get("Configured Clock Speed"))
            .cloned()
            .unwrap_or_default();
        let configured_speed_mts = if conf_speed_str.is_empty() {
            speed_mts
        } else {
            Self::parse_mts(&conf_speed_str)
        };

        let type_str = fields.get("Type").cloned().unwrap_or_default();
        let memory_type = Self::parse_memory_type(&type_str);

        let ff_str = fields.get("Form Factor").cloned().unwrap_or_default();
        let form_factor = Self::parse_form_factor(&ff_str);

        // 64 is the common data width and not this DIMM's, and defaulting the
        // total width to the data width asserts "no ECC" for a field that was
        // not read.
        let data_width = fields
            .get("Data Width")
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse::<u32>().ok());

        let total_width = fields
            .get("Total Width")
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse::<u32>().ok());

        let rank = fields
            .get("Rank")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);

        let manufacturer = fields.get("Manufacturer").cloned().unwrap_or_default();
        let part_number = fields.get("Part Number").cloned().unwrap_or_default();
        let serial_number = fields.get("Serial Number").cloned().unwrap_or_default();

        let voltage = fields
            .get("Configured Voltage")
            .or(fields.get("Minimum Voltage"))
            .and_then(|s| {
                s.split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
            })
            .map(|v| if v > 10.0 { v / 1000.0 } else { v }) // mV to V
            // SMBIOS Type 17 defines zero in these fields as "unknown", so a
            // parsed zero is an absence rather than a reading. `profile/memory`
            // already knew this and filtered on it downstream.
            .filter(|v| *v > 0.0);

        Some(DimmInfo {
            locator,
            bank,
            capacity_bytes,
            speed_mts,
            configured_speed_mts,
            memory_type,
            form_factor,
            data_width_bits: data_width,
            total_width_bits: total_width,
            ranks: rank,
            manufacturer,
            part_number,
            serial_number,
            populated,
            voltage,
        })
    }

    fn parse_size_to_bytes(s: &str) -> u64 {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() < 2 {
            return 0;
        }
        let value: u64 = parts[0].parse().unwrap_or(0);
        match parts[1].to_uppercase().as_str() {
            "KB" => value * 1024,
            "MB" => value * 1024 * 1024,
            "GB" => value * 1024 * 1024 * 1024,
            "TB" => value * 1024 * 1024 * 1024 * 1024,
            _ => value,
        }
    }

    fn parse_mts(s: &str) -> u32 {
        s.split_whitespace()
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0)
    }

    fn parse_memory_type(s: &str) -> MemoryType {
        let upper = s.to_uppercase();
        if upper.contains("LPDDR5X") {
            MemoryType::LPDDR5X
        } else if upper.contains("LPDDR5") {
            MemoryType::LPDDR5
        } else if upper.contains("LPDDR4X") {
            MemoryType::LPDDR4X
        } else if upper.contains("LPDDR4") {
            MemoryType::LPDDR4
        } else if upper.contains("DDR5") {
            MemoryType::DDR5
        } else if upper.contains("DDR4") {
            MemoryType::DDR4
        } else if upper.contains("DDR3") {
            MemoryType::DDR3
        } else if upper.contains("DDR2") {
            MemoryType::DDR2
        } else if upper.contains("HBM3E") {
            MemoryType::HBM3E
        } else if upper.contains("HBM3") {
            MemoryType::HBM3
        } else if upper.contains("HBM2E") {
            MemoryType::HBM2E
        } else if upper.contains("HBM2") {
            MemoryType::HBM2
        } else {
            MemoryType::Unknown
        }
    }

    fn parse_form_factor(s: &str) -> FormFactor {
        let upper = s.to_uppercase();
        if upper.contains("SODIMM") || upper.contains("SO-DIMM") {
            FormFactor::SODIMM
        } else if upper.contains("LRDIMM") {
            FormFactor::LRDIMM
        } else if upper.contains("RDIMM") {
            FormFactor::RDIMM
        } else if upper.contains("NVDIMM") {
            FormFactor::NVDIMM
        } else if upper.contains("DIMM") {
            FormFactor::DIMM
        } else {
            FormFactor::Unknown
        }
    }

    #[allow(dead_code)]
    fn parse_windows_wmi(text: &str) -> Vec<DimmInfo> {
        // Parse JSON array from PowerShell
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // Try to parse as JSON array or single object
        let items: Vec<serde_json::Value> = if trimmed.starts_with('[') {
            serde_json::from_str(trimmed).unwrap_or_default()
        } else if trimmed.starts_with('{') {
            vec![serde_json::from_str(trimmed).unwrap_or_default()]
        } else {
            Vec::new()
        };

        items
            .iter()
            .map(|item| {
                let capacity = item["Capacity"]
                    .as_u64()
                    .or_else(|| item["Capacity"].as_str().and_then(|s| s.parse().ok()))
                    .unwrap_or(0);
                let speed = item["Speed"].as_u64().unwrap_or(0) as u32;
                let conf_speed = item["ConfiguredClockSpeed"]
                    .as_u64()
                    .unwrap_or(speed as u64) as u32;
                let data_width = item["DataWidth"].as_u64().map(|w| w as u32);
                let total_width = item["TotalWidth"].as_u64().map(|w| w as u32);

                let smbios_type = item["SMBIOSMemoryType"].as_u64().unwrap_or(0);
                let memory_type = match smbios_type {
                    20 => MemoryType::DDR,
                    21 => MemoryType::DDR2,
                    24 => MemoryType::DDR3,
                    26 => MemoryType::DDR4,
                    34 => MemoryType::DDR5,
                    _ => MemoryType::Unknown,
                };

                let ff_val = item["FormFactor"].as_u64().unwrap_or(0);
                let form_factor = match ff_val {
                    8 => FormFactor::DIMM,
                    12 => FormFactor::SODIMM,
                    _ => FormFactor::Unknown,
                };

                DimmInfo {
                    locator: item["DeviceLocator"].as_str().unwrap_or("").to_string(),
                    bank: item["BankLabel"].as_str().unwrap_or("").to_string(),
                    capacity_bytes: capacity,
                    speed_mts: speed,
                    configured_speed_mts: conf_speed,
                    memory_type,
                    form_factor,
                    data_width_bits: data_width,
                    total_width_bits: total_width,
                    ranks: 1, // WMI doesn't expose rank count easily
                    manufacturer: item["Manufacturer"]
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                    part_number: item["PartNumber"].as_str().unwrap_or("").trim().to_string(),
                    serial_number: item["SerialNumber"]
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                    populated: capacity > 0,
                    // `ConfiguredVoltage` is in millivolts. A zero there means
                    // the firmware filled the field with nothing, which is not
                    // an operating voltage of zero.
                    voltage: item["ConfiguredVoltage"]
                        .as_u64()
                        .filter(|mv| *mv > 0)
                        .map(|mv| mv as f64 / 1000.0),
                }
            })
            .collect()
    }

    #[allow(dead_code)]
    fn parse_macos_profiler(text: &str) -> Vec<DimmInfo> {
        // macOS system_profiler SPMemoryDataType -json
        let root: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let mut dimms = Vec::new();
        if let Some(items) = root["SPMemoryDataType"].as_array() {
            for item in items {
                if let Some(slots) = item["_items"].as_array() {
                    for slot in slots {
                        let size_str = slot["dimm_size"].as_str().unwrap_or("0");
                        let capacity_bytes = Self::parse_size_to_bytes(size_str);
                        let speed_str = slot["dimm_speed"].as_str().unwrap_or("0 MHz");
                        let speed_mts = Self::parse_mts(speed_str);
                        let type_str = slot["dimm_type"].as_str().unwrap_or("Unknown");

                        dimms.push(DimmInfo {
                            locator: slot["_name"].as_str().unwrap_or("").to_string(),
                            bank: String::new(),
                            capacity_bytes,
                            speed_mts,
                            configured_speed_mts: speed_mts,
                            memory_type: Self::parse_memory_type(type_str),
                            form_factor: FormFactor::Unknown,
                            // system_profiler reports neither width, and
                            // 64/64 asserted "no ECC" on every Mac.
                            data_width_bits: None,
                            total_width_bits: None,
                            ranks: 1,
                            manufacturer: slot["dimm_manufacturer"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                            part_number: slot["dimm_part_number"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                            serial_number: slot["dimm_serial_number"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                            populated: capacity_bytes > 0,
                            // `system_profiler` reports no DIMM voltage.
                            voltage: None,
                        });
                    }
                }
            }
        }

        dimms
    }
}

impl Default for MemoryTopologyMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            topology: MemoryTopology {
                dimms: Vec::new(),
                analysis: MemoryAnalysis {
                    total_capacity_bytes: 0,
                    max_capacity_bytes: 0,
                    populated_slots: 0,
                    total_slots: 0,
                    dual_channel: false,
                    quad_channel: false,
                    matched_speeds: true,
                    matched_capacities: true,
                    ecc_active: false,
                    estimated_bandwidth_gbs: 0.0,
                    channel_count: 0,
                    recommendations: Vec::new(),
                    efficiency_score: 0,
                },
            },
        })
    }
}

// DDR variant used only for Windows SMBIOS type 20
#[allow(dead_code)]
impl MemoryType {
    const DDR: MemoryType = MemoryType::Unknown; // DDR1 is too old to specifically handle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dimm_capacity_gib() {
        let dimm = DimmInfo {
            locator: "DIMM_A1".into(),
            bank: "BANK 0".into(),
            capacity_bytes: 16 * 1024 * 1024 * 1024,
            speed_mts: 3200,
            configured_speed_mts: 3200,
            memory_type: MemoryType::DDR4,
            form_factor: FormFactor::DIMM,
            data_width_bits: Some(64),
            total_width_bits: Some(64),
            ranks: 2,
            manufacturer: "Samsung".into(),
            part_number: "M378A2G43AB3-CWE".into(),
            serial_number: "12345678".into(),
            populated: true,
            voltage: Some(1.2),
        };
        assert!((dimm.capacity_gib() - 16.0).abs() < 0.01);
        assert_eq!(dimm.is_ecc(), Some(false));
    }

    /// A DIMM whose widths were not read has no ECC answer.
    ///
    /// ECC is inferred from the total width exceeding the data width. The
    /// readers defaulted `data_width` to 64 and `total_width` to whatever
    /// `data_width` became — and macOS hardcoded both to 64 — so equal widths
    /// reported **no ECC** for any DIMM nothing had measured.
    ///
    /// The ontology guarded this with `> 0`, under a comment that named the
    /// danger exactly ("Two zeros are equal, and would otherwise report 'no
    /// ECC'"), and the guard never fired because the sentinel was 64.
    #[test]
    fn unread_widths_give_no_ecc_answer() {
        let mut dimm = DimmInfo {
            locator: "DIMM_A1".into(),
            bank: String::new(),
            capacity_bytes: 0,
            speed_mts: 0,
            configured_speed_mts: 0,
            memory_type: MemoryType::Unknown,
            form_factor: FormFactor::Unknown,
            data_width_bits: None,
            total_width_bits: None,
            ranks: 1,
            manufacturer: String::new(),
            part_number: String::new(),
            serial_number: String::new(),
            populated: true,
            voltage: None,
        };
        assert_eq!(dimm.is_ecc(), None, "neither width read");

        dimm.data_width_bits = Some(64);
        assert_eq!(dimm.is_ecc(), None, "only one width read");

        dimm.total_width_bits = Some(72);
        assert_eq!(dimm.is_ecc(), Some(true), "72 carrying 64 is ECC");

        dimm.total_width_bits = Some(64);
        assert_eq!(dimm.is_ecc(), Some(false), "equal widths, both read");
    }

    #[test]
    fn test_ecc_detection() {
        let dimm = DimmInfo {
            locator: "DIMM_A1".into(),
            bank: "".into(),
            capacity_bytes: 32 * 1024 * 1024 * 1024,
            speed_mts: 3200,
            configured_speed_mts: 3200,
            memory_type: MemoryType::DDR4,
            form_factor: FormFactor::RDIMM,
            data_width_bits: Some(64),
            total_width_bits: Some(72),
            ranks: 2,
            manufacturer: "SK Hynix".into(),
            part_number: "".into(),
            serial_number: "".into(),
            populated: true,
            voltage: Some(1.2),
        };
        assert_eq!(dimm.is_ecc(), Some(true));
    }

    #[test]
    fn test_bandwidth_estimation() {
        // DDR4-3200 dual-channel: 3200 × 8 × 2 / 1000 = 51.2 GB/s
        let bw = MemoryTopologyMonitor::estimate_bandwidth(3200, 2);
        assert!((bw - 51.2).abs() < 0.1);

        // DDR5-5600 dual-channel: 5600 × 8 × 2 / 1000 = 89.6 GB/s
        let bw5 = MemoryTopologyMonitor::estimate_bandwidth(5600, 2);
        assert!((bw5 - 89.6).abs() < 0.1);
    }

    #[test]
    fn test_memory_type_parsing() {
        assert_eq!(
            MemoryTopologyMonitor::parse_memory_type("DDR4"),
            MemoryType::DDR4
        );
        assert_eq!(
            MemoryTopologyMonitor::parse_memory_type("LPDDR5X"),
            MemoryType::LPDDR5X
        );
        assert_eq!(
            MemoryTopologyMonitor::parse_memory_type("DDR5"),
            MemoryType::DDR5
        );
    }

    #[test]
    fn test_analysis_dual_channel() {
        let dimms = vec![
            DimmInfo {
                locator: "DIMM_A1".into(),
                bank: "BANK 0".into(),
                capacity_bytes: 8 * 1024 * 1024 * 1024,
                speed_mts: 3200,
                configured_speed_mts: 3200,
                memory_type: MemoryType::DDR4,
                form_factor: FormFactor::DIMM,
                data_width_bits: Some(64),
                total_width_bits: Some(64),
                ranks: 1,
                manufacturer: "Samsung".into(),
                part_number: "".into(),
                serial_number: "".into(),
                populated: true,
                voltage: Some(1.2),
            },
            DimmInfo {
                locator: "DIMM_B1".into(),
                bank: "BANK 1".into(),
                capacity_bytes: 8 * 1024 * 1024 * 1024,
                speed_mts: 3200,
                configured_speed_mts: 3200,
                memory_type: MemoryType::DDR4,
                form_factor: FormFactor::DIMM,
                data_width_bits: Some(64),
                total_width_bits: Some(64),
                ranks: 1,
                manufacturer: "Samsung".into(),
                part_number: "".into(),
                serial_number: "".into(),
                populated: true,
                voltage: Some(1.2),
            },
        ];
        let analysis = MemoryTopologyMonitor::analyze(&dimms);
        assert_eq!(analysis.populated_slots, 2);
        assert!(analysis.dual_channel);
        assert!(analysis.matched_speeds);
        assert!(analysis.matched_capacities);
        assert!(analysis.estimated_bandwidth_gbs > 40.0);
    }

    #[test]
    fn test_monitor_default() {
        let monitor = MemoryTopologyMonitor::default();
        let _topo = monitor.topology();
    }

    #[test]
    fn test_serialization() {
        let analysis = MemoryAnalysis {
            total_capacity_bytes: 32 * 1024 * 1024 * 1024,
            max_capacity_bytes: 128 * 1024 * 1024 * 1024,
            populated_slots: 2,
            total_slots: 4,
            dual_channel: true,
            quad_channel: false,
            matched_speeds: true,
            matched_capacities: true,
            ecc_active: false,
            estimated_bandwidth_gbs: 51.2,
            channel_count: 2,
            recommendations: vec!["2 empty DIMM slot(s) available for expansion".into()],
            efficiency_score: 85,
        };
        let json = serde_json::to_string(&analysis).unwrap();
        assert!(json.contains("dual_channel"));
        let _: MemoryAnalysis = serde_json::from_str(&json).unwrap();
    }
}
