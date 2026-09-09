//! System Context Materialization
//!
//! This module implements the filesystem-as-context principle by materializing
//! system state as structured, queryable context that AI systems can reason about.
//!
//! The context provides a snapshot of the entire system state that can be:
//! - Serialized to JSON for LLM context windows
//! - Queried via JSONPath or structured queries
//! - Diffed against previous states for change detection
//! - Filtered by capabilities for permission-aware access

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Complete system context - materialized state for AI reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemContext {
    /// Context generation timestamp
    pub timestamp: u64,
    /// Context schema version
    pub schema_version: String,
    /// System identification
    pub system: SystemIdentity,
    /// Hardware inventory
    pub hardware: HardwareContext,
    /// Software/OS context
    pub software: SoftwareContext,
    /// Current metrics snapshot
    pub metrics: MetricsContext,
    /// Active alerts and events
    pub alerts: Vec<AlertContext>,
    /// Metadata about this context
    pub meta: ContextMeta,
}

impl SystemContext {
    /// Create a new context builder
    pub fn builder() -> SystemContextBuilder {
        SystemContextBuilder::new()
    }

    /// Get context as pretty-printed JSON
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Get context as compact JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Get estimated token count (rough estimate: 4 chars per token)
    pub fn estimated_tokens(&self) -> usize {
        self.to_json().len() / 4
    }

    /// Create a minimal context with just essential info
    pub fn minimal(&self) -> MinimalContext {
        MinimalContext {
            timestamp: self.timestamp,
            hostname: self.system.hostname.clone(),
            os: self.system.os_name.clone(),
            cpu_count: self
                .hardware
                .cpu
                .as_ref()
                .map(|c| c.core_count)
                .unwrap_or(0),
            gpu_count: self.hardware.gpus.len(),
            memory_total_gb: self
                .hardware
                .memory
                .as_ref()
                .map(|m| m.total_gb)
                .unwrap_or(0.0),
            alert_count: self.alerts.len(),
        }
    }
}

/// Minimal context for constrained environments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimalContext {
    pub timestamp: u64,
    pub hostname: String,
    pub os: String,
    pub cpu_count: usize,
    pub gpu_count: usize,
    pub memory_total_gb: f64,
    pub alert_count: usize,
}

/// System identification information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemIdentity {
    /// Hostname
    pub hostname: String,
    /// Operating system name
    pub os_name: String,
    /// OS version
    pub os_version: String,
    /// Kernel version
    pub kernel_version: String,
    /// Architecture (x86_64, aarch64, etc.)
    pub architecture: String,
    /// Machine ID (unique system identifier)
    pub machine_id: Option<String>,
    /// Boot time
    pub boot_time: Option<u64>,
    /// Uptime in seconds
    pub uptime_seconds: Option<u64>,
}

/// Hardware inventory context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HardwareContext {
    /// CPU information
    pub cpu: Option<CpuContext>,
    /// GPU inventory
    pub gpus: Vec<GpuContext>,
    /// Memory information
    pub memory: Option<MemoryContext>,
    /// Disk inventory
    pub disks: Vec<DiskContext>,
    /// Network interface inventory
    pub network_interfaces: Vec<NetworkInterfaceContext>,
    /// Motherboard information
    pub motherboard: Option<MotherboardContext>,
    /// Power supply information
    pub power_supply: Option<PowerSupplyContext>,
    /// Fans
    pub fans: Vec<FanContext>,
    /// Temperature sensors
    pub temperature_sensors: Vec<TemperatureSensorContext>,
}

/// CPU context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuContext {
    /// CPU model name
    pub model: String,
    /// Vendor (Intel, AMD, Apple, etc.)
    pub vendor: String,
    /// Number of physical cores
    pub core_count: usize,
    /// Number of logical processors (with hyperthreading)
    pub thread_count: usize,
    /// Base frequency in MHz
    pub base_frequency_mhz: Option<u32>,
    /// Max boost frequency in MHz
    pub max_frequency_mhz: Option<u32>,
    /// L1 cache size
    pub l1_cache_kb: Option<u32>,
    /// L2 cache size
    pub l2_cache_kb: Option<u32>,
    /// L3 cache size
    pub l3_cache_kb: Option<u32>,
    /// CPU features
    pub features: Vec<String>,
}

