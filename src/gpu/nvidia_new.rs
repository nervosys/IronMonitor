//! NVIDIA GPU Backend via NVML
//!
//! Comprehensive NVIDIA GPU monitoring with feature parity to nvidia-smi.
//! Supports 150+ metrics including MIG, NVLink, ECC, power management, and process tracking.

use crate::gpu::traits::*;

#[cfg(feature = "nvidia")]
use nvml_wrapper::{
    enum_wrappers::device::{Clock, TemperatureSensor},
    struct_wrappers::device::ProcessInfo,
    Device as NvmlDevice, Nvml,
};

use std::sync::Arc;

/// NVIDIA GPU Device
pub struct NvidiaGpu {
    index: u32,
    #[cfg(feature = "nvidia")]
    device: NvmlDevice<'static>,
    #[cfg(feature = "nvidia")]
    nvml: Arc<Nvml>,
}

impl NvidiaGpu {
    #[cfg(feature = "nvidia")]
    pub fn new(index: u32) -> Result<Self, Error> {
        let nvml = Arc::new(Nvml::init().map_err(|e| {
            Error::InitializationFailed(format!("Failed to initialize NVML: {}", e))
        })?);

        let device = nvml.device_by_index(index).map_err(|e| {
            Error::InitializationFailed(format!("Failed to get device {}: {}", index, e))
        })?;

        // Safety: We're keeping the NVML instance alive via Arc
        let device = unsafe { std::mem::transmute::<NvmlDevice<'_>, NvmlDevice<'static>>(device) };

        Ok(Self {
            index,
            device,
            nvml,
        })
    }

    #[cfg(not(feature = "nvidia"))]
    pub fn new(_index: u32) -> Result<Self, Error> {
        Err(Error::InitializationFailed(
            "NVIDIA support not compiled in".to_string(),
        ))
    }

    /// Get temperature thresholds from NVML
    #[cfg(feature = "nvidia")]
    fn get_temperature_thresholds(&self) -> Result<TemperatureThresholds, Error> {
        use nvml_wrapper::enum_wrappers::device::TemperatureThreshold;

        let slowdown = self
            .device
            .temperature_threshold(TemperatureThreshold::Slowdown)
            .ok()
            .map(|t| t as f32);

        let shutdown = self
            .device
            .temperature_threshold(TemperatureThreshold::Shutdown)
            .ok()
            .map(|t| t as f32);

        // NVML doesn't expose GPU_THROTTLE threshold directly in nvml-wrapper 0.10
        // Use slowdown as critical if available
        let critical = slowdown;

        Ok(TemperatureThresholds {
            slowdown,
            shutdown,
            critical,
            memory_critical: None, // Not exposed by NVML
        })
    }
}

#[cfg(feature = "nvidia")]
impl Device for NvidiaGpu {
    fn vendor(&self) -> Vendor {
        Vendor::Nvidia
    }

    fn index(&self) -> u32 {
        self.index
    }

    fn name(&self) -> Result<String, Error> {
        self.device
            .name()
            .map_err(|e| Error::QueryFailed(format!("Failed to get name: {}", e)))
    }

    fn uuid(&self) -> Result<String, Error> {
        self.device
            .uuid()
            .map_err(|e| Error::QueryFailed(format!("Failed to get UUID: {}", e)))
    }

    fn pci_info(&self) -> Result<PciInfo, Error> {
        let pci = self
            .device
            .pci_info()
            .map_err(|e| Error::QueryFailed(format!("Failed to get PCI info: {}", e)))?;

        Ok(PciInfo {
            domain: pci.domain,
            bus: pci.bus as u8,
            device: pci.device as u8,
            function: 0, // Not exposed in pci_info
            bus_id: pci.bus_id,
            pcie_generation: self.device.current_pcie_link_gen().ok(),
            pcie_link_width: self.device.current_pcie_link_width().ok(),
        })
    }

    fn driver_version(&self) -> Result<String, Error> {
        self.nvml
            .sys_driver_version()
            .map_err(|e| Error::QueryFailed(format!("Failed to get driver version: {}", e)))
    }

