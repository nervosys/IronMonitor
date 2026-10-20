//! Unified silicon monitoring module
//!
//! This module provides comprehensive monitoring for all types of silicon:
//! - CPUs (including hybrid architectures like P/E cores)
//! - NPUs/ASICs (Neural engines, AI accelerators)
//! - I/O controllers (PCIe, NVMe, USB, Thunderbolt)
//! - Network silicon (WiFi, Ethernet, offload engines)

#[cfg(feature = "apple")]
pub mod apple;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

use crate::error::Result;

/// CPU cluster type (for hybrid architectures)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuClusterType {
    /// Performance cores (P-cores)
    Performance,
    /// Efficiency cores (E-cores)
    Efficiency,
    /// Standard cores (no hybrid architecture)
    Standard,
}

/// Mean frequency over the cores that reported one, or `None` when none did.
///
/// Averaging a list where an unread core contributes zero drags the figure
/// toward zero in proportion to how much was unreadable, and reports the
/// result as a measurement.
pub fn average_reported_mhz(cores: &[CpuCore]) -> Option<u32> {
    let reported: Vec<u32> = cores.iter().filter_map(|c| c.frequency_mhz).collect();
    if reported.is_empty() {
        return None;
    }
    Some(reported.iter().sum::<u32>() / reported.len() as u32)
}

/// Mean utilization over the cores that reported one, or `None` when none did.
pub fn average_reported_util(cores: &[CpuCore]) -> Option<u8> {
    let reported: Vec<u32> = cores
        .iter()
        .filter_map(|c| c.utilization.map(u32::from))
        .collect();
    if reported.is_empty() {
        return None;
    }
    Some((reported.iter().sum::<u32>() / reported.len() as u32) as u8)
}

/// Per-core CPU information
#[derive(Debug, Clone)]
pub struct CpuCore {
    /// Core ID
    pub id: u32,
    /// Cluster type
    pub cluster: CpuClusterType,
    /// Current frequency in MHz, or `None` where it was not measured.
    ///
    /// This was `u32`. On Windows the reader below calls the same
    /// `CallNtPowerInformation(ProcessorInformation)` as
    /// `platform::windows::get_cpu_frequency`, which returns the *nominal*
    /// clock — 4400 for every core of a 9900X, whatever it is doing. That was
    /// fixed there and not here, because the fix was applied where the defect
    /// was seen rather than where it is. Two readers, one API, one lie.
    pub frequency_mhz: Option<u32>,
    /// Per-core utilization, or `None` where no per-core figure was read.
    ///
    /// Windows had no per-core source wired: `read_cpu_utilization` called
    /// `GetSystemTimes`, which is **system-wide**, and copied that one number
    /// into every core's entry. That is the macOS defect from `24a7314` — "the
    /// system-wide figure repeated across every core" — on a second platform,
    /// in a second module. Linux reads real per-cpu lines from `/proc/stat`
    /// and is unaffected.
    pub utilization: Option<u8>,
    /// Temperature in Celsius (if available)
    pub temperature: Option<i32>,
}

/// CPU cluster information
#[derive(Debug, Clone)]
pub struct CpuCluster {
    /// Cluster type
    pub cluster_type: CpuClusterType,
    /// Core IDs in this cluster
    pub core_ids: Vec<u32>,
    /// Average frequency in MHz over the cores that reported one, or `None`
    /// when none did. See [`CpuCore::frequency_mhz`].
    pub frequency_mhz: Option<u32>,
    /// Average utilization over the cores that reported one, or `None`.
    pub utilization: Option<u8>,
    /// Power consumption in watts (if available)
    pub power_watts: Option<f32>,
}

