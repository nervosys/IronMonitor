// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Intel GPU monitoring via i915/xe drivers
//!
//! This module provides Intel GPU support using the DRM (Direct Rendering Manager)
//! interface, supporting both legacy i915 and modern xe drivers. It monitors:
//! - GPU utilization (render, video, video enhancement engines)
//! - Memory usage (system and stolen memory for iGPUs)
//! - Temperature and power
//! - Frequency and turbo states
//! - Process tracking via fdinfo
//!
//! Based on nvtop's extract_gpuinfo_intel.c implementation.

use crate::gpu::{
    Gpu, GpuClocks, GpuCollection, GpuDynamicInfo, GpuEngines, GpuMemory, GpuPower, GpuProcess,
    GpuStaticInfo, GpuThermal, GpuVendor, PcieLinkInfo,
};
// Only the Linux fdinfo path constructs a GpuProcess.
#[cfg(target_os = "linux")]
use crate::gpu::GpuProcessType;
use crate::Error;

#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::Path;

/// Intel GPU driver type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntelDriver {
    /// Legacy i915 driver
    I915,
    /// Modern Xe driver
    Xe,
}

/// Intel GPU implementation
pub struct IntelGpu {
    index: usize,
    name: String,
    /// `domain:bus:device.function`, or `None` when no address could be read.
    pci_bus_id: Option<String>,
    driver: IntelDriver,
    #[cfg(target_os = "linux")]
    card_path: String,
    #[cfg(target_os = "linux")]
    hwmon_path: Option<String>,
}

impl IntelGpu {
    /// Create new Intel GPU instance
    #[cfg(target_os = "linux")]
    pub fn new(
        index: usize,
        pci_bus_id: Option<String>,
        card_path: String,
        driver: IntelDriver,
    ) -> Result<Self, Error> {
        let device_path = format!("{}/device", card_path);

        // Try to read GPU name from product_name or derive from PCI ID
        let name = read_intel_gpu_name(&device_path, &driver)
            .unwrap_or_else(|| format!("Intel GPU {} ({})", index, driver.name()));

        // Find hwmon path
        let hwmon_path = find_hwmon_path(&device_path);

        Ok(Self {
            index,
            name,
            pci_bus_id,
            driver,
            card_path,
            hwmon_path,
        })
    }

    /// Create new Intel GPU instance (non-Linux stub)
    #[cfg(not(target_os = "linux"))]
    pub fn new(
        index: usize,
        pci_bus_id: Option<String>,
        driver: IntelDriver,
    ) -> Result<Self, Error> {
        let name = format!("Intel GPU {} ({})", index, driver.name());
        Ok(Self {
            index,
            name,
            pci_bus_id,
            driver,
        })
    }

    /// Get driver type
    pub fn driver(&self) -> IntelDriver {
        self.driver
    }
}

impl IntelDriver {
    /// Get driver name
    pub fn name(&self) -> &'static str {
        match self {
            IntelDriver::I915 => "i915",
            IntelDriver::Xe => "xe",
        }
    }
}

