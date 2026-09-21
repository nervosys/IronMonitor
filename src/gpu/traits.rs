//! Unified traits and types for GPU monitoring
//!
//! This module defines the common interface that all GPU backends must implement.

use serde::{Deserialize, Serialize};
use std::fmt;

/// GPU Vendor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Vendor {
    /// NVIDIA GPU (NVML/CUDA)
    Nvidia,
    /// AMD GPU (ROCm/sysfs)
    Amd,
    /// Intel GPU (i915/xe/Level Zero)
    Intel,
    /// Apple Silicon GPU (Metal/powermetrics)
    Apple,
}

impl fmt::Display for Vendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Vendor::Nvidia => write!(f, "NVIDIA"),
            Vendor::Amd => write!(f, "AMD"),
            Vendor::Intel => write!(f, "Intel"),
            Vendor::Apple => write!(f, "Apple"),
        }
    }
}

/// GPU Device trait - common interface for all GPUs
pub trait Device: Send + Sync {
    /// Get the GPU vendor
    fn vendor(&self) -> Vendor;

    /// Get the device index (0-based)
    fn index(&self) -> u32;

    /// Get the device name (e.g., "NVIDIA GeForce RTX 4090")
    fn name(&self) -> Result<String, Error>;

    /// Get the device UUID (unique identifier)
    fn uuid(&self) -> Result<String, Error>;

    /// Get PCI bus information
    fn pci_info(&self) -> Result<PciInfo, Error>;

    /// Get driver version
    fn driver_version(&self) -> Result<String, Error>;

    // === Real-time Metrics ===

    /// Get temperature readings from all available sensors
    fn temperature(&self) -> Result<Temperature, Error>;

    /// Get power consumption and limits
    fn power(&self) -> Result<Power, Error>;

    /// Get clock frequencies (graphics, memory, SM, video)
    fn clocks(&self) -> Result<Clocks, Error>;

    /// Get utilization percentages (GPU, memory, encoders, decoders)
    fn utilization(&self) -> Result<Utilization, Error>;

    /// Get memory usage (total, used, free)
    fn memory(&self) -> Result<Memory, Error>;

    /// Get fan speed (RPM or percentage)
    fn fan_speed(&self) -> Result<Option<FanSpeed>, Error>;

    /// Get performance state (P0-P12 for NVIDIA, power states for others)
    fn performance_state(&self) -> Result<Option<String>, Error>;

    // === Process Monitoring ===

    /// Get list of processes using this GPU
    fn processes(&self) -> Result<Vec<Box<dyn GpuProcess>>, Error>;

    // === Vendor-Specific Features (optional) ===

    /// Get NVLink status (NVIDIA only)
    fn nvlink_status(&self) -> Result<Vec<NvLinkStatus>, Error> {
        Err(Error::NotSupported)
    }

    /// Get MIG mode status (NVIDIA only)
    fn mig_mode(&self) -> Result<MigMode, Error> {
        Err(Error::NotSupported)
    }

    /// Get ECC error counts (NVIDIA, AMD)
    fn ecc_errors(&self) -> Result<EccErrors, Error> {
        Err(Error::NotSupported)
    }

    /// Get compute mode (NVIDIA)
    fn compute_mode(&self) -> Result<Option<ComputeMode>, Error> {
        Ok(None)
    }

    /// Get persistence mode (NVIDIA)
    fn persistence_mode(&self) -> Result<Option<bool>, Error> {
        Ok(None)
    }

    // === Control Functions (may require root/admin) ===

    /// Set power limit (Watts)
    fn set_power_limit(&mut self, _watts: f32) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    /// Lock GPU clocks to specified frequency (MHz)
    fn lock_gpu_clocks(&mut self, _min_mhz: u32, _max_mhz: u32) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    /// Reset GPU clocks to default
    fn reset_gpu_clocks(&mut self) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    /// Set persistence mode (NVIDIA)
    fn set_persistence_mode(&mut self, _enabled: bool) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    /// Set compute mode (NVIDIA)
    fn set_compute_mode(&mut self, _mode: ComputeMode) -> Result<(), Error> {
        Err(Error::NotSupported)
    }
}

