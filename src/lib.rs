//! # IronMonitor (ironmon)
//!
//! A comprehensive, cross-platform Rust library for hardware monitoring. IronMonitor provides
//! a unified API for monitoring CPUs, GPUs (NVIDIA/AMD/Intel), memory, disks, motherboards,
//! processes, and network interfaces across Windows, Linux, and macOS.
//!
//! ## Features
//!
//! - **Multi-Vendor GPU Support**: NVIDIA (NVML), AMD (sysfs), Intel (i915/xe)
//! - **Process Monitoring**: System-wide process tracking with GPU attribution
//! - **Network Monitoring**: Interface statistics with bandwidth rate calculation
//! - **Cross-Platform**: Unified API across Windows, Linux, and macOS
//! - **Zero-Cost Abstractions**: Built in Rust for maximum performance and safety
//! - **Terminal UI**: Beautiful TUI for real-time monitoring
//!
//! ## Quick Start
//!
//! ### GPU Monitoring
//!
//! ```no_run
//! use ironmonlib::gpu::GpuCollection;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Auto-detect all available GPUs
//! let gpus = GpuCollection::auto_detect()?;
//!
//! // Get snapshot of all GPUs
//! for (idx, info) in gpus.snapshot_all()?.iter().enumerate() {
//!     println!("GPU {}: {} ({})",
//!         idx,
//!         info.static_info.name,
//!         info.static_info.vendor
//!     );
//!     if let Some(util) = info.dynamic_info.utilization {
//!         println!("  Utilization: {}%", util);
//!     }
//!     // `None` where the device reported no figure, which is why these are
//!     // `Option` -- an adapter with no readable memory is not one with none.
//!     if let (Some(used), Some(total)) =
//!         (info.dynamic_info.memory.used, info.dynamic_info.memory.total)
//!     {
//!         println!("  Memory: {} / {} MB", used / 1024 / 1024, total / 1024 / 1024);
//!     }
//!     if let Some(temp) = info.dynamic_info.thermal.temperature {
//!         println!("  Temperature: {}°C", temp);
//!     }
//!     if let Some(power) = info.dynamic_info.power.draw {
//!         println!("  Power: {:.1}W", power as f32 / 1000.0);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### Process Monitoring with GPU Attribution
//!
//! ```no_run
//! use ironmonlib::{ProcessMonitor, GpuCollection};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let gpus = GpuCollection::auto_detect()?;
//! let mut monitor = ProcessMonitor::with_gpus(gpus)?;
//!
//! // Get processes sorted by GPU memory usage
//! let gpu_procs = monitor.processes_by_gpu_memory()?;
//! for proc in gpu_procs.iter().take(10) {
//!     println!("{} (PID {}): {} MB GPU memory",
//!         proc.name,
//!         proc.pid,
//!         proc.gpu_memory_mb()
//!             .map_or_else(|| "n/a".to_string(), |mb| format!("{mb:.0}"))
//!     );
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### Network Monitoring
//!
//! ```no_run
//! use ironmonlib::NetworkMonitor;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut monitor = NetworkMonitor::new()?;
//!
//! // Get all active interfaces
//! for iface in monitor.active_interfaces()? {
//!     // `None` on the first call for an interface: a rate is a difference
//!     // between two samples, and this is the first.
//!     if let Some((rx_rate, tx_rate)) = monitor.bandwidth_rate(&iface.name, &iface) {
//!         println!("{}: ↓{:.2} MB/s ↑{:.2} MB/s",
//!             iface.name,
//!             rx_rate / 1_000_000.0,
//!             tx_rate / 1_000_000.0
//!         );
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### AI Data API - Tool-Based System Access
//!
//! The AI Data API provides a comprehensive tool-based interface for AI systems
//! to query hardware monitoring data:
//!
//! ```no_run
//! use ironmonlib::ai_api::{AiDataApi, ToolCategory};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut api = AiDataApi::new()?;
//!
//! // List available tools (35+ tools across 8 categories)
//! for tool in api.list_tools_by_category(ToolCategory::Gpu) {
//!     println!("{}: {}", tool.name, tool.description);
//! }
//!
//! // Call a tool
//! let result = api.call_tool("get_gpu_status", serde_json::json!({}))?;
//! if result.success {
//!     println!("{}", serde_json::to_string_pretty(&result.data)?);
//! }
//!
//! // Get tools in OpenAI function calling format
//! let openai_tools = api.tools_as_openai_functions();
//!
//! // Get tools in Anthropic tools format
//! let anthropic_tools = api.tools_as_anthropic_tools();
//! # Ok(())
//! # }
//! ```
//!
//! ## Feature Flags
//!
//! - `nvidia` - NVIDIA GPU support via NVML (requires CUDA toolkit or NVIDIA driver)
//! - `amd` - AMD GPU support via sysfs/DRM (Linux only, requires amdgpu driver)
//! - `intel` - Intel GPU support via i915/xe drivers (Linux only)
//! - `cli` - Command-line interface and TUI
//! - `full` - All features enabled
//!
//! ## Platform Support
//!
//! | Platform | CPU | Memory | Disk | GPU (NVIDIA) | GPU (AMD) | GPU (Intel) | Network | Processes |
//! |----------|-----|--------|------|--------------|-----------|-------------|---------|-----------|
//! | Linux    | ✅  | ✅     | ✅   | ✅           | ✅        | ✅          | ✅      | ✅        |
//! | Windows  | ✅  | ✅     | ✅   | ✅           | 🚧        | 🚧          | 🚧      | 🚧        |
//! | macOS    | ✅  | ✅     | ✅   | ❌           | ❌        | ❌          | 🚧      | 🚧        |
//!
//! ## Examples
//!
//! See the `examples/` directory for comprehensive examples:
//!
//! - `gpu_monitor.rs` - Multi-vendor GPU monitoring
//! - `process_monitor.rs` - Process tracking with GPU attribution
//! - `network_monitor.rs` - Network interface statistics
//! - `tui.rs` - Interactive terminal UI
//! - `all_gpus.rs` - Unified multi-vendor GPU example
//! - `ai_api_demo.rs` - AI Data API demonstration with tool-based system access