#[cfg(target_os = "linux")]
fn read_intel_gpu_name(device_path: &str, driver: &IntelDriver) -> Option<String> {
    // Try lspci-style parsing from PCI IDs
    let device = fs::read_to_string(format!("{}/device", device_path))
        .ok()
        .map(|s| s.trim().to_string())?;

    // Map common Intel GPU device IDs to names
    let name = match device.as_str() {
        // Integrated Graphics (common ones)
        "0x9a49" | "0x9a40" => "Intel UHD Graphics (Tiger Lake)",
        "0x46a6" | "0x46a8" => "Intel UHD Graphics (Alder Lake)",
        "0xa7a0" | "0xa7a1" => "Intel UHD Graphics (Raptor Lake)",
        "0x7d55" | "0x7d45" => "Intel UHD Graphics (Meteor Lake)",
        "0x5917" | "0x5912" => "Intel UHD Graphics 620 (Kaby Lake)",
        "0x3e92" | "0x3e91" => "Intel UHD Graphics 630 (Coffee Lake)",
        "0x8a52" | "0x8a56" => "Intel UHD Graphics (Ice Lake)",
        // Arc discrete GPUs
        "0x5690" | "0x5691" | "0x5692" => "Intel Arc A770",
        "0x5693" | "0x5694" => "Intel Arc A750",
        "0x56a0" | "0x56a1" => "Intel Arc A580",
        "0x5696" | "0x5697" => "Intel Arc A380",
        "0x56a5" | "0x56a6" => "Intel Arc A310",
        // Xe discrete
        "0x0bd0" | "0x0bd5" | "0x0bd6" | "0x0bd7" => "Intel Data Center GPU Max",
        _ => {
            // Generic name with driver info
            return Some(format!("Intel Graphics [{}] ({})", device, driver.name()));
        }
    };

    Some(name.to_string())
}

#[cfg(target_os = "linux")]
fn find_hwmon_path(device_path: &str) -> Option<String> {
    let hwmon_base = format!("{}/hwmon", device_path);
    if let Ok(entries) = fs::read_dir(&hwmon_base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }
    None
}

impl Gpu for IntelGpu {
    fn static_info(&self) -> Result<GpuStaticInfo, Error> {
        #[cfg(target_os = "linux")]
        {
            // Read driver version
            let driver_path = match self.driver {
                IntelDriver::I915 => "/sys/module/i915/version",
                IntelDriver::Xe => "/sys/module/xe/version",
            };
            let driver_version = fs::read_to_string(driver_path)
                .ok()
                .map(|s| s.trim().to_string());

            Ok(GpuStaticInfo {
                index: self.index,
                vendor: GpuVendor::Intel,
                name: self.name.clone(),
                pci_bus_id: self.pci_bus_id.clone(),
                uuid: None,
                vbios_version: None,
                driver_version,
                compute_capability: None,
                shader_cores: None,
                l2_cache: None,
                num_engines: None,
                // Not read. This used to be `!is_discrete`, a guess from the name ("arc").
                integrated: None,
            })
        }

        #[cfg(not(target_os = "linux"))]
        Ok(GpuStaticInfo {
            index: self.index,
            vendor: GpuVendor::Intel,
            name: self.name.clone(),
            pci_bus_id: self.pci_bus_id.clone(),
            uuid: None,
            vbios_version: None,
            // The driver's name is not its version. This used to be `Some(name)`.
            driver_version: None,
            compute_capability: None,
            shader_cores: None,
            l2_cache: None,
            num_engines: None,
            // Not read. This used to be a constant `true` for every Intel part, Arc included.
            integrated: None,
        })
    }

    fn dynamic_info(&self) -> Result<GpuDynamicInfo, Error> {
        #[cfg(target_os = "linux")]
        {
            let device_path = format!("{}/device", self.card_path);

            // Read GPU frequency
            let freq_path = match self.driver {
                IntelDriver::I915 => format!("{}/gt_cur_freq_mhz", device_path),
                IntelDriver::Xe => format!("{}/gt/gt0/freq0/cur_freq", device_path),
            };
            let graphics_clock = fs::read_to_string(&freq_path)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok());

            // Read max frequency
            let freq_max_path = match self.driver {
                IntelDriver::I915 => format!("{}/gt_max_freq_mhz", device_path),
                IntelDriver::Xe => format!("{}/gt/gt0/freq0/max_freq", device_path),
            };
            let graphics_max = fs::read_to_string(&freq_max_path)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok());

            // Utilization is not read on this path. This used to be the ratio
            // of the current clock to the maximum clock, labelled as utilization
            // and as the graphics engine's. A GPU at its maximum clock is not
            // necessarily busy, and a throttled one under full load reads low.
            // i915 PMU engine-busy counters would be the real source; nothing
            // reads them yet, so this is `None`.
            let utilization: Option<u8> = None;