/// GPU Process trait - information about a process using GPU
pub trait GpuProcess: Send + Sync {
    /// Process ID
    fn pid(&self) -> u32;

    /// Process name/command
    fn name(&self) -> Result<String, Error>;

    /// Process type (Compute, Graphics, Mixed)
    fn process_type(&self) -> ProcessType;

    /// GPU memory used by this process (bytes)
    fn gpu_memory_used(&self) -> Result<u64, Error>;

    /// SM utilization percentage (0-100)
    fn sm_utilization(&self) -> Result<Option<f32>, Error> {
        Ok(None)
    }

    /// Encoder utilization percentage (0-100)
    fn encoder_utilization(&self) -> Result<Option<f32>, Error> {
        Ok(None)
    }

    /// Decoder utilization percentage (0-100)
    fn decoder_utilization(&self) -> Result<Option<f32>, Error> {
        Ok(None)
    }

    /// Host CPU utilization percentage (0-100)
    fn cpu_utilization(&self) -> Result<Option<f32>, Error> {
        Ok(None)
    }

    /// Host memory used (bytes)
    fn host_memory_used(&self) -> Result<Option<u64>, Error> {
        Ok(None)
    }

    /// Process running time (seconds)
    fn running_time(&self) -> Result<Option<u64>, Error> {
        Ok(None)
    }
}

// === Data Structures ===

/// PCI bus location and link information for a GPU device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PciInfo {
    /// PCI domain number
    pub domain: u32,
    /// PCI bus number
    pub bus: u8,
    /// PCI device number
    pub device: u8,
    /// PCI function number
    pub function: u8,
    /// Full PCI bus ID string (e.g., `"0000:01:00.0"`)
    pub bus_id: String,
    /// PCIe generation (1-5), if available
    pub pcie_generation: Option<u32>,
    /// PCIe link width in lanes, if available
    pub pcie_link_width: Option<u32>,
}

/// Temperature readings from various sensors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Temperature {
    /// Edge temperature (AMD) in Celsius
    pub edge: Option<f32>,
    /// Junction/GPU temperature (NVIDIA, Intel) in Celsius
    pub junction: Option<f32>,
    /// Memory temperature in Celsius
    pub memory: Option<f32>,
    /// Hotspot temperature (AMD) in Celsius
    pub hotspot: Option<f32>,
    /// VR GFX voltage regulator temperature (AMD) in Celsius
    pub vr_gfx: Option<f32>,
    /// VR SOC voltage regulator temperature (AMD) in Celsius
    pub vr_soc: Option<f32>,
    /// VR MEM voltage regulator temperature (AMD) in Celsius
    pub vr_mem: Option<f32>,
    /// HBM memory temperatures (AMD MI100/MI200/MI300) in Celsius
    pub hbm: Option<Vec<f32>>,
    /// Temperature thresholds (slowdown, shutdown, critical)
    pub thresholds: Option<TemperatureThresholds>,
}

/// Temperature thresholds for GPU thermal management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureThresholds {
    /// Slowdown threshold (°C) - GPU begins throttling performance
    pub slowdown: Option<f32>,
    /// Shutdown threshold (°C) - GPU will shut down to prevent damage
    pub shutdown: Option<f32>,
    /// Critical threshold (°C) - Maximum safe operating temperature
    pub critical: Option<f32>,
    /// Memory critical threshold (°C) - Maximum safe memory temperature
    pub memory_critical: Option<f32>,
}

impl Temperature {
    /// Get the primary/most important temperature reading
    pub fn primary(&self) -> Option<f32> {
        self.junction.or(self.hotspot).or(self.edge).or(self.memory)
    }

    /// Get the maximum temperature across all sensors
    pub fn max(&self) -> Option<f32> {
        let mut temps = Vec::new();
        if let Some(t) = self.edge {
            temps.push(t);
        }
        if let Some(t) = self.junction {
            temps.push(t);
        }
        if let Some(t) = self.memory {
            temps.push(t);
        }
        if let Some(t) = self.hotspot {
            temps.push(t);
        }
        if let Some(t) = self.vr_gfx {
            temps.push(t);
        }
        if let Some(t) = self.vr_soc {
            temps.push(t);
        }
        if let Some(t) = self.vr_mem {
            temps.push(t);
        }
        if let Some(hbm_temps) = &self.hbm {
            temps.extend(hbm_temps);
        }
        temps.into_iter().reduce(f32::max)
    }