/// NPU/Neural Engine information
#[derive(Debug, Clone)]
pub struct NpuInfo {
    /// NPU name (e.g., "Apple Neural Engine", "Intel AI Boost")
    pub name: String,
    /// Vendor
    pub vendor: String,
    /// Core count, where it was read.
    ///
    /// Never inferred from the vendor. This carried `Some(16)` for any Intel
    /// NPU, `Some(8)` for Qualcomm, `Some(16)` for every Apple Neural Engine
    /// and `Some(128)` for a TPU, under comments reading "~16 compute units",
    /// "Most Apple Silicon has" and "Typical TPU core count" — three
    /// admissions of a guess beside a published value. The vendor those
    /// numbers keyed off was itself a substring match on the device name.
    pub cores: Option<u32>,
    /// Utilization percentage (0-100), or `None` where it was not read.
    ///
    /// This was `u8` and every implementation wrote `0`, each beside a comment
    /// saying the figure needs a vendor-specific API — Windows, Linux TPU and
    /// the generic Linux path alike. An NPU at 0% and an NPU nothing can
    /// measure are different facts.
    pub utilization: Option<u8>,
    /// Power consumption in watts (if available)
    pub power_watts: Option<f32>,
    /// Frequency in MHz (if available)
    pub frequency_mhz: Option<u32>,
}

/// I/O controller information
#[derive(Debug, Clone)]
pub struct IoController {
    /// Controller type (e.g., "PCIe", "NVMe", "USB", "Thunderbolt")
    pub controller_type: String,
    /// Controller name
    pub name: String,
    /// Current bandwidth in MB/s, or `None` where no traffic counter is read.
    ///
    /// The Linux USB, Thunderbolt and SATA paths each wrote `0.0` beside a
    /// comment saying the figure "would need USB traffic monitoring" — three
    /// admissions of an unread value published as an idle bus.
    pub bandwidth_mbps: Option<f64>,
    /// Theoretical maximum bandwidth in MB/s, where it was derived from the
    /// device's actual link.
    ///
    /// Only the Linux NVMe path derives one, from `max_link_speed` and
    /// `max_link_width` in sysfs. Everything else assumed the fastest variant
    /// of its class and published that as this device's ceiling: 3500 for every
    /// disk on Windows regardless of bus, 2500 for any USB controller (USB 3.2
    /// Gen 2x2, when a USB 3.0 controller is 500), 7000 and 5000 on Apple. A
    /// SATA SSD given a 3500 MB/s ceiling is wrong by about six times.
    ///
    /// `None` where nothing about the actual link was read.
    pub max_bandwidth_mbps: Option<f64>,
    /// Power consumption in watts (if available)
    pub power_watts: Option<f32>,
}

/// Network silicon information
#[derive(Debug, Clone)]
pub struct NetworkSilicon {
    /// Interface name (e.g., "WiFi", "Ethernet")
    pub interface: String,
    /// Link speed in Mbps
    /// Negotiated link speed in Mbps, where it was read.
    ///
    /// Never estimated. macOS filled this from `estimate_link_speed`, which
    /// guessed from the interface *name* — `en0` became 1200 ("WiFi 6 ~1.2
    /// Gbps"), any other `en*` or `bridge*` became 1000, everything else 100.
    /// `en0` is not always WiFi and a 10GbE port is not 1000. Linux left it at
    /// zero for a down interface and for the wireless path, under a comment
    /// reading "Would need iwconfig/nl80211".
    pub link_speed_mbps: Option<u32>,
    /// RX bandwidth in MB/s. `None` where no rate was measured: one sample of a
    /// counter cannot give a rate, and Linux and macOS expose only totals since
    /// boot.
    pub rx_bandwidth_mbps: Option<f64>,
    /// TX bandwidth in MB/s. `None` where no rate was measured: one sample of a
    /// counter cannot give a rate, and Linux and macOS expose only totals since
    /// boot.
    pub tx_bandwidth_mbps: Option<f64>,
    /// Packet rate (packets/sec). `None` where no rate was measured: a packet
    /// counter is cumulative since boot, and one sample cannot give a rate.
    pub packet_rate: Option<u64>,
    /// Power state (if available)
    pub power_state: Option<String>,
}

/// Silicon monitor trait
pub trait SiliconMonitor {
    /// Get CPU information (cores and clusters)
    fn cpu_info(&self) -> Result<(Vec<CpuCore>, Vec<CpuCluster>)>;

    /// Get NPU information
    fn npu_info(&self) -> Result<Vec<NpuInfo>>;

    /// Get I/O controller information
    fn io_info(&self) -> Result<Vec<IoController>>;

    /// Get network silicon information
    fn network_info(&self) -> Result<Vec<NetworkSilicon>>;
}
