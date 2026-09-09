// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! NVIDIA GPU monitoring via NVML
//!
//! This module provides NVIDIA GPU support using the NVIDIA Management Library (NVML).
//! It supports both Jetson devices and desktop GPUs, monitoring:
//! - GPU utilization, frequency, memory
//! - Temperature and fan control
//! - Power consumption and limits
//! - Video encode/decode engines (NVENC/NVDEC)
//! - Process tracking
//! - Jetson-specific features (jetson_clocks, nvpmodel, etc.)
//!
//! This integrates the existing IronMonitor NVIDIA implementation with the unified GPU interface.

use crate::gpu::{
    Gpu, GpuClocks, GpuCollection, GpuDynamicInfo, GpuEngines, GpuMemory, GpuPower, GpuProcess,
    GpuProcessType, GpuStaticInfo, GpuThermal, GpuVendor, PcieLinkInfo,
};
use crate::Error;

#[cfg(feature = "nvidia")]
use nvml_wrapper::{Device, Nvml};

#[cfg(feature = "nvidia")]
use std::sync::Arc;

/// NVIDIA GPU implementation
pub struct NvidiaGpu {
    index: usize,
    #[cfg(feature = "nvidia")]
    device: Device<'static>,
    #[cfg(feature = "nvidia")]
    _nvml: Arc<Nvml>, // Keep NVML alive
    #[cfg(not(feature = "nvidia"))]
    _phantom: std::marker::PhantomData<()>,
}

impl NvidiaGpu {
    /// Create new NVIDIA GPU instance
    #[cfg(feature = "nvidia")]
    pub fn new(index: usize, device: Device<'static>, nvml: Arc<Nvml>) -> Result<Self, Error> {
        Ok(Self {
            index,
            device,
            _nvml: nvml,
        })
    }

    #[cfg(not(feature = "nvidia"))]
    pub fn new(index: usize) -> Result<Self, Error> {
        Ok(Self {
            index,
            _phantom: std::marker::PhantomData,
        })
    }

    /// Check if this is a Jetson device (integrated GPU)
    #[cfg(feature = "nvidia")]
    fn is_jetson_device(&self) -> bool {
        // Jetson GPUs are integrated and have specific names
        if let Ok(name) = self.device.name() {
            let name_lower = name.to_lowercase();
            // Jetson GPU names typically contain these identifiers
            name_lower.contains("tegra")
                || name_lower.contains("orin")
                || name_lower.contains("xavier")
                || name_lower.contains("nano")
                || name_lower.contains("tx1")
                || name_lower.contains("tx2")
                || name_lower.contains("agx")
                || name_lower.contains("gv11b")
                || name_lower.contains("gp10b")
                || name_lower.contains("ga10b")
        } else {
            // Also check if this is a Jetson by bus type (integrated = no PCIe bus)
            // Jetson GPUs don't have PCIe info
            self.device.pci_info().is_err()
        }
    }

    #[cfg(not(feature = "nvidia"))]
    fn is_jetson_device(&self) -> bool {
        false
    }
}

impl Gpu for NvidiaGpu {
    #[cfg(feature = "nvidia")]
    fn static_info(&self) -> Result<GpuStaticInfo, Error> {
        use nvml_wrapper::Nvml;

        let name = self
            .device
            .name()
            .map_err(|e| Error::GpuError(format!("Failed to get NVIDIA GPU name: {}", e)))?;

        let pci_info = self.device.pci_info().ok();
        let pci_bus_id = pci_info.map(|info| info.bus_id);

        let uuid = self.device.uuid().ok();

        let vbios_version = self.device.vbios_version().ok();

        // Create an NVML instance to get driver version
        let driver_version = Nvml::init()
            .ok()
            .and_then(|nvml| nvml.sys_driver_version().ok());

        let compute_capability = self
            .device
            .cuda_compute_capability()
            .ok()
            .map(|cap| (cap.major as u32, cap.minor as u32));

        let shader_cores = self.device.num_cores().ok();

        let l2_cache = None; // Not available in nvml-wrapper 0.10

        // Detect if this is a Jetson (integrated GPU)
        let integrated = self.is_jetson_device();

        Ok(GpuStaticInfo {
            index: self.index,
            vendor: GpuVendor::Nvidia,
            name,
            pci_bus_id,
            uuid,
            vbios_version,
            driver_version,
            compute_capability,
            shader_cores,
            l2_cache,
            num_engines: None,
            integrated,
        })
    }