    /// Get temperature status relative to thresholds
    pub fn status(&self) -> TemperatureStatus {
        let Some(temp) = self.primary() else {
            return TemperatureStatus::Unknown;
        };

        let Some(thresholds) = &self.thresholds else {
            return TemperatureStatus::Normal;
        };

        // Check critical first (most severe)
        if let Some(critical) = thresholds.critical {
            if temp >= critical {
                return TemperatureStatus::Critical;
            }
        }

        // Check shutdown threshold
        if let Some(shutdown) = thresholds.shutdown {
            if temp >= shutdown {
                return TemperatureStatus::Shutdown;
            }
        }

        // Check slowdown/throttle threshold
        if let Some(slowdown) = thresholds.slowdown {
            if temp >= slowdown {
                return TemperatureStatus::Throttling;
            }
        }

        // Normal if below all thresholds
        TemperatureStatus::Normal
    }
}

/// Temperature status based on thresholds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemperatureStatus {
    /// Temperature is normal
    Normal,
    /// Temperature is above slowdown threshold (throttling)
    Throttling,
    /// Temperature is approaching shutdown threshold
    Shutdown,
    /// Temperature is at or above critical threshold
    Critical,
    /// Temperature data not available or no thresholds defined
    Unknown,
}

impl fmt::Display for TemperatureStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemperatureStatus::Normal => write!(f, "Normal"),
            TemperatureStatus::Throttling => write!(f, "Throttling"),
            TemperatureStatus::Shutdown => write!(f, "Shutdown Warning"),
            TemperatureStatus::Critical => write!(f, "CRITICAL"),
            TemperatureStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Power consumption and limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Power {
    /// Current power draw in Watts, or `None` where unreported.
    ///
    /// Only `average` carried an `Option` here, and its doc said "(if
    /// available)" -- implying the other six always were. They are not. Every
    /// one of them is a driver query that can decline, and all three backends
    /// read them *as* `Option` and then threw the absence away at the moment of
    /// construction:
    ///
    /// ```text
    /// current: current.unwrap_or(0.0),
    /// limit: limit.unwrap_or(0.0),
    /// default_limit: default_limit.unwrap_or(0.0),
    /// ```
    ///
    /// The absence then got *re-wrapped* as present: `GpuPower::default_limit`
    /// is `Option<u32>` and was filled with `Some((p.default_limit * 1000.0) as
    /// u32)`, so a card whose default limit NVML declined published `Some(0)` --
    /// a type that could say "unreported" being handed a zero by a type that
    /// could not.
    pub current: Option<f32>,
    /// Average power draw in Watts, where the device reports one separately
    /// from the instantaneous draw.
    pub average: Option<f32>,
    /// Power limit in Watts, or `None` where unreported.
    pub limit: Option<f32>,
    /// Default power limit in Watts, or `None` where unreported.
    pub default_limit: Option<f32>,
    /// Minimum settable power limit in Watts, or `None` where unreported.
    pub min_limit: Option<f32>,
    /// Maximum settable power limit in Watts, or `None` where unreported.
    pub max_limit: Option<f32>,
    /// Enforced power limit, which may differ from `limit` when the system caps
    /// it. `None` where unreported.
    pub enforced_limit: Option<f32>,
}

/// Clock frequencies
///
/// **Every field is optional, including the two that were not.** `graphics` and
/// `memory` were bare `u32`, so a backend whose sysfs read or NVML query failed
/// had no way to say so and wrote `0` — a GPU idling at 0 MHz and a GPU that
/// could not be asked became the same number. `GpuClocks` in the parent module
/// was corrected for exactly this; these types were a second family that missed
/// the change, and the backends behind them are the *preferred* path for AMD and
/// Intel on Linux.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clocks {
    /// Graphics/shader clock in MHz, or `None` where it could not be read.
    pub graphics: Option<u32>,
    /// Memory clock in MHz, or `None` where it could not be read.
    pub memory: Option<u32>,
    /// SM (Streaming Multiprocessor) clock in MHz (NVIDIA)
    pub sm: Option<u32>,
    /// Video clock in MHz (NVIDIA)
    pub video: Option<u32>,
}

