// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Unified GPU abstraction layer
//!
//! This module provides a vendor-agnostic interface for GPU monitoring across NVIDIA, AMD, and
//! Intel GPUs through a common trait-based system. The [`Device`] trait defines the core API
//! that all GPU backends must implement, while [`GpuCollection`] provides convenient
//! multi-vendor GPU management.
//!
//! # Examples
//!
//! ## Auto-detect all GPUs
//!
//! ```no_run
//! use ironmonlib::gpu::GpuCollection;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Automatically detect and initialize all available GPUs
//! let gpus = GpuCollection::auto_detect()?;
//!
//! println!("Detected {} GPUs:", gpus.device_count());
//!
//! // Get snapshot of all GPUs
//! for (idx, info) in gpus.snapshot_all()?.iter().enumerate() {
//!     println!("GPU {}: {} ({})",
//!         idx,
//!         info.static_info.name,
//!         info.static_info.vendor
//!     );
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Access individual GPU devices
//!
//! ```no_run
//! use ironmonlib::gpu::GpuCollection;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let gpus = GpuCollection::auto_detect()?;
//!
//! // Iterate through devices and query properties
//! for device in gpus.gpus() {
//!     let info = device.info()?;
//!     println!("{} ({})", info.static_info.name, info.static_info.vendor);
//!     
//!     // Temperature
//!     if let Some(temp) = info.dynamic_info.thermal.temperature {
//!         println!("  Temperature: {}°C", temp);
//!     }
//!     
//!     // Utilization
//!     if let Some(util) = info.dynamic_info.utilization {
//!         println!("  Utilization: {}%", util);
//!     }
//!     
//!     // Memory
//!     // `None` where the device reported no figure, which is why these are
//!     // `Option` -- an adapter with no readable memory is not one with none.
//!     if let (Some(used), Some(total)) =
//!         (info.dynamic_info.memory.used, info.dynamic_info.memory.total)
//!     {
//!         println!("  Memory: {} / {} MB", used / 1024 / 1024, total / 1024 / 1024);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Vendor-specific initialization
//!
//! ```no_run,ignore
//! // Note: Vendor-specific initialization requires feature flags
//! // and uses internal types. Use GpuCollection::auto_detect() instead.
//! use ironmonlib::gpu::GpuCollection;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let gpus = GpuCollection::auto_detect()?;
//!
//! // Filter to NVIDIA GPUs only
//! for device in gpus.nvidia_gpus() {
//!     let info = device.info()?;
//!     println!("{} - Power: {:.1}W",
//!         info.static_info.name,
//!         info.dynamic_info.power.draw.unwrap_or(0) as f32 / 1000.0
//!     );
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Feature Flags
//!
//! - `nvidia` - NVIDIA GPU support via NVML (requires CUDA toolkit or NVIDIA driver)
//! - `amd` - AMD GPU support via sysfs/DRM (Linux only, requires amdgpu driver)
//! - `intel` - Intel GPU support via i915/xe drivers (Linux only)
//!
//! # Platform Support
//!
//! | Backend | Linux | Windows | macOS | Requirements |
//! |---------|-------|---------|-------|--------------|
//! | NVIDIA  | ✅    | ✅      | ❌    | libnvidia-ml.so / nvml.dll |
//! | AMD     | ✅    | 🚧      | ❌    | amdgpu driver, /sys/class/drm |
//! | Intel   | ✅    | 🚧      | ❌    | i915 or xe driver, /sys/class/drm |

use serde::{Deserialize, Serialize};
use std::fmt;

// New unified traits module
pub mod traits;

// Re-export key types from traits (with GpuProcess renamed to avoid conflict with legacy)
pub use traits::{
    Clocks, ComputeMode, Device, EccErrors, Error as GpuError, FanSpeed,
    GpuProcess as GpuProcessTrait, LinkState, Memory, MigMode, NvLinkStatus, PciInfo, Power,
    ProcessType, Temperature, TemperatureStatus, TemperatureThresholds, Utilization, Vendor,
};