pub mod agent; // AI agent for system analysis and predictions
pub mod ai_api; // AI Data API for full system visibility with tool-based access
pub mod ai_workload; // AI training and inference workload monitoring
pub mod anomaly; // Anomaly detection and optimization recommendations
pub mod audio; // Audio device monitoring and control
pub mod bandwidth; // Network bandwidth testing (iperf-style)
pub mod battery; // Battery and power status monitoring
pub mod bluetooth; // Bluetooth device monitoring
pub mod boot_config; // Boot configuration and startup management
pub mod config; // Configuration management with TOML persistence
pub mod connections; // Network connection monitoring (netstat-like)
pub mod consent; // User consent management for ethical data collection
pub mod core;
pub mod cpufreq; // CPU frequency scaling and governor control
pub mod disk; // Disk/storage monitoring
pub mod display; // Display/monitor information
pub mod error;
pub mod fan_control; // Advanced fan monitoring and control
pub mod fetch; // neofetch-style summary: logo beside the machine, unknowns named
pub mod gpu; // GPU abstraction layer
pub mod health; // System health scoring and alerts
pub mod hwmon; // Hardware monitoring (temperatures, voltages, fans) - native implementation
pub mod legacy_paths; // Adoption of files stranded by the simon -> IronMonitor rename
pub mod memory_management; // Memory and swap management (jetson_stats style)
pub mod motherboard; // Motherboard sensors, BIOS, system information
pub mod network_monitor; // Network interface monitoring
pub mod network_tools; // Network diagnostic tools (ping, traceroute, port scan) - nmap/netcat style
pub mod observability; // Full system observability API with MCP-like permissions for external AI access
pub mod ontology; // Machine-readable entity/unit/provenance schema shared by CLI, TUI and GUI
pub mod platform;
pub mod power_supply; // Battery and power supply monitoring
pub mod process_monitor; // Unified process monitoring with GPU attribution
pub mod process_tree; // Process tree visualization with container/cgroup awareness
pub mod profile; // Hardware profile inspector (NVPI / XTU / Ryzen Master / nvme-cli style)
pub mod sandbox; // Sandbox and VM detection for ethical data collection
pub mod services; // System service monitoring and control
pub mod silicon; // New: Unified silicon monitoring (CPU, NPU, I/O, network)
pub mod stats;
pub mod system_stats; // System-wide stats (load avg, vmstat, uptime) - Linux/BSD style
pub mod tsdb; // Time-series database for recording metrics
pub mod tuning; // Use-case detection and hardware profile recommendations
pub mod usb; // USB device enumeration