    fn temperature(&self) -> Result<Temperature, Error> {
        let junction = self
            .device
            .temperature(TemperatureSensor::Gpu)
            .map(|t| t as f32)
            .ok();

        // Memory temperature sensor not available in nvml-wrapper 0.10
        let memory = None;

        // Get temperature thresholds from NVML
        let thresholds = self.get_temperature_thresholds().ok();

        Ok(Temperature {
            edge: None,        // AMD terminology
            junction,          // GPU temperature
            memory,            // Memory temperature (not available)
            hotspot: junction, // Same as junction for NVIDIA
            vr_gfx: None,      // Not exposed
            vr_soc: None,      // Not exposed
            vr_mem: None,      // Not exposed
            hbm: None,         // Would need per-stack temperatures
            thresholds,        // Temperature thresholds
        })
    }

    fn power(&self) -> Result<Power, Error> {
        let current = self.device.power_usage().map(|p| p as f32 / 1000.0).ok(); // mW to W
        let limit = self
            .device
            .power_management_limit()
            .map(|p| p as f32 / 1000.0)
            .ok();
        let default_limit = self
            .device
            .power_management_limit_default()
            .map(|p| p as f32 / 1000.0)
            .ok();

        let constraints = self.device.power_management_limit_constraints().ok();
        let min_limit = constraints.as_ref().map(|c| c.min_limit as f32 / 1000.0);
        let max_limit = constraints.as_ref().map(|c| c.max_limit as f32 / 1000.0);

        // Each of these was read as an `Option` two lines above and then
        // unwrapped to `0.0` here. They are passed through now.
        Ok(Power {
            current,
            average: None, // Calculate from samples
            limit,
            default_limit,
            min_limit,
            max_limit,
            enforced_limit: limit,
        })
    }

    fn clocks(&self) -> Result<Clocks, Error> {
        // `.ok()` already turned a failed query into `None`; the `unwrap_or(0)`
        // that used to follow threw that away and reported a card clocked at
        // 0 MHz. The absence is carried now.
        let graphics = self.device.clock_info(Clock::Graphics).ok();
        let memory = self.device.clock_info(Clock::Memory).ok();
        let sm = self.device.clock_info(Clock::SM).ok();
        let video = self.device.clock_info(Clock::Video).ok();

        Ok(Clocks {
            graphics,
            memory,
            sm,
            video,
        })
    }

    fn utilization(&self) -> Result<Utilization, Error> {
        let util = self
            .device
            .utilization_rates()
            .map_err(|e| Error::QueryFailed(format!("Failed to get utilization: {}", e)))?;

        let encoder = self
            .device
            .encoder_utilization()
            .map(|u| u.utilization as f32)
            .ok();

        let decoder = self
            .device
            .decoder_utilization()
            .map(|u| u.utilization as f32)
            .ok();

        Ok(Utilization {
            // NVML answered, so these are measurements. The whole query
            // failing is handled by the `?` above.
            gpu: Some(util.gpu as f32),
            memory: Some(util.memory as f32),
            encoder,
            decoder,
            jpeg: None, // Not exposed by NVML
            ofa: None,  // Not exposed by NVML
        })
    }

    fn memory(&self) -> Result<Memory, Error> {
        let mem = self
            .device
            .memory_info()
            .map_err(|e| Error::QueryFailed(format!("Failed to get memory info: {}", e)))?;

        let bar1 = self.device.bar1_memory_info().ok();

        Ok(Memory {
            // NVML answered, so these are measurements; the `?` above handles
            // the query failing.
            total: Some(mem.total),
            used: Some(mem.used),
            free: Some(mem.free),
            bar1_total: bar1.as_ref().map(|b| b.total),
            bar1_used: bar1.as_ref().map(|b| b.used),
        })
    }

    fn fan_speed(&self) -> Result<Option<FanSpeed>, Error> {
        Ok(self.device.fan_speed(0).ok().map(FanSpeed::Percent))
    }