// New vendor implementations
#[cfg(feature = "nvidia")]
pub mod nvidia_new;

#[cfg(feature = "amd")]
pub mod amd_rocm;

#[cfg(feature = "intel")]
pub mod intel_levelzero;

// Legacy implementations (backward compatibility)
#[cfg(feature = "nvidia")]
pub mod nvidia;

#[cfg(feature = "amd")]
pub mod amd;

#[cfg(feature = "intel")]
pub mod intel;

#[cfg(feature = "apple")]
pub mod apple;

/// Shared Windows GPU helpers (DXGI, WMI perf counters, OHM/LHM temperature)
#[cfg(windows)]
pub mod windows_helpers;

/// GPU vendor identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GpuVendor {
    /// NVIDIA GPUs (NVML-based, Jetson and desktop)
    Nvidia,
    /// AMD GPUs (amdgpu driver via libdrm)
    Amd,
    /// Intel GPUs (i915/xe driver via DRM)
    Intel,
    /// Apple GPUs (Metal-based, M1/M2/M3/M4)
    Apple,
}

impl fmt::Display for GpuVendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpuVendor::Nvidia => write!(f, "NVIDIA"),
            GpuVendor::Amd => write!(f, "AMD"),
            GpuVendor::Intel => write!(f, "Intel"),
            GpuVendor::Apple => write!(f, "Apple"),
        }
    }
}

/// GPU process type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuProcessType {
    /// Graphics/rendering workload
    Graphics,
    /// Compute workload
    Compute,
    /// Mixed graphics and compute
    GraphicsAndCompute,
    /// Unknown/unclassified
    Unknown,
}

/// GPU process information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuProcess {
    /// Process ID
    pub pid: u32,
    /// Process name/command
    pub name: String,
    /// User running the process
    pub user: String,
    /// Process type
    pub process_type: GpuProcessType,
    /// GPU utilization percentage (0-100)
    pub gpu_usage: Option<u8>,
    /// Memory usage in bytes
    pub memory_usage: Option<u64>,
    /// Memory usage percentage (0-100)
    pub memory_usage_percent: Option<u8>,
    /// Encoder utilization percentage (0-100)
    pub encoder_usage: Option<u8>,
    /// Decoder utilization percentage (0-100)
    pub decoder_usage: Option<u8>,
    /// CPU usage percentage (0-100)
    pub cpu_usage: Option<u8>,
    /// CPU memory usage in bytes
    pub cpu_memory: Option<u64>,
}

/// GPU memory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMemory {
    /// Total memory in bytes, or `None` where the driver reports none.
    ///
    /// Every field here was a bare number, and four backends independently
    /// wrote zeros into them because there was no way to say otherwise. Two
    /// said so in a comment while doing it -- `used: 0, // Not available from
    /// powermetrics` in the Apple reader, and the Intel Linux reader writing a
    /// zero total for an integrated part that has no separate VRAM at all. A
    /// GPU reporting 0 bytes of memory is not a plausible reading; it is the
    /// absence of one, and it reached the TUI, the GUI, the Prometheus
    /// exporter, the ontology and the recorded database as a measurement.
    pub total: Option<u64>,
    /// Used memory in bytes, or `None` where unreported. See [`Self::total`].
    pub used: Option<u64>,
    /// Free memory in bytes, or `None` where unreported. See [`Self::total`].
    pub free: Option<u64>,
    /// Memory utilization percentage (0-100), or `None`.
    ///
    /// Derived from `used / total`, so it is absent whenever either is.
    pub utilization: Option<u8>,
}

impl GpuMemory {
    /// Nothing about this device's memory was readable.
    ///
    /// Distinct from a device reporting zeros, which no real adapter does.
    pub fn unreported() -> Self {
        Self {
            total: None,
            used: None,
            free: None,
            utilization: None,
        }
    }