// Additional hardware monitors
pub mod camera; // Camera and webcam device monitoring
pub mod codec; // Hardware codec and media capability detection
pub mod cpu_cache; // CPU cache topology (L1/L2/L3 sizes, associativity)
pub mod firmware; // Firmware inventory (BIOS, ME, NIC, storage)
pub mod hardware_ai; // AI-based hardware inference engine
pub mod ids; // Network and file intrusion detection: observe and report, never block
pub mod input; // Input device monitoring (keyboards, mice, touchpads, game controllers)
pub mod numa; // NUMA topology and memory affinity
pub mod os_info; // OS and kernel information (version, modules, boot info)
pub mod pci_devices; // PCI device enumeration and classification
pub mod power_profile; // Power profile and energy management
pub mod printer; // Printer and print queue monitoring
pub mod sensors; // Environmental sensors (accelerometer, gyroscope, light)
pub mod smart; // S.M.A.R.T. disk health monitoring with AI inference
pub mod storage_controller; // Storage controllers, RAID arrays, NVMe
pub mod tpm; // Trusted Platform Module monitoring

// Deep hardware topology, security, and resource monitors
pub mod cgroup_monitor; // Linux cgroup v1/v2 resource monitoring for containers
pub mod cpu_microarch; // CPU microarchitecture detection and ISA extensions
pub mod crypto_accel; // Cryptographic hardware acceleration (AES-NI, SHA, RNG)
pub mod drm_monitor; // DRM/KMS subsystem monitoring (connectors, CRTCs, planes)
pub mod interconnect; // CPU interconnect topology (QPI/UPI, Infinity Fabric, Apple UMA)
pub mod interrupt_map; // IRQ topology, per-CPU counts, MSI/MSI-X, affinity
pub mod memory_bandwidth; // Memory bandwidth monitoring and estimation
pub mod memory_topology; // Physical DIMM enumeration, speed, timing, ECC
pub mod rapl; // Intel RAPL / AMD power capping energy monitoring
pub mod security_mitigations; // CPU vulnerability mitigations, kernel security, posture scoring

// OS-level, device, and silicon subsystem monitors
pub mod dma_engine; // DMA engine monitoring (Intel IOAT/DSA/IAX, channels)
pub mod edac; // Error Detection and Correction (ECC memory errors)
pub mod gpu_topology; // Multi-GPU topology, NVLink/xGMI/PCIe P2P, NUMA affinity
pub mod io_scheduler; // Block I/O scheduler monitoring (mq-deadline, BFQ, kyber)
pub mod iommu; // IOMMU detection (VT-d, AMD-Vi, SMMU), group enumeration
pub mod kernel_params; // Kernel sysctl parameter monitoring and security scoring
pub mod scheduler; // Linux process scheduler (CFS/EEVDF, PSI, schedstat)
pub mod thermal_zone; // Thermal zone, trip point, and cooling device monitoring
pub mod voltage_regulator; // Voltage regulator monitoring (SoC/CPU/GPU rails)
pub mod watchdog; // Hardware/software watchdog timer monitoring

// Datacenter, virtualization, and fleet monitoring
pub mod daemon; // Monitoring daemon for headless/remote operation
pub mod datacenter; // Datacenter chassis, IPMI, rack topology
pub mod fleet; // Fleet-level multi-host monitoring and aggregation
pub mod http_server; // HTTP server for REST API and Prometheus
pub mod pcie; // PCIe device monitoring
pub mod predictive; // Predictive maintenance and failure analysis
pub mod prometheus; // Prometheus metrics exporter
pub mod virtualization; // VM/container detection and resource monitoring
pub mod wsl; // WSL2 detection and monitoring