    #[cfg(not(feature = "nvidia"))]
    fn static_info(&self) -> Result<GpuStaticInfo, Error> {
        Err(Error::NotSupported(
            "NVIDIA support not compiled in".to_string(),
        ))
    }

    #[cfg(feature = "nvidia")]
    fn dynamic_info(&self) -> Result<GpuDynamicInfo, Error> {
        use nvml_wrapper::enum_wrappers::device::Clock;

        // GPU utilization
        // `.unwrap_or(0)` here reported an idle card for a query that
        // failed. The clocks below have always been `Option`; this now is too.
        let utilization = self.device.utilization_rates().ok().map(|u| u.gpu as u8);

        // Memory
        // An NVML query that failed reports nothing, rather than a card with
        // no memory. The clocks immediately below have always been `Option` for
        // this reason; the memory fields could not say it until 6.0.0.
        let memory = match self.device.memory_info().ok() {
            Some(m) => GpuMemory::from_total_used(m.total, m.used),
            None => GpuMemory::unreported(),
        };

        // Clocks
        let clocks = GpuClocks {
            graphics: self.device.clock_info(Clock::Graphics).ok(),
            graphics_max: self.device.max_clock_info(Clock::Graphics).ok(),
            memory: self.device.clock_info(Clock::Memory).ok(),
            memory_max: self.device.max_clock_info(Clock::Memory).ok(),
            sm: self.device.clock_info(Clock::SM).ok(),
            video: self.device.clock_info(Clock::Video).ok(),
        };

        // Power
        let power_draw = self.device.power_usage().ok();
        let power_limit = self.device.power_management_limit().ok();
        let default_limit = self.device.power_management_limit_default().ok();
        let power = GpuPower {
            draw: power_draw,
            limit: power_limit,
            default_limit,
            usage_percent: match (power_draw, power_limit) {
                (Some(draw), Some(limit)) if limit > 0 => {
                    Some(((draw as f64 / limit as f64) * 100.0) as u8)
                }
                _ => None,
            },
        };

        // Thermal
        let temperature = self
            .device
            .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            .ok()
            .map(|t| t as i32);

        let fan_speed = self.device.fan_speed(0).ok().map(|s| s as u8);

        // Get thermal thresholds from NVML
        let max_temperature = self
            .device
            .temperature_threshold(
                nvml_wrapper::enum_wrappers::device::TemperatureThreshold::Slowdown,
            )
            .ok()
            .map(|t| t as i32);

        let critical_temperature = self
            .device
            .temperature_threshold(
                nvml_wrapper::enum_wrappers::device::TemperatureThreshold::Shutdown,
            )
            .ok()
            .map(|t| t as i32);

        let thermal = GpuThermal {
            temperature,
            max_temperature,
            critical_temperature,
            fan_speed,
            fan_rpm: None,
        };

        // PCIe
        let pcie_gen = self.device.current_pcie_link_gen().ok().map(|g| g as u8);
        let max_pcie_gen = self.device.max_pcie_link_gen().ok().map(|g| g as u8);
        let pcie_width = self.device.current_pcie_link_width().ok().map(|w| w as u8);
        let max_pcie_width = self.device.max_pcie_link_width().ok().map(|w| w as u8);

        let pcie = PcieLinkInfo {
            current_gen: pcie_gen,
            max_gen: max_pcie_gen,
            current_width: pcie_width,
            max_width: max_pcie_width,
            current_speed: None,
            max_speed: None,
            tx_throughput: None,
            rx_throughput: None,
        };

        // Engines
        let encoder_util = self
            .device
            .encoder_utilization()
            .ok()
            .map(|u| u.utilization as u8);
        let decoder_util = self
            .device
            .decoder_utilization()
            .ok()
            .map(|u| u.utilization as u8);

        let engines = GpuEngines {
            graphics: utilization,
            compute: None,
            encoder: encoder_util,
            decoder: decoder_util,
            copy: None,
            vendor_specific: vec![],
        };

        // Processes
        let processes = self.get_processes_internal()?;

        Ok(GpuDynamicInfo {
            utilization,
            memory,
            clocks,
            power,
            thermal,
            pcie,
            engines,
            processes,
        })
    }