    /// Build from a total and a used figure, deriving free and utilization.
    ///
    /// The derivation is the reason this exists: four backends computed the
    /// percentage by hand and three of them divided by a total that could be
    /// zero.
    pub fn from_total_used(total: u64, used: u64) -> Self {
        Self {
            total: Some(total),
            used: Some(used),
            free: Some(total.saturating_sub(used)),
            utilization: (total > 0).then(|| ((used as f64 / total as f64) * 100.0) as u8),
        }
    }
}

/// GPU clock information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuClocks {
    /// Current graphics/shader clock in MHz
    pub graphics: Option<u32>,
    /// Maximum graphics/shader clock in MHz
    pub graphics_max: Option<u32>,
    /// Current memory clock in MHz
    pub memory: Option<u32>,
    /// Maximum memory clock in MHz
    pub memory_max: Option<u32>,
    /// Current SM/compute clock in MHz
    pub sm: Option<u32>,
    /// Current video clock in MHz
    pub video: Option<u32>,
}

/// GPU power information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuPower {
    /// Current power draw in milliwatts
    pub draw: Option<u32>,
    /// Maximum power limit in milliwatts
    pub limit: Option<u32>,
    /// Default power limit in milliwatts
    pub default_limit: Option<u32>,
    /// Power usage percentage (0-100)
    pub usage_percent: Option<u8>,
}

/// GPU thermal information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuThermal {
    /// Current temperature in Celsius
    pub temperature: Option<i32>,
    /// Maximum temperature threshold in Celsius
    pub max_temperature: Option<i32>,
    /// Critical temperature threshold in Celsius
    pub critical_temperature: Option<i32>,
    /// Fan speed percentage (0-100)
    pub fan_speed: Option<u8>,
    /// Fan speed in RPM
    pub fan_rpm: Option<u32>,
}

/// PCIe link information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcieLinkInfo {
    /// Current PCIe generation (1-6)
    pub current_gen: Option<u8>,
    /// Maximum PCIe generation (1-6)
    pub max_gen: Option<u8>,
    /// Current link width (lanes)
    pub current_width: Option<u8>,
    /// Maximum link width (lanes)
    pub max_width: Option<u8>,
    /// Current link speed in MT/s (Megatransfers per second)
    pub current_speed: Option<u32>,
    /// Maximum link speed in MT/s
    pub max_speed: Option<u32>,
    /// TX throughput in bytes/sec
    pub tx_throughput: Option<u64>,
    /// RX throughput in bytes/sec
    pub rx_throughput: Option<u64>,
}

/// GPU hardware engine utilization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuEngines {
    /// Graphics/3D engine utilization (0-100)
    pub graphics: Option<u8>,
    /// Compute engine utilization (0-100)
    pub compute: Option<u8>,
    /// Video encoder utilization (0-100)
    pub encoder: Option<u8>,
    /// Video decoder utilization (0-100)
    pub decoder: Option<u8>,
    /// Copy/DMA engine utilization (0-100)
    pub copy: Option<u8>,
    /// Vendor-specific engines (e.g., DLA on Jetson)
    pub vendor_specific: Vec<(String, u8)>,
}

/// Static GPU information (doesn't change during runtime)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStaticInfo {
    /// GPU index/ID
    pub index: usize,
    /// GPU vendor
    pub vendor: GpuVendor,
    /// GPU name/model
    pub name: String,
    /// PCI bus ID (e.g., "0000:01:00.0")
    pub pci_bus_id: Option<String>,
    /// GPU UUID (vendor-specific format)
    pub uuid: Option<String>,
    /// VBIOS version
    pub vbios_version: Option<String>,
    /// Driver version
    pub driver_version: Option<String>,
    /// Compute capability (for NVIDIA)
    pub compute_capability: Option<(u32, u32)>,
    /// CUDA cores / Shader units / Execution units
    pub shader_cores: Option<u32>,
    /// L2 cache size in bytes
    pub l2_cache: Option<u64>,
    /// Number of execution engines
    pub num_engines: Option<u32>,
    /// Is integrated GPU (shares system memory)
    pub integrated: bool,
}

