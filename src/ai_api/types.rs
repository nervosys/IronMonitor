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
    /// Number of GPUs detected, or `None` when GPU enumeration was not
    /// available or failed. Every count here defaulted to 0, which read as
    /// "this machine has none".
    pub gpu_count: Option<usize>,
    /// Summary for each GPU, or `None` when GPU enumeration failed. It used to
    /// be an empty list in that case, which read as "no GPUs".
    pub gpus: Option<Vec<GpuSummary>>,

    // Process Summary
    /// Total number of processes, or `None` when enumeration failed
    pub process_count: Option<usize>,
    /// Total CPU usage across all processes, or `None` when any process's
    /// figure was not known -- a sum that left it out would be short.
    pub total_cpu_percent: Option<f32>,
    /// Number of processes using GPU, or `None` when not enumerated
    pub gpu_process_count: Option<usize>,
    /// Top 5 CPU-consuming processes, or `None` when processes were not read.
    pub top_cpu_processes: Option<Vec<ProcessSummary>>,
    /// Top 5 memory-consuming processes, or `None` when processes were not read.
    pub top_memory_processes: Option<Vec<ProcessSummary>>,
    /// Top 5 GPU memory-consuming processes, or `None` when not enumerated.
    pub top_gpu_processes: Option<Vec<ProcessSummary>>,

    // Memory Summary
    /// Memory information
    pub memory: Option<MemorySummary>,

    // Network Summary
    /// Number of active network interfaces, or `None` when not enumerated
    pub active_network_interfaces: Option<usize>,
    /// Network interface summaries, or `None` when interfaces were not read.
    pub network_interfaces: Option<Vec<NetworkSummary>>,

    // Disk Summary
    /// Number of disks that were read, or `None` when enumeration failed
    pub disk_count: Option<usize>,
    /// Number of enumerated disks whose details could not be read. Those disks
    /// are absent from `disks` and their errors are in `errors`.
    pub unreadable_disk_count: Option<usize>,
    /// Disk summaries for the disks that were read, or `None` when enumeration
    /// failed.
    pub disks: Option<Vec<DiskSummary>>,

    /// One entry per section that could not be read, naming the section and
    /// why. An empty list means every section above was read.
    pub errors: Vec<String>,
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
    /// CPU usage percentage, `None` when not yet known (see
    /// [`crate::process_monitor::ProcessMonitorInfo::cpu_percent`])
    pub cpu_percent: Option<f32>,
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
    /// Free RAM in MB, or `None` where it was not read.
    pub free_mb: Option<u64>,
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
    /// Memory usage percentage, or `None` where RAM was not read.
    pub usage_percent: Option<f32>,
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
    /// Whether interface is up, or `None` where the state was not read
    pub is_up: Option<bool>,
}

/// Disk summary information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSummary {
    /// Disk device name
    pub name: String,
    /// Disk model, or `None` where the device did not report one
    pub model: Option<String>,
    /// Size in GB
    /// Total size in GB, or `None` where the capacity was not read. A disk
    /// of unknown size is not a disk of zero size. Fractional, so a sub-GiB
    /// device is not rounded to zero.
    pub size_gb: Option<f64>,
    /// Disk type (NVMe, SATA, etc.), or `None` where it was not classified.
    pub disk_type: Option<String>,
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
    /// OS name. `None` when it was not read.
    pub os_name: Option<String>,
    /// OS version. `None` when the version was not read.
    pub os_version: Option<String>,
    /// Kernel version. `None` when it could not be read, rather than empty.
    pub kernel_version: Option<String>,
    /// Architecture. `None` when it was not read.
    pub architecture: Option<String>,
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
