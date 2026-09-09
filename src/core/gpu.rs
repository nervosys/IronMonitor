//! GPU monitoring and control

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// GPU type identifier
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuType {
    /// Integrated GPU (e.g., Jetson)
    Integrated,
    /// Discrete GPU (e.g., desktop)
    Discrete,
}

/// GPU frequency information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuFrequency {
    /// Current frequency in MHz
    pub current: u32,
    /// Minimum frequency in MHz
    pub min: u32,
    /// Maximum frequency in MHz
    pub max: u32,
    /// Frequency governor
    pub governor: String,
    /// GPC frequencies for Orin/Thor series (optional)
    pub gpc: Option<Vec<u32>>,
}

/// GPU status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStatus {
    /// GPU load percentage (0.0 - 100.0)
    /// Utilization percentage, or `None` where the device reports no
    /// utilization counter.
    ///
    /// A GPU at 0% and a GPU whose counter could not be read are the same
    /// number and different facts. This was `f32` and took whichever one the
    /// backend happened to produce.
    pub load: Option<f32>,
    /// Railgate status (Jetson only)
    pub railgate: Option<bool>,
    /// TPC PG mask status (Jetson only)
    pub tpc_pg_mask: Option<bool>,
    /// 3D scaling enabled (Jetson only)
    pub scaling_3d: Option<bool>,
    /// Memory used in bytes
    pub memory_used: Option<u64>,
    /// Total memory in bytes
    pub memory_total: Option<u64>,
    /// Free memory in bytes
    pub memory_free: Option<u64>,
    /// Temperature in Celsius
    pub temperature: Option<f32>,
    /// Power draw in watts
    pub power_draw: Option<f32>,
    /// Power limit in watts
    pub power_limit: Option<f32>,
}

/// Complete GPU information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    /// GPU type
    pub gpu_type: GpuType,
    /// GPU status
    pub status: GpuStatus,
    /// Frequency information
    pub frequency: GpuFrequency,
    /// Power control method
    pub power_control: String,
}

/// GPU statistics container
#[derive(Debug, Default)]
pub struct GpuStats {
    /// Map of GPU name to GPU information
    gpus: HashMap<String, GpuInfo>,
}

impl GpuStats {
    /// Create a new GPU stats instance
    pub fn new() -> Self {
        Self {
            gpus: HashMap::new(),
        }
    }

    /// Get all GPUs
    pub fn gpus(&self) -> &HashMap<String, GpuInfo> {
        &self.gpus
    }

    /// Get a specific GPU by name
    pub fn get_gpu(&self, name: &str) -> Option<&GpuInfo> {
        self.gpus.get(name)
    }

    /// Get mutable reference to GPUs
    pub fn gpus_mut(&mut self) -> &mut HashMap<String, GpuInfo> {
        &mut self.gpus
    }

    /// Set 3D scaling for a GPU (Jetson only)
    pub fn set_3d_scaling(&mut self, name: &str, enabled: bool) -> Result<()> {
        if !self.gpus.contains_key(name) {
            return Err(IronError::DeviceNotFound(format!(
                "GPU '{}' not found",
                name
            )));
        }

        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::jetson::set_gpu_3d_scaling;
            set_gpu_3d_scaling(name, enabled)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = enabled; // Suppress unused warning
            Err(IronError::UnsupportedPlatform(
                "3D scaling control only available on Linux Jetson devices".to_string(),
            ))
        }
    }

    /// Set railgate for a GPU (Jetson only)
    pub fn set_railgate(&mut self, name: &str, enabled: bool) -> Result<()> {
        if !self.gpus.contains_key(name) {
            return Err(IronError::DeviceNotFound(format!(
                "GPU '{}' not found",
                name
            )));
        }

        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::jetson::set_gpu_railgate;
            set_gpu_railgate(name, enabled)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = enabled; // Suppress unused warning
            Err(IronError::UnsupportedPlatform(
                "Railgate control only available on Linux Jetson devices".to_string(),
            ))
        }
    }
}