    #[cfg(not(feature = "nvidia"))]
    fn dynamic_info(&self) -> Result<GpuDynamicInfo, Error> {
        Err(Error::NotSupported(
            "NVIDIA support not compiled in".to_string(),
        ))
    }

    fn vendor(&self) -> GpuVendor {
        GpuVendor::Nvidia
    }

    fn index(&self) -> usize {
        self.index
    }

    #[cfg(feature = "nvidia")]
    fn name(&self) -> Result<String, Error> {
        self.device
            .name()
            .map_err(|e| Error::GpuError(format!("Failed to get GPU name: {}", e)))
    }

    #[cfg(not(feature = "nvidia"))]
    fn name(&self) -> Result<String, Error> {
        Err(Error::NotSupported(
            "NVIDIA support not compiled in".to_string(),
        ))
    }

    #[cfg(feature = "nvidia")]
    fn processes(&self) -> Result<Vec<GpuProcess>, Error> {
        self.get_processes_internal()
    }

    #[cfg(not(feature = "nvidia"))]
    fn processes(&self) -> Result<Vec<GpuProcess>, Error> {
        Err(Error::NotSupported(
            "NVIDIA support not compiled in".to_string(),
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
        #[cfg(windows)]
        {
            // Windows constants
            const PROCESS_TERMINATE: u32 = 0x0001;

            // Windows API declarations
            #[link(name = "kernel32")]
            extern "system" {
                fn OpenProcess(
                    dwDesiredAccess: u32,
                    bInheritHandle: i32,
                    dwProcessId: u32,
                ) -> *mut std::ffi::c_void;
                fn TerminateProcess(hProcess: *mut std::ffi::c_void, uExitCode: u32) -> i32;
                fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
            }

            unsafe {
                // Open the process with terminate permission
                let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
                if handle.is_null() {
                    return Err(Error::ProcessError(format!(
                        "Failed to open process {}: access denied or process not found",
                        pid
                    )));
                }

                // Terminate the process
                let result = TerminateProcess(handle, 1);
                CloseHandle(handle);

                if result == 0 {
                    return Err(Error::ProcessError(format!(
                        "Failed to terminate process {}",
                        pid
                    )));
                }
            }

            Ok(())
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = pid;
            Err(Error::NotSupported(
                "Process killing not supported on this platform".to_string(),
            ))
        }
    }

    #[cfg(feature = "nvidia")]
    fn set_power_limit(&mut self, limit_mw: u32) -> Result<(), Error> {
        self.device
            .set_power_management_limit(limit_mw)
            .map_err(|e| Error::GpuError(format!("Failed to set power limit: {}", e)))?;
        Ok(())
    }
}

#[cfg(feature = "nvidia")]
impl NvidiaGpu {
    fn get_processes_internal(&self) -> Result<Vec<GpuProcess>, Error> {
        use nvml_wrapper::enums::device::UsedGpuMemory;

        let mut processes = Vec::new();

        // Get graphics processes
        if let Ok(graphics_procs) = self.device.running_graphics_processes() {
            for proc in graphics_procs {
                let memory_usage = match proc.used_gpu_memory {
                    UsedGpuMemory::Used(bytes) => Some(bytes),
                    _ => None,
                };

                processes.push(GpuProcess {
                    pid: proc.pid,
                    name: get_process_name(proc.pid),
                    user: get_process_user(proc.pid),
                    process_type: GpuProcessType::Graphics,
                    gpu_usage: None,
                    memory_usage,
                    memory_usage_percent: None,
                    encoder_usage: None,
                    decoder_usage: None,
                    cpu_usage: None,
                    cpu_memory: None,
                });
            }
        }

        // Get compute processes
        if let Ok(compute_procs) = self.device.running_compute_processes() {
            for proc in compute_procs {
                // Check if already in list (some processes are both graphics and compute)
                if let Some(existing) = processes.iter_mut().find(|p| p.pid == proc.pid) {
                    existing.process_type = GpuProcessType::GraphicsAndCompute;
                } else {
                    let memory_usage = match proc.used_gpu_memory {
                        UsedGpuMemory::Used(bytes) => Some(bytes),
                        _ => None,
                    };

                    processes.push(GpuProcess {
                        pid: proc.pid,
                        name: get_process_name(proc.pid),
                        user: get_process_user(proc.pid),
                        process_type: GpuProcessType::Compute,
                        gpu_usage: None,
                        memory_usage,
                        memory_usage_percent: None,
                        encoder_usage: None,
                        decoder_usage: None,
                        cpu_usage: None,
                        cpu_memory: None,
                    });
                }
            }
        }

        Ok(processes)
    }
}

#[cfg(feature = "nvidia")]
fn get_process_name(pid: u32) -> String {
    #[cfg(unix)]
    {
        std::fs::read_to_string(format!("/proc/{}/comm", pid))
            .ok()
            .and_then(|s| s.lines().next().map(|l| l.to_string()))
            .unwrap_or_else(|| format!("<unknown:{}>", pid))
    }
    #[cfg(windows)]
    {
        use std::mem;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };

        unsafe {
            if let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                let mut entry: PROCESSENTRY32W = mem::zeroed();
                entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;

                if Process32FirstW(snapshot, &mut entry).is_ok() {
                    loop {
                        if entry.th32ProcessID == pid {
                            let name_len = entry
                                .szExeFile
                                .iter()
                                .position(|&c| c == 0)
                                .unwrap_or(entry.szExeFile.len());
                            let name = String::from_utf16_lossy(&entry.szExeFile[..name_len]);
                            let _ = CloseHandle(snapshot);
                            return name;
                        }
                        if Process32NextW(snapshot, &mut entry).is_err() {
                            break;
                        }
                    }
                }
                let _ = CloseHandle(snapshot);
            }
        }
        format!("process_{}", pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        format!("process_{}", pid)
    }
}

#[cfg(feature = "nvidia")]
fn get_process_user(pid: u32) -> String {
    #[cfg(unix)]
    {
        use std::fs;
        use std::os::unix::fs::MetadataExt;

        fs::metadata(format!("/proc/{}", pid))
            .ok()
            .and_then(|meta| {
                nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(meta.uid()))
                    .ok()
                    .flatten()
                    .map(|u| u.name)
            })
            .unwrap_or_else(|| "<unknown>".to_string())
    }
    #[cfg(windows)]
    {
        // Get username from Windows API
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        extern "system" {
            fn OpenProcess(
                dwDesiredAccess: u32,
                bInheritHandle: i32,
                dwProcessId: u32,
            ) -> *mut std::ffi::c_void;
            fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
            fn OpenProcessToken(
                ProcessHandle: *mut std::ffi::c_void,
                DesiredAccess: u32,
                TokenHandle: *mut *mut std::ffi::c_void,
            ) -> i32;
            fn GetTokenInformation(
                TokenHandle: *mut std::ffi::c_void,
                TokenInformationClass: u32,
                TokenInformation: *mut std::ffi::c_void,
                TokenInformationLength: u32,
                ReturnLength: *mut u32,
            ) -> i32;
            fn LookupAccountSidW(
                lpSystemName: *const u16,
                Sid: *mut std::ffi::c_void,
                Name: *mut u16,
                cchName: *mut u32,
                ReferencedDomainName: *mut u16,
                cchReferencedDomainName: *mut u32,
                peUse: *mut u32,
            ) -> i32;
        }

        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const TOKEN_QUERY: u32 = 0x0008;
        const TOKEN_USER: u32 = 1;

        #[repr(C)]
        struct TokenUser {
            user: SidAndAttributes,
        }

        #[repr(C)]
        struct SidAndAttributes {
            sid: *mut std::ffi::c_void,
            attributes: u32,
        }

        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return "<unknown>".to_string();
            }

