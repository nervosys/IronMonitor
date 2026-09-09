//! PCIe Bandwidth Monitoring
//!
//! Monitors PCI Express link speed, width, and bandwidth utilization for GPUs
//! and other PCIe devices. Reads from sysfs on Linux and configuration space
//! on Windows.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::pcie::{PcieMonitor, PcieDevice};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let devices = PcieMonitor::enumerate()?;
//! for dev in &devices {
//!     println!("{} [{:04x}:{:04x}]", dev.name, dev.vendor_id, dev.device_id);
//!     println!("  BDF: {}", dev.bdf);
//!     // A link speed sysfs did not give us reads as `None`, not as Gen0.
//!     match (dev.current_link_speed.gen_number(), dev.max_bandwidth_gbps()) {
//!         (Some(gen), Some(bw)) => println!(
//!             "  Link: Gen{} x{} ({:.1} GB/s max)", gen, dev.current_link_width, bw
//!         ),
//!         _ => println!("  Link: speed not read, x{}", dev.current_link_width),
//!     }
//!     if let Some(gen) = dev.max_link_speed.and_then(|s| s.gen_number()) {
//!         println!("  Capable: Gen{} x{}", gen, dev.max_link_width);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};

/// PCIe generation / link speed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PcieLinkSpeed {
    /// PCIe 1.0 — 2.5 GT/s
    Gen1,
    /// PCIe 2.0 — 5.0 GT/s
    Gen2,
    /// PCIe 3.0 — 8.0 GT/s
    Gen3,
    /// PCIe 4.0 — 16.0 GT/s
    Gen4,
    /// PCIe 5.0 — 32.0 GT/s
    Gen5,
    /// PCIe 6.0 — 64.0 GT/s
    Gen6,
    /// Unknown speed
    Unknown,
}

impl PcieLinkSpeed {
    /// Generation number (1-6), or `None` for a link whose speed was not read.
    ///
    /// `Unknown` used to answer `0`, and there is no PCIe Gen0. All three
    /// lookups on this enum did it, and because `Unknown` is what `from_sysfs`
    /// returns for anything it fails to parse, each one turned an unreadable
    /// link into a specific, wrong, low reading.
    pub fn gen_number(&self) -> Option<u8> {
        match self {
            Self::Gen1 => Some(1),
            Self::Gen2 => Some(2),
            Self::Gen3 => Some(3),
            Self::Gen4 => Some(4),
            Self::Gen5 => Some(5),
            Self::Gen6 => Some(6),
            Self::Unknown => None,
        }
    }

    /// Transfer rate in GT/s (gigatransfers per second). See [`Self::gen_number`].
    pub fn transfer_rate_gts(&self) -> Option<f64> {
        match self {
            Self::Gen1 => Some(2.5),
            Self::Gen2 => Some(5.0),
            Self::Gen3 => Some(8.0),
            Self::Gen4 => Some(16.0),
            Self::Gen5 => Some(32.0),
            Self::Gen6 => Some(64.0),
            Self::Unknown => None,
        }
    }

    /// Per-lane bandwidth in GB/s, after 8b/10b or 128b/130b encoding overhead.
    ///
    /// See [`Self::gen_number`]. This one did the most damage: it feeds
    /// [`PcieDevice::max_bandwidth_gbps`], which is summed across every device
    /// into an aggregate figure, so a link of unknown speed contributed 0 GB/s
    /// to a total presented as the bus's bandwidth.
    pub fn per_lane_gbps(&self) -> Option<f64> {
        match self {
            // Gen1/2: 8b/10b encoding (20% overhead)
            Self::Gen1 => Some(0.25), // 2.5 GT/s * 8/10 / 8
            Self::Gen2 => Some(0.5),  // 5.0 GT/s * 8/10 / 8
            // Gen3+: 128b/130b encoding (~1.5% overhead)
            Self::Gen3 => Some(0.985), // ~1 GB/s per lane
            Self::Gen4 => Some(1.969), // ~2 GB/s per lane
            Self::Gen5 => Some(3.938), // ~4 GB/s per lane
            Self::Gen6 => Some(7.877), // ~8 GB/s per lane (PAM4)
            Self::Unknown => None,
        }
    }