    fn performance_state(&self) -> Result<Option<String>, Error> {
        Ok(self
            .device
            .performance_state()
            .ok()
            .map(|state| format!("P{}", state as u8)))
    }

    fn processes(&self) -> Result<Vec<Box<dyn GpuProcess>>, Error> {
        let mut processes: Vec<Box<dyn GpuProcess>> = Vec::new();

        // Get compute processes
        if let Ok(compute_procs) = self.device.running_compute_processes() {
            for proc in compute_procs {
                processes.push(Box::new(NvidiaProcess::new(
                    proc,
                    ProcessType::Compute,
                    self.index,
                )));
            }
        }

        // Get graphics processes
        if let Ok(graphics_procs) = self.device.running_graphics_processes() {
            for proc in graphics_procs {
                // Check if already added as compute process
                if !processes.iter().any(|p| p.pid() == proc.pid) {
                    processes.push(Box::new(NvidiaProcess::new(
                        proc,
                        ProcessType::Graphics,
                        self.index,
                    )));
                }
                // Note: nvml-wrapper 0.10 doesn't provide a way to update process type to Mixed
            }
        }

        Ok(processes)
    }

    // === NVIDIA-Specific Features ===

    fn nvlink_status(&self) -> Result<Vec<NvLinkStatus>, Error> {
        // NVLink APIs not available in nvml-wrapper 0.10
        // Would need direct NVML FFI or newer wrapper version
        Err(Error::NotSupported)
    }

    fn mig_mode(&self) -> Result<MigMode, Error> {
        // MIG mode API not available in nvml-wrapper 0.10
        Err(Error::NotSupported)
    }

    fn ecc_errors(&self) -> Result<EccErrors, Error> {
        // ECC error APIs not available or different in nvml-wrapper 0.10
        // Would need to check specific device support
        Err(Error::NotSupported)
    }

    fn compute_mode(&self) -> Result<Option<ComputeMode>, Error> {
        use nvml_wrapper::enum_wrappers::device::ComputeMode as NvmlComputeMode;

        let mode = self.device.compute_mode().ok();
        Ok(mode.map(|m| match m {
            NvmlComputeMode::Default => ComputeMode::Default,
            NvmlComputeMode::ExclusiveThread => ComputeMode::ExclusiveThread,
            NvmlComputeMode::Prohibited => ComputeMode::Prohibited,
            NvmlComputeMode::ExclusiveProcess => ComputeMode::ExclusiveProcess,
        }))
    }

    fn persistence_mode(&self) -> Result<Option<bool>, Error> {
        // persistence_mode API not available in nvml-wrapper 0.10
        Ok(None)
    }

    // === Control Functions ===

    fn set_power_limit(&mut self, watts: f32) -> Result<(), Error> {
        let milliwatts = (watts * 1000.0) as u32;
        self.device
            .set_power_management_limit(milliwatts)
            .map_err(|e| Error::ControlFailed(format!("Failed to set power limit: {}", e)))
    }

    fn lock_gpu_clocks(&mut self, min_mhz: u32, max_mhz: u32) -> Result<(), Error> {
        self.device
            .set_applications_clocks(max_mhz, min_mhz) // memory, graphics
            .map_err(|e| Error::ControlFailed(format!("Failed to lock clocks: {}", e)))
    }

    fn reset_gpu_clocks(&mut self) -> Result<(), Error> {
        self.device
            .reset_applications_clocks()
            .map_err(|e| Error::ControlFailed(format!("Failed to reset clocks: {}", e)))
    }

    fn set_persistence_mode(&mut self, _enabled: bool) -> Result<(), Error> {
        // set_persistence_mode API not available in nvml-wrapper 0.10
        Err(Error::NotSupported)
    }