// Unsafe utilities (swap, clocks, power_mode) - gated behind feature flag
// See SECURITY.md for vulnerability details
#[cfg(feature = "jetson-utils")]
pub mod utils;

// Unified backend for CLI, TUI, and GUI
pub mod backend;

// Lock-free snapshot pipeline: decouples hardware collection from rendering
pub mod pipeline;

#[cfg(feature = "cli")]
pub mod tui; // Terminal UI

#[cfg(feature = "cli")]
pub use tui::{AcceleratorInfo, AcceleratorType};

#[cfg(feature = "gui")]
pub mod gui; // Graphical UI

// Read-only reporting of an IronVault model vault. Behind `vault` because
// IronVault requires Rust 1.89 and the rest of this crate requires 1.70.
#[cfg(feature = "vault")]
pub mod model_vault;

// Re-export new hardware monitors
pub use audio::{AudioDevice, AudioDeviceType, AudioMonitor, AudioState};
pub use bluetooth::{
    BluetoothAdapter, BluetoothDevice, BluetoothDeviceType, BluetoothEvent, BluetoothMonitor,
    BluetoothState,
};
pub use display::{DisplayConnection, DisplayInfo, DisplayMonitor, HdrMode};
pub use usb::{UsbDevice, UsbDeviceClass, UsbEvent, UsbMonitor, UsbSpeed};
// Re-export main types
pub use core::{
    cpu::CpuStats,
    engine::{EngineInfo, EngineStats},
    fan::FanStats,
    gpu::{GpuInfo as CoreGpuInfo, GpuStats, GpuType},
    memory::MemoryStats,
    power::PowerStats,
    process::{ProcessInfo, ProcessStats},
    temperature::TemperatureStats,
};
pub use error::{Error, IronError, Result};
pub use stats::{IronMonitor, Snapshot};

// Re-export unified GPU interface (legacy)
pub use gpu::{
    Gpu, GpuClocks, GpuCollection, GpuDynamicInfo, GpuEngines, GpuInfo, GpuMemory, GpuPower,
    GpuProcess as LegacyGpuProcess, GpuProcessType, GpuStaticInfo, GpuThermal, GpuVendor,
    PcieLinkInfo,
};

// Re-export new unified GPU traits (preferred)
pub use gpu::traits;
// Note: GpuError conflicts with crate::Error::GpuError variant
pub use gpu::{Clocks, Device, Memory, Power, Temperature, Utilization, Vendor};

// Re-export process monitor
pub use process_monitor::{
    CategoryStats, ProcessCategory, ProcessGpuType, ProcessMonitor, ProcessMonitorInfo,
};

// Re-export network monitor
pub use network_monitor::{NetworkInterfaceInfo, NetworkMonitor};

// Re-export connection monitor (netstat-like)
pub use connections::{ConnectionInfo, ConnectionMonitor, ConnectionState, Protocol};

// Re-export AI workload monitoring
pub use ai_workload::{
    AiFramework, AiWorkload, AiWorkloadMonitor, CloudProvider, DistributedConfig, InferenceMetrics,
    TpuConfig, TrainingMetrics, WorkloadType,
};

// Re-export AI Data API for full system visibility
pub use ai_api::{AiDataApi, ToolCall, ToolCategory, ToolDefinition, ToolResult};

// Re-export Observability API for external AI access with MCP-like permissions
pub use observability::{
    // Permission types
    ApiConfig,
    ApiKey,
    // API types
    ApiResponse,
    Capability,
    EventCategory,
    EventFilter,
    // Event types
    EventManager,
    EventSeverity,
    HardwareContext,
    HttpRequest,
    HttpResponse,
    // Metric types
    MetricCollector,
    MetricSnapshot,
    MetricStats,
    MetricValue,
    MinimalContext,
    ObservabilityApi,
    ObservabilityError,
    OpenApiSpec,
    Permission,
    PermissionChecker,
    RequestContext,
    RequestHandler,
    ResponseMeta,
    Scope,
    // Server types
    ServerConfig,
    SoftwareContext,
    StreamChannel,
    // Streaming types
    StreamManager,
    StreamMessage,
    Subscription,
    // Context types
    SystemContext,
    SystemContextBuilder,
    SystemEvent,
};