    /// Parse from sysfs speed string (e.g. "8.0 GT/s PCIe")
    pub fn from_sysfs(s: &str) -> Self {
        let s = s.trim();
        if s.contains("64") || s.contains("Gen6") {
            Self::Gen6
        } else if s.contains("32") || s.contains("Gen5") {
            Self::Gen5
        } else if s.contains("16") || s.contains("Gen4") {
            Self::Gen4
        } else if s.starts_with("8") || s.contains("Gen3") {
            Self::Gen3
        } else if s.starts_with("5") || s.contains("Gen2") {
            Self::Gen2
        } else if s.starts_with("2.5") || s.contains("Gen1") {
            Self::Gen1
        } else {
            Self::Unknown
        }
    }
}

impl std::fmt::Display for PcieLinkSpeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gen1 => write!(f, "Gen1 (2.5 GT/s)"),
            Self::Gen2 => write!(f, "Gen2 (5.0 GT/s)"),
            Self::Gen3 => write!(f, "Gen3 (8.0 GT/s)"),
            Self::Gen4 => write!(f, "Gen4 (16.0 GT/s)"),
            Self::Gen5 => write!(f, "Gen5 (32.0 GT/s)"),
            Self::Gen6 => write!(f, "Gen6 (64.0 GT/s)"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// PCIe device class
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PcieDeviceClass {
    /// Display controller (GPU)
    DisplayController,
    /// 3D controller (compute GPU)
    Controller3D,
    /// VGA compatible controller
    VgaCompatible,
    /// Network controller
    NetworkController,
    /// Storage controller (NVMe, SATA, etc.)
    StorageController,
    /// USB controller
    UsbController,
    /// Audio device
    AudioDevice,
    /// Bridge device (PCIe switch, host bridge)
    Bridge,
    /// Other
    Other(u32),
}

impl PcieDeviceClass {
    /// Parse from sysfs class code (e.g. "0x030000" for VGA)
    pub fn from_class_code(code: u32) -> Self {
        match (code >> 16) & 0xFF {
            0x03 => match (code >> 8) & 0xFF {
                0x00 => Self::VgaCompatible,
                0x02 => Self::Controller3D,
                0x80 => Self::DisplayController,
                _ => Self::DisplayController,
            },
            0x02 => Self::NetworkController,
            0x01 => Self::StorageController,
            0x0C if ((code >> 8) & 0xFF) == 0x03 => Self::UsbController,
            0x04 => Self::AudioDevice,
            0x06 => Self::Bridge,
            _ => Self::Other(code),
        }
    }

    /// Whether this is a GPU device
    pub fn is_gpu(&self) -> bool {
        matches!(
            self,
            Self::DisplayController | Self::Controller3D | Self::VgaCompatible
        )
    }
}

impl std::fmt::Display for PcieDeviceClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DisplayController => write!(f, "Display Controller"),
            Self::Controller3D => write!(f, "3D Controller"),
            Self::VgaCompatible => write!(f, "VGA Compatible"),
            Self::NetworkController => write!(f, "Network Controller"),
            Self::StorageController => write!(f, "Storage Controller"),
            Self::UsbController => write!(f, "USB Controller"),
            Self::AudioDevice => write!(f, "Audio Device"),
            Self::Bridge => write!(f, "Bridge"),
            Self::Other(c) => write!(f, "Other (0x{:06x})", c),
        }
    }
}

/// A PCIe device with link information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcieDevice {
    /// Bus:Device.Function address (e.g. "0000:01:00.0")
    pub bdf: String,
    /// Vendor ID (e.g. 0x10DE for NVIDIA)
    pub vendor_id: u16,
    /// Device ID
    pub device_id: u16,
    /// Device class
    pub device_class: PcieDeviceClass,
    /// Device name (from sysfs or PCI IDs)
    pub name: String,
    /// Vendor name
    pub vendor_name: String,
    /// NUMA node (-1 if not applicable)
    pub numa_node: i32,
    /// IOMMU group
    pub iommu_group: Option<u32>,
    /// Current negotiated link speed
    pub current_link_speed: PcieLinkSpeed,
    /// Maximum device-capable link speed
    pub max_link_speed: Option<PcieLinkSpeed>,
    /// Current negotiated link width (number of lanes)
    pub current_link_width: u8,
    /// Maximum device-capable link width
    pub max_link_width: u8,
    /// Driver currently bound to this device
    pub driver: Option<String>,
    /// Subsystem vendor:device
    pub subsystem: Option<String>,
    /// D-state (power state: D0=active, D3=off, etc.)
    pub power_state: Option<String>,
}