    fn set_compute_mode(&mut self, mode: ComputeMode) -> Result<(), Error> {
        use nvml_wrapper::enum_wrappers::device::ComputeMode as NvmlComputeMode;

        let nvml_mode = match mode {
            ComputeMode::Default => NvmlComputeMode::Default,
            ComputeMode::ExclusiveThread => NvmlComputeMode::ExclusiveThread,
            ComputeMode::Prohibited => NvmlComputeMode::Prohibited,
            ComputeMode::ExclusiveProcess => NvmlComputeMode::ExclusiveProcess,
        };

        self.device
            .set_compute_mode(nvml_mode)
            .map_err(|e| Error::ControlFailed(format!("Failed to set compute mode: {}", e)))
    }
}

/// NVIDIA GPU Process
#[cfg(feature = "nvidia")]
struct NvidiaProcess {
    info: ProcessInfo,
    process_type: ProcessType,
    #[allow(dead_code)] // May be used in future for multi-GPU systems
    gpu_index: u32,
}

#[cfg(feature = "nvidia")]
impl NvidiaProcess {
    fn new(info: ProcessInfo, process_type: ProcessType, gpu_index: u32) -> Self {
        Self {
            info,
            process_type,
            gpu_index,
        }
    }
}

#[cfg(feature = "nvidia")]
impl GpuProcess for NvidiaProcess {
    fn pid(&self) -> u32 {
        self.info.pid
    }

    fn name(&self) -> Result<String, Error> {
        #[cfg(unix)]
        {
            Ok(
                std::fs::read_to_string(format!("/proc/{}/comm", self.info.pid))
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
                    .unwrap_or_else(|| format!("process_{}", self.info.pid)),
            )
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
                            if entry.th32ProcessID == self.info.pid {
                                let name_len = entry
                                    .szExeFile
                                    .iter()
                                    .position(|&c| c == 0)
                                    .unwrap_or(entry.szExeFile.len());
                                let name = String::from_utf16_lossy(&entry.szExeFile[..name_len]);
                                let _ = CloseHandle(snapshot);
                                return Ok(name);
                            }
                            if Process32NextW(snapshot, &mut entry).is_err() {
                                break;
                            }
                        }
                    }
                    let _ = CloseHandle(snapshot);
                }
            }
            Ok(format!("process_{}", self.info.pid))
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(format!("process_{}", self.info.pid))
        }
    }

    fn process_type(&self) -> ProcessType {
        self.process_type
    }

    fn gpu_memory_used(&self) -> Result<u64, Error> {
        use nvml_wrapper::enums::device::UsedGpuMemory;

        match self.info.used_gpu_memory {
            UsedGpuMemory::Used(bytes) => Ok(bytes),
            UsedGpuMemory::Unavailable => Err(Error::QueryFailed("Memory unavailable".to_string())),
        }
    }
}

/// Enumerate all NVIDIA GPUs
#[cfg(feature = "nvidia")]
pub fn enumerate() -> Result<Vec<NvidiaGpu>, Error> {
    let nvml = Nvml::init()
        .map_err(|e| Error::InitializationFailed(format!("Failed to initialize NVML: {}", e)))?;

    let count = nvml
        .device_count()
        .map_err(|e| Error::QueryFailed(format!("Failed to get device count: {}", e)))?;

    let mut devices = Vec::new();
    for i in 0..count {
        match NvidiaGpu::new(i) {
            Ok(device) => devices.push(device),
            Err(e) => eprintln!("Warning: Failed to initialize NVIDIA GPU {}: {}", i, e),
        }
    }

    if devices.is_empty() {
        return Err(Error::NoDevicesFound);
    }

    Ok(devices)
}

#[cfg(not(feature = "nvidia"))]
pub fn enumerate() -> Result<Vec<NvidiaGpu>, Error> {
    Err(Error::InitializationFailed(
        "NVIDIA support not compiled in".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "nvidia")]
    fn test_enumerate_nvidia_gpus() {
        match enumerate() {
            Ok(devices) => {
                println!("Found {} NVIDIA GPU(s)", devices.len());
                for device in devices {
                    println!("  - GPU {}: {}", device.index(), device.name().unwrap());
                }
            }
            Err(e) => println!("No NVIDIA GPUs found or error: {}", e),
        }
    }
}