// Re-export AI agent
pub use agent::{Agent, AgentConfig, AgentResponse, ModelSize, Query, QueryType, SystemState};

// Re-export configuration management
pub use config::{ChartConfig, Config, GeneralConfig, GpuConfig, ProcessConfig};

// Re-export consent management
pub use consent::{
    is_offline_mode, is_telemetry_disabled, ConsentConfig, ConsentManager, ConsentRecord,
    ConsentScope,
};

// Re-export sandbox detection
pub use sandbox::{SandboxDetector, SandboxInfo};

// Re-export system-wide stats (Linux/BSD style)
pub use system_stats::{CpuTime, LoadAverage, SystemStats, VmStats};

// Re-export network diagnostic tools (nmap/traceroute/ping/tcpdump style)
pub use network_tools::{
    // Tcpdump-style packet capture
    capture_dns,
    capture_http,
    capture_packets,
    capture_tcp,
    // Ping/Traceroute
    check_connectivity,
    check_port,
    // Port scanning
    common_ports,
    dns_lookup,
    // Nmap-style scanning
    full_scan,
    get_service_name,
    grab_banner,
    is_capture_available,
    latency_test,
    list_capture_interfaces,
    nmap_scan,
    parallel_scan,
    parallel_scan_with_banners,
    ping,
    quick_capture,
    quick_scan,
    reverse_dns,
    scan_port_range,
    scan_ports,
    scan_ports_with_timeout,
    traceroute,
    CaptureConfig,
    CaptureProtocol,
    CaptureResult,
    CapturedPacket,
    // Utility
    NetworkTools,
    NmapScanResult,
    OsFingerprint,
    PingResult,
    PortScanResult,
    PortStatus,
    ServiceInfo,
    TracerouteHop,
    TracerouteResult,
};

// Re-export profile inspector
pub use profile::{
    active::{active_profiles_for_processes, matched_active_profiles, ActiveAppProfile},
    apply::{apply_setting, writable_setting_ids, ApplyOutcome, ApplyStatus},
    cache::{CachedProfileInspector, CACHE_STATS as PROFILE_CACHE_STATS},
    deviation::{deviations_from_default, Deviation},
    diff::{diff_snapshots, DiffChange, DiffEntry, DiffGroup, ProfileDiff},
    explain::{candidates as explain_candidates, explain, SettingExplanation},
    ProfileGroup, ProfileInspector, ProfileProvider, ProfileSnapshot, Setting, SettingRisk,
    SettingValue, Subsystem as ProfileSubsystem,
};

// Re-export power supply / battery monitoring
pub use power_supply::{
    battery_percent, is_on_ac_power, is_on_battery, power_summary, BatteryHealth, ChargingStatus,
    PowerSupplyInfo, PowerSupplyMonitor, PowerSupplyType,
};

// Re-export system health monitoring
pub use health::{
    has_critical_issues, health_score, quick_health_check, HealthCheck, HealthStatus,
    HealthThresholds, SystemHealth,
};

// Re-export bandwidth testing (iperf-style)
pub use bandwidth::{
    bandwidth_test, loopback_test, memory_bandwidth_test, quick_bandwidth_estimate,
    BandwidthConfig, BandwidthResult, MemoryBandwidthResult, DEFAULT_BUFFER_SIZE, DEFAULT_PORT,
};

// Re-export fan control
pub use fan_control::{
    fan_summary, list_fans, list_thermal_zones, FanControlMode, FanCurve, FanCurvePoint, FanInfo,
    FanMonitor, FanProfile, FanSummary, FanType, ThermalZone, TripPoint,
};

// Re-export CPU frequency scaling
pub use cpufreq::{
    available_governors, cpufreq_summary, list_cpus, CpuFreqInfo, CpuFreqMonitor, CpuFreqPolicy,
    CpuFreqSummary, CpuIdleState, EnergyPreference, Governor, TurboStatus,
};