impl PcieDevice {
    /// Bandwidth in GB/s at the current link speed and width, or `None` when
    /// the speed was not read.
    pub fn max_bandwidth_gbps(&self) -> Option<f64> {
        Some(self.current_link_speed.per_lane_gbps()? * self.current_link_width as f64)
    }

    /// Bandwidth in GB/s the device is capable of. See [`Self::max_bandwidth_gbps`].
    pub fn capable_bandwidth_gbps(&self) -> Option<f64> {
        let speed = self.max_link_speed.unwrap_or(self.current_link_speed);
        Some(speed.per_lane_gbps()? * self.max_link_width as f64)
    }

    /// Whether the link is running below its maximum capability, or `None` when
    /// there is not enough read to say.
    ///
    /// This returned `bool` and compared generation numbers in which `Unknown`
    /// was `0`, so it got the answer wrong in both directions. A device whose
    /// *maximum* speed was unreadable compared as Gen0 and so never looked
    /// downgraded; a device whose *current* speed was unreadable compared as
    /// Gen0 against a known maximum and was reported as degraded -- an alarm
    /// raised about a link nobody measured.
    ///
    /// Width is the cheaper signal and still settles it alone: a link narrower
    /// than its maximum is downgraded whatever the speeds turn out to be. Only
    /// when width says no does the speed comparison matter, and that needs both
    /// speeds.
    pub fn is_downgraded(&self) -> Option<bool> {
        if self.current_link_width < self.max_link_width {
            return Some(true);
        }
        match &self.max_link_speed {
            Some(max_speed) => {
                Some(max_speed.gen_number()? > self.current_link_speed.gen_number()?)
            }
            // No maximum to compare against, and width is at its full value.
            None => Some(false),
        }
    }

    /// Whether this is a GPU device
    pub fn is_gpu(&self) -> bool {
        self.device_class.is_gpu()
    }
}

/// PCIe bus monitor
pub struct PcieMonitor;

impl PcieMonitor {
    /// Enumerate all PCIe devices with link information
    pub fn enumerate() -> Result<Vec<PcieDevice>, crate::error::IronError> {
        #[cfg(target_os = "linux")]
        {
            Self::enumerate_linux()
        }
        #[cfg(windows)]
        {
            Self::enumerate_windows()
        }
        #[cfg(target_os = "macos")]
        {
            Self::enumerate_macos()
        }
        #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
        {
            Err(crate::error::IronError::NotImplemented(
                "PCIe monitoring not supported on this platform".into(),
            ))
        }
    }

    /// Enumerate only GPU PCIe devices
    pub fn gpu_devices() -> Result<Vec<PcieDevice>, crate::error::IronError> {
        Ok(Self::enumerate()?
            .into_iter()
            .filter(|d| d.is_gpu())
            .collect())
    }

    /// Devices known to be running below their maximum capability.
    ///
    /// A device whose link speed was not readable is left out rather than
    /// included: this list is what a caller acts on, and putting an unmeasured
    /// link in it is the alarm described on [`PcieDevice::is_downgraded`].
    /// `enumerate()` still returns those devices, so a caller who wants to see
    /// them can filter for `is_downgraded() == None`.
    pub fn downgraded_devices() -> Result<Vec<PcieDevice>, crate::error::IronError> {
        Ok(Self::enumerate()?
            .into_iter()
            .filter(|d| d.is_downgraded() == Some(true))
            .collect())
    }

