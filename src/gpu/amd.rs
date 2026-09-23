// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! AMD GPU monitoring via amdgpu driver and libdrm
//!
//! This module provides AMD GPU support using the DRM (Direct Rendering Manager)
//! interface, specifically the amdgpu kernel driver. It monitors:
//! - GPU utilization (graphics, compute engines)
//! - VRAM usage
//! - Temperature and fan control
//! - Power consumption
//! - Video encode/decode engines (VCN)
//! - Process tracking via fdinfo
//!
//! Based on nvtop's extract_gpuinfo_amdgpu.c implementation.

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

/// AMD GPU implementation
pub struct AmdGpu {
    index: usize,
    name: String,
    pci_bus_id: String,
    #[cfg(target_os = "linux")]
    card_path: String,
    #[cfg(target_os = "linux")]
    hwmon_path: Option<String>,
}

impl AmdGpu {
    /// Create new AMD GPU instance
    #[cfg(target_os = "linux")]
    pub fn new(index: usize, pci_bus_id: String, card_path: String) -> Result<Self, Error> {
        let device_path = format!("{}/device", card_path);

        // Try to read GPU name from product_name or uevent
        let name = read_gpu_name(&device_path).unwrap_or_else(|| format!("AMD GPU {}", index));

        // Find hwmon path
        let hwmon_path = find_hwmon_path(&device_path);

        Ok(Self {
            index,
            name,
            pci_bus_id,
            card_path,
            hwmon_path,
        })
    }

    /// Create new AMD GPU instance (non-Linux stub)
    #[cfg(not(target_os = "linux"))]
    pub fn new(index: usize, pci_bus_id: String) -> Result<Self, Error> {
        let name = format!("AMD GPU {}", index);
        Ok(Self {
            index,
            name,
            pci_bus_id,
        })
    }
}