            // Read power from hwmon (if available)
            let power_draw = self.hwmon_path.as_ref().and_then(|hwmon| {
                fs::read_to_string(format!("{}/power1_average", hwmon))
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .map(|uw| (uw / 1000) as u32)
            });

            // Read temperature from hwmon
            let temperature = self.hwmon_path.as_ref().and_then(|hwmon| {
                fs::read_to_string(format!("{}/temp1_input", hwmon))
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .map(|t| (t / 1000) as u32)
            });

            Ok(GpuDynamicInfo {
                utilization,
                // Intel integrated parts have no separate VRAM and expose no
                // memory figures in sysfs, so there is nothing to report --
                // which is not the same as reporting a card with no memory.
                memory: GpuMemory::unreported(),
                clocks: GpuClocks {
                    graphics: graphics_clock,
                    graphics_max,
                    memory: None,
                    memory_max: None,
                    sm: None,
                    video: None,
                },
                power: GpuPower {
                    draw: power_draw,
                    limit: None,
                    default_limit: None,
                    usage_percent: None,
                },
                thermal: GpuThermal {
                    temperature: temperature.map(|t| t as i32),
                    max_temperature: None,
                    critical_temperature: None,
                    fan_speed: None,
                    fan_rpm: None,
                },
                pcie: PcieLinkInfo {
                    current_gen: None,
                    max_gen: None,
                    current_width: None,
                    max_width: None,
                    current_speed: None,
                    max_speed: None,
                    tx_throughput: None,
                    rx_throughput: None,
                },
                engines: GpuEngines {
                    graphics: utilization,
                    compute: None,
                    encoder: None,
                    decoder: None,
                    copy: None,
                    vendor_specific: vec![],
                },
                processes: vec![],
            })
        }

        // Nothing is read on this platform, so nothing is reported.
        #[cfg(not(target_os = "linux"))]
        Ok(GpuDynamicInfo {
            utilization: None,
            memory: GpuMemory::unreported(),
            clocks: GpuClocks {
                graphics: None,
                graphics_max: None,
                memory: None,
                memory_max: None,
                sm: None,
                video: None,
            },
            power: GpuPower {
                draw: None,
                limit: None,
                default_limit: None,
                usage_percent: None,
            },
            thermal: GpuThermal {
                temperature: None,
                max_temperature: None,
                critical_temperature: None,
                fan_speed: None,
                fan_rpm: None,
            },
            pcie: PcieLinkInfo {
                current_gen: None,
                max_gen: None,
                current_width: None,
                max_width: None,
                current_speed: None,
                max_speed: None,
                tx_throughput: None,
                rx_throughput: None,
            },
            engines: GpuEngines {
                graphics: None,
                compute: None,
                encoder: None,
                decoder: None,
                copy: None,
                vendor_specific: vec![],
            },
            processes: vec![],
        })
    }

    fn vendor(&self) -> GpuVendor {
        GpuVendor::Intel
    }

    fn index(&self) -> usize {
        self.index
    }

    fn name(&self) -> Result<String, Error> {
        Ok(self.name.clone())
    }

    fn processes(&self) -> Result<Vec<GpuProcess>, Error> {
        #[cfg(target_os = "linux")]
        {
            parse_intel_fdinfo_processes(&self.card_path, &self.driver)
        }
        // The fdinfo reader is Linux-only. Elsewhere no list was read, which is
        // not the same as an empty one. This used to return `Ok(vec![])`.
        #[cfg(not(target_os = "linux"))]
        Err(Error::NotSupported(
            "GPU process lists are read from /proc/*/fdinfo, which exists only on Linux"
                .to_string(),
        ))
    }

    fn kill_process(&self, pid: u32) -> Result<(), Error> {
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            kill(Pid::from_raw(pid as i32), Signal::SIGTERM).map_err(|e| {
                Error::ProcessError(format!("Failed to kill process {}: {}", pid, e))
            })?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = pid;
            Err(Error::NotSupported(
                "Process killing not supported on this platform".to_string(),
            ))
        }
    }
}