/// GPU context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuContext {
    /// GPU index
    pub index: usize,
    /// GPU name/model
    pub name: String,
    /// Vendor (NVIDIA, AMD, Intel, Apple)
    pub vendor: String,
    /// Total VRAM in MB
    /// Video memory in MB, or `None` where the driver reported no capacity.
    pub vram_mb: Option<u64>,
    /// Driver version
    pub driver_version: Option<String>,
    /// CUDA/ROCm/Metal version
    pub compute_version: Option<String>,
    /// PCIe information
    pub pcie: Option<PcieContext>,
    /// Supported capabilities
    pub capabilities: Vec<String>,
}

/// PCIe context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcieContext {
    /// PCIe generation (1-6)
    pub generation: u8,
    /// Link width (lanes)
    pub width: u8,
    /// Bus ID
    pub bus_id: Option<String>,
}

/// Memory context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryContext {
    /// Total RAM in GB
    pub total_gb: f64,
    /// Memory type (DDR4, DDR5, etc.)
    pub memory_type: Option<String>,
    /// Memory speed in MHz
    pub speed_mhz: Option<u32>,
    /// Number of DIMMs
    pub dimm_count: Option<u32>,
    /// Swap/page file size in GB
    /// Swap total in GB, or `None` where the pagefile was not read.
    pub swap_total_gb: Option<f64>,
}

/// Disk context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskContext {
    /// Disk device path or name
    pub device: String,
    /// Disk model
    pub model: Option<String>,
    /// Disk type (SSD, HDD, NVMe)
    pub disk_type: String,
    /// Total size in GB
    pub size_gb: f64,
    /// Mount point (if mounted)
    pub mount_point: Option<String>,
    /// Filesystem type
    pub filesystem: Option<String>,
    /// Serial number (if available)
    pub serial: Option<String>,
}

/// Network interface context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterfaceContext {
    /// Interface name
    pub name: String,
    /// Interface type (ethernet, wifi, loopback, etc.)
    pub interface_type: String,
    /// MAC address
    pub mac_address: Option<String>,
    /// IP addresses
    pub ip_addresses: Vec<String>,
    /// Link speed in Mbps
    pub speed_mbps: Option<u32>,
    /// MTU
    pub mtu: Option<u32>,
    /// Whether interface is up
    pub is_up: bool,
}

/// Motherboard context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotherboardContext {
    /// Manufacturer
    pub manufacturer: String,
    /// Product name
    pub product: String,
    /// BIOS vendor
    pub bios_vendor: Option<String>,
    /// BIOS version
    pub bios_version: Option<String>,
    /// BIOS date
    pub bios_date: Option<String>,
    /// Chassis type
    pub chassis_type: Option<String>,
}

/// Power supply context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerSupplyContext {
    /// Power supply status
    pub status: String,
    /// Whether on AC power. `None` when the platform did not report it --
    /// which used to be indistinguishable from `false`, "on battery".
    pub on_ac_power: Option<bool>,
    /// Battery percentage (if applicable)
    pub battery_percent: Option<f32>,
    /// Battery status (charging, discharging, full)
    pub battery_status: Option<String>,
    /// Estimated time remaining (minutes)
    pub time_remaining_minutes: Option<u32>,
}

/// Fan context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanContext {
    /// Fan name/label
    pub name: String,
    /// Current speed in RPM
    pub speed_rpm: Option<u32>,
    /// Speed percentage (0-100)
    pub speed_percent: Option<u8>,
    /// Minimum RPM
    pub min_rpm: Option<u32>,
    /// Maximum RPM
    pub max_rpm: Option<u32>,
}

/// Temperature sensor context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureSensorContext {
    /// Sensor name/label
    pub name: String,
    /// Current temperature in Celsius
    pub temperature_c: f32,
    /// High threshold
    pub high_threshold_c: Option<f32>,
    /// Critical threshold
    pub critical_threshold_c: Option<f32>,
    /// Sensor location (CPU, GPU, Chassis, etc.)
    pub location: Option<String>,
}

/// Software/OS context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SoftwareContext {
    /// Running processes count
    pub process_count: usize,
    /// Running services
    pub services: Vec<ServiceContext>,
    /// Installed drivers
    pub drivers: Vec<DriverContext>,
    /// Boot configuration
    pub boot_config: Option<BootConfigContext>,
}

/// Service context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceContext {
    /// Service name
    pub name: String,
    /// Display name
    pub display_name: Option<String>,
    /// Status (running, stopped, etc.)
    pub status: String,
    /// Start type (auto, manual, disabled)
    pub start_type: Option<String>,
    /// PID if running
    pub pid: Option<u32>,
}