/// Utilization percentages
///
/// `gpu` and `memory` are optional for the reason given on [`Clocks`]: an idle
/// card and an unreadable one are different facts, and `0%` is the one a
/// capacity planner acts on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utilization {
    /// GPU utilization (0-100%), or `None` where it could not be read.
    pub gpu: Option<f32>,
    /// Memory controller utilization (0-100%), or `None` where unreadable.
    pub memory: Option<f32>,
    /// Video encoder utilization (0-100%)
    pub encoder: Option<f32>,
    /// Video decoder utilization (0-100%)
    pub decoder: Option<f32>,
    /// JPEG encoder utilization (0-100%, NVIDIA only)
    pub jpeg: Option<f32>,
    /// Optical Flow Accelerator utilization (0-100%, NVIDIA only)
    pub ofa: Option<f32>,
}

/// Memory usage
///
/// Optional for the reason given on [`Clocks`]. A GPU reporting 0 bytes of
/// memory is not a plausible reading; it is the absence of one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Total memory in bytes, or `None` where it could not be read.
    pub total: Option<u64>,
    /// Used memory in bytes, or `None` where it could not be read.
    pub used: Option<u64>,
    /// Free memory in bytes, or `None` where it could not be read or derived.
    pub free: Option<u64>,
    /// BAR1 total memory (NVIDIA)
    pub bar1_total: Option<u64>,
    /// BAR1 used memory (NVIDIA)
    pub bar1_used: Option<u64>,
}

impl Memory {
    /// Memory utilization percentage.
    ///
    /// `None` whenever either operand is absent, and whenever the total is
    /// zero: a percentage of nothing is not a measurement, and returning `0.0`
    /// for it is the same fabrication this type was changed to prevent.
    pub fn utilization_percent(&self) -> Option<f32> {
        let (total, used) = (self.total?, self.used?);
        if total == 0 {
            return None;
        }
        Some((used as f64 / total as f64 * 100.0) as f32)
    }
}

/// Fan speed measurement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FanSpeed {
    /// Fan speed in revolutions per minute
    Rpm(u32),
    /// Fan speed as a percentage of maximum
    Percent(u32),
}

/// Type of workload a GPU process is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessType {
    /// GPGPU / compute-only workload
    Compute,
    /// Graphics / rendering workload
    Graphics,
    /// Mixed compute and graphics workload
    Mixed,
}

/// Compute mode controlling how many processes may use the GPU (NVIDIA).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputeMode {
    /// Multiple contexts allowed per device (default)
    Default,
    /// Only one context per device, per thread
    ExclusiveThread,
    /// No compute contexts allowed
    Prohibited,
    /// Only one context per device, per process
    ExclusiveProcess,
}

/// NVLink interconnect status (NVIDIA).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvLinkStatus {
    /// NVLink lane identifier
    pub link_id: u32,
    /// Current link state
    pub state: LinkState,
    /// NVLink version (e.g., 3 for NVLink 3.0)
    pub version: u32,
    /// Type of remote device (e.g., "GPU", "CPU")
    pub remote_device_type: String,
    /// PCI bus ID of the remote device
    pub remote_pci_bus_id: String,
}

/// State of an NVLink connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkState {
    /// Link is up and operational
    Active,
    /// Link is down or disabled
    Inactive,
    /// Link state could not be determined
    Unknown,
}

/// MIG mode (NVIDIA Multi-Instance GPU).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigMode {
    /// Whether MIG is currently enabled
    pub current: bool,
    /// Whether MIG will be enabled after next GPU reset
    pub pending: bool,
}

/// ECC error counts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EccErrors {
    /// Volatile single-bit errors (since last reboot)
    pub volatile_single_bit: u64,
    /// Volatile double-bit errors (since last reboot)
    pub volatile_double_bit: u64,
    /// Aggregate single-bit errors (total)
    pub aggregate_single_bit: u64,
    /// Aggregate double-bit errors (total)
    pub aggregate_double_bit: u64,
}