/// Dynamic GPU information (updated each snapshot)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDynamicInfo {
    /// GPU utilization percentage (0-100), or `None` where the device reports
    /// no utilization counter.
    ///
    /// This was a bare `u8`. Two backends returned a literal `0` having read
    /// nothing at all, and the NVIDIA reader wrote `utilization_rates().ok()
    /// .map(..).unwrap_or(0)` -- so an NVML query that failed reported an idle
    /// card. An idle GPU and an unreadable one are the same number and
    /// different facts, and `0%` is the one a capacity planner acts on.
    pub utilization: Option<u8>,
    /// Memory information
    pub memory: GpuMemory,
    /// Clock speeds
    pub clocks: GpuClocks,
    /// Power information
    pub power: GpuPower,
    /// Thermal information
    pub thermal: GpuThermal,
    /// PCIe link information
    pub pcie: PcieLinkInfo,
    /// Hardware engine utilization
    pub engines: GpuEngines,
    /// Running processes on this GPU
    pub processes: Vec<GpuProcess>,
}

/// Complete GPU information (static + dynamic)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    /// Static information
    pub static_info: GpuStaticInfo,
    /// Dynamic information
    pub dynamic_info: GpuDynamicInfo,
}

impl GpuInfo {
    /// Get GPU index
    pub fn index(&self) -> usize {
        self.static_info.index
    }

    /// Get GPU vendor
    pub fn vendor(&self) -> GpuVendor {
        self.static_info.vendor
    }

    /// Get GPU name
    pub fn name(&self) -> &str {
        &self.static_info.name
    }

    /// GPU utilization percentage, or `None` where the device reports no
    /// utilization counter.
    pub fn utilization(&self) -> Option<u8> {
        self.dynamic_info.utilization
    }

    /// Get GPU temperature
    pub fn temperature(&self) -> Option<i32> {
        self.dynamic_info.thermal.temperature
    }

    /// Memory usage percentage, or `None` where the device reported no
    /// memory figures to derive it from.
    pub fn memory_utilization(&self) -> Option<u8> {
        self.dynamic_info.memory.utilization
    }

    /// Get power draw in watts
    pub fn power_draw_watts(&self) -> Option<f32> {
        self.dynamic_info.power.draw.map(|mw| mw as f32 / 1000.0)
    }

    /// Get number of running processes
    pub fn num_processes(&self) -> usize {
        self.dynamic_info.processes.len()
    }
}

/// Unified GPU interface trait
///
/// This trait provides a vendor-agnostic interface for GPU monitoring and control.
/// All GPU vendors (NVIDIA, AMD, Intel) implement this trait.
pub trait Gpu: Send + Sync {
    /// Get static GPU information
    fn static_info(&self) -> Result<GpuStaticInfo, crate::Error>;

    /// Get dynamic GPU information (snapshot)
    fn dynamic_info(&self) -> Result<GpuDynamicInfo, crate::Error>;

    /// Get complete GPU information
    fn info(&self) -> Result<GpuInfo, crate::Error> {
        Ok(GpuInfo {
            static_info: self.static_info()?,
            dynamic_info: self.dynamic_info()?,
        })
    }

    /// Get GPU vendor
    fn vendor(&self) -> GpuVendor;

    /// Get GPU index
    fn index(&self) -> usize;

    /// Get GPU name/model
    fn name(&self) -> Result<String, crate::Error>;

    /// Get running processes on this GPU
    fn processes(&self) -> Result<Vec<GpuProcess>, crate::Error>;

    /// Kill a process by PID
    fn kill_process(&self, pid: u32) -> Result<(), crate::Error>;