    #[cfg(target_os = "linux")]
    fn enumerate_linux() -> Result<Vec<PcieDevice>, crate::error::IronError> {
        use std::fs;
        use std::path::Path;

        let mut devices = Vec::new();
        let pci_base = Path::new("/sys/bus/pci/devices");

        let entries = fs::read_dir(pci_base).map_err(|e| {
            crate::error::IronError::Other(format!("Cannot read PCI devices: {}", e))
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            let bdf = entry.file_name().to_string_lossy().to_string();

            // Read vendor and device IDs
            let vendor_id = Self::read_hex_file(&path.join("vendor")).unwrap_or(0) as u16;
            let device_id = Self::read_hex_file(&path.join("device")).unwrap_or(0) as u16;
            let class_code = Self::read_hex_file(&path.join("class")).unwrap_or(0) as u32;

            let device_class = PcieDeviceClass::from_class_code(class_code);

            // Read link information
            let current_speed_str =
                fs::read_to_string(path.join("current_link_speed")).unwrap_or_default();
            let max_speed_str = fs::read_to_string(path.join("max_link_speed")).unwrap_or_default();
            let current_width_str =
                fs::read_to_string(path.join("current_link_width")).unwrap_or_default();
            let max_width_str = fs::read_to_string(path.join("max_link_width")).unwrap_or_default();

            let current_link_speed = PcieLinkSpeed::from_sysfs(&current_speed_str);
            let max_link_speed = if max_speed_str.trim().is_empty() {
                None
            } else {
                Some(PcieLinkSpeed::from_sysfs(&max_speed_str))
            };

            let current_link_width = current_width_str.trim().parse::<u8>().unwrap_or(0);
            let max_link_width = max_width_str.trim().parse::<u8>().unwrap_or(0);

            // Skip devices with no link info (bridges, etc. with width 0)
            // but include if class is known
            if current_link_width == 0 && !device_class.is_gpu() {
                // Still include bridges and endpoint devices
                if matches!(device_class, PcieDeviceClass::Bridge) {
                    // skip pure bridges
                    continue;
                }
            }

            // Read driver
            let driver = fs::read_link(path.join("driver"))
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));

            // Read NUMA node
            let numa_node = fs::read_to_string(path.join("numa_node"))
                .ok()
                .and_then(|s| s.trim().parse::<i32>().ok())
                .unwrap_or(-1);

            // Read IOMMU group
            let iommu_group = fs::read_link(path.join("iommu_group")).ok().and_then(|p| {
                p.file_name()
                    .and_then(|n| n.to_string_lossy().parse::<u32>().ok())
            });

            // Read subsystem
            let subsys_vendor =
                Self::read_hex_file(&path.join("subsystem_vendor")).map(|v| format!("{:04x}", v));
            let subsys_device =
                Self::read_hex_file(&path.join("subsystem_device")).map(|v| format!("{:04x}", v));
            let subsystem = match (subsys_vendor, subsys_device) {
                (Some(v), Some(d)) => Some(format!("{}:{}", v, d)),
                _ => None,
            };

            // Power state
            let power_state = fs::read_to_string(path.join("power_state"))
                .ok()
                .map(|s| s.trim().to_string());

            // Vendor name lookup
            let vendor_name = match vendor_id {
                0x10DE => "NVIDIA".to_string(),
                0x1002 => "AMD".to_string(),
                0x8086 => "Intel".to_string(),
                0x14E4 => "Broadcom".to_string(),
                0x1B73 => "Fresco Logic".to_string(),
                0x1912 => "Renesas".to_string(),
                0x1B21 => "ASMedia".to_string(),
                0x1B4B => "Marvell".to_string(),
                0x15B3 => "Mellanox".to_string(),
                0x144D => "Samsung".to_string(),
                0x1C5C | 0x1E0F => "SK Hynix".to_string(),
                0x1179 | 0xC0A9 => "Toshiba / Kioxia".to_string(),
                0x126F => "Silicon Motion".to_string(),
                0x1987 | 0x1E4B => "Phison".to_string(),
                _ => format!("{:04x}", vendor_id),
            };

            let name = format!("{} {} [{}]", vendor_name, device_class, bdf);

            devices.push(PcieDevice {
                bdf,
                vendor_id,
                device_id,
                device_class,
                name,
                vendor_name,
                numa_node,
                iommu_group,
                current_link_speed,
                max_link_speed,
                current_link_width,
                max_link_width,
                driver,
                subsystem,
                power_state,
            });
        }