/// Errors that can occur during GPU operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The requested operation is not supported by this GPU backend.
    #[error("Operation not supported on this device")]
    NotSupported,

    /// No GPU devices were detected on the system.
    #[error("No GPU devices found")]
    NoDevicesFound,

    /// GPU driver or library initialization failed.
    #[error("Device initialization failed: {0}")]
    InitializationFailed(String),

    /// A query for device metrics or properties failed.
    #[error("Failed to query device: {0}")]
    QueryFailed(String),

    /// A device control operation (e.g., setting clocks) failed.
    #[error("Failed to control device: {0}")]
    ControlFailed(String),

    /// Insufficient permissions to perform the operation.
    #[error("Insufficient permissions: {0}")]
    PermissionDenied(String),

    /// An invalid argument was provided.
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Error from the NVML wrapper library.
    #[cfg(feature = "nvidia")]
    #[error("NVML error: {0}")]
    NvmlError(#[from] nvml_wrapper::error::NvmlError),

    /// An unclassified error.
    #[error("Unknown error: {0}")]
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Temperature tests ===

    #[test]
    fn test_temperature_primary_junction_first() {
        let temp = Temperature {
            edge: Some(55.0),
            junction: Some(65.0),
            memory: Some(50.0),
            hotspot: Some(70.0),
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds: None,
        };
        assert_eq!(temp.primary(), Some(65.0));
    }

    #[test]
    fn test_temperature_primary_fallback_hotspot() {
        let temp = Temperature {
            edge: Some(55.0),
            junction: None,
            memory: None,
            hotspot: Some(70.0),
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds: None,
        };
        assert_eq!(temp.primary(), Some(70.0));
    }

    #[test]
    fn test_temperature_primary_fallback_edge() {
        let temp = Temperature {
            edge: Some(55.0),
            junction: None,
            memory: None,
            hotspot: None,
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds: None,
        };
        assert_eq!(temp.primary(), Some(55.0));
    }

    #[test]
    fn test_temperature_primary_none() {
        let temp = Temperature {
            edge: None,
            junction: None,
            memory: None,
            hotspot: None,
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds: None,
        };
        assert_eq!(temp.primary(), None);
    }

    #[test]
    fn test_temperature_max() {
        let temp = Temperature {
            edge: Some(55.0),
            junction: Some(65.0),
            memory: Some(50.0),
            hotspot: Some(80.0),
            vr_gfx: Some(72.0),
            vr_soc: None,
            vr_mem: None,
            hbm: Some(vec![60.0, 62.0, 85.0]),
            thresholds: None,
        };
        assert_eq!(temp.max(), Some(85.0));
    }

    #[test]
    fn test_temperature_max_none_when_empty() {
        let temp = Temperature {
            edge: None,
            junction: None,
            memory: None,
            hotspot: None,
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds: None,
        };
        assert_eq!(temp.max(), None);
    }

    #[test]
    fn test_temperature_status_normal() {
        let temp = Temperature {
            edge: None,
            junction: Some(50.0),
            memory: None,
            hotspot: None,
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds: Some(TemperatureThresholds {
                slowdown: Some(80.0),
                shutdown: Some(95.0),
                critical: Some(100.0),
                memory_critical: None,
            }),
        };
        assert_eq!(temp.status(), TemperatureStatus::Normal);
    }

    #[test]
    fn test_temperature_status_throttling() {
        let temp = Temperature {
            edge: None,
            junction: Some(85.0),
            memory: None,
            hotspot: None,
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds: Some(TemperatureThresholds {
                slowdown: Some(80.0),
                shutdown: Some(95.0),
                critical: Some(100.0),
                memory_critical: None,
            }),
        };
        assert_eq!(temp.status(), TemperatureStatus::Throttling);
    }

    #[test]
    fn test_temperature_status_shutdown() {
        let temp = Temperature {
            edge: None,
            junction: Some(96.0),
            memory: None,
            hotspot: None,
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds: Some(TemperatureThresholds {
                slowdown: Some(80.0),
                shutdown: Some(95.0),
                critical: Some(100.0),
                memory_critical: None,
            }),
        };
        assert_eq!(temp.status(), TemperatureStatus::Shutdown);
    }

    #[test]
    fn test_temperature_status_critical() {
        let temp = Temperature {
            edge: None,
            junction: Some(105.0),
            memory: None,
            hotspot: None,
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds: Some(TemperatureThresholds {
                slowdown: Some(80.0),
                shutdown: Some(95.0),
                critical: Some(100.0),
                memory_critical: None,
            }),
        };
        assert_eq!(temp.status(), TemperatureStatus::Critical);
    }

    #[test]
    fn test_temperature_status_unknown_when_no_primary() {
        let temp = Temperature {
            edge: None,
            junction: None,
            memory: None,
            hotspot: None,
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds: Some(TemperatureThresholds {
                slowdown: Some(80.0),
                shutdown: Some(95.0),
                critical: Some(100.0),
                memory_critical: None,
            }),
        };
        assert_eq!(temp.status(), TemperatureStatus::Unknown);
    }

    #[test]
    fn test_temperature_status_normal_when_no_thresholds() {
        let temp = Temperature {
            edge: None,
            junction: Some(90.0),
            memory: None,
            hotspot: None,
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds: None,
        };
        assert_eq!(temp.status(), TemperatureStatus::Normal);
    }

    // === Memory tests ===

    #[test]
    fn test_memory_utilization_percent() {
        let mem = Memory {
            total: Some(8 * 1024 * 1024 * 1024), // 8GB
            used: Some(4 * 1024 * 1024 * 1024),  // 4GB
            free: Some(4 * 1024 * 1024 * 1024),
            bar1_total: None,
            bar1_used: None,
        };
        assert!((mem.utilization_percent().expect("both operands present") - 50.0).abs() < 0.01);
    }

    /// A zero total is the absence of a reading, not a card with no memory.
    ///
    /// This asserted `0.0` and therefore asserted the defect: a percentage
    /// computed against a total nobody read is not a measurement, and reporting
    /// it as 0% puts a fabricated figure where a real one belongs.
    #[test]
    fn test_memory_utilization_zero_total() {
        let mem = Memory {
            total: Some(0),
            used: Some(0),
            free: Some(0),
            bar1_total: None,
            bar1_used: None,
        };
        assert_eq!(mem.utilization_percent(), None);
    }

    /// An absent operand yields no percentage at all.
    #[test]
    fn an_unread_total_yields_no_utilization() {
        let mem = Memory {
            total: None,
            used: Some(1024),
            free: None,
            bar1_total: None,
            bar1_used: None,
        };
        assert_eq!(
            mem.utilization_percent(),
            None,
            concat!(
                "a percentage needs both operands; inventing one is how a ",
                "failed read becomes a plausible-looking number"
            )
        );
    }

    #[test]
    fn test_memory_utilization_full() {
        let mem = Memory {
            total: Some(1024),
            used: Some(1024),
            free: Some(0),
            bar1_total: None,
            bar1_used: None,
        };
        assert!((mem.utilization_percent().expect("both present") - 100.0).abs() < 0.01);
    }

    // === Vendor tests ===

    #[test]
    fn test_vendor_display() {
        assert_eq!(Vendor::Nvidia.to_string(), "NVIDIA");
        assert_eq!(Vendor::Amd.to_string(), "AMD");
        assert_eq!(Vendor::Intel.to_string(), "Intel");
        assert_eq!(Vendor::Apple.to_string(), "Apple");
    }

    // === TemperatureStatus tests ===

    #[test]
    fn test_temperature_status_display() {
        assert_eq!(TemperatureStatus::Normal.to_string(), "Normal");
        assert_eq!(TemperatureStatus::Throttling.to_string(), "Throttling");
        assert_eq!(TemperatureStatus::Shutdown.to_string(), "Shutdown Warning");
        assert_eq!(TemperatureStatus::Critical.to_string(), "CRITICAL");
        assert_eq!(TemperatureStatus::Unknown.to_string(), "Unknown");
    }
}