    /// Set power limit in milliwatts (if supported)
    fn set_power_limit(&mut self, limit_mw: u32) -> Result<(), crate::Error> {
        let _ = limit_mw;
        Err(crate::Error::NotSupported(
            "Power limit control not supported for this GPU".to_string(),
        ))
    }

    /// Set fan speed percentage (if supported)
    fn set_fan_speed(&mut self, speed_percent: u8) -> Result<(), crate::Error> {
        let _ = speed_percent;
        Err(crate::Error::NotSupported(
            "Fan control not supported for this GPU".to_string(),
        ))
    }

    /// Enable/disable auto fan control (if supported)
    fn set_fan_auto(&mut self, enabled: bool) -> Result<(), crate::Error> {
        let _ = enabled;
        Err(crate::Error::NotSupported(
            "Fan control not supported for this GPU".to_string(),
        ))
    }

    /// Get vendor-specific data as JSON (for advanced features)
    fn vendor_specific_data(&self) -> Result<serde_json::Value, crate::Error> {
        Ok(serde_json::Value::Null)
    }
}

/// Adapter to convert new trait-based Device to legacy Gpu trait
#[allow(dead_code)]
struct TraitGpuAdapter {
    device: Box<dyn Device>,
}

impl TraitGpuAdapter {
    #[allow(dead_code)]
    fn new(device: Box<dyn Device>) -> Self {
        Self { device }
    }

    #[allow(dead_code)]
    fn convert_vendor(vendor: Vendor) -> GpuVendor {
        match vendor {
            Vendor::Nvidia => GpuVendor::Nvidia,
            Vendor::Amd => GpuVendor::Amd,
            Vendor::Intel => GpuVendor::Intel,
            Vendor::Apple => GpuVendor::Apple,
        }
    }
}

impl Gpu for TraitGpuAdapter {
    fn static_info(&self) -> Result<GpuStaticInfo, crate::Error> {
        let name = self.device.name().unwrap_or_else(|_| "Unknown".to_string());
        let pci = self.device.pci_info().ok();
        let uuid = self.device.uuid().ok();
        let driver = self.device.driver_version().ok();

        Ok(GpuStaticInfo {
            index: self.device.index() as usize,
            vendor: Self::convert_vendor(self.device.vendor()),
            name,
            pci_bus_id: pci.as_ref().map(|p| p.bus_id.clone()),
            uuid,
            vbios_version: None,
            driver_version: driver,
            compute_capability: None,
            shader_cores: None,
            l2_cache: None,
            num_engines: None,
            integrated: false,
        })
    }