            let mut token: *mut std::ffi::c_void = std::ptr::null_mut();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
                CloseHandle(process);
                return "<unknown>".to_string();
            }

            // Get required buffer size
            let mut return_length: u32 = 0;
            GetTokenInformation(
                token,
                TOKEN_USER,
                std::ptr::null_mut(),
                0,
                &mut return_length,
            );

            if return_length == 0 {
                CloseHandle(token);
                CloseHandle(process);
                return "<unknown>".to_string();
            }

            let mut buffer = vec![0u8; return_length as usize];
            if GetTokenInformation(
                token,
                TOKEN_USER,
                buffer.as_mut_ptr() as *mut _,
                return_length,
                &mut return_length,
            ) == 0
            {
                CloseHandle(token);
                CloseHandle(process);
                return "<unknown>".to_string();
            }

            let token_user = &*(buffer.as_ptr() as *const TokenUser);

            // Look up the account name
            let mut name_size: u32 = 256;
            let mut domain_size: u32 = 256;
            let mut name_buf = vec![0u16; name_size as usize];
            let mut domain_buf = vec![0u16; domain_size as usize];
            let mut sid_type: u32 = 0;

            if LookupAccountSidW(
                std::ptr::null(),
                token_user.user.sid,
                name_buf.as_mut_ptr(),
                &mut name_size,
                domain_buf.as_mut_ptr(),
                &mut domain_size,
                &mut sid_type,
            ) == 0
            {
                CloseHandle(token);
                CloseHandle(process);
                return "<unknown>".to_string();
            }

            CloseHandle(token);
            CloseHandle(process);

            // Convert wide string to Rust string
            let name = OsString::from_wide(&name_buf[..name_size as usize])
                .to_string_lossy()
                .to_string();
            name
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        "user".to_string()
    }
}