        // Sort by BDF address
        devices.sort_by(|a, b| a.bdf.cmp(&b.bdf));
        Ok(devices)
    }

    #[cfg(target_os = "linux")]
    fn read_hex_file(path: &std::path::Path) -> Option<u64> {
        let content = std::fs::read_to_string(path).ok()?;
        let trimmed = content.trim().trim_start_matches("0x");
        u64::from_str_radix(trimmed, 16).ok()
    }

    #[cfg(windows)]
    fn enumerate_windows() -> Result<Vec<PcieDevice>, crate::error::IronError> {
        // On Windows we can use SetupAPI or WMI to enumerate PCI devices
        // For now, use WMI Win32_PnPEntity with PCI bus
        // This is a simplified implementation
        Err(crate::error::IronError::NotImplemented(
            "PCIe enumeration on Windows is not yet implemented. Use GPU-specific backends for GPU PCIe info.".into(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn enumerate_macos() -> Result<Vec<PcieDevice>, crate::error::IronError> {
        Err(crate::error::IronError::NotImplemented(
            "PCIe enumeration on macOS requires IOKit bindings (not yet implemented)".into(),
        ))
    }
}

/// Summary of PCIe bandwidth for all devices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcieBandwidthSummary {
    /// Total number of PCIe devices
    pub total_devices: usize,
    /// Number of GPU devices
    pub gpu_devices: usize,
    /// Number of devices known to be running below max capability.
    pub downgraded_devices: usize,
    /// Devices whose link speed was not readable.
    ///
    /// These are excluded from [`Self::total_bandwidth_gbps`] and from
    /// [`Self::downgraded_devices`] rather than counted as zero and as healthy.
    /// A non-zero value here is how a reader knows the other two are partial.
    pub unreadable_devices: usize,
    /// Aggregate bandwidth in GB/s over the devices whose speed was read, or
    /// `None` if none of them were.
    pub total_bandwidth_gbps: Option<f64>,
    /// Highest PCIe generation found, or `None` if no link speed was read.
    pub max_generation: Option<u8>,
}

impl PcieBandwidthSummary {
    /// Generate a summary from a list of devices
    pub fn from_devices(devices: &[PcieDevice]) -> Self {
        let gpu_count = devices.iter().filter(|d| d.is_gpu()).count();
        let downgraded = devices
            .iter()
            .filter(|d| d.is_downgraded() == Some(true))
            .count();

        let bandwidths: Vec<f64> = devices
            .iter()
            .filter_map(|d| d.max_bandwidth_gbps())
            .collect();
        let unreadable = devices.len() - bandwidths.len();
        let total_bw = if bandwidths.is_empty() {
            None
        } else {
            Some(bandwidths.iter().sum())
        };

        let max_gen = devices
            .iter()
            .filter_map(|d| d.current_link_speed.gen_number())
            .max();

        Self {
            total_devices: devices.len(),
            gpu_devices: gpu_count,
            downgraded_devices: downgraded,
            unreadable_devices: unreadable,
            total_bandwidth_gbps: total_bw,
            max_generation: max_gen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_link_speed_gen3() {
        let speed = PcieLinkSpeed::from_sysfs("8.0 GT/s PCIe");
        assert_eq!(speed, PcieLinkSpeed::Gen3);
        assert_eq!(speed.gen_number(), Some(3));
        assert!((speed.per_lane_gbps().unwrap() - 0.985).abs() < 0.01);
    }

    #[test]
    fn test_link_speed_gen4() {
        let speed = PcieLinkSpeed::from_sysfs("16.0 GT/s PCIe");
        assert_eq!(speed, PcieLinkSpeed::Gen4);
    }

    #[test]
    fn test_bandwidth_calculation() {
        let dev = PcieDevice {
            bdf: "0000:01:00.0".into(),
            vendor_id: 0x10DE,
            device_id: 0x2684,
            device_class: PcieDeviceClass::VgaCompatible,
            name: "NVIDIA RTX 4090".into(),
            vendor_name: "NVIDIA".into(),
            numa_node: 0,
            iommu_group: None,
            current_link_speed: PcieLinkSpeed::Gen4,
            max_link_speed: Some(PcieLinkSpeed::Gen4),
            current_link_width: 16,
            max_link_width: 16,
            driver: Some("nvidia".into()),
            subsystem: None,
            power_state: Some("D0".into()),
        };
        // Gen4 x16 should be ~31.5 GB/s
        assert!(dev.max_bandwidth_gbps().unwrap() > 30.0);
        assert_eq!(dev.is_downgraded(), Some(false));
    }

    #[test]
    fn test_downgraded_detection() {
        let dev = PcieDevice {
            bdf: "0000:01:00.0".into(),
            vendor_id: 0x10DE,
            device_id: 0x2684,
            device_class: PcieDeviceClass::VgaCompatible,
            name: "Test GPU".into(),
            vendor_name: "NVIDIA".into(),
            numa_node: 0,
            iommu_group: None,
            current_link_speed: PcieLinkSpeed::Gen3,
            max_link_speed: Some(PcieLinkSpeed::Gen4),
            current_link_width: 8,
            max_link_width: 16,
            driver: None,
            subsystem: None,
            power_state: None,
        };
        assert_eq!(dev.is_downgraded(), Some(true));
    }

    #[test]
    fn test_device_class_gpu() {
        assert!(PcieDeviceClass::from_class_code(0x030000).is_gpu()); // VGA
        assert!(PcieDeviceClass::from_class_code(0x030200).is_gpu()); // 3D controller
        assert!(!PcieDeviceClass::from_class_code(0x020000).is_gpu()); // Network
    }

    #[test]
    fn test_bandwidth_summary() {
        let devices = vec![PcieDevice {
            bdf: "0000:01:00.0".into(),
            vendor_id: 0x10DE,
            device_id: 0x2684,
            device_class: PcieDeviceClass::VgaCompatible,
            name: "GPU".into(),
            vendor_name: "NVIDIA".into(),
            numa_node: 0,
            iommu_group: None,
            current_link_speed: PcieLinkSpeed::Gen4,
            max_link_speed: Some(PcieLinkSpeed::Gen4),
            current_link_width: 16,
            max_link_width: 16,
            driver: None,
            subsystem: None,
            power_state: None,
        }];
        let summary = PcieBandwidthSummary::from_devices(&devices);
        assert_eq!(summary.gpu_devices, 1);
        assert_eq!(summary.downgraded_devices, 0);
        assert_eq!(summary.unreadable_devices, 0);
        assert_eq!(summary.max_generation, Some(4));
    }

    /// A device whose link speed did not parse must not read as a healthy Gen0
    /// contributing 0 GB/s.
    #[test]
    fn an_unread_link_speed_is_absent_rather_than_zero() {
        let unknown = PcieLinkSpeed::from_sysfs("something the kernel did not give us");
        assert_eq!(unknown, PcieLinkSpeed::Unknown);
        assert_eq!(unknown.gen_number(), None);
        assert_eq!(unknown.transfer_rate_gts(), None);
        assert_eq!(unknown.per_lane_gbps(), None);

        let dev = PcieDevice {
            bdf: "0000:02:00.0".into(),
            vendor_id: 0x8086,
            device_id: 0x1234,
            device_class: PcieDeviceClass::VgaCompatible,
            name: "Unreadable link".into(),
            vendor_name: "Intel".into(),
            numa_node: 0,
            iommu_group: None,
            current_link_speed: PcieLinkSpeed::Unknown,
            max_link_speed: Some(PcieLinkSpeed::Gen4),
            current_link_width: 16,
            max_link_width: 16,
            driver: None,
            subsystem: None,
            power_state: None,
        };
        assert_eq!(dev.max_bandwidth_gbps(), None);
        // Not `Some(true)`: comparing Gen0 against Gen4 used to raise a
        // degradation alarm about a link that was never measured.
        assert_eq!(dev.is_downgraded(), None);

        let summary = PcieBandwidthSummary::from_devices(&[dev]);
        assert_eq!(summary.total_devices, 1);
        assert_eq!(summary.unreadable_devices, 1);
        assert_eq!(summary.downgraded_devices, 0);
        assert_eq!(summary.total_bandwidth_gbps, None);
        assert_eq!(summary.max_generation, None);
    }
}