    fn dynamic_info(&self) -> Result<GpuDynamicInfo, crate::Error> {
        let util = self.device.utilization().ok();
        let mem = self.device.memory().ok();
        let clocks = self.device.clocks().ok();
        let power = self.device.power().ok();
        let temp = self.device.temperature().ok();
        let fan = self.device.fan_speed().ok().flatten();
        let pci = self.device.pci_info().ok();

        // `.max(1)` in the old percentage was guarding a division by a total
        // that could be zero -- which is to say the total could be absent, and
        // the guard turned that into "0% of one byte" rather than into an
        // absence. A device whose memory query failed now reports nothing.
        let memory = match mem {
            Some(m) => GpuMemory::from_total_used(m.total, m.used),
            None => GpuMemory::unreported(),
        };

        let clocks_info = if let Some(c) = clocks {
            GpuClocks {
                graphics: Some(c.graphics),
                graphics_max: None,
                memory: Some(c.memory),
                memory_max: None,
                sm: c.sm,
                video: c.video,
            }
        } else {
            GpuClocks {
                graphics: None,
                graphics_max: None,
                memory: None,
                memory_max: None,
                sm: None,
                video: None,
            }
        };

        let power_info = if let Some(p) = power {
            // Watts to milliwatts, preserving absence rather than wrapping a
            // zero in `Some`. The percentage needs both figures and is absent
            // when either is -- it used to guard only against a zero limit,
            // which was the only way absence could reach it.
            let mw = |w: Option<f32>| w.map(|v| (v * 1000.0) as u32);
            GpuPower {
                draw: mw(p.current),
                limit: mw(p.limit),
                default_limit: mw(p.default_limit),
                usage_percent: match (p.current, p.limit) {
                    (Some(cur), Some(lim)) if lim > 0.0 => Some(((cur / lim) * 100.0) as u8),
                    _ => None,
                },
            }
        } else {
            GpuPower {
                draw: None,
                limit: None,
                default_limit: None,
                usage_percent: None,
            }
        };

        let thermal_info = if let Some(t) = temp {
            GpuThermal {
                temperature: t.edge.map(|e| e as i32),
                max_temperature: t
                    .thresholds
                    .as_ref()
                    .and_then(|th| th.critical.map(|c| c as i32)),
                critical_temperature: t
                    .thresholds
                    .as_ref()
                    .and_then(|th| th.shutdown.map(|s| s as i32)),
                fan_speed: fan.as_ref().map(|f| match f {
                    FanSpeed::Percent(p) => *p as u8,
                    FanSpeed::Rpm(_) => 0u8, // RPM doesn't map to percent
                }),
                fan_rpm: fan.as_ref().and_then(|f| match f {
                    FanSpeed::Rpm(r) => Some(*r),
                    _ => None,
                }),
            }
        } else {
            GpuThermal {
                temperature: None,
                max_temperature: None,
                critical_temperature: None,
                fan_speed: None,
                fan_rpm: None,
            }
        };

        let pcie_info = if let Some(p) = pci {
            PcieLinkInfo {
                current_gen: p.pcie_generation.map(|g| g as u8),
                max_gen: p.pcie_generation.map(|g| g as u8),
                current_width: p.pcie_link_width.map(|w| w as u8),
                max_width: p.pcie_link_width.map(|w| w as u8),
                current_speed: None,
                max_speed: None,
                tx_throughput: None,
                rx_throughput: None,
            }
        } else {
            PcieLinkInfo {
                current_gen: None,
                max_gen: None,
                current_width: None,
                max_width: None,
                current_speed: None,
                max_speed: None,
                tx_throughput: None,
                rx_throughput: None,
            }
        };

        Ok(GpuDynamicInfo {
            // A failed utilization query reports nothing, matching the
            // memory beside it rather than claiming an idle device.
            utilization: util.as_ref().map(|u| u.gpu as u8),
            memory,
            clocks: clocks_info,
            power: power_info,
            thermal: thermal_info,
            pcie: pcie_info,
            engines: GpuEngines {
                graphics: util.as_ref().map(|u| u.gpu as u8),
                compute: None,
                encoder: util.as_ref().and_then(|u| u.encoder.map(|e| e as u8)),
                decoder: util.as_ref().and_then(|u| u.decoder.map(|d| d as u8)),
                copy: None,
                vendor_specific: vec![],
            },
            processes: vec![],
        })
    }

    fn vendor(&self) -> GpuVendor {
        Self::convert_vendor(self.device.vendor())
    }

    fn index(&self) -> usize {
        self.device.index() as usize
    }

    fn name(&self) -> Result<String, crate::Error> {
        self.device
            .name()
            .map_err(|e| crate::Error::GpuError(e.to_string()))
    }