/// Parse fdinfo for Intel GPU processes
#[cfg(target_os = "linux")]
fn parse_intel_fdinfo_processes(
    _card_path: &str,
    driver: &IntelDriver,
) -> Result<Vec<GpuProcess>, Error> {
    let mut processes = Vec::new();
    let proc_dir = Path::new("/proc");

    let driver_name = driver.name();

    // An unlistable /proc is a failed read, not a machine with no GPU clients.
    // This used to return an empty list.
    let proc_entries = fs::read_dir(proc_dir)
        .map_err(|e| Error::Other(format!("cannot list /proc for DRM clients: {e}")))?;
    for proc_entry in proc_entries.flatten() {
        let pid_str = proc_entry.file_name();
        let pid_str = pid_str.to_string_lossy();

        let pid: u32 = match pid_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let fdinfo_dir = proc_entry.path().join("fdinfo");
        if !fdinfo_dir.exists() {
            continue;
        }

        if let Ok(fdinfo_entries) = fs::read_dir(&fdinfo_dir) {
            for fdinfo_entry in fdinfo_entries.flatten() {
                if let Ok(content) = fs::read_to_string(fdinfo_entry.path()) {
                    // Check for i915 or xe driver
                    let driver_match = format!("drm-driver:\t{}", driver_name);
                    if content.contains(&driver_match) {
                        // Parse engine usage
                        let mut total_time = 0u64;

                        for line in content.lines() {
                            // Parse drm-engine-render, drm-engine-video, etc.
                            if line.starts_with("drm-engine-") {
                                if let Some(time) = parse_engine_time(line) {
                                    total_time += time;
                                }
                            }
                        }

                        if total_time > 0 {
                            let name = fs::read_to_string(proc_entry.path().join("comm"))
                                .map(|s| s.trim().to_string())
                                .unwrap_or_else(|_| format!("Process {}", pid));

                            processes.push(GpuProcess {
                                pid,
                                name,
                                user: String::new(),
                                process_type: GpuProcessType::Unknown,
                                gpu_usage: None,
                                // An Intel iGPU shares system memory, so there
                                // is no separate figure to report — `None` says
                                // that, where the previous `0` claimed the
                                // process used no memory at all.
                                memory_usage: None,
                                memory_usage_percent: None,
                                encoder_usage: None,
                                decoder_usage: None,
                                cpu_usage: None,
                                cpu_memory: None,
                            });
                        }
                        break;
                    }
                }
            }
        }
    }

    processes.sort_by_key(|p| p.pid);
    processes.dedup_by_key(|p| p.pid);

    Ok(processes)
}

#[cfg(target_os = "linux")]
fn parse_engine_time(line: &str) -> Option<u64> {
    // Parse "drm-engine-render:\t12345 ns"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        return parts[1].parse().ok();
    }
    None
}

