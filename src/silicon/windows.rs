//! Windows silicon monitoring
//!
//! Comprehensive hardware monitoring for Windows using WMI and Performance Counters

use super::*;
use crate::error::Result;
use std::collections::HashMap;

#[cfg(target_os = "windows")]
use serde::Deserialize;
#[cfg(target_os = "windows")]
use wmi::{COMLibrary, WMIConnection};

#[cfg(target_os = "windows")]
use ::windows::Win32::System::SystemInformation::*;

// WMI structures for CPU temperature and model information

/// MSAcpi_ThermalZoneTemperature from root\WMI namespace
/// Win32_Processor for CPU model detection
#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32Processor {
    name: Option<String>,
}

// Global state for per-core utilization tracking

/// Windows silicon monitor
pub struct WindowsSiliconMonitor {
    cpu_count: usize,
}

impl WindowsSiliconMonitor {
    /// Create a new Windows silicon monitor
    pub fn new() -> Result<Self> {
        let cpu_count = Self::detect_cpu_count();

        Ok(Self { cpu_count })
    }

    #[cfg(target_os = "windows")]
    fn detect_cpu_count() -> usize {
        unsafe {
            let mut system_info: SYSTEM_INFO = std::mem::zeroed();
            GetSystemInfo(&mut system_info);
            system_info.dwNumberOfProcessors as usize
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_cpu_count() -> usize {
        1
    }

    /// Detect base CPU frequency from registry
    #[cfg(target_os = "windows")]
    #[cfg(not(target_os = "windows"))]
    fn detect_base_frequency() -> u32 {
        0
    }

    /// Read current CPU frequency using CallNtPowerInformation
    #[cfg(target_os = "windows")]
    fn read_cpu_frequencies(&self) -> Vec<Option<u32>> {
        use std::mem;

        // PROCESSOR_POWER_INFORMATION structure
        #[repr(C)]
        #[derive(Clone, Default)]
        struct ProcessorPowerInformation {
            number: u32,
            max_mhz: u32,
            current_mhz: u32,
            mhz_limit: u32,
            max_idle_state: u32,
            current_idle_state: u32,
        }

        // PowerInformationLevel::ProcessorInformation = 11
        const PROCESSOR_INFORMATION: u32 = 11;

        #[link(name = "powrprof")]
        extern "system" {
            fn CallNtPowerInformation(
                InformationLevel: u32,
                InputBuffer: *const std::ffi::c_void,
                InputBufferLength: u32,
                OutputBuffer: *mut std::ffi::c_void,
                OutputBufferLength: u32,
            ) -> i32;
        }

        let buffer_size = self.cpu_count * mem::size_of::<ProcessorPowerInformation>();
        let mut buffer: Vec<ProcessorPowerInformation> =
            vec![ProcessorPowerInformation::default(); self.cpu_count];

        let result = unsafe {
            CallNtPowerInformation(
                PROCESSOR_INFORMATION,
                std::ptr::null(),
                0,
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                buffer_size as u32,
            )
        };

        if result == 0 {
            // STATUS_SUCCESS. `current_mhz` equal to `max_mhz` is the nominal
            // clock, not a measurement — Windows reports the former through
            // this API and has no unprivileged way to give the latter.
            buffer
                .iter()
                .map(|p| (p.current_mhz != p.max_mhz).then_some(p.current_mhz))
                .collect()
        } else {
            // The call failed. Filling the vector with the base frequency
            // published a nominal figure as each core's current one.
            vec![None; self.cpu_count]
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn read_cpu_frequencies(&self) -> Vec<Option<u32>> {
        vec![None; self.cpu_count]
    }
    // `read_cpu_utilization_percent` and its two `AtomicU64` statics were here.
    //
    // They sampled `GetSystemTimes` and differenced it — a whole-machine
    // figure, correct in shape — and reported 100% on an idle desktop where
    // `core::cpu` read 7.3% at the same moment. Probing the API directly over
    // the same window gives deltas that check out (kernel+user = 12.0s across
    // 24 cores in 500ms) and an idle delta implying ~35%, so the arithmetic is
    // not obviously at fault and the disagreement was never explained.
    //
    // Removed rather than left behind an `#[allow(dead_code)]`: a reader
    // nothing calls, which disagreed with a known-good source by an order of
    // magnitude, is not an asset. `core::cpu` reads this correctly.
    /// Per-core CPU temperature. Always `None` on Windows.
    ///
    /// The only source wired here was `MSAcpi_ThermalZoneTemperature`, which has
    /// one row per ACPI thermal zone. A zone is a board sensor, not a core, and
    /// nothing in the table says which core it belongs to. The reader used to
    /// return the zone at index N as core N's temperature, and a single zone as
    /// the temperature of every core. Both were invented per-core values.
    fn read_cpu_temperature(&self, _cpu_id: u32) -> Option<i32> {
        None
    }

    /// Read CPU utilization using Performance Counters
    /// Per-core utilization. Empty on Windows: there is no per-core source
    /// wired here.
    ///
    /// This called `read_cpu_utilization_percent`, a **system-wide**
    /// `GetSystemTimes` figure, and copied it into every core's entry — so a
    /// 24-core machine reported the same number 24 times as though each had
    /// been measured. That is the macOS defect from `24a7314` on a second
    /// platform, in a second module. The real per-core figures exist in
    /// `core::cpu`, which this module does not use; until it does, each core
    /// reports `None`.
    fn read_cpu_utilization(&self) -> HashMap<u32, u8> {
        HashMap::new()
    }

    /// Determine if CPU has hybrid architecture (P+E cores)
    /// Intel 12th gen and later support this on Windows
    /// AMD Ryzen 9000 series with Zen 5 also has hybrid (CCD-based)
    #[allow(dead_code)]
    #[cfg(target_os = "windows")]
    fn has_hybrid_architecture(&self) -> bool {
        Self::query_cpu_model()
            .map(|name| {
                let name_lower = name.to_lowercase();
                // Intel hybrid: 12th Gen (Alder Lake) and later
                name_lower.contains("12th gen")
                    || name_lower.contains("13th gen")
                    || name_lower.contains("14th gen")
                    || name_lower.contains("core ultra")
                    // AMD Ryzen with heterogeneous CCDs
                    || (name_lower.contains("ryzen") && name_lower.contains("9"))
            })
            .unwrap_or(false)
    }

    #[cfg(target_os = "windows")]
    fn query_cpu_model() -> Option<String> {
        let com = COMLibrary::new().ok()?;
        let wmi = WMIConnection::new(com).ok()?;
        let procs: Vec<Win32Processor> = wmi.query().ok()?;
        procs.first().and_then(|p| p.name.clone())
    }

    #[allow(dead_code)]
    #[cfg(not(target_os = "windows"))]
    fn has_hybrid_architecture(&self) -> bool {
        false
    }

    /// Determine cluster type for a core
    #[cfg(target_os = "windows")]
    fn determine_cluster_type(&self, cpu_id: u32) -> CpuClusterType {
        // Use GetSystemCpuSetInformation to detect P/E cores on Windows 10+
        use std::sync::OnceLock;
        static CORE_TYPES: OnceLock<Vec<CpuClusterType>> = OnceLock::new();

        let types = CORE_TYPES.get_or_init(|| {
            Self::detect_core_types()
                .unwrap_or_else(|| vec![CpuClusterType::Standard; self.cpu_count])
        });

        types
            .get(cpu_id as usize)
            .copied()
            .unwrap_or(CpuClusterType::Standard)
    }

    #[cfg(target_os = "windows")]
    fn detect_core_types() -> Option<Vec<CpuClusterType>> {
        use ::windows::Win32::Foundation::HANDLE;
        use ::windows::Win32::System::SystemInformation::{
            GetSystemCpuSetInformation, SYSTEM_CPU_SET_INFORMATION,
        };

        // First call to get required buffer size
        let mut length: u32 = 0;
        unsafe {
            let _ = GetSystemCpuSetInformation(None, 0, &mut length, HANDLE::default(), 0);
        }

        if length == 0 {
            return None;
        }

        // Allocate buffer and get info
        let count = length as usize / std::mem::size_of::<SYSTEM_CPU_SET_INFORMATION>();
        let mut buffer: Vec<SYSTEM_CPU_SET_INFORMATION> =
            vec![unsafe { std::mem::zeroed() }; count];

        let result = unsafe {
            GetSystemCpuSetInformation(
                Some(buffer.as_mut_ptr()),
                length,
                &mut length,
                HANDLE::default(),
                0,
            )
        };

        if !result.as_bool() {
            return None;
        }

        // Parse the CPU set information to detect efficiency cores
        let mut types = Vec::new();
        for info in &buffer {
            // Check EfficiencyClass: 0 = Performance, 1 = Efficiency
            let cluster_type = unsafe {
                if info.Anonymous.CpuSet.EfficiencyClass > 0 {
                    CpuClusterType::Efficiency // E-core
                } else {
                    CpuClusterType::Performance // P-core
                }
            };
            types.push(cluster_type);
        }

        if types.is_empty() {
            None
        } else {
            Some(types)
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn determine_cluster_type(&self, _cpu_id: u32) -> CpuClusterType {
        CpuClusterType::Standard
    }
}

impl SiliconMonitor for WindowsSiliconMonitor {
    fn cpu_info(&self) -> Result<(Vec<CpuCore>, Vec<CpuCluster>)> {
        let mut cores = Vec::new();
        let utilization_map = self.read_cpu_utilization();
        let frequencies = self.read_cpu_frequencies();

        for cpu_id in 0..self.cpu_count as u32 {
            let cluster = self.determine_cluster_type(cpu_id);
            let frequency = frequencies.get(cpu_id as usize).copied().flatten();
            let utilization = utilization_map.get(&cpu_id).copied();
            let temperature = self.read_cpu_temperature(cpu_id);

            cores.push(CpuCore {
                id: cpu_id,
                cluster,
                frequency_mhz: frequency,
                utilization,
                temperature,
            });
        }

        // Calculate cluster averages
        // Average over the cores that reported a frequency; `None` when none
        // did, rather than an average of zeros.
        let reported: Vec<u32> = cores.iter().filter_map(|c| c.frequency_mhz).collect();
        let avg_freq = if reported.is_empty() {
            None
        } else {
            Some(reported.iter().sum::<u32>() / reported.len() as u32)
        };

        let avg_util = crate::silicon::average_reported_util(&cores);

        let clusters = vec![CpuCluster {
            cluster_type: CpuClusterType::Standard,
            core_ids: Some((0..self.cpu_count as u32).collect()),
            frequency_mhz: avg_freq,
            utilization: avg_util,
            power_watts: None,
        }];

        Ok((cores, clusters))
    }

    fn npu_info(&self) -> Result<Vec<NpuInfo>> {
        // Detect Intel AI Boost (NPU) or other NPUs via WMI
        #[cfg(target_os = "windows")]
        {
            let mut npus = Vec::new();

            // A WMI failure is an error, not an empty list. It used to be swallowed
            // by `if let Ok`, so a query that never ran read as "no NPU". The error
            // now reaches the ontology, which reports the NPU domain as unreadable.
            let com = wmi::COMLibrary::new().map_err(|e| {
                crate::error::IronError::System(format!("NPU query: COM did not start: {e}"))
            })?;
            let wmi_conn = wmi::WMIConnection::new(com).map_err(|e| {
                crate::error::IronError::System(format!("NPU query: WMI did not connect: {e}"))
            })?;
            #[derive(serde::Deserialize, Debug)]
            #[serde(rename_all = "PascalCase")]
            struct PnpDevice {
                name: Option<String>,
                manufacturer: Option<String>,
                #[allow(dead_code)]
                status: Option<String>,
            }

            // Query specifically for NPU device classes:
            //   - "Processing accelerators" is the correct Windows class for NPUs (Win11+)
            //   - Fall back to "System" class devices with NPU-related names
            // This excludes USB, HID, Bluetooth, and other unrelated device classes.
            let devices = wmi_conn
                .raw_query::<PnpDevice>(
                    // `PNPClass` is filtered on but not selected: the WHERE clause
                    // already guarantees which class each row belongs to.
                    "SELECT Name, Manufacturer, Status FROM Win32_PnPEntity WHERE \
                         PNPClass = 'Processing accelerators' OR \
                         (PNPClass = 'System' AND \
                          (Name LIKE '%NPU%' OR Name LIKE '%Neural%' OR \
                           Name LIKE '%AI Boost%' OR Name LIKE '%AI Accelerator%'))",
                )
                .map_err(|e| {
                    crate::error::IronError::System(format!("NPU query: WMI read failed: {e}"))
                })?;
            for device in devices {
                if let Some(name) = device.name {
                    // The vendor is the WMI Manufacturer, or unknown.
                    // It used to be inferred from the device name
                    // ("intel", "qualcomm") when Manufacturer was
                    // absent, and the name is not the vendor.
                    let vendor = device.manufacturer.filter(|m| !m.trim().is_empty());
                    // No core count is read here. It used to be
                    // guessed from `vendor`, which is itself a
                    // substring match on the device name when the
                    // manufacturer string is absent — a guess keyed
                    // off a guess.
                    let cores = None;
                    npus.push(NpuInfo {
                        name,
                        vendor,
                        cores,
                        // Needs a vendor-specific API; 0 was not a reading.
                        utilization: None,
                        power_watts: None,
                        frequency_mhz: None,
                    });
                }
            }

            Ok(npus)
        }

        #[cfg(not(target_os = "windows"))]
        Ok(Vec::new())
    }

    fn io_info(&self) -> Result<Vec<IoController>> {
        let mut controllers = Vec::new();

        // Use WMI to get disk I/O performance data
        if let Ok(com) = wmi::COMLibrary::new() {
            if let Ok(wmi_conn) = wmi::WMIConnection::new(com) {
                // Query disk I/O performance
                #[derive(serde::Deserialize, Debug)]
                #[serde(rename_all = "PascalCase")]
                struct DiskPerf {
                    name: Option<String>,
                    disk_read_bytes_per_sec: Option<u64>,
                    disk_write_bytes_per_sec: Option<u64>,
                }

                if let Ok(disks) = wmi_conn.raw_query::<DiskPerf>(
                    "SELECT Name, DiskReadBytesPerSec, DiskWriteBytesPerSec FROM Win32_PerfFormattedData_PerfDisk_PhysicalDisk",
                ) {
                    for disk in disks {
                        if let Some(name) = disk.name {
                            // Skip the _Total aggregate
                            if name == "_Total" {
                                continue;
                            }

                            // A counter that reported nothing is not a disk
                            // doing no I/O; sum only what was reported.
                            let reported = [
                                disk.disk_read_bytes_per_sec,
                                disk.disk_write_bytes_per_sec,
                            ];
                            let bandwidth = if reported.iter().all(Option::is_none) {
                                None
                            } else {
                                Some(
                                    reported.iter().flatten().sum::<u64>() as f64
                                        / (1024.0 * 1024.0),
                                )
                            };

                            // These are `Win32_PerfFormattedData_PerfDisk_
                            // PhysicalDisk` instances, whose Name is "0 C:" —
                            // a disk index and drive letters, never a model
                            // string. So the two branches below cannot match
                            // in practice, and the type is "Unknown" rather than
                            // a label that merely describes every row. It used
                            // to be "Storage" for all of them.
                            let controller_type = if name.contains("NVMe") {
                                "NVMe"
                            } else if name.contains("SSD") {
                                "SATA SSD"
                            } else {
                                "Unknown"
                            }
                            .to_string();

                            controllers.push(IoController {
                                controller_type,
                                name: name.clone(),
                                bandwidth_mbps: bandwidth,
                                // 3500 was assumed for every disk regardless of
                                // bus — wrong by about six times for a SATA SSD.
                                // Nothing about this disk's link is read.
                                max_bandwidth_mbps: None,
                                power_watts: None,
                            });
                        }
                    }
                }
            }
        }

        Ok(controllers)
    }

    fn network_info(&self) -> Result<Vec<NetworkSilicon>> {
        let mut networks = Vec::new();

        // Use WMI to get network interface performance
        if let Ok(com) = wmi::COMLibrary::new() {
            if let Ok(wmi_conn) = wmi::WMIConnection::new(com) {
                // Query network adapter status
                #[derive(serde::Deserialize, Debug)]
                #[serde(rename_all = "PascalCase")]
                #[allow(dead_code)]
                struct NetAdapter {
                    name: Option<String>,
                    speed: Option<u64>,
                    net_connection_status: Option<u16>,
                }

                // Query network performance data
                #[derive(serde::Deserialize, Debug)]
                #[serde(rename_all = "PascalCase")]
                struct NetPerf {
                    name: Option<String>,
                    bytes_received_per_sec: Option<u64>,
                    bytes_sent_per_sec: Option<u64>,
                    packets_per_sec: Option<u64>,
                    current_bandwidth: Option<u64>,
                }

                // Get adapters first
                let mut adapter_speeds: HashMap<String, u64> = HashMap::new();
                if let Ok(adapters) = wmi_conn.raw_query::<NetAdapter>(
                    "SELECT Name, Speed, NetConnectionStatus FROM Win32_NetworkAdapter WHERE NetConnectionStatus = 2",
                ) {
                    for adapter in adapters {
                        if let (Some(name), Some(speed)) = (adapter.name, adapter.speed) {
                            adapter_speeds.insert(name, speed);
                        }
                    }
                }

                // Get performance data
                if let Ok(perfs) = wmi_conn.raw_query::<NetPerf>(
                    "SELECT Name, BytesReceivedPerSec, BytesSentPerSec, PacketsPerSec, CurrentBandwidth FROM Win32_PerfFormattedData_Tcpip_NetworkInterface",
                ) {
                    for perf in perfs {
                        if let Some(name) = perf.name {
                            // Skip loopback and virtual adapters
                            if name.contains("Loopback") || name.contains("Virtual") || name.contains("Hyper-V") {
                                continue;
                            }

                            // These WMI PerfFormattedData counters are per-second rates, so a
                            // value that is present is a reading; an absent one stays None.
                            let rx_mbps = perf.bytes_received_per_sec.map(|b| b as f64 / (1024.0 * 1024.0));
                            let tx_mbps = perf.bytes_sent_per_sec.map(|b| b as f64 / (1024.0 * 1024.0));
                            let packets = perf.packets_per_sec;
                            // `CurrentBandwidth` absent is not a link at
                            // zero megabits; the counter simply did not
                            // report one for this interface.
                            let link_speed_mbps = perf
                                .current_bandwidth
                                .filter(|b| *b > 0)
                                .map(|b| (b / 1_000_000) as u32);

                            // Determine interface type from name
                            let interface = if name.contains("Wi-Fi") || name.contains("Wireless") || name.contains("WLAN") {
                                "WiFi"
                            } else if name.contains("Bluetooth") {
                                "Bluetooth"
                            } else if name.contains("10Gbit") || name.contains("10GbE") {
                                "10GbE"
                            } else {
                                "Ethernet"
                            }.to_string();

                            networks.push(NetworkSilicon {
                                interface,
                                link_speed_mbps,
                                rx_bandwidth_mbps: rx_mbps,
                                tx_bandwidth_mbps: tx_mbps,
                                packet_rate: packets,
                                power_state: None,
                            });
                        }
                    }
                }
            }
        }

        Ok(networks)
    }
}