    fn processes(&self) -> Result<Vec<GpuProcess>, crate::Error> {
        // Get processes from the underlying Device trait
        let trait_procs = self
            .device
            .processes()
            .map_err(|e| crate::Error::GpuError(e.to_string()))?;

        // Convert from trait GpuProcess to struct GpuProcess
        let mut processes = Vec::new();
        for proc in trait_procs {
            let name = proc
                .name()
                .unwrap_or_else(|_| format!("pid_{}", proc.pid()));
            let memory_usage = proc.gpu_memory_used().ok();
            let gpu_usage = proc.sm_utilization().ok().flatten().map(|u| u as u8);
            let encoder_usage = proc.encoder_utilization().ok().flatten().map(|u| u as u8);
            let decoder_usage = proc.decoder_utilization().ok().flatten().map(|u| u as u8);

            let process_type = match proc.process_type() {
                crate::gpu::traits::ProcessType::Compute => GpuProcessType::Compute,
                crate::gpu::traits::ProcessType::Graphics => GpuProcessType::Graphics,
                crate::gpu::traits::ProcessType::Mixed => GpuProcessType::GraphicsAndCompute,
            };

            processes.push(GpuProcess {
                pid: proc.pid(),
                name,
                user: String::new(), // Not available from trait
                process_type,
                gpu_usage,
                memory_usage,
                memory_usage_percent: None,
                encoder_usage,
                decoder_usage,
                cpu_usage: proc.cpu_utilization().ok().flatten().map(|u| u as u8),
                cpu_memory: proc.host_memory_used().ok().flatten(),
            });
        }

        Ok(processes)
    }

    fn kill_process(&self, _pid: u32) -> Result<(), crate::Error> {
        Err(crate::Error::NotSupported(
            "Process kill not implemented for this adapter".to_string(),
        ))
    }
}

/// GPU collection representing all detected GPUs
pub struct GpuCollection {
    gpus: Vec<Box<dyn Gpu>>,
}

impl GpuCollection {
    /// Create a new GPU collection
    pub fn new() -> Self {
        Self { gpus: Vec::new() }
    }

    /// Auto-detect and add all available GPUs
    pub fn auto_detect() -> Result<Self, crate::Error> {
        #[allow(unused_mut)]
        let mut collection = Self::new();

        #[cfg(feature = "nvidia")]
        collection.detect_nvidia()?;

        #[cfg(feature = "amd")]
        collection.detect_amd()?;

        #[cfg(feature = "intel")]
        collection.detect_intel()?;

        #[cfg(feature = "apple")]
        collection.detect_apple()?;

        Ok(collection)
    }

    /// Detect NVIDIA GPUs
    #[cfg(feature = "nvidia")]
    pub fn detect_nvidia(&mut self) -> Result<(), crate::Error> {
        nvidia::detect_gpus(self)?;
        Ok(())
    }

    /// Detect AMD GPUs
    #[cfg(feature = "amd")]
    pub fn detect_amd(&mut self) -> Result<(), crate::Error> {
        // Try new trait-based implementation first (sysfs)
        #[cfg(target_os = "linux")]
        {
            if let Ok(gpus) = amd_rocm::enumerate() {
                for gpu in gpus {
                    self.add_gpu(Box::new(TraitGpuAdapter::new(gpu)));
                }
                return Ok(());
            }
        }

        // Fall back to legacy implementation
        amd::detect_gpus(self)?;
        Ok(())
    }

    /// Detect Intel GPUs
    #[cfg(feature = "intel")]
    pub fn detect_intel(&mut self) -> Result<(), crate::Error> {
        // Try new trait-based implementation first (sysfs)
        #[cfg(target_os = "linux")]
        {
            if let Ok(gpus) = intel_levelzero::enumerate() {
                for gpu in gpus {
                    self.add_gpu(Box::new(TraitGpuAdapter::new(gpu)));
                }
                return Ok(());
            }
        }

        // Fall back to legacy implementation
        intel::detect_gpus(self)?;
        Ok(())
    }

    /// Detect Apple Silicon GPUs
    #[cfg(feature = "apple")]
    pub fn detect_apple(&mut self) -> Result<(), crate::Error> {
        #[cfg(target_os = "macos")]
        {
            match apple::AppleGpu::detect_gpus() {
                Ok(gpus) => {
                    for gpu in gpus {
                        self.add_gpu(Box::new(gpu));
                    }
                }
                Err(_) => {} // Silently skip if no Apple GPU found
            }
        }
        Ok(())
    }

    /// Add a GPU to the collection
    pub fn add_gpu(&mut self, gpu: Box<dyn Gpu>) {
        self.gpus.push(gpu);
    }