/// Detect all Intel GPUs in the system
pub fn detect_gpus(collection: &mut GpuCollection) -> Result<(), Error> {
    #[cfg(target_os = "linux")]
    {
        let dri_path = Path::new("/sys/class/drm");

        if !dri_path.exists() {
            return Ok(());
        }

        let mut gpu_index = 0;

        let entries = fs::read_dir(dri_path)
            .map_err(|e| Error::GpuError(format!("failed to read /sys/class/drm: {}", e)))?;
        let mut cards: Vec<_> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("card") && !name.contains('-') {
                    Some((name, e.path()))
                } else {
                    None
                }
            })
            .collect();

        cards.sort_by(|a, b| a.0.cmp(&b.0));

        for (_card_name, card_path) in cards {
            let device_path = card_path.join("device");
            let driver_path = device_path.join("driver");

            if let Ok(driver_target) = fs::read_link(&driver_path) {
                let driver_name = driver_target
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");

                let driver = match driver_name {
                    "i915" => Some(IntelDriver::I915),
                    "xe" => Some(IntelDriver::Xe),
                    _ => None,
                };

                if let Some(driver) = driver {
                    // PCI bus ID from the device link's file name. A link that
                    // cannot be read is `None`, not the string "unknown".
                    let pci_bus_id = fs::read_link(&device_path).ok().and_then(|dev_link| {
                        dev_link
                            .file_name()
                            .and_then(|s| s.to_str())
                            .map(str::to_string)
                    });

                    let label = pci_bus_id
                        .clone()
                        .unwrap_or_else(|| card_path.to_string_lossy().to_string());
                    match IntelGpu::new(
                        gpu_index,
                        pci_bus_id,
                        card_path.to_string_lossy().to_string(),
                        driver,
                    ) {
                        Ok(gpu) => {
                            collection.add_gpu(Box::new(gpu));
                            gpu_index += 1;
                        }
                        Err(e) => {
                            // Record the failure instead of dropping the device. The index
                            // still advances so that later devices keep their indices.
                            collection
                                .detection_failures
                                .push(("intel", format!("{}: {}", label, e)));
                            gpu_index += 1;
                        }
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    {
        detect_intel_gpus_wmi(collection)?;
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = collection;
    }

    Ok(())
}

/// Detect Intel GPUs on Windows via WMI Win32_VideoController
#[cfg(windows)]
fn detect_intel_gpus_wmi(collection: &mut GpuCollection) -> Result<(), Error> {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    #[allow(dead_code)]
    struct Win32VideoController {
        name: Option<String>,
        adapter_r_a_m: Option<u64>,
        driver_version: Option<String>,
        video_processor: Option<String>,
        pnp_device_i_d: Option<String>,
        status: Option<String>,
    }

    let com = wmi::COMLibrary::new()
        .map_err(|e| Error::Other(format!("Failed to initialize COM: {}", e)))?;
    let wmi_con = wmi::WMIConnection::with_namespace_path("root\\CIMV2", com)
        .map_err(|e| Error::Other(format!("Failed to connect to WMI: {}", e)))?;

    let controllers: Vec<Win32VideoController> = wmi_con
        .raw_query("SELECT Name, AdapterRAM, DriverVersion, VideoProcessor, PNPDeviceID, Status FROM Win32_VideoController")
        .map_err(|e| Error::Other(format!("WMI Win32_VideoController query failed: {}", e)))?;

    let mut gpu_index = 0;
    for ctrl in &controllers {
        let name = ctrl.name.as_deref().unwrap_or("");
        let name_lower = name.to_lowercase();

        // Filter for Intel GPUs
        if !name_lower.contains("intel") {
            continue;
        }

        // Skip non-GPU Intel devices (e.g., Intel Management Engine)
        if name_lower.contains("management") || name_lower.contains("serial") {
            continue;
        }

        // WMI's PNPDeviceID is a PnP instance path, not a PCI address. This used
        // to be stored as the bus id. The address comes from the PCI enumerator,
        // and is `None` when Windows records no PCI location.
        let pnp_id = ctrl.pnp_device_i_d.as_deref().unwrap_or("");
        let pci_bus_id = crate::pci_devices::pci_address_of(pnp_id);
        let label = pci_bus_id.clone().unwrap_or_else(|| pnp_id.to_string());

        // A discrete Intel GPU sits behind a PCI bridge, so its bus is nonzero.
        // An integrated GPU is on the root complex at bus 0. `None` when Windows
        // records no PCI location. This used to be a name test ("arc",
        // "data center"), which guessed where the product name gave no answer.
        let is_discrete = is_discrete_from_pci_address(pci_bus_id.as_deref());

        // Determine driver type from name
        let driver = if name_lower.contains("arc") {
            IntelDriver::Xe
        } else {
            IntelDriver::I915
        };

        // A controller that fails is recorded and the rest are still read; the
        // index still advances so later adapters keep theirs.
        let mut gpu = match IntelGpu::new(gpu_index, pci_bus_id, driver) {
            Ok(gpu) => gpu,
            Err(e) => {
                collection
                    .detection_failures
                    .push(("intel", format!("{label}: {e}")));
                gpu_index += 1;
                continue;
            }
        };
        gpu.name = name.to_string();

        // Use DXGI for accurate VRAM and LUID (fixes 4GB WMI cap)
        let dxgi_adapters = super::windows_helpers::enumerate_dxgi_adapters();
        let dxgi_match =
            super::windows_helpers::find_dxgi_adapter(&dxgi_adapters, name).or_else(|| {
                super::windows_helpers::find_dxgi_adapter_by_vendor(&dxgi_adapters, 0x8086)
            });

        // Without DXGI there is no trustworthy capacity. WMI's AdapterRAM is a
        // 32-bit field that wraps for adapters of 4 GB and more, so it was
        // published as the total for cards it misreports. It is not used here.
        let (dedicated_video_memory, shared_system_memory, luid_filter) =
            if let Some(dxgi) = &dxgi_match {
                let luid = super::windows_helpers::format_luid(dxgi.luid_high, dxgi.luid_low);
                (
                    dxgi.dedicated_video_memory,
                    dxgi.shared_system_memory,
                    Some(luid),
                )
            } else {
                (0, 0, None)
            };

        collection.add_gpu(Box::new(WmiIntelGpu {
            inner: gpu,
            dedicated_video_memory,
            shared_system_memory,
            driver_version: ctrl.driver_version.clone(),
            is_discrete,
            luid_filter,
            gpu_name_hint: name.to_string(),
        }));
        gpu_index += 1;
    }

    Ok(())
}

/// Whether an adapter is discrete, from its PCI address (`dddd:bb:dd.f`). A
/// discrete Intel GPU sits behind a PCI bridge, so its bus is nonzero; an
/// integrated one is on the root complex at bus 0. `None` when no address was
/// read.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_discrete_from_pci_address(address: Option<&str>) -> Option<bool> {
    address
        .and_then(super::traits::PciInfo::parse_address)
        .map(|(_, bus, _, _)| bus != 0)
}

/// Intel GPU with WMI-sourced data on Windows
#[cfg(windows)]
struct WmiIntelGpu {
    inner: IntelGpu,
    /// Accurate VRAM from DXGI (64-bit, no 4GB cap). Falls back to WMI AdapterRAM.
    dedicated_video_memory: u64,
    /// Shared system memory from DXGI
    shared_system_memory: u64,
    driver_version: Option<String>,
    /// From the PCI bus of the adapter. `None` when no PCI location was read,
    /// and the memory figures that depend on it are then not reported.
    is_discrete: Option<bool>,
    /// LUID hex string for matching with GPU performance counters
    luid_filter: Option<String>,
    /// GPU name used as hint for OHM/LHM temperature lookup
    gpu_name_hint: String,
}

/// Windows GPU performance counter data
#[cfg(windows)]
#[derive(Debug, Default)]
struct WinIntelPerfData {
    /// `None` where no engine performance counter answered. See
    /// `windows_helpers::EngineUtilization::overall`.
    utilization: Option<u8>,
    dedicated_used: Option<u64>,
    shared_used: Option<u64>,
    temperature: Option<i32>,
    /// Per-engine utilization breakdown
    engines_graphics: Option<u8>,
    engines_compute: Option<u8>,
    engines_video_decode: Option<u8>,
    engines_video_encode: Option<u8>,
    engines_copy: Option<u8>,
}

#[cfg(windows)]
fn query_intel_gpu_perf_counters(
    adapter_name_filter: &str,
    luid_filter: Option<&str>,
) -> WinIntelPerfData {
    // Use shared helper for engine + memory data
    let perf = super::windows_helpers::query_gpu_perf_counters(luid_filter);

    // Use OHM/LHM for GPU-specific temperature
    let temperature = super::windows_helpers::query_gpu_temperature_ohm(adapter_name_filter);

    WinIntelPerfData {
        utilization: perf.engines.overall,
        dedicated_used: perf.dedicated_used,
        shared_used: perf.shared_used,
        temperature,
        engines_graphics: perf.engines.graphics,
        engines_compute: perf.engines.compute,
        engines_video_decode: perf.engines.video_decode,
        engines_video_encode: perf.engines.video_encode,
        engines_copy: perf.engines.copy,
    }
}

#[cfg(windows)]
impl Gpu for WmiIntelGpu {
    fn static_info(&self) -> Result<GpuStaticInfo, Error> {
        Ok(GpuStaticInfo {
            index: self.inner.index,
            vendor: GpuVendor::Intel,
            name: self.inner.name.clone(),
            pci_bus_id: self.inner.pci_bus_id.clone(),
            uuid: None,
            vbios_version: None,
            driver_version: self.driver_version.clone(),
            compute_capability: None,
            shader_cores: None,
            l2_cache: None,
            num_engines: None,
            // From the PCI bus, not the name. `None` when no location was read.
            integrated: self.is_discrete.map(|discrete| !discrete),
        })
    }

    fn dynamic_info(&self) -> Result<GpuDynamicInfo, Error> {
        // Query real-time GPU performance counters via shared helpers
        let perf = query_intel_gpu_perf_counters(&self.gpu_name_hint, self.luid_filter.as_deref());

        // For Intel iGPUs, memory is shared with system
        // For Arc discrete GPUs, we have dedicated VRAM
        // An iGPU's usage is shared plus dedicated, and a sum needs both: if
        // either counter did not answer, the total in use is not known.
        // Without a PCI location there is no way to tell the two kinds apart,
        // so no memory figure is reported rather than one picked by a guess.
        let mem_used = match self.is_discrete {
            Some(true) => perf.dedicated_used,
            Some(false) => match (perf.shared_used, perf.dedicated_used) {
                (Some(shared), Some(dedicated)) => Some(shared + dedicated),
                _ => None,
            },
            None => None,
        };
        // The final `else` fell back to *the used figure itself* as the
        // total, so an adapter whose capacity WMI did not report came out at
        // exactly 100% memory utilisation, permanently, on every scrape. The
        // used bytes are a real reading and are kept; the capacity is not
        // invented from them.
        let mem_total = match self.is_discrete {
            Some(true) => (self.dedicated_video_memory > 0).then_some(self.dedicated_video_memory),
            Some(false) => {
                if self.shared_system_memory > 0 {
                    Some(self.shared_system_memory)
                } else if self.dedicated_video_memory > 0 {
                    Some(self.dedicated_video_memory)
                } else {
                    None
                }
            }
            None => None,
        };
        let memory = match (mem_total, mem_used) {
            (Some(total), Some(used)) => GpuMemory::from_total_used(total, used),
            (total, used) => GpuMemory {
                total,
                used,
                free: None,
                utilization: None,
            },
        };

        Ok(GpuDynamicInfo {
            // Carried through rather than wrapped: `Some(0)` here was a
            // measured zero for a GPU whose counters never answered.
            utilization: perf.utilization,
            memory,
            clocks: GpuClocks {
                graphics: None,
                graphics_max: None,
                memory: None,
                memory_max: None,
                sm: None,
                video: None,
            },
            power: GpuPower {
                draw: None,
                limit: None,
                default_limit: None,
                usage_percent: None,
            },
            thermal: GpuThermal {
                temperature: perf.temperature,
                max_temperature: None,
                critical_temperature: None,
                fan_speed: None,
                fan_rpm: None,
            },
            pcie: PcieLinkInfo {
                current_gen: None,
                max_gen: None,
                current_width: None,
                max_width: None,
                current_speed: None,
                max_speed: None,
                tx_throughput: None,
                rx_throughput: None,
            },
            engines: GpuEngines {
                graphics: perf.engines_graphics,
                compute: perf.engines_compute,
                encoder: perf.engines_video_encode,
                decoder: perf.engines_video_decode,
                copy: perf.engines_copy,
                vendor_specific: vec![],
            },
            processes: vec![],
        })
    }

    fn vendor(&self) -> GpuVendor {
        GpuVendor::Intel
    }

    fn index(&self) -> usize {
        self.inner.index
    }

    fn name(&self) -> Result<String, Error> {
        Ok(self.inner.name.clone())
    }

    fn processes(&self) -> Result<Vec<GpuProcess>, Error> {
        // No process reader exists on this path. This used to return an empty
        // list, which users read as "no GPU processes".
        Err(Error::NotSupported(
            "GPU process lists are not read on Windows for Intel GPUs".to_string(),
        ))
    }

    fn kill_process(&self, _pid: u32) -> Result<(), Error> {
        Err(Error::NotSupported(
            "Process killing not supported on Windows for Intel GPUs".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discrete_is_read_from_the_pci_bus_not_the_name() {
        // Arc behind a root port sits on a nonzero bus; the iGPU is at 00:02.0.
        assert_eq!(
            is_discrete_from_pci_address(Some("0000:03:00.0")),
            Some(true)
        );
        assert_eq!(
            is_discrete_from_pci_address(Some("0000:00:02.0")),
            Some(false)
        );
        // No PCI location was read: no answer, not a guess.
        assert_eq!(is_discrete_from_pci_address(None), None);
        assert_eq!(is_discrete_from_pci_address(Some("PCI\\VEN_8086")), None);
    }

    #[test]
    fn test_intel_gpu_creation() {
        #[cfg(target_os = "linux")]
        {
            let result = IntelGpu::new(
                0,
                Some("0000:00:02.0".to_string()),
                "/sys/class/drm/card0".to_string(),
                IntelDriver::I915,
            );
            let _ = result;
        }
    }

    #[test]
    fn test_driver_name() {
        assert_eq!(IntelDriver::I915.name(), "i915");
        assert_eq!(IntelDriver::Xe.name(), "xe");
    }

    #[cfg(windows)]
    #[test]
    fn test_win_intel_perf_data_default() {
        let data = WinIntelPerfData::default();
        // `Default` means "nothing queried yet", which is the same thing a
        // failed query leaves behind -- so all three are absent, not zero.
        assert_eq!(data.utilization, None);
        assert_eq!(data.dedicated_used, None);
        assert_eq!(data.shared_used, None);
        assert!(data.temperature.is_none());
        assert!(data.engines_graphics.is_none());
        assert!(data.engines_compute.is_none());
        assert!(data.engines_video_decode.is_none());
        assert!(data.engines_video_encode.is_none());
        assert!(data.engines_copy.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn test_win_intel_perf_data_fields() {
        let data = WinIntelPerfData {
            utilization: Some(42),
            dedicated_used: Some(256 * 1024 * 1024),
            shared_used: Some(64 * 1024 * 1024),
            temperature: Some(55),
            engines_graphics: Some(40),
            engines_compute: Some(5),
            engines_video_decode: Some(20),
            engines_video_encode: Some(15),
            engines_copy: Some(2),
        };
        assert_eq!(data.utilization, Some(42));
        assert_eq!(data.dedicated_used, Some(256 * 1024 * 1024));
        assert_eq!(data.shared_used, Some(64 * 1024 * 1024));
        assert_eq!(data.temperature, Some(55));
        assert_eq!(data.engines_graphics, Some(40));
        assert_eq!(data.engines_video_decode, Some(20));
    }
}