// Re-export system services monitoring
pub use services::{
    common_services, get_services_status, is_service_running, service_summary,
    ServiceInfo as SystemServiceInfo, ServiceMonitor, ServiceStatus, ServiceSummary, ServiceType,
    StartupType,
};

// Re-export memory management (jetson_stats style)
pub use memory_management::{
    format_bytes, memory_summary, MemoryInfo, MemoryMonitor, MemoryPressure, MemorySummary,
    ProcessMemory, SwapDevice, SwapInfo, SwapType, ZramInfo,
};

// Re-export anomaly detection
pub use anomaly::{
    Anomaly, AnomalyConfig, AnomalyDetector, AnomalySeverity, AnomalySummary, Recommendation,
};

// Re-export process tree
pub use process_tree::{ContainerRuntime, ProcessNode, ProcessTree, ProcessTreeSummary};

// Re-export boot configuration
pub use boot_config::{
    boot_summary, format_uptime, BootInfo, BootMonitor, BootSummary, BootTime, BootType,
    KernelParams, StartupItem, StartupItemStatus, StartupItemType,
};

// Re-export unified backend
pub use backend::{
    AcceleratorState, BackendConfig, CpuState, DiskState, FullSystemState, HistoryBuffer,
    MemoryState, MonitoringBackend, NetworkState, ProcessState, SystemInfoState,
    DEFAULT_HISTORY_SIZE, DEFAULT_UPDATE_INTERVAL,
};

// Re-export time-series database for metrics recording
pub use tsdb::{
    format_size, parse_size, DatabaseStats, MetricSample, MetricsRecorder, ProcessSnapshot,
    SystemSnapshot, TimeSeriesDb,
};

// Re-export datacenter monitoring
pub use datacenter::{
    ChassisInfo, ChassisType, DatacenterError, DatacenterMonitor, DatacenterSnapshot, FormFactor,
    IpmiController, IpmiSensor, RackInfo,
};

// Re-export virtualization detection
pub use virtualization::{
    ContainerEngine, ContainerInfo, CpuVirtCapability, GuestResources, Hypervisor, HypervisorInfo,
    VirtMonitor, VirtPlatform, VirtSnapshot,
};

// Re-export fleet management
pub use fleet::{
    AlertCategory, AlertSeverity, FleetAlert, FleetConfig, FleetManager, FleetSnapshot,
    FleetThresholds, HostInfo, HostMetrics, HostStatus, HostSummary,
};

// Re-export daemon
pub use daemon::{DaemonConfig, DaemonError, MonitoringDaemon};

// Re-export camera monitoring
pub use camera::{CameraCapability, CameraConnection, CameraInfo, CameraMonitor};

// Re-export CPU cache topology
pub use cpu_cache::{CacheLevel, CacheType, CpuCacheInfo, CpuCacheMonitor, CpuCacheTopology};

// Re-export input device monitoring
pub use input::{InputDevice, InputDeviceType, InputInterface, InputMonitor};

// Re-export OS/kernel information
pub use os_info::{BootMode, KernelModule, OsFamily, OsInfo, OsInfoMonitor};

// Re-export printer monitoring
pub use printer::{PrinterConnection, PrinterInfo, PrinterMonitor, PrinterStatus, PrinterType};

// Re-export storage controller monitoring
pub use storage_controller::{
    NvmeControllerInfo, RaidArrayInfo, RaidLevel, RaidStatus, StorageControllerInfo,
    StorageControllerMonitor, StorageInterface,
};

// Re-export TPM monitoring
pub use tpm::{TpmInfo, TpmMonitor, TpmStatus, TpmVersion};