/// Detect all NVIDIA GPUs in the system
#[cfg(feature = "nvidia")]
pub fn detect_gpus(collection: &mut GpuCollection) -> Result<(), Error> {
    let nvml = Arc::new(
        Nvml::init().map_err(|e| Error::GpuError(format!("Failed to initialize NVML: {}", e)))?,
    );

    let device_count = nvml
        .device_count()
        .map_err(|e| Error::GpuError(format!("Failed to get NVIDIA device count: {}", e)))?;

    for i in 0..device_count {
        let device = nvml
            .device_by_index(i)
            .map_err(|e| Error::GpuError(format!("Failed to get NVIDIA device {}: {}", i, e)))?;

        // Transmute the device to get a 'static lifetime (safe because we keep nvml alive via Arc)
        let static_device = unsafe { std::mem::transmute::<Device<'_>, Device<'static>>(device) };

        let gpu = NvidiaGpu::new(i as usize, static_device, Arc::clone(&nvml))?;
        collection.add_gpu(Box::new(gpu));
    }

    Ok(())
}

#[cfg(not(feature = "nvidia"))]
pub fn detect_gpus(_collection: &mut GpuCollection) -> Result<(), Error> {
    // NVIDIA support not compiled in
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "nvidia")]
    fn test_nvidia_detection() {
        let mut collection = GpuCollection::new();
        // This may fail if no NVIDIA GPU is present
        let _ = detect_gpus(&mut collection);
    }
}