/// Driver context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverContext {
    /// Driver name
    pub name: String,
    /// Version
    pub version: Option<String>,
    /// Provider
    pub provider: Option<String>,
    /// Date
    pub date: Option<String>,
}

/// Boot configuration context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootConfigContext {
    /// Boot mode (UEFI, Legacy)
    pub boot_mode: String,
    /// Secure boot enabled
    pub secure_boot: Option<bool>,
    /// Boot entries
    pub boot_entries: Vec<String>,
}

/// Current metrics snapshot
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsContext {
    /// CPU metrics
    pub cpu: Option<CpuMetrics>,
    /// GPU metrics (per GPU)
    pub gpus: Vec<GpuMetrics>,
    /// Memory metrics
    pub memory: Option<MemoryMetrics>,
    /// Disk metrics (per disk)
    pub disks: Vec<DiskMetrics>,
    /// Network metrics (per interface)
    pub network: Vec<NetworkMetrics>,
    /// Top processes
    pub top_processes: Vec<ProcessMetrics>,
    /// System load
    pub system_load: Option<SystemLoadMetrics>,
}

/// CPU metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuMetrics {
    /// Overall CPU utilization (0-100)
    pub utilization_percent: f32,
    /// Per-core utilization
    /// Per-core utilization, `None` for a core whose idle time was not read.
    ///
    /// A core that reported nothing used to arrive here as `100.0 - 100.0`, a
    /// measured 0% busy. See `agent::state::CpuState::per_core_usage`.
    pub per_core_utilization: Vec<Option<f32>>,
    /// Current frequency in MHz
    pub frequency_mhz: Option<u32>,
    /// Temperature in Celsius
    pub temperature_c: Option<f32>,
}

/// GPU metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMetrics {
    /// GPU index
    pub index: usize,
    /// GPU utilization (0-100)
    /// Utilization percentage, or `None` where the device reports no counter.
    pub utilization_percent: Option<f32>,
    /// Memory used in MB
    /// GPU memory used in MB, or `None` where unreported.
    pub memory_used_mb: Option<u64>,
    /// Memory total in MB
    /// GPU memory total in MB, or `None` where unreported.
    pub memory_total_mb: Option<u64>,
    /// Temperature in Celsius
    pub temperature_c: Option<f32>,
    /// Power draw in watts
    pub power_watts: Option<f32>,
    /// Fan speed percentage
    pub fan_speed_percent: Option<u8>,
    /// Clock speeds
    pub clocks: Option<GpuClocks>,
}

/// GPU clock speeds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuClocks {
    /// Graphics clock in MHz
    pub graphics_mhz: Option<u32>,
    /// Memory clock in MHz
    pub memory_mhz: Option<u32>,
    /// SM/shader clock in MHz
    pub sm_mhz: Option<u32>,
}

/// Memory metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    /// Used memory in MB
    pub used_mb: u64,
    /// Free memory in MB
    pub free_mb: u64,
    /// Total memory in MB
    pub total_mb: u64,
    /// Cached memory in MB
    pub cached_mb: Option<u64>,
    /// Buffers in MB
    pub buffers_mb: Option<u64>,
    /// Swap used in MB
    /// Swap in use in MB, or `None` where the pagefile was not read.
    pub swap_used_mb: Option<u64>,
    /// Swap total in MB
    /// Swap total in MB, or `None` where the pagefile was not read.
    pub swap_total_mb: Option<u64>,
}

/// Disk metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetrics {
    /// Device name
    pub device: String,
    /// Used space in GB
    pub used_gb: f64,
    /// Free space in GB
    pub free_gb: f64,
    /// Total space in GB
    pub total_gb: f64,
    /// Read bytes per second
    pub read_bps: Option<u64>,
    /// Write bytes per second
    pub write_bps: Option<u64>,
    /// IOPS read
    pub iops_read: Option<u64>,
    /// IOPS write
    pub iops_write: Option<u64>,
}

/// Network metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    /// Interface name
    pub interface: String,
    /// Received bytes per second, or `None` until a second sample.
    ///
    /// A rate is a difference between two readings; the first reading has
    /// nothing to difference against. This was a bare `f64` reporting `0.0`,
    /// which states the link is idle.
    pub rx_bps: Option<f64>,
    /// Transmitted bytes per second. See [`Self::rx_bps`].
    pub tx_bps: Option<f64>,
    /// Total received bytes
    pub rx_bytes_total: u64,
    /// Total transmitted bytes
    pub tx_bytes_total: u64,
    /// Packets received
    pub rx_packets: u64,
    /// Packets transmitted
    pub tx_packets: u64,
    /// Errors
    pub errors: u64,
    /// Dropped packets
    pub dropped: u64,
}