#[cfg(target_os = "linux")]
fn read_gpu_name(device_path: &str) -> Option<String> {
    // Try product_name first (newer drivers)
    if let Ok(name) = fs::read_to_string(format!("{}/product_name", device_path)) {
        let name = name.trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    // Try parsing uevent for DEVNAME
    if let Ok(uevent) = fs::read_to_string(format!("{}/uevent", device_path)) {
        for line in uevent.lines() {
            if line.starts_with("PCI_ID=") {
                let pci_id = line.trim_start_matches("PCI_ID=");
                return Some(format!("AMD GPU [{}]", pci_id));
            }
        }
    }

    // Try reading device/vendor from PCI
    let vendor = fs::read_to_string(format!("{}/vendor", device_path))
        .map(|s| s.trim().to_string())
        .ok();
    let device = fs::read_to_string(format!("{}/device", device_path))
        .map(|s| s.trim().to_string())
        .ok();

    if let (Some(v), Some(d)) = (vendor, device) {
        return Some(format!("AMD GPU [{}:{}]", v, d));
    }

    None
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

impl Gpu for AmdGpu {
    fn static_info(&self) -> Result<GpuStaticInfo, Error> {
        #[cfg(target_os = "linux")]
        {
            let device_path = format!("{}/device", self.card_path);

            // Read VBIOS version
            let vbios_version = fs::read_to_string(format!("{}/vbios_version", device_path))
                .ok()
                .map(|s| s.trim().to_string());

            // Read driver version
            let driver_version = fs::read_to_string("/sys/module/amdgpu/version")
                .ok()
                .map(|s| s.trim().to_string());

            // Read EU/shader info if available
            let shader_cores = fs::read_to_string(format!("{}/gpu_busy_percent", device_path))
                .ok()
                .and({
                    // This indicates the file exists, but we need a different approach for EU count
                    // Try reading from debugfs or another source
                    None
                });

            Ok(GpuStaticInfo {
                index: self.index,
                vendor: GpuVendor::Amd,
                name: self.name.clone(),
                pci_bus_id: Some(self.pci_bus_id.clone()),
                uuid: None,
                vbios_version,
                driver_version,
                compute_capability: None,
                shader_cores,
                l2_cache: None,
                num_engines: None,
                integrated: self.name.to_lowercase().contains("vega")
                    || self.name.to_lowercase().contains("integrated")
                    || self.name.to_lowercase().contains("renoir")
                    || self.name.to_lowercase().contains("cezanne")
                    || self.name.to_lowercase().contains("phoenix"),
            })
        }

        #[cfg(not(target_os = "linux"))]
        Ok(GpuStaticInfo {
            index: self.index,
            vendor: GpuVendor::Amd,
            name: self.name.clone(),
            pci_bus_id: Some(self.pci_bus_id.clone()),
            uuid: None,
            vbios_version: None,
            driver_version: None,
            compute_capability: None,
            shader_cores: None,
            l2_cache: None,
            num_engines: None,
            integrated: false,
        })
    }

    fn dynamic_info(&self) -> Result<GpuDynamicInfo, Error> {
        #[cfg(target_os = "linux")]
        {
            let device_path = format!("{}/device", self.card_path);

            // Read GPU utilization. `gpu_busy_percent` is absent on older
            // drivers and on some parts; that used to read as an idle GPU.
            let utilization = fs::read_to_string(format!("{}/gpu_busy_percent", device_path))
                .ok()
                .and_then(|s| s.trim().parse::<u8>().ok());

            // Read memory info. `mem_info_vram_*` is absent on parts with no
            // dedicated VRAM and on drivers that do not export it, and this
            // used to answer 0 bytes for both.
            let memory = match (
                read_sysfs_bytes(&format!("{}/mem_info_vram_total", device_path)),
                read_sysfs_bytes(&format!("{}/mem_info_vram_used", device_path)),
            ) {
                (Some(total), Some(used)) => GpuMemory::from_total_used(total, used),
                _ => GpuMemory::unreported(),
            };

            // Read clocks from hwmon
            let (graphics_clock, memory_clock) = if let Some(ref hwmon) = self.hwmon_path {
                let gfx = read_sysfs_mhz(&format!("{}/freq1_input", hwmon));
                let mem = read_sysfs_mhz(&format!("{}/freq2_input", hwmon));
                (gfx, mem)
            } else {
                (None, None)
            };

            // Read power from hwmon
            let power_draw = self
                .hwmon_path
                .as_ref()
                .and_then(|hwmon| read_sysfs_microwatts(&format!("{}/power1_average", hwmon)));

            // Read temperature from hwmon
            let temperature = self.hwmon_path.as_ref().and_then(|hwmon| {
                fs::read_to_string(format!("{}/temp1_input", hwmon))
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .map(|t| (t / 1000) as u32)
            });

            // Read critical temp
            let critical_temp = self.hwmon_path.as_ref().and_then(|hwmon| {
                fs::read_to_string(format!("{}/temp1_crit", hwmon))
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .map(|t| (t / 1000) as u32)
            });

            // Read fan speed
            let fan_rpm = self.hwmon_path.as_ref().and_then(|hwmon| {
                fs::read_to_string(format!("{}/fan1_input", hwmon))
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
            });

            let fan_speed = self.hwmon_path.as_ref().and_then(|hwmon| {
                let current = fs::read_to_string(format!("{}/pwm1", hwmon))
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())?;
                // PWM is 0-255
                Some(((current as f32 / 255.0) * 100.0) as u8)
            });

            Ok(GpuDynamicInfo {
                utilization,
                memory,
                clocks: GpuClocks {
                    graphics: graphics_clock,
                    graphics_max: None,
                    memory: memory_clock,
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
                    // sysfs reports these unsigned; the field is i32 so a sensor
                    // below freezing can be represented.
                    temperature: temperature.map(|t| t as i32),
                    max_temperature: None,
                    critical_temperature: critical_temp.map(|t| t as i32),
                    fan_speed,
                    fan_rpm,
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

        // Nothing is read on this platform, so nothing is reported. Every
        // `Option` field below was already `None`; the four bare ones said `0`,
        // which is a reading nobody took. The AMD path that actually runs on
        // Windows is `WmiAmdGpu`, further down this file.
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
        GpuVendor::Amd
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
            parse_fdinfo_processes(&self.card_path)
        }
        #[cfg(not(target_os = "linux"))]
        Ok(vec![])
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

/// Read a sysfs value as bytes
#[cfg(target_os = "linux")]
fn read_sysfs_bytes(path: &str) -> Option<u64> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
}

/// Read frequency in Hz, convert to MHz
#[cfg(target_os = "linux")]
fn read_sysfs_mhz(path: &str) -> Option<u32> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|hz| (hz / 1_000_000) as u32)
}

/// Read power in microwatts, convert to milliwatts
#[cfg(target_os = "linux")]
fn read_sysfs_microwatts(path: &str) -> Option<u32> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|uw| (uw / 1000) as u32)
}

/// Parse fdinfo for AMD GPU processes
#[cfg(target_os = "linux")]
fn parse_fdinfo_processes(card_path: &str) -> Result<Vec<GpuProcess>, Error> {
    let mut processes = Vec::new();
    let proc_dir = Path::new("/proc");

    // Get the card's DRM minor number from card path
    let _card_name = Path::new(card_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if let Ok(proc_entries) = fs::read_dir(proc_dir) {
        for proc_entry in proc_entries.flatten() {
            let pid_str = proc_entry.file_name();
            let pid_str = pid_str.to_string_lossy();

            // Skip non-numeric entries
            let pid: u32 = match pid_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Check fdinfo for this process
            let fdinfo_dir = proc_entry.path().join("fdinfo");
            if !fdinfo_dir.exists() {
                continue;
            }

            if let Ok(fdinfo_entries) = fs::read_dir(&fdinfo_dir) {
                for fdinfo_entry in fdinfo_entries.flatten() {
                    if let Ok(content) = fs::read_to_string(fdinfo_entry.path()) {
                        // Look for drm-client-id and drm-driver: amdgpu
                        if content.contains("drm-driver:\tamdgpu") {
                            // `Option`, not `0`: a client whose
                            // `drm-memory-vram:` line is absent or
                            // unparseable is not a client using no VRAM,
                            // and the two were indistinguishable here.
                            let mut vram_mem = None;
                            let mut gtt_mem = None;

                            for line in content.lines() {
                                if line.starts_with("drm-memory-vram:") {
                                    // Parse "drm-memory-vram:\t1234 KiB"
                                    if let Some(val) = parse_fdinfo_memory(line) {
                                        vram_mem = Some(val);
                                    }
                                } else if line.starts_with("drm-memory-gtt:") {
                                    if let Some(val) = parse_fdinfo_memory(line) {
                                        gtt_mem = Some(val);
                                    }
                                }
                            }

                            // Reported if either figure was read at all. The
                            // test was `vram_mem > 0 || gtt_mem > 0`, which
                            // also dropped every client that genuinely holds
                            // no VRAM -- a compute job between allocations.
                            if vram_mem.is_some() || gtt_mem.is_some() {
                                // Get process name
                                let name = fs::read_to_string(proc_entry.path().join("comm"))
                                    .map(|s| s.trim().to_string())
                                    .unwrap_or_else(|_| format!("Process {}", pid));

                                processes.push(GpuProcess {
                                    pid,
                                    name,
                                    // fdinfo names the process's own VRAM use but
                                    // not who owns it or what class of work it is.
                                    user: String::new(),
                                    process_type: GpuProcessType::Unknown,
                                    gpu_usage: None,
                                    // Carried through rather than wrapped:
                                    // with a readable gtt line and an
                                    // unreadable vram one, `Some(vram_mem)`
                                    // published a measured 0 B.
                                    memory_usage: vram_mem,
                                    memory_usage_percent: None,
                                    encoder_usage: None,
                                    decoder_usage: None,
                                    cpu_usage: None,
                                    cpu_memory: None,
                                });
                            }
                            break; // Found amdgpu for this process
                        }
                    }
                }
            }
        }
    }

    // Deduplicate by PID
    processes.sort_by_key(|p| p.pid);
    processes.dedup_by_key(|p| p.pid);

    Ok(processes)
}

#[cfg(target_os = "linux")]
fn parse_fdinfo_memory(line: &str) -> Option<u64> {
    // Parse lines like "drm-memory-vram:\t1234 KiB" or "drm-memory-vram: 1234 KiB"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        let value: u64 = parts[1].parse().ok()?;
        let unit = parts.get(2).unwrap_or(&"B");
        let bytes = match unit.to_uppercase().as_str() {
            "KIB" | "KB" => value * 1024,
            "MIB" | "MB" => value * 1024 * 1024,
            "GIB" | "GB" => value * 1024 * 1024 * 1024,
            _ => value,
        };
        return Some(bytes);
    }
    None
}

/// Detect all AMD GPUs in the system
pub fn detect_gpus(collection: &mut GpuCollection) -> Result<(), Error> {
    #[cfg(target_os = "linux")]
    {
        let dri_path = Path::new("/sys/class/drm");

        if !dri_path.exists() {
            return Ok(());
        }

        let mut gpu_index = 0;

        // Scan card* entries
        if let Ok(entries) = fs::read_dir(dri_path) {
            let mut cards: Vec<_> = entries
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    // Match card0, card1, etc. but not card0-DP-1, etc.
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

                // Check if it's an AMD GPU by reading driver symlink
                let driver_path = device_path.join("driver");
                if let Ok(driver_target) = fs::read_link(&driver_path) {
                    let driver_name = driver_target
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");

                    if driver_name == "amdgpu" {
                        // Get PCI bus ID from device path symlink
                        let pci_bus_id = if let Ok(dev_link) = fs::read_link(&device_path) {
                            dev_link
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("unknown")
                                .to_string()
                        } else {
                            "unknown".to_string()
                        };

                        if let Ok(gpu) = AmdGpu::new(
                            gpu_index,
                            pci_bus_id,
                            card_path.to_string_lossy().to_string(),
                        ) {
                            collection.add_gpu(Box::new(gpu));
                            gpu_index += 1;
                        }
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    {
        detect_amd_gpus_wmi(collection)?;
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = collection;
    }

    Ok(())
}

/// Detect AMD GPUs on Windows via WMI Win32_VideoController
#[cfg(windows)]
fn detect_amd_gpus_wmi(collection: &mut GpuCollection) -> Result<(), Error> {
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
        adapter_d_a_c_type: Option<String>,
        status: Option<String>,
    }

    let com = wmi::COMLibrary::new()
        .map_err(|e| Error::Other(format!("Failed to initialize COM: {}", e)))?;
    let wmi_con = wmi::WMIConnection::with_namespace_path("root\\CIMV2", com)
        .map_err(|e| Error::Other(format!("Failed to connect to WMI: {}", e)))?;

    let controllers: Vec<Win32VideoController> = wmi_con
        .raw_query("SELECT Name, AdapterRAM, DriverVersion, VideoProcessor, PNPDeviceID, AdapterDACType, Status FROM Win32_VideoController")
        .unwrap_or_default();

    let mut gpu_index = 0;
    for ctrl in &controllers {
        let name = ctrl.name.as_deref().unwrap_or("");
        let name_lower = name.to_lowercase();

        // Filter for AMD GPUs
        if !name_lower.contains("amd")
            && !name_lower.contains("radeon")
            && !name_lower.contains("ati")
        {
            continue;
        }

        let pci_bus_id = ctrl
            .pnp_device_i_d
            .as_deref()
            .unwrap_or("unknown")
            .to_string();

        let mut gpu = AmdGpu::new(gpu_index, pci_bus_id.clone())?;
        gpu.name = name.to_string();

        // Use DXGI for accurate VRAM and LUID (fixes 4GB WMI cap)
        let dxgi_adapters = super::windows_helpers::enumerate_dxgi_adapters();
        let dxgi_match =
            super::windows_helpers::find_dxgi_adapter(&dxgi_adapters, name).or_else(|| {
                super::windows_helpers::find_dxgi_adapter_by_vendor(&dxgi_adapters, 0x1002)
            });

        let (dedicated_video_memory, luid_filter) = if let Some(dxgi) = &dxgi_match {
            let luid = super::windows_helpers::format_luid(dxgi.luid_high, dxgi.luid_low);
            (dxgi.dedicated_video_memory, Some(luid))
        } else {
            (ctrl.adapter_r_a_m.unwrap_or(0), None)
        };

        // Store WMI data for static_info/dynamic_info with perf counter support
        collection.add_gpu(Box::new(WmiAmdGpu {
            inner: gpu,
            dedicated_video_memory,
            driver_version: ctrl.driver_version.clone(),
            luid_filter,
            gpu_name_hint: name.to_string(),
        }));
        gpu_index += 1;
    }

    Ok(())
}

/// AMD GPU with WMI-sourced data on Windows
#[cfg(windows)]
struct WmiAmdGpu {
    inner: AmdGpu,
    /// Accurate VRAM from DXGI (64-bit, no 4GB cap). Falls back to WMI AdapterRAM.
    dedicated_video_memory: u64,
    driver_version: Option<String>,
    /// LUID hex string for matching with GPU performance counters
    luid_filter: Option<String>,
    /// GPU name used as hint for OHM/LHM temperature lookup
    gpu_name_hint: String,
}

/// Windows GPU performance counter data from D3DKMT/WMI
#[cfg(windows)]
#[derive(Debug, Default)]
struct WinGpuPerfData {
    /// `None` where no engine performance counter answered. See
    /// `windows_helpers::EngineUtilization::overall`.
    utilization: Option<u8>,
    dedicated_used: Option<u64>,
    #[allow(dead_code)]
    shared_used: Option<u64>,
    temperature: Option<u32>,
    /// Per-engine utilization breakdown
    engines_graphics: Option<u8>,
    engines_compute: Option<u8>,
    engines_video_decode: Option<u8>,
    engines_video_encode: Option<u8>,
    engines_copy: Option<u8>,
}

#[cfg(windows)]
fn query_gpu_perf_counters_wmi(
    adapter_name_filter: &str,
    luid_filter: Option<&str>,
) -> WinGpuPerfData {
    // Use shared helper for engine + memory data
    let perf = super::windows_helpers::query_gpu_perf_counters(luid_filter);

    // Use OHM/LHM for GPU-specific temperature instead of generic ACPI zones
    let temperature =
        super::windows_helpers::query_gpu_temperature_ohm(adapter_name_filter).map(|t| t as u32);

    WinGpuPerfData {
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
impl Gpu for WmiAmdGpu {
    fn static_info(&self) -> Result<GpuStaticInfo, Error> {
        let is_integrated = self.inner.name.to_lowercase().contains("vega")
            || self.inner.name.to_lowercase().contains("integrated")
            || self.inner.name.to_lowercase().contains("renoir")
            || self.inner.name.to_lowercase().contains("cezanne")
            || self.inner.name.to_lowercase().contains("phoenix")
            || self.dedicated_video_memory == 0;

        Ok(GpuStaticInfo {
            index: self.inner.index,
            vendor: GpuVendor::Amd,
            name: self.inner.name.clone(),
            pci_bus_id: Some(self.inner.pci_bus_id.clone()),
            uuid: None,
            vbios_version: None,
            driver_version: self.driver_version.clone(),
            compute_capability: None,
            shader_cores: None,
            l2_cache: None,
            num_engines: None,
            integrated: is_integrated,
        })
    }

    fn dynamic_info(&self) -> Result<GpuDynamicInfo, Error> {
        // Query real-time GPU performance counters via shared helpers
        let perf = query_gpu_perf_counters_wmi(&self.gpu_name_hint, self.luid_filter.as_deref());

        // The counters give bytes in use; the adapter's declared capacity comes
        // from WMI at construction. Where WMI reported no capacity, the total
        // fell back to *the used figure itself*, which makes every adapter look
        // exactly full -- 100% memory utilisation, permanently. Reporting the
        // used bytes with no total says what is actually known.
        // Capacity and usage now fail independently: WMI may report the
        // adapter's size while the performance counters report nothing in use,
        // and each is stated on its own rather than one standing in for the
        // other.
        let mem_total = (self.dedicated_video_memory > 0).then_some(self.dedicated_video_memory);
        let memory = match (mem_total, perf.dedicated_used) {
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
                temperature: perf.temperature.map(|t| t as i32),
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
        GpuVendor::Amd
    }

    fn index(&self) -> usize {
        self.inner.index
    }

    fn name(&self) -> Result<String, Error> {
        Ok(self.inner.name.clone())
    }

    fn processes(&self) -> Result<Vec<GpuProcess>, Error> {
        Ok(vec![])
    }

    fn kill_process(&self, _pid: u32) -> Result<(), Error> {
        Err(Error::NotSupported(
            "Process killing not supported on Windows for AMD GPUs".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_amd_gpu_creation() {
        #[cfg(target_os = "linux")]
        {
            // This will fail on systems without AMD GPUs, which is expected
            let result = AmdGpu::new(
                0,
                "0000:03:00.0".to_string(),
                "/sys/class/drm/card0".to_string(),
            );
            // Just verify it doesn't panic
            let _ = result;
        }
    }

    #[cfg(windows)]
    #[test]
    fn test_win_gpu_perf_data_default() {
        let data = WinGpuPerfData::default();
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
    fn test_win_gpu_perf_data_fields() {
        let data = WinGpuPerfData {
            utilization: Some(75),
            dedicated_used: Some(1024 * 1024 * 512),
            shared_used: Some(1024 * 1024 * 128),
            temperature: Some(65),
            engines_graphics: Some(80),
            engines_compute: Some(10),
            engines_video_decode: Some(50),
            engines_video_encode: Some(30),
            engines_copy: Some(5),
        };
        assert_eq!(data.utilization, Some(75));
        assert_eq!(data.dedicated_used, Some(512 * 1024 * 1024));
        assert_eq!(data.shared_used, Some(128 * 1024 * 1024));
        assert_eq!(data.temperature, Some(65));
        assert_eq!(data.engines_graphics, Some(80));
        assert_eq!(data.engines_video_decode, Some(50));
    }
}