// Re-export new hardware monitors
pub use codec::{
    BitDepth, CapabilitySource, CodecCapability, CodecDirection, CodecMonitor, ComputeCapability,
    MaxResolution, VideoCodec,
};
pub use firmware::{
    BootMode as FirmwareBootMode, FirmwareComponent, FirmwareEntry, FirmwareInventory,
    SecureBootStatus,
};
pub use hardware_ai::{
    AnomalySeverity as HwAnomalySeverity, Bottleneck, BottleneckType, HardwareAge,
    HardwareAnalysisReport, HardwareAnomaly, HardwareInferenceEngine, PerformanceTier, SystemClass,
    ThermalEnvelope, ThermalHeadroom, UpgradeRecommendation, Workload, WorkloadSuitability,
};
pub use numa::{NumaDistanceMatrix, NumaMonitor, NumaNode, NumaSummary};
pub use pci_devices::{PciClass, PciDeviceInfo, PciDeviceMonitor, PciLinkInfo};
pub use power_profile::{
    ChargePolicy, CpuFreqConfig, CpuGovernor, InferredPowerBehavior, PowerPlanInfo, PowerProfile,
    PowerProfileMonitor,
};
pub use sensors::{SensorInfo, SensorMonitor, SensorType, SensorValue};
pub use smart::{DiskHealth, DriveMediaType, SmartAttribute, SmartDiskInfo, SmartMonitor};

// Re-export deep hardware topology, security, and resource monitors
pub use cgroup_monitor::{
    CgroupCpu, CgroupInfo, CgroupMemory, CgroupMonitor, CgroupOverview, CgroupVersion,
};
pub use cpu_microarch::{
    CpuMicroarchMonitor, CpuMicroarchReport, CpuVendor as MicroarchVendor, InferredPerformance,
    IsaExtension, Microarchitecture,
};
pub use crypto_accel::{
    CryptoAccelMonitor, CryptoAccelReport, CryptoCategory, CryptoFeature, HardwareRng, TpmCrypto,
};
pub use drm_monitor::{
    ConnectorStatus, ConnectorType, DrmClient, DrmConnector, DrmDevice, DrmMonitor, DrmOverview,
};
pub use interconnect::{
    ChipletTopology, CoherenceProtocol, InterconnectLink, InterconnectMonitor,
    InterconnectTopology, InterconnectType,
};
pub use interrupt_map::{InterruptAnalysis, InterruptInfo, InterruptMapMonitor, InterruptType};
pub use memory_bandwidth::{
    BandwidthAnalysis, BandwidthEstimate, ChannelConfig, MemoryBandwidthMonitor, MemoryGeneration,
};
pub use memory_topology::{
    DimmInfo, FormFactor as DimmFormFactor, MemoryAnalysis, MemoryTopologyMonitor,
    MemoryType as DimmMemoryType,
};
pub use rapl::{EnergyReading, PowerDomain, PowerEfficiency, PowerSnapshot, RaplMonitor};
pub use security_mitigations::{
    CpuVulnerability, KernelHardening, MitigationStatus, SecurityMitigationsMonitor,
    SecurityModule, SecurityPosture,
};

// Re-export OS-level, device, and silicon subsystem monitors
pub use dma_engine::{
    DmaCapabilities, DmaChannel, DmaController, DmaEngineMonitor, DmaEngineType, DmaOverview,
};
pub use edac::{EdacCsRow, EdacMemType, EdacMemoryController, EdacMonitor, EdacOverview};
pub use gpu_topology::{
    GpuInterconnectType, GpuLink, GpuTopologyMonitor, GpuTopologyNode, GpuTopologyOverview,
};
pub use io_scheduler::{
    BlockDeviceIo, IoSchedulerMonitor, IoSchedulerOverview, IoSchedulerType, IoStats,
};
pub use iommu::{IommuDevice, IommuGroup, IommuMonitor, IommuOverview, IommuType};
pub use kernel_params::{KernelParam, KernelParamsMonitor, KernelParamsReport, ParamCategory};
pub use scheduler::{
    CpuSchedStats, PressureInfo, SchedPolicy, SchedTuning, SchedulerAnalysis, SchedulerMonitor,
};
pub use thermal_zone::{
    CoolingDeviceInfo, ThermalZoneInfo, ThermalZoneMonitor, ThermalZoneOverview, ThermalZoneType,
    TripPoint as ThermalTripPoint, TripPointType,
};
pub use voltage_regulator::{
    RegulatorMode, RegulatorState, VoltageRegulatorInfo, VoltageRegulatorMonitor,
    VoltageRegulatorOverview,
};
pub use watchdog::{
    PreTimeoutGovernor, WatchdogInfo, WatchdogMonitor, WatchdogOverview, WatchdogType,
};

