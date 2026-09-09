//! Observability API
//!
//! The main API for the observability system. Provides comprehensive access to
//! all system metrics with permission checking and rate limiting.

use std::sync::{Arc, RwLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::context::*;
use super::permissions::*;

/// Result type for observability operations
pub type Result<T> = std::result::Result<T, ObservabilityError>;

/// Errors from the observability API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObservabilityError {
    /// Permission denied
    PermissionDenied(String),
    /// Rate limited
    RateLimited { retry_after_secs: u64 },
    /// Resource not found
    NotFound(String),
    /// Internal error
    Internal(String),
    /// Invalid request
    InvalidRequest(String),
    /// Feature not available
    NotAvailable(String),
}

impl std::fmt::Display for ObservabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            Self::RateLimited { retry_after_secs } => {
                write!(f, "Rate limited, retry after {} seconds", retry_after_secs)
            }
            Self::NotFound(msg) => write!(f, "Not found: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
            Self::InvalidRequest(msg) => write!(f, "Invalid request: {}", msg),
            Self::NotAvailable(msg) => write!(f, "Not available: {}", msg),
        }
    }
}

impl std::error::Error for ObservabilityError {}

impl From<PermissionError> for ObservabilityError {
    fn from(err: PermissionError) -> Self {
        match err {
            PermissionError::InvalidKey => Self::PermissionDenied("Invalid API key".into()),
            PermissionError::KeyDisabled => Self::PermissionDenied("API key is disabled".into()),
            PermissionError::RateLimited { retry_after } => Self::RateLimited {
                retry_after_secs: retry_after.map(|d| d.as_secs()).unwrap_or(60),
            },
            PermissionError::AccessDenied { capability, scope } => {
                Self::PermissionDenied(format!("Access denied for {}:{:?}", capability, scope))
            }
        }
    }
}

/// Request context for API calls
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// API key used for the request
    pub api_key: String,
    /// Request ID for tracing
    pub request_id: Option<String>,
    /// Client IP address
    pub client_ip: Option<String>,
    /// Request timestamp
    pub timestamp: Instant,
}

impl RequestContext {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            request_id: None,
            client_ip: None,
            timestamp: Instant::now(),
        }
    }

    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.request_id = Some(id.into());
        self
    }

    pub fn with_client_ip(mut self, ip: impl Into<String>) -> Self {
        self.client_ip = Some(ip.into());
        self
    }
}

/// API response wrapper with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Response data
    pub data: T,
    /// Response metadata
    pub meta: ResponseMeta,
}

/// Response metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMeta {
    /// Request ID
    pub request_id: Option<String>,
    /// Response timestamp
    pub timestamp: u64,
    /// Processing time in milliseconds
    pub duration_ms: u64,
    /// Rate limit info
    pub rate_limit: Option<RateLimitInfo>,
}

/// Rate limit information in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitInfo {
    /// Remaining requests in current window
    pub remaining: u32,
    /// Total allowed requests
    pub limit: u32,
    /// When the window resets (unix timestamp)
    pub reset_at: u64,
}

/// The main Observability API
#[allow(dead_code)]
pub struct ObservabilityApi {
    /// Permission checker
    permission_checker: Arc<RwLock<PermissionChecker>>,
    /// System context builder (caches static info)
    system_identity: Option<SystemIdentity>,
    hardware_inventory: Option<HardwareContext>,
}

impl ObservabilityApi {
    /// Create a new ObservabilityApi with the given configuration
    pub fn new(config: ApiConfig) -> Self {
        Self {
            permission_checker: Arc::new(RwLock::new(PermissionChecker::new(config.keys))),
            system_identity: None,
            hardware_inventory: None,
        }
    }

    /// Create with a permission checker
    pub fn with_permission_checker(checker: PermissionChecker) -> Self {
        Self {
            permission_checker: Arc::new(RwLock::new(checker)),
            system_identity: None,
            hardware_inventory: None,
        }
    }

    /// Get a clone of the permission checker Arc
    pub fn permission_checker(&self) -> Arc<RwLock<PermissionChecker>> {
        Arc::clone(&self.permission_checker)
    }

    /// Check if a request has a specific permission
    fn check_permission(
        &self,
        ctx: &RequestContext,
        capability: Capability,
        scope: Scope,
    ) -> Result<()> {
        self.permission_checker
            .write()
            .unwrap()
            .check(&ctx.api_key, capability, &scope)
            .map_err(Into::into)
    }

    /// Get rate limit info for the current API key
    fn get_rate_limit_info(&self, ctx: &RequestContext) -> Option<RateLimitInfo> {
        let checker = self.permission_checker.read().unwrap();
        checker
            .rate_limit_status(&ctx.api_key)
            .map(|(remaining, limit, reset_at)| RateLimitInfo {
                remaining,
                limit,
                reset_at,
            })
    }

    /// Build response metadata with rate limit info
    fn build_response_meta(&self, ctx: &RequestContext, start: Instant) -> ResponseMeta {
        ResponseMeta {
            request_id: ctx.request_id.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            duration_ms: start.elapsed().as_millis() as u64,
            rate_limit: self.get_rate_limit_info(ctx),
        }
    }