    /// Get all GPUs
    pub fn gpus(&self) -> &[Box<dyn Gpu>] {
        &self.gpus
    }

    /// Get mutable reference to all GPUs
    pub fn gpus_mut(&mut self) -> &mut [Box<dyn Gpu>] {
        &mut self.gpus
    }

    /// Get number of GPUs
    pub fn len(&self) -> usize {
        self.gpus.len()
    }

    /// Get number of GPUs (alias for len())
    pub fn device_count(&self) -> usize {
        self.gpus.len()
    }

    /// Check if collection is empty
    pub fn is_empty(&self) -> bool {
        self.gpus.is_empty()
    }

    /// Get GPU by index
    pub fn get(&self, index: usize) -> Option<&dyn Gpu> {
        self.gpus.get(index).map(|gpu| gpu.as_ref())
    }

    /// Get mutable GPU by index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut (dyn Gpu + 'static)> {
        self.gpus.get_mut(index).map(|gpu| gpu.as_mut())
    }

    /// Get all NVIDIA GPUs
    pub fn nvidia_gpus(&self) -> Vec<&dyn Gpu> {
        self.gpus
            .iter()
            .filter(|gpu| gpu.vendor() == GpuVendor::Nvidia)
            .map(|gpu| gpu.as_ref())
            .collect()
    }

    /// Get all AMD GPUs
    pub fn amd_gpus(&self) -> Vec<&dyn Gpu> {
        self.gpus
            .iter()
            .filter(|gpu| gpu.vendor() == GpuVendor::Amd)
            .map(|gpu| gpu.as_ref())
            .collect()
    }

    /// Get all Intel GPUs
    pub fn intel_gpus(&self) -> Vec<&dyn Gpu> {
        self.gpus
            .iter()
            .filter(|gpu| gpu.vendor() == GpuVendor::Intel)
            .map(|gpu| gpu.as_ref())
            .collect()
    }

    /// Get all GPU info snapshots.
    ///
    /// Queries run concurrently across devices. Vendor queries are dominated by
    /// driver round-trips (NVML, DXGI, sysfs) rather than CPU, so on a multi-GPU
    /// host this turns an N-device serial walk into roughly the cost of the
    /// slowest single device.
    ///
    /// Short-circuits on the first error, matching the original semantics. Use
    /// [`GpuCollection::snapshot_all_partial`] when one failing device should not
    /// blank the rest.
    pub fn snapshot_all(&self) -> Result<Vec<GpuInfo>, crate::Error> {
        self.snapshot_all_partial().into_iter().collect()
    }

    /// Get all GPU info snapshots, preserving per-device results.
    ///
    /// The returned vector is always the same length as [`GpuCollection::len`] and
    /// is index-aligned with [`GpuCollection::gpus`], so a failing device does not
    /// shift the indices of its neighbours. This matters for callers that pair
    /// dynamic samples against static descriptors captured earlier.
    pub fn snapshot_all_partial(&self) -> Vec<Result<GpuInfo, crate::Error>> {
        // Spawning is only worth it past one device.
        if self.gpus.len() <= 1 {
            return self.gpus.iter().map(|gpu| gpu.info()).collect();
        }

        std::thread::scope(|scope| {
            let handles: Vec<_> = self
                .gpus
                .iter()
                .map(|gpu| {
                    scope.spawn(move || {
                        // Some Windows GPU paths reach DXGI/WMI, which require COM
                        // to be initialized on the calling thread.
                        #[cfg(target_os = "windows")]
                        let _com = wmi::COMLibrary::new().ok();

                        gpu.info()
                    })
                })
                .collect();

            handles
                .into_iter()
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        Err(crate::error::Error::GpuError(
                            "GPU query panicked".to_string(),
                        ))
                    })
                })
                .collect()
        })
    }
}

impl Default for GpuCollection {
    fn default() -> Self {
        Self::new()
    }
}