/// Main entry point for unified silicon monitoring.
///
/// Provides a single struct that wraps all available hardware monitors (GPU,
/// CPU, memory, disk, network, and processes).
pub struct UnifiedMonitor {
    gpus: GpuCollection,
    process_monitor: Option<ProcessMonitor>,
    network_monitor: Option<NetworkMonitor>,
}

impl UnifiedMonitor {
    /// Create new silicon monitor with auto-detection of all subsystems.
    pub fn new() -> Result<Self> {
        let gpus = GpuCollection::auto_detect()
            .map_err(|e| IronError::InitializationError(e.to_string()))?;

        let process_monitor = ProcessMonitor::new().ok();
        let network_monitor = NetworkMonitor::new().ok();

        Ok(Self {
            gpus,
            process_monitor,
            network_monitor,
        })
    }

    /// Get all GPU information snapshots.
    pub fn snapshot_gpus(&self) -> Result<Vec<GpuInfo>> {
        self.gpus
            .snapshot_all()
            .map_err(|e| IronError::Other(e.to_string()))
    }

    /// Get GPU collection reference.
    pub fn gpus(&self) -> &GpuCollection {
        &self.gpus
    }

    /// Get mutable GPU collection reference.
    pub fn gpus_mut(&mut self) -> &mut GpuCollection {
        &mut self.gpus
    }

    /// Get number of detected GPUs.
    pub fn gpu_count(&self) -> usize {
        self.gpus.len()
    }

    /// Snapshot current CPU statistics.
    ///
    /// Reads the platform. This called `CpuStats::new()` until 6.0.0, which is a
    /// zero-constructor: every caller of a method named "snapshot current CPU
    /// statistics" received 100% idle and no cores, and nothing said otherwise.
    /// The same defect had already been found and fixed twice in the GUI and
    /// once in the HTTP server before anyone looked at the library's own API.
    pub fn snapshot_cpu(&self) -> Result<CpuStats> {
        crate::stats::platform_cpu_stats()
    }

    /// Snapshot current memory statistics.
    ///
    /// Reads the platform, for the reason given on [`Self::snapshot_cpu`].
    pub fn snapshot_memory(&self) -> Result<MemoryStats> {
        crate::stats::platform_memory_stats()
    }

    /// Snapshot disk information for all detected volumes.
    pub fn snapshot_disks(
        &self,
    ) -> std::result::Result<Vec<Box<dyn disk::DiskDevice>>, disk::Error> {
        disk::enumerate_disks()
    }

    /// Snapshot all network interface statistics.
    pub fn snapshot_network(&mut self) -> Option<Result<Vec<NetworkInterfaceInfo>>> {
        self.network_monitor.as_mut().map(|nm| nm.interfaces())
    }

    /// Get network monitor reference, if available.
    pub fn network_monitor(&self) -> Option<&NetworkMonitor> {
        self.network_monitor.as_ref()
    }

    /// Get mutable network monitor reference, if available.
    pub fn network_monitor_mut(&mut self) -> Option<&mut NetworkMonitor> {
        self.network_monitor.as_mut()
    }

    /// Snapshot all running processes with optional GPU attribution.
    pub fn snapshot_processes(&mut self) -> Option<Result<Vec<ProcessMonitorInfo>>> {
        self.process_monitor.as_mut().map(|pm| pm.processes())
    }

    /// Get process monitor reference, if available.
    pub fn process_monitor(&self) -> Option<&ProcessMonitor> {
        self.process_monitor.as_ref()
    }

    /// Get mutable process monitor reference, if available.
    pub fn process_monitor_mut(&mut self) -> Option<&mut ProcessMonitor> {
        self.process_monitor.as_mut()
    }
}

impl Default for UnifiedMonitor {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            gpus: GpuCollection::new(),
            process_monitor: None,
            network_monitor: None,
        })
    }
}

// Backward compatibility alias
#[deprecated(since = "0.3.0", note = "Use `UnifiedMonitor` directly instead")]
pub type GpuInterface = UnifiedMonitor;

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