    /// Get full system context
    ///
    /// Returns a complete snapshot of the system state based on permissions.
    /// This is the primary method for AI systems to get context about the system.
    pub fn get_context(&mut self, ctx: &RequestContext) -> Result<ApiResponse<SystemContext>> {
        let start = Instant::now();

        let mut builder = SystemContext::builder();
        let mut included = Vec::new();
        let mut excluded = Vec::new();

        // System info - requires SystemInfo:Read
        if self
            .check_permission(ctx, Capability::SystemInfo, Scope::Read)
            .is_ok()
        {
            builder = builder.system(self.collect_system_identity());
            included.push("system".to_string());
        } else {
            excluded.push("system".to_string());
        }

        // Hardware - requires various capabilities
        let hardware = self.collect_hardware_context(ctx, &mut included, &mut excluded);
        builder = builder.hardware(hardware);

        // Software context
        let software = self.collect_software_context(ctx, &mut included, &mut excluded);
        builder = builder.software(software);

        // Metrics
        let metrics = self.collect_metrics_context(ctx, &mut included, &mut excluded);
        builder = builder.metrics(metrics);

        // Alerts
        if self
            .check_permission(ctx, Capability::Events, Scope::Read)
            .is_ok()
        {
            builder = builder.alerts(self.collect_alerts());
            included.push("alerts".to_string());
        } else {
            excluded.push("alerts".to_string());
        }

        let mut context = builder.build();
        context.meta.included_capabilities = included;
        context.meta.excluded_capabilities = excluded;

        Ok(ApiResponse {
            data: context,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get minimal context (for constrained environments)
    pub fn get_minimal_context(
        &mut self,
        ctx: &RequestContext,
    ) -> Result<ApiResponse<MinimalContext>> {
        self.check_permission(ctx, Capability::SystemInfo, Scope::Read)?;

        let start = Instant::now();
        let full_context = self.get_context(ctx)?;
        let minimal = full_context.data.minimal();

        Ok(ApiResponse {
            data: minimal,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get GPU information
    pub fn get_gpus(&self, ctx: &RequestContext) -> Result<ApiResponse<Vec<GpuContext>>> {
        self.check_permission(ctx, Capability::Gpu, Scope::Read)?;

        let start = Instant::now();
        let gpus = self.collect_gpu_inventory();

        Ok(ApiResponse {
            data: gpus,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get GPU metrics
    pub fn get_gpu_metrics(&self, ctx: &RequestContext) -> Result<ApiResponse<Vec<GpuMetrics>>> {
        self.check_permission(ctx, Capability::Gpu, Scope::Read)?;

        let start = Instant::now();
        let metrics = self.collect_gpu_metrics();

        Ok(ApiResponse {
            data: metrics,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get CPU metrics
    pub fn get_cpu_metrics(&self, ctx: &RequestContext) -> Result<ApiResponse<CpuMetrics>> {
        self.check_permission(ctx, Capability::Cpu, Scope::Read)?;

        let start = Instant::now();
        let metrics = self
            .collect_cpu_metrics()
            .ok_or_else(|| ObservabilityError::NotAvailable("CPU metrics not available".into()))?;

        Ok(ApiResponse {
            data: metrics,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get memory metrics
    pub fn get_memory_metrics(&self, ctx: &RequestContext) -> Result<ApiResponse<MemoryMetrics>> {
        self.check_permission(ctx, Capability::Memory, Scope::Read)?;

        let start = Instant::now();
        let metrics = self.collect_memory_metrics().ok_or_else(|| {
            ObservabilityError::NotAvailable("Memory metrics not available".into())
        })?;

        Ok(ApiResponse {
            data: metrics,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get disk information
    pub fn get_disks(&self, ctx: &RequestContext) -> Result<ApiResponse<Vec<DiskContext>>> {
        self.check_permission(ctx, Capability::Disk, Scope::Read)?;

        let start = Instant::now();
        let disks = self.collect_disk_inventory();

        Ok(ApiResponse {
            data: disks,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get disk metrics
    pub fn get_disk_metrics(&self, ctx: &RequestContext) -> Result<ApiResponse<Vec<DiskMetrics>>> {
        self.check_permission(ctx, Capability::Disk, Scope::Read)?;

        let start = Instant::now();
        let metrics = self.collect_disk_metrics();

        Ok(ApiResponse {
            data: metrics,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get network interface information
    pub fn get_network_interfaces(
        &self,
        ctx: &RequestContext,
    ) -> Result<ApiResponse<Vec<NetworkInterfaceContext>>> {
        self.check_permission(ctx, Capability::Network, Scope::Read)?;

        let start = Instant::now();
        let interfaces = self.collect_network_interfaces();

        Ok(ApiResponse {
            data: interfaces,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get network metrics
    pub fn get_network_metrics(
        &self,
        ctx: &RequestContext,
    ) -> Result<ApiResponse<Vec<NetworkMetrics>>> {
        self.check_permission(ctx, Capability::Network, Scope::Read)?;

        let start = Instant::now();
        let metrics = self.collect_network_metrics();

        Ok(ApiResponse {
            data: metrics,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get process list
    pub fn get_processes(
        &self,
        ctx: &RequestContext,
        limit: Option<usize>,
    ) -> Result<ApiResponse<Vec<ProcessMetrics>>> {
        self.check_permission(ctx, Capability::Process, Scope::Read)?;

        let start = Instant::now();
        let mut processes = self.collect_processes();

        if let Some(limit) = limit {
            processes.truncate(limit);
        }

        Ok(ApiResponse {
            data: processes,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get motherboard information
    pub fn get_motherboard(&self, ctx: &RequestContext) -> Result<ApiResponse<MotherboardContext>> {
        self.check_permission(ctx, Capability::Motherboard, Scope::Read)?;

        let start = Instant::now();
        let motherboard = self.collect_motherboard_info().ok_or_else(|| {
            ObservabilityError::NotAvailable("Motherboard info not available".into())
        })?;

        Ok(ApiResponse {
            data: motherboard,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get power/battery status
    pub fn get_power_status(
        &self,
        ctx: &RequestContext,
    ) -> Result<ApiResponse<PowerSupplyContext>> {
        self.check_permission(ctx, Capability::Power, Scope::Read)?;

        let start = Instant::now();
        let power = self
            .collect_power_info()
            .ok_or_else(|| ObservabilityError::NotAvailable("Power info not available".into()))?;

        Ok(ApiResponse {
            data: power,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get fan status
    pub fn get_fans(&self, ctx: &RequestContext) -> Result<ApiResponse<Vec<FanContext>>> {
        self.check_permission(ctx, Capability::Fan, Scope::Read)?;

        let start = Instant::now();
        let fans = self.collect_fans();

        Ok(ApiResponse {
            data: fans,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// Get temperature sensors
    pub fn get_temperatures(
        &self,
        ctx: &RequestContext,
    ) -> Result<ApiResponse<Vec<TemperatureSensorContext>>> {
        self.check_permission(ctx, Capability::Cpu, Scope::Read)?; // Temperatures are part of general system monitoring

        let start = Instant::now();
        let temps = self.collect_temperature_sensors();

        Ok(ApiResponse {
            data: temps,
            meta: self.build_response_meta(ctx, start),
        })
    }

    /// List available capabilities for an API key
    pub fn list_capabilities(&self, ctx: &RequestContext) -> Result<ApiResponse<Vec<String>>> {
        let start = Instant::now();

        let checker = self.permission_checker.read().unwrap();
        let api_key = checker
            .get_key(&ctx.api_key)
            .ok_or_else(|| ObservabilityError::PermissionDenied("Invalid API key".into()))?;

        let capabilities: Vec<String> = api_key.permissions.iter().map(|p| p.to_string()).collect();
        drop(checker);

        Ok(ApiResponse {
            data: capabilities,
            meta: self.build_response_meta(ctx, start),
        })
    }

    // ========== Data Collection Methods ==========
    // These would integrate with the actual monitoring systems

    fn collect_system_identity(&self) -> SystemIdentity {
        use crate::motherboard;
        let sys_info = motherboard::get_system_info().ok();
        let os_version = sys_info
            .as_ref()
            .map(|s| s.os_version.clone())
            .unwrap_or_default();
        let kernel_version = sys_info
            .as_ref()
            .and_then(|s| s.kernel_version.clone())
            .unwrap_or_default();
        let hostname = sys_info
            .as_ref()
            .and_then(|s| s.hostname.clone())
            .or_else(|| std::env::var("COMPUTERNAME").ok())
            .or_else(|| std::env::var("HOSTNAME").ok())
            .unwrap_or_else(|| "unknown".into());

        #[cfg(target_os = "windows")]
        let uptime_seconds = Some(crate::platform::windows::get_system_uptime().as_secs());
        #[cfg(not(target_os = "windows"))]
        let uptime_seconds = std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| {
                s.split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
            })
            .map(|v| v as u64);

        SystemIdentity {
            hostname,
            os_name: sys_info
                .as_ref()
                .map(|s| s.os_name.clone())
                .unwrap_or_else(|| std::env::consts::OS.to_string()),
            os_version,
            kernel_version,
            architecture: sys_info
                .as_ref()
                .map(|s| s.architecture.clone())
                .unwrap_or_else(|| std::env::consts::ARCH.to_string()),
            machine_id: sys_info.as_ref().and_then(|s| s.uuid.clone()),
            boot_time: None,
            uptime_seconds,
        }
    }

    fn collect_hardware_context(
        &self,
        ctx: &RequestContext,
        included: &mut Vec<String>,
        excluded: &mut Vec<String>,
    ) -> HardwareContext {
        let mut hw = HardwareContext::default();

        // CPU
        if self
            .check_permission(ctx, Capability::Cpu, Scope::Read)
            .is_ok()
        {
            hw.cpu = self.collect_cpu_info();
            included.push("cpu".to_string());
        } else {
            excluded.push("cpu".to_string());
        }

        // GPU
        if self
            .check_permission(ctx, Capability::Gpu, Scope::Read)
            .is_ok()
        {
            hw.gpus = self.collect_gpu_inventory();
            included.push("gpu".to_string());
        } else {
            excluded.push("gpu".to_string());
        }

        // Memory
        if self
            .check_permission(ctx, Capability::Memory, Scope::Read)
            .is_ok()
        {
            hw.memory = self.collect_memory_info();
            included.push("memory".to_string());
        } else {
            excluded.push("memory".to_string());
        }

        // Disk
        if self
            .check_permission(ctx, Capability::Disk, Scope::Read)
            .is_ok()
        {
            hw.disks = self.collect_disk_inventory();
            included.push("disk".to_string());
        } else {
            excluded.push("disk".to_string());
        }

        // Network
        if self
            .check_permission(ctx, Capability::Network, Scope::Read)
            .is_ok()
        {
            hw.network_interfaces = self.collect_network_interfaces();
            included.push("network".to_string());
        } else {
            excluded.push("network".to_string());
        }

        // Motherboard
        if self
            .check_permission(ctx, Capability::Motherboard, Scope::Read)
            .is_ok()
        {
            hw.motherboard = self.collect_motherboard_info();
            included.push("motherboard".to_string());
        } else {
            excluded.push("motherboard".to_string());
        }

        // Power
        if self
            .check_permission(ctx, Capability::Power, Scope::Read)
            .is_ok()
        {
            hw.power_supply = self.collect_power_info();
            included.push("power".to_string());
        } else {
            excluded.push("power".to_string());
        }

        // Fans
        if self
            .check_permission(ctx, Capability::Fan, Scope::Read)
            .is_ok()
        {
            hw.fans = self.collect_fans();
            included.push("fan".to_string());
        } else {
            excluded.push("fan".to_string());
        }

        hw
    }

    fn collect_software_context(
        &self,
        ctx: &RequestContext,
        included: &mut Vec<String>,
        excluded: &mut Vec<String>,
    ) -> SoftwareContext {
        let mut sw = SoftwareContext::default();

        // Process count
        if self
            .check_permission(ctx, Capability::Process, Scope::Read)
            .is_ok()
        {
            sw.process_count = self.collect_processes().len();
            included.push("processes".to_string());
        } else {
            excluded.push("processes".to_string());
        }

        // Services
        if self
            .check_permission(ctx, Capability::Services, Scope::Read)
            .is_ok()
        {
            sw.services = self.collect_services();
            included.push("services".to_string());
        } else {
            excluded.push("services".to_string());
        }

        // Drivers
        if self
            .check_permission(ctx, Capability::Drivers, Scope::Read)
            .is_ok()
        {
            sw.drivers = self.collect_drivers();
            included.push("drivers".to_string());
        } else {
            excluded.push("drivers".to_string());
        }

        // Boot config
        if self
            .check_permission(ctx, Capability::BootConfig, Scope::Read)
            .is_ok()
        {
            sw.boot_config = self.collect_boot_config();
            included.push("boot_config".to_string());
        } else {
            excluded.push("boot_config".to_string());
        }

        sw
    }

    fn collect_metrics_context(
        &self,
        ctx: &RequestContext,
        included: &mut Vec<String>,
        excluded: &mut Vec<String>,
    ) -> MetricsContext {
        let mut metrics = MetricsContext::default();

        // CPU metrics
        if self
            .check_permission(ctx, Capability::Cpu, Scope::Read)
            .is_ok()
        {
            metrics.cpu = self.collect_cpu_metrics();
            included.push("cpu_metrics".to_string());
        } else {
            excluded.push("cpu_metrics".to_string());
        }

        // GPU metrics
        if self
            .check_permission(ctx, Capability::Gpu, Scope::Read)
            .is_ok()
        {
            metrics.gpus = self.collect_gpu_metrics();
            included.push("gpu_metrics".to_string());
        } else {
            excluded.push("gpu_metrics".to_string());
        }

        // Memory metrics
        if self
            .check_permission(ctx, Capability::Memory, Scope::Read)
            .is_ok()
        {
            metrics.memory = self.collect_memory_metrics();
            included.push("memory_metrics".to_string());
        } else {
            excluded.push("memory_metrics".to_string());
        }

        // Disk metrics
        if self
            .check_permission(ctx, Capability::Disk, Scope::Read)
            .is_ok()
        {
            metrics.disks = self.collect_disk_metrics();
            included.push("disk_metrics".to_string());
        } else {
            excluded.push("disk_metrics".to_string());
        }

        // Network metrics
        if self
            .check_permission(ctx, Capability::Network, Scope::Read)
            .is_ok()
        {
            metrics.network = self.collect_network_metrics();
            included.push("network_metrics".to_string());
        } else {
            excluded.push("network_metrics".to_string());
        }

        // Top processes
        if self
            .check_permission(ctx, Capability::Process, Scope::Read)
            .is_ok()
        {
            let mut procs = self.collect_processes();
            procs.sort_by(|a, b| {
                b.cpu_percent
                    .partial_cmp(&a.cpu_percent)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            procs.truncate(10); // Top 10
            metrics.top_processes = procs;
            included.push("top_processes".to_string());
        } else {
            excluded.push("top_processes".to_string());
        }

        // System load
        if self
            .check_permission(ctx, Capability::SystemInfo, Scope::Read)
            .is_ok()
        {
            metrics.system_load = self.collect_system_load();
            included.push("system_load".to_string());
        } else {
            excluded.push("system_load".to_string());
        }

        metrics
    }

    // ========== Real Data Collection Methods ==========
    // These integrate with the actual monitoring subsystems

    fn collect_cpu_info(&self) -> Option<CpuContext> {
        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::cpu;
            if let Ok(stats) = cpu::read_cpu_stats() {
                let first_core = stats.cores.first();
                let model = first_core.map(|c| c.model.clone()).unwrap_or_default();
                let freq = first_core.and_then(|c| c.frequency.as_ref());
                return Some(CpuContext {
                    model,
                    vendor: String::new(), // parsed from model
                    core_count: stats.cores.len(),
                    thread_count: stats.cores.len(), // /proc doesn't distinguish easily
                    base_frequency_mhz: freq.and_then(|f| f.current),
                    max_frequency_mhz: freq.and_then(|f| f.max),
                    l1_cache_kb: None,
                    l2_cache_kb: None,
                    l3_cache_kb: None,
                    features: Vec::new(),
                });
            }
        }
        #[cfg(target_os = "windows")]
        {
            use crate::platform::windows;
            if let Ok(stats) = windows::read_cpu_stats() {
                let first_core = stats.cores.first();
                let model = first_core.map(|c| c.model.clone()).unwrap_or_default();
                let freq = first_core.and_then(|c| c.frequency.as_ref());
                return Some(CpuContext {
                    model,
                    vendor: String::new(),
                    core_count: stats.cores.len(),
                    thread_count: stats.cores.len(),
                    base_frequency_mhz: freq.and_then(|f| f.current),
                    max_frequency_mhz: freq.and_then(|f| f.max),
                    l1_cache_kb: None,
                    l2_cache_kb: None,
                    l3_cache_kb: None,
                    features: Vec::new(),
                });
            }
        }
        None
    }

    fn collect_gpu_inventory(&self) -> Vec<GpuContext> {
        use crate::gpu::GpuCollection;
        let mut result = Vec::new();
        if let Ok(gpus) = GpuCollection::auto_detect() {
            if let Ok(snapshots) = gpus.snapshot_all() {
                for snap in &snapshots {
                    result.push(GpuContext {
                        index: snap.static_info.index,
                        name: snap.static_info.name.clone(),
                        vendor: format!("{:?}", snap.static_info.vendor),
                        vram_mb: snap.dynamic_info.memory.total.map(|v| v / (1024 * 1024)),
                        driver_version: snap.static_info.driver_version.clone(),
                        compute_version: snap
                            .static_info
                            .compute_capability
                            .map(|(major, minor)| format!("{}.{}", major, minor)),
                        pcie: snap
                            .static_info
                            .pci_bus_id
                            .as_ref()
                            .map(|bus_id| PcieContext {
                                generation: 0,
                                width: 0,
                                bus_id: Some(bus_id.clone()),
                            }),
                        capabilities: Vec::new(),
                    });
                }
            }
        }
        result
    }

    fn collect_memory_info(&self) -> Option<MemoryContext> {
        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::memory;
            if let Ok(stats) = memory::read_memory_stats() {
                return Some(MemoryContext {
                    total_gb: stats.ram.total as f64 / (1024.0 * 1024.0),
                    memory_type: None,
                    speed_mhz: None,
                    dimm_count: None,
                    swap_total_gb: stats.swap.total.map(|v| v as f64 / (1024.0 * 1024.0)),
                });
            }
        }
        #[cfg(target_os = "windows")]
        {
            use crate::platform::windows;
            if let Ok(stats) = windows::read_memory_stats() {
                return Some(MemoryContext {
                    total_gb: stats.ram.total as f64 / (1024.0 * 1024.0),
                    memory_type: None,
                    speed_mhz: None,
                    dimm_count: None,
                    swap_total_gb: stats.swap.total.map(|v| v as f64 / (1024.0 * 1024.0)),
                });
            }
        }
        None
    }

    fn collect_disk_inventory(&self) -> Vec<DiskContext> {
        use crate::disk;
        let mut result = Vec::new();
        if let Ok(disks) = disk::enumerate_disks() {
            for d in &disks {
                let info = d.info().ok();
                let fs_info = d.filesystem_info().ok().unwrap_or_default();
                let first_fs = fs_info.first();
                result.push(DiskContext {
                    device: d.name().to_string(),
                    model: info.as_ref().and_then(|i| {
                        if i.model.is_empty() {
                            None
                        } else {
                            Some(i.model.clone())
                        }
                    }),
                    disk_type: format!("{:?}", d.disk_type()),
                    size_gb: info
                        .as_ref()
                        .map(|i| i.capacity as f64 / (1024.0 * 1024.0 * 1024.0))
                        .unwrap_or(0.0),
                    mount_point: first_fs.map(|f| f.mount_point.to_string_lossy().to_string()),
                    filesystem: first_fs.and_then(|f| {
                        if f.fs_type.is_empty() {
                            None
                        } else {
                            Some(f.fs_type.clone())
                        }
                    }),
                    serial: info
                        .as_ref()
                        .and_then(|i| i.serial.as_ref().filter(|s| !s.is_empty()).cloned()),
                });
            }
        }
        result
    }

    fn collect_network_interfaces(&self) -> Vec<NetworkInterfaceContext> {
        use crate::NetworkMonitor;
        let mut result = Vec::new();
        if let Ok(mut monitor) = NetworkMonitor::new() {
            if let Ok(ifaces) = monitor.interfaces() {
                for iface in &ifaces {
                    let mut ips = iface.ipv4_addresses.clone();
                    ips.extend(iface.ipv6_addresses.clone());
                    result.push(NetworkInterfaceContext {
                        name: iface.name.clone(),
                        interface_type: if iface.name.starts_with("lo")
                            || iface.name == "Loopback Pseudo-Interface 1"
                        {
                            "loopback".to_string()
                        } else if iface.name.starts_with("wl")
                            || iface.name.contains("Wi-Fi")
                            || iface.name.contains("Wireless")
                        {
                            "wifi".to_string()
                        } else {
                            "ethernet".to_string()
                        },
                        mac_address: iface.mac_address.clone(),
                        ip_addresses: ips,
                        speed_mbps: iface.speed_mbps,
                        mtu: iface.mtu,
                        is_up: iface.is_up,
                    });
                }
            }
        }
        result
    }

    fn collect_motherboard_info(&self) -> Option<MotherboardContext> {
        use crate::motherboard;
        if let Ok(sys_info) = motherboard::get_system_info() {
            return Some(MotherboardContext {
                manufacturer: sys_info.manufacturer.unwrap_or_default(),
                product: sys_info.product_name.unwrap_or_default(),
                bios_vendor: sys_info.bios.vendor.clone(),
                bios_version: sys_info.bios.version.clone(),
                bios_date: sys_info.bios.release_date.clone(),
                chassis_type: None,
            });
        }
        None
    }

    fn collect_power_info(&self) -> Option<PowerSupplyContext> {
        use crate::power_supply::PowerSupplyMonitor;
        if let Ok(monitor) = PowerSupplyMonitor::new() {
            let battery = monitor.primary_battery();
            return Some(PowerSupplyContext {
                status: match monitor.on_ac_power() {
                    Some(true) => "AC Power".to_string(),
                    Some(false) => "Battery".to_string(),
                    // Not "Battery". An empty supply list and a mains supply
                    // that did not report its state both used to arrive here as
                    // `false`.
                    None => "Unknown".to_string(),
                },
                on_ac_power: monitor.on_ac_power(),
                battery_percent: battery.and_then(|b| b.capacity_percent.map(|c| c as f32)),
                battery_status: battery.map(|b| format!("{:?}", b.status)),
                time_remaining_minutes: battery.and_then(|b| b.time_to_empty_min),
            });
        }
        None
    }

    fn collect_fans(&self) -> Vec<FanContext> {
        use crate::fan_control::FanMonitor;
        let mut result = Vec::new();
        if let Ok(monitor) = FanMonitor::new() {
            for fan in monitor.fans() {
                result.push(FanContext {
                    name: fan.name.clone(),
                    speed_rpm: fan.rpm,
                    speed_percent: Some(fan.speed_percent as u8),
                    min_rpm: fan.rpm_min,
                    max_rpm: fan.rpm_max,
                });
            }
        }
        result
    }

    fn collect_temperature_sensors(&self) -> Vec<TemperatureSensorContext> {
        let mut result = Vec::new();
        // Use fan_control ThermalZone data for temperature sensors
        use crate::fan_control::FanMonitor;
        if let Ok(monitor) = FanMonitor::new() {
            for zone in monitor.thermal_zones() {
                result.push(TemperatureSensorContext {
                    name: zone.name.clone(),
                    temperature_c: zone.temp_celsius,
                    high_threshold_c: None,
                    critical_threshold_c: None,
                    location: Some(zone.zone_type.clone()),
                });
            }
        }
        // Also try hwmon/motherboard sensors
        use crate::motherboard;
        if let Ok(sensors) = motherboard::enumerate_sensors() {
            for sensor_dev in &sensors {
                if let Ok(temp_sensors) = sensor_dev.temperature_sensors() {
                    for ts in &temp_sensors {
                        result.push(TemperatureSensorContext {
                            name: ts.label.clone(),
                            temperature_c: ts.temperature,
                            high_threshold_c: ts.max,
                            critical_threshold_c: ts.critical,
                            location: Some(format!("{:?}", ts.sensor_type)),
                        });
                    }
                }
            }
        }
        result
    }

    fn collect_cpu_metrics(&self) -> Option<CpuMetrics> {
        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::cpu;
            if let Ok(stats) = cpu::read_cpu_stats() {
                let utilization = 100.0 - stats.total.idle;
                let per_core: Vec<Option<f32>> = stats
                    .cores
                    .iter()
                    .map(|c| c.idle.map(|i| 100.0 - i))
                    .collect();
                let freq = stats
                    .cores
                    .first()
                    .and_then(|c| c.frequency.as_ref())
                    .and_then(|f| f.current);
                return Some(CpuMetrics {
                    utilization_percent: utilization,
                    per_core_utilization: per_core,
                    frequency_mhz: freq,
                    temperature_c: None,
                });
            }
        }
        #[cfg(target_os = "windows")]
        {
            use crate::platform::windows;
            if let Ok(stats) = windows::read_cpu_stats() {
                let utilization = 100.0 - stats.total.idle;
                let per_core: Vec<Option<f32>> = stats
                    .cores
                    .iter()
                    .map(|c| c.idle.map(|i| 100.0 - i))
                    .collect();
                let freq = stats
                    .cores
                    .first()
                    .and_then(|c| c.frequency.as_ref())
                    .and_then(|f| f.current);
                return Some(CpuMetrics {
                    utilization_percent: utilization,
                    per_core_utilization: per_core,
                    frequency_mhz: freq,
                    temperature_c: None,
                });
            }
        }
        None
    }

    fn collect_gpu_metrics(&self) -> Vec<GpuMetrics> {
        use crate::gpu::GpuCollection;
        let mut result = Vec::new();
        if let Ok(gpus) = GpuCollection::auto_detect() {
            if let Ok(snapshots) = gpus.snapshot_all() {
                for snap in &snapshots {
                    result.push(GpuMetrics {
                        index: snap.static_info.index,
                        utilization_percent: snap.dynamic_info.utilization.map(f32::from),
                        memory_used_mb: snap.dynamic_info.memory.used.map(|v| v / (1024 * 1024)),
                        memory_total_mb: snap.dynamic_info.memory.total.map(|v| v / (1024 * 1024)),
                        temperature_c: snap.dynamic_info.thermal.temperature.map(|t| t as f32),
                        power_watts: snap.dynamic_info.power.draw.map(|p| p as f32 / 1000.0),
                        fan_speed_percent: snap.dynamic_info.thermal.fan_speed,
                        clocks: Some(GpuClocks {
                            graphics_mhz: snap.dynamic_info.clocks.graphics,
                            memory_mhz: snap.dynamic_info.clocks.memory,
                            sm_mhz: snap.dynamic_info.clocks.sm,
                        }),
                    });
                }
            }
        }
        result
    }

    fn collect_memory_metrics(&self) -> Option<MemoryMetrics> {
        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::memory;
            if let Ok(stats) = memory::read_memory_stats() {
                return Some(MemoryMetrics {
                    used_mb: stats.ram.used / 1024,
                    free_mb: stats.ram.free / 1024,
                    total_mb: stats.ram.total / 1024,
                    cached_mb: stats.ram.cached.map(|v| v / 1024),
                    buffers_mb: stats.ram.buffers.map(|v| v / 1024),
                    swap_used_mb: stats.swap.used.map(|v| v / 1024),
                    swap_total_mb: stats.swap.total.map(|v| v / 1024),
                });
            }
        }
        #[cfg(target_os = "windows")]
        {
            use crate::platform::windows;
            if let Ok(stats) = windows::read_memory_stats() {
                return Some(MemoryMetrics {
                    used_mb: stats.ram.used / 1024,
                    free_mb: stats.ram.free / 1024,
                    total_mb: stats.ram.total / 1024,
                    // These were `u64` sentinels that this layer converted
                    // back to `None` with `> 0` -- which also erased a genuine
                    // zero. The absence is carried in the type now.
                    cached_mb: stats.ram.cached.map(|v| v / 1024),
                    buffers_mb: stats.ram.buffers.map(|v| v / 1024),
                    swap_used_mb: stats.swap.used.map(|v| v / 1024),
                    swap_total_mb: stats.swap.total.map(|v| v / 1024),
                });
            }
        }
        None
    }

    fn collect_disk_metrics(&self) -> Vec<DiskMetrics> {
        use crate::disk;
        let mut result = Vec::new();
        if let Ok(disks) = disk::enumerate_disks() {
            for d in &disks {
                let fs_list = d.filesystem_info().ok().unwrap_or_default();
                let io = d.io_stats().ok();
                for fs in &fs_list {
                    result.push(DiskMetrics {
                        device: format!("{} ({})", d.name(), fs.mount_point.display()),
                        used_gb: fs.used_size as f64 / (1024.0 * 1024.0 * 1024.0),
                        free_gb: fs.available_size as f64 / (1024.0 * 1024.0 * 1024.0),
                        total_gb: fs.total_size as f64 / (1024.0 * 1024.0 * 1024.0),
                        read_bps: io.as_ref().and_then(|i| i.read_throughput),
                        write_bps: io.as_ref().and_then(|i| i.write_throughput),
                        iops_read: io.as_ref().map(|i| i.read_ops),
                        iops_write: io.as_ref().map(|i| i.write_ops),
                    });
                }
            }
        }
        result
    }

    fn collect_network_metrics(&self) -> Vec<NetworkMetrics> {
        use crate::NetworkMonitor;
        let mut result = Vec::new();
        if let Ok(mut monitor) = NetworkMonitor::new() {
            if let Ok(ifaces) = monitor.interfaces() {
                for iface in &ifaces {
                    let rate = monitor.bandwidth_rate(&iface.name, iface);
                    result.push(NetworkMetrics {
                        interface: iface.name.clone(),
                        rx_bps: rate.map(|(rx, _)| rx),
                        tx_bps: rate.map(|(_, tx)| tx),
                        rx_bytes_total: iface.rx_bytes,
                        tx_bytes_total: iface.tx_bytes,
                        rx_packets: iface.rx_packets,
                        tx_packets: iface.tx_packets,
                        errors: iface.rx_errors + iface.tx_errors,
                        dropped: iface.rx_drops + iface.tx_drops,
                    });
                }
            }
        }
        result
    }

    fn collect_processes(&self) -> Vec<ProcessMetrics> {
        use crate::ProcessMonitor;
        let mut result = Vec::new();
        if let Ok(mut monitor) = ProcessMonitor::without_gpu() {
            if let Ok(procs) = monitor.processes() {
                for p in &procs {
                    result.push(ProcessMetrics {
                        pid: p.pid,
                        name: p.name.clone(),
                        cpu_percent: p.cpu_percent,
                        memory_mb: p.memory_bytes / (1024 * 1024),
                        // This guarded `> 0` to undo a sentinel the source
                        // should not have made. The source says None now.
                        gpu_memory_mb: p.total_gpu_memory_bytes.map(|b| b / (1024 * 1024)),
                        threads: p.thread_count,
                        user: p.user.clone(),
                        command: Some(p.name.clone()),
                    });
                }
            }
        }
        result
    }

    fn collect_services(&self) -> Vec<ServiceContext> {
        // Service enumeration is platform-specific and expensive; return empty for now
        Vec::new()
    }

    fn collect_drivers(&self) -> Vec<DriverContext> {
        use crate::motherboard;
        let mut result = Vec::new();
        if let Ok(drivers) = motherboard::get_driver_versions() {
            for d in &drivers {
                result.push(DriverContext {
                    name: d.name.clone(),
                    version: Some(d.version.clone()),
                    provider: d.vendor.clone(),
                    date: d.date.clone(),
                });
            }
        }
        result
    }

    fn collect_boot_config(&self) -> Option<BootConfigContext> {
        use crate::motherboard;
        if let Ok(sys_info) = motherboard::get_system_info() {
            {
                let bios = &sys_info.bios;
                return Some(BootConfigContext {
                    boot_mode: format!("{:?}", bios.firmware_type),
                    secure_boot: bios.secure_boot,
                    boot_entries: Vec::new(),
                });
            }
        }
        None
    }

    fn collect_system_load(&self) -> Option<SystemLoadMetrics> {
        #[cfg(target_os = "linux")]
        {
            if let Ok(content) = std::fs::read_to_string("/proc/loadavg") {
                let parts: Vec<&str> = content.split_whitespace().collect();
                if parts.len() >= 5 {
                    // A field that does not parse is absent, not zero. A
                    // load average of 0.00 is a specific claim about an idle
                    // machine, and `/proc/loadavg` failing to parse says
                    // nothing about how busy the machine is.
                    let load_1 = parts[0].parse::<f64>().ok();
                    let load_5 = parts[1].parse::<f64>().ok();
                    let load_15 = parts[2].parse::<f64>().ok();
                    // parts[3] is "running/total" processes
                    let proc_parts: Vec<&str> = parts[3].split('/').collect();
                    let running = proc_parts.first().and_then(|s| s.parse::<u32>().ok());
                    let total = proc_parts.get(1).and_then(|s| s.parse::<u32>().ok());
                    return Some(SystemLoadMetrics {
                        load_1,
                        load_5,
                        load_15,
                        running_processes: running,
                        total_processes: total,
                    });
                }
            }
        }
        #[cfg(target_os = "windows")]
        {
            // Windows has no load average, and this used to synthesise one:
            // `(usage / 100) * cores` from a single CPU reading, assigned to all
            // three windows. See `SystemLoadMetrics::load_1` for why that figure
            // is not a load average and cannot be made into one. It is absent.
            //
            // The process count is a real reading and is reported.
            if let Ok(stats) = crate::system_stats::SystemStats::new() {
                return Some(SystemLoadMetrics {
                    load_1: None,
                    load_5: None,
                    load_15: None,
                    running_processes: stats.running_processes,
                    total_processes: stats.total_processes,
                });
            }
        }
        None
    }

    fn collect_alerts(&self) -> Vec<AlertContext> {
        // Alert collection could be wired to health module in the future
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_info_serialization() {
        let info = RateLimitInfo {
            remaining: 95,
            limit: 100,
            reset_at: 1700000000,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"remaining\":95"));
        assert!(json.contains("\"limit\":100"));
        assert!(json.contains("\"reset_at\":1700000000"));
    }

    #[test]
    fn test_rate_limit_info_deserialization() {
        let json = r#"{"remaining":50,"limit":200,"reset_at":1700000060}"#;
        let info: RateLimitInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.remaining, 50);
        assert_eq!(info.limit, 200);
        assert_eq!(info.reset_at, 1700000060);
    }

    #[test]
    fn test_response_meta_without_rate_limit() {
        let meta = ResponseMeta {
            request_id: Some("req-123".to_string()),
            timestamp: 1700000000,
            duration_ms: 42,
            rate_limit: None,
        };
        assert_eq!(meta.request_id.as_deref(), Some("req-123"));
        assert_eq!(meta.timestamp, 1700000000);
        assert_eq!(meta.duration_ms, 42);
        assert!(meta.rate_limit.is_none());
    }

    #[test]
    fn test_response_meta_with_rate_limit() {
        let meta = ResponseMeta {
            request_id: None,
            timestamp: 1700000000,
            duration_ms: 5,
            rate_limit: Some(RateLimitInfo {
                remaining: 99,
                limit: 100,
                reset_at: 1700000060,
            }),
        };
        assert!(meta.request_id.is_none());
        let rl = meta.rate_limit.unwrap();
        assert_eq!(rl.remaining, 99);
        assert_eq!(rl.limit, 100);
    }

    #[test]
    fn test_api_response_serialization() {
        let response: ApiResponse<String> = ApiResponse {
            data: "hello".to_string(),
            meta: ResponseMeta {
                request_id: Some("test-id".to_string()),
                timestamp: 1700000000,
                duration_ms: 1,
                rate_limit: None,
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"data\":\"hello\""));
        assert!(json.contains("\"request_id\":\"test-id\""));
    }

    #[test]
    fn test_observability_api_new() {
        let config = ApiConfig::default();
        let api = ObservabilityApi::new(config);
        // Should create successfully with empty config
        let checker = api.permission_checker();
        assert!(checker
            .read()
            .unwrap()
            .rate_limit_status("nonexistent")
            .is_none());
    }
}
