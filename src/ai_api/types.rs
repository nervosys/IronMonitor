//! AI API Types - Data structures for tool results
//!
//! These types are designed to be serializable and provide clear, structured
//! data that AI systems can easily understand and reason about.

use serde::{Deserialize, Serialize};

/// Complete system summary - single snapshot of all key metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemSummary {
    /// Unix timestamp of this snapshot
    pub timestamp: u64,

    // GPU Summary
    /// Number of GPUs detected
    pub gpu_count: usize,
    /// Summary for each GPU
    pub gpus: Vec<GpuSummary>,

    // Process Summary
    /// Total number of processes
    pub process_count: usize,
    /// Total CPU usage across all processes
    pub total_cpu_percent: f32,
    /// Number of processes using GPU
    pub gpu_process_count: usize,
    /// Top 5 CPU-consuming processes
    pub top_cpu_processes: Vec<ProcessSummary>,
    /// Top 5 memory-consuming processes
    pub top_memory_processes: Vec<ProcessSummary>,
    /// Top 5 GPU memory-consuming processes
    pub top_gpu_processes: Vec<ProcessSummary>,

    // Memory Summary
    /// Memory information
    pub memory: Option<MemorySummary>,

    // Network Summary
    /// Number of active network interfaces
    pub active_network_interfaces: usize,
    /// Network interface summaries
    pub network_interfaces: Vec<NetworkSummary>,

    // Disk Summary
    /// Number of disks
    pub disk_count: usize,
    /// Disk summaries
    pub disks: Vec<DiskSummary>,
}

/// GPU summary information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSummary {
    /// GPU name/model
    pub name: String,
    /// Vendor (NVIDIA, AMD, Intel, Apple)
    pub vendor: String,
    /// Current utilization percentage
    /// Utilization percentage, or `None` where the device reports no counter.
    pub utilization_percent: Option<f32>,
    /// Used memory in MB
    /// GPU memory used in MB, or `None` where the device reported none.
    pub memory_used_mb: Option<u64>,
    /// Total memory in MB
    /// GPU memory total in MB, or `None` where the device reported none.
    pub memory_total_mb: Option<u64>,
    /// Temperature in Celsius
    pub temperature_c: Option<i32>,
    /// Power draw in watts
    pub power_watts: Option<f32>,
}

/// Process summary information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSummary {
    /// Process ID
    pub pid: u32,
    /// Process name
    pub name: String,
    /// CPU usage percentage
    pub cpu_percent: f32,
    /// Memory usage in MB
    pub memory_mb: u64,
    /// GPU memory usage in MB, or `None` where no device reported one.
    ///
    /// NVML does not expose per-process GPU memory under Windows' WDDM driver
    /// model, so this was `0` for every process there and an agent had no way
    /// to tell that from a process using none.
    pub gpu_memory_mb: Option<u64>,
}

/// Memory summary information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySummary {
    /// Total RAM in MB
    pub total_mb: u64,
    /// Used RAM in MB
    pub used_mb: u64,
    /// Free RAM in MB
    pub free_mb: u64,
    /// Cached RAM in MB
    /// Cached RAM in MB, or `None` where the platform reports none.
    pub cached_mb: Option<u64>,
    /// Total swap in MB
    /// Total swap in MB, or `None` where the platform did not report it.
    ///
    /// Was `u64` filled by `total_or_zero()`, so a machine whose pagefile could
    /// not be read was described to an agent as a machine with no swap.
    pub swap_total_mb: Option<u64>,
    /// Used swap in MB
    /// Swap in use, in MB. See [`Self::swap_total_mb`].
    pub swap_used_mb: Option<u64>,
    /// Memory usage percentage
    pub usage_percent: f32,
}

/// Network interface summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSummary {
    /// Interface name
    pub name: String,
    /// Receive rate in bytes/sec
    /// Receive rate, or `None` until a second sample establishes one.
    ///
    /// A rate is a difference between two readings. This was a bare
    /// number and the first reading reported `0`, which says the link
    /// is idle -- and because the baseline was overwritten before the
    /// subtraction, every reading was the first reading.
    pub rx_bytes_per_sec: Option<u64>,
    /// Transmit rate in bytes/sec
    /// Transmit rate. See [`Self::rx_bytes_per_sec`].
    pub tx_bytes_per_sec: Option<u64>,
    /// Total received in MB
    pub rx_total_mb: u64,
    /// Total transmitted in MB
    pub tx_total_mb: u64,
    /// Whether interface is up
    pub is_up: bool,
}

/// Disk summary information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSummary {
    /// Disk device name
    pub name: String,
    /// Disk model
    pub model: String,
    /// Size in GB
    /// Total size in GB, or `None` where the capacity was not read. A disk
    /// of unknown size is not a disk of zero size.
    pub size_gb: Option<u64>,
    /// Disk type (NVMe, SATA, etc.)
    pub disk_type: String,
    /// Temperature in Celsius
    pub temperature_c: Option<f32>,
}

/// Motherboard sensor reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReading {
    /// Sensor name
    pub name: String,
    /// Sensor type
    pub sensor_type: String,
    /// Current value
    pub value: f64,
    /// Unit
    pub unit: String,
    /// Min value
    pub min: Option<f64>,
    /// Max value
    pub max: Option<f64>,
    /// Critical threshold
    pub critical: Option<f64>,
}

/// System information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfoDetails {
    /// Hostname. `None` when the platform did not report one -- a container
    /// or a freshly imaged host may genuinely have none set, and an empty
    /// string does not say that.
    pub hostname: Option<String>,
    /// OS name
    pub os_name: String,
    /// OS version
    pub os_version: String,
    /// Kernel version. `None` when it could not be read, rather than empty.
    pub kernel_version: Option<String>,
    /// Architecture
    pub architecture: String,
    /// BIOS vendor
    pub bios_vendor: Option<String>,
    /// BIOS version
    pub bios_version: Option<String>,
    /// System manufacturer
    pub manufacturer: Option<String>,
    /// System model
    pub model: Option<String>,
    /// Seconds since boot. `None` when uptime could not be read. This was a
    /// hardcoded `0` presented to an agent as a measurement, which reads as a
    /// machine that just booted.
    pub uptime_seconds: Option<u64>,
    /// Unix timestamp of boot, derived from uptime. `None` for the same
    /// reason, rather than the epoch.
    pub boot_time: Option<u64>,
}