/// Process metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetrics {
    /// Process ID
    pub pid: u32,
    /// Process name
    pub name: String,
    /// CPU usage percentage
    pub cpu_percent: f32,
    /// Memory usage in MB
    pub memory_mb: u64,
    /// GPU memory in MB (if using GPU)
    pub gpu_memory_mb: Option<u64>,
    /// Thread count
    pub threads: u32,
    /// User
    pub user: Option<String>,
    /// Command line
    pub command: Option<String>,
}

/// System load metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemLoadMetrics {
    /// 1-minute load average, or `None` where the platform has no such figure.
    ///
    /// Load average is a Unix quantity: a decaying average of the number of
    /// tasks runnable or in uninterruptible sleep. Windows has no equivalent
    /// and publishes nothing that can be converted into one.
    ///
    /// It said `f64`, and the Windows arm filled all three from instantaneous
    /// CPU utilisation -- `(usage / 100) * cores`, assigned to the 1, 5 and 15
    /// minute figures alike. That is not a load average by any reading. It
    /// cannot exceed the core count, so the queue depth that makes the metric
    /// worth having is exactly what it cannot express; it has no history, so
    /// the three windows that distinguish a spike from a trend were identical;
    /// and it counts busy CPUs rather than waiting tasks, so a machine wedged
    /// on I/O -- the case load average exists to expose -- read as idle.
    pub load_1: Option<f64>,
    /// 5-minute load average. See [`Self::load_1`].
    pub load_5: Option<f64>,
    /// 15-minute load average. See [`Self::load_1`].
    pub load_15: Option<f64>,
    /// Processes in the running state, or `None` where unreported.
    pub running_processes: Option<u32>,
    /// Total processes, or `None` where unreported.
    pub total_processes: Option<u32>,
}

/// Alert context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertContext {
    /// Alert ID
    pub id: String,
    /// Alert severity (info, warning, critical)
    pub severity: String,
    /// Alert message
    pub message: String,
    /// Alert source (cpu, gpu, memory, etc.)
    pub source: String,
    /// When the alert was triggered
    pub timestamp: u64,
    /// Additional context/metadata
    pub metadata: HashMap<String, String>,
}

/// Metadata about the context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextMeta {
    /// Time to generate context in milliseconds
    pub generation_time_ms: u64,
    /// Context size in bytes
    pub size_bytes: usize,
    /// Which capabilities were included
    pub included_capabilities: Vec<String>,
    /// Which capabilities were filtered out (due to permissions)
    pub excluded_capabilities: Vec<String>,
}

/// Builder for creating system context
pub struct SystemContextBuilder {
    context: SystemContext,
    start_time: std::time::Instant,
}

impl SystemContextBuilder {
    pub fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            context: SystemContext {
                timestamp,
                schema_version: "1.0.0".to_string(),
                system: SystemIdentity::default(),
                hardware: HardwareContext::default(),
                software: SoftwareContext::default(),
                metrics: MetricsContext::default(),
                alerts: Vec::new(),
                meta: ContextMeta::default(),
            },
            start_time: std::time::Instant::now(),
        }
    }

    pub fn system(mut self, system: SystemIdentity) -> Self {
        self.context.system = system;
        self
    }

    pub fn hardware(mut self, hardware: HardwareContext) -> Self {
        self.context.hardware = hardware;
        self
    }

    pub fn software(mut self, software: SoftwareContext) -> Self {
        self.context.software = software;
        self
    }

    pub fn metrics(mut self, metrics: MetricsContext) -> Self {
        self.context.metrics = metrics;
        self
    }

    pub fn alerts(mut self, alerts: Vec<AlertContext>) -> Self {
        self.context.alerts = alerts;
        self
    }

    pub fn build(mut self) -> SystemContext {
        let generation_time = self.start_time.elapsed().as_millis() as u64;
        let json = serde_json::to_string(&self.context).unwrap_or_default();

        self.context.meta.generation_time_ms = generation_time;
        self.context.meta.size_bytes = json.len();

        self.context
    }
}

impl Default for SystemContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}
