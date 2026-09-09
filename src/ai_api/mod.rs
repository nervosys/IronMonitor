//! AI Data API - Full System Visibility for AI Agents
//!
//! This module provides a comprehensive API that gives AI systems full visibility
//! into hardware monitoring data. It exposes "tools" that AI agents can call to
//! query specific aspects of the system.
//!
//! # Architecture
//!
//! The API is organized around "tools" that mirror how AI agents think:
//! - Each tool has a name, description, and parameters
//! - Tools return structured JSON-serializable data
//! - The system maintains state to calculate rates and deltas
//!
//! # Example
//!
//! ```no_run
//! use ironmonlib::ai_api::{AiDataApi, ToolCall, ToolResult};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut api = AiDataApi::new()?;
//!
//! // List available tools
//! let tools = api.list_tools();
//! for tool in &tools {
//!     println!("{}: {}", tool.name, tool.description);
//! }
//!
//! // Call a tool
//! let result = api.call_tool("get_gpu_status", serde_json::json!({}))?;
//! println!("{}", serde_json::to_string_pretty(&result)?);
//! # Ok(())
//! # }
//! ```

pub mod formats;
pub mod mcp_server;
pub mod ontology;
pub mod tools;
pub mod types;

use crate::error::{IronError, Result};
use crate::gpu::GpuCollection;
use crate::NetworkMonitor;
use crate::ProcessMonitor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub use formats::*;
pub use mcp_server::{McpServer, MCP_PROTOCOL_VERSION};
pub use ontology::*;
pub use tools::*;
pub use types::*;

/// Tool definition for AI systems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name (identifier)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Parameter schema (JSON Schema format)
    pub parameters: serde_json::Value,
    /// Category for organization
    pub category: ToolCategory,
    /// Example usage
    pub example: Option<String>,
}

/// Tool categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolCategory {
    /// GPU monitoring tools
    Gpu,
    /// CPU monitoring tools
    Cpu,
    /// Memory monitoring tools
    Memory,
    /// Disk monitoring tools
    Disk,
    /// Network monitoring tools
    Network,
    /// Process monitoring tools
    Process,
    /// System-wide tools
    System,
    /// Motherboard/hardware tools
    Hardware,
    /// Audio device tools
    Audio,
    /// Bluetooth device tools
    Bluetooth,
    /// Display/monitor tools
    Display,
    /// USB device tools
    Usb,
}

impl std::fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolCategory::Gpu => write!(f, "GPU"),
            ToolCategory::Cpu => write!(f, "CPU"),
            ToolCategory::Memory => write!(f, "Memory"),
            ToolCategory::Disk => write!(f, "Disk"),
            ToolCategory::Network => write!(f, "Network"),
            ToolCategory::Process => write!(f, "Process"),
            ToolCategory::System => write!(f, "System"),
            ToolCategory::Hardware => write!(f, "Hardware"),
            ToolCategory::Audio => write!(f, "Audio"),
            ToolCategory::Bluetooth => write!(f, "Bluetooth"),
            ToolCategory::Display => write!(f, "Display"),
            ToolCategory::Usb => write!(f, "USB"),
        }
    }
}

/// Result from a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the call succeeded
    pub success: bool,
    /// Result data (if successful)
    pub data: Option<serde_json::Value>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Tool that was called
    pub tool_name: String,
}

impl ToolResult {
    /// Create a successful result
    pub fn success(tool_name: &str, data: serde_json::Value, exec_time_ms: u64) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            execution_time_ms: exec_time_ms,
            tool_name: tool_name.to_string(),
        }
    }

    /// Create a failed result
    pub fn error(tool_name: &str, error: impl ToString, exec_time_ms: u64) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error.to_string()),
            execution_time_ms: exec_time_ms,
            tool_name: tool_name.to_string(),
        }
    }
}

/// A request to call a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool name to call
    pub name: String,
    /// Parameters to pass
    pub parameters: serde_json::Value,
}

/// Replace values that name an absence with `null`, in place.
///
/// The ontology has rejected `"Unknown"`-as-a-value since the absence-word guard
/// went into `push_str_as`; the agent tool surface never had the equivalent. On
/// one machine `get_usb_devices` returned `"speed": "Unknown"` for thirty-odd
/// devices, `get_bluetooth_devices` returned `"type": "Unknown"` for nine and an
/// empty `"address"` for one — seventy values in total, each of which an agent
/// reads as a fact about the hardware. `null` says the same thing truthfully.
///
/// This is weaker than what the ontology does, and deliberately so: a `Reading`
/// carries *why* it is unavailable, and a `null` here carries nothing but the
/// absence. Getting the reason onto this surface means giving each tool the
/// `Reading` shape, which is a larger change than this one. Until then, `null`
/// beats a string that looks like a measurement and is not.
///
/// The predicate is `ontology::resolve::names_an_absence` rather than a second
/// copy of the word list, so the two surfaces cannot drift apart.
fn null_out_absence_words(v: &mut serde_json::Value) {
    use serde_json::Value;
    match v {
        Value::String(s) => {
            if s.trim().is_empty() || crate::ontology::resolve::names_an_absence(s) {
                *v = Value::Null;
            }
        }
        Value::Object(m) => {
            for val in m.values_mut() {
                null_out_absence_words(val);
            }
        }
        Value::Array(a) => {
            for val in a.iter_mut() {
                null_out_absence_words(val);
            }
        }
        _ => {}
    }
}

/// AI Data API - Main interface for AI system data access
pub struct AiDataApi {
    /// GPU collection for GPU queries
    gpus: Option<GpuCollection>,
    /// Process monitor with GPU attribution
    process_monitor: Option<ProcessMonitor>,
    /// Network monitor
    network_monitor: Option<NetworkMonitor>,
    /// Cache for expensive operations
    cache: Arc<Mutex<ApiCache>>,
    /// Last update times for rate calculations
    #[allow(dead_code)]
    last_update: Instant,
    /// Historical data context (injected from GUI)
    historical_context: Option<String>,
}

/// Cache for API data
struct ApiCache {
    /// Cached system summary
    system_summary: Option<(Instant, SystemSummary)>,
    /// Cache TTL in milliseconds
    ttl_ms: u64,
}

impl ApiCache {
    fn new() -> Self {
        Self {
            system_summary: None,
            ttl_ms: 1000, // 1 second default TTL
        }
    }

    fn is_valid(&self, cached_at: Instant) -> bool {
        cached_at.elapsed().as_millis() < self.ttl_ms as u128
    }
}

impl AiDataApi {
    /// Create a new AI Data API instance
    pub fn new() -> Result<Self> {
        // Try to initialize components, but don't fail if some aren't available
        let gpus = GpuCollection::auto_detect().ok();

        // Create process monitor (without GPU integration for now, as GpuCollection doesn't implement Clone)
        let process_monitor = ProcessMonitor::new().ok();
        let network_monitor = NetworkMonitor::new().ok();

        Ok(Self {
            gpus,
            process_monitor,
            network_monitor,
            cache: Arc::new(Mutex::new(ApiCache::new())),
            last_update: Instant::now(),
            historical_context: None,
        })
    }

    /// Create API with pre-initialized components
    pub fn with_components(
        gpus: Option<GpuCollection>,
        process_monitor: Option<ProcessMonitor>,
        network_monitor: Option<NetworkMonitor>,
    ) -> Self {
        Self {
            gpus,
            process_monitor,
            network_monitor,
            cache: Arc::new(Mutex::new(ApiCache::new())),
            last_update: Instant::now(),
            historical_context: None,
        }
    }

    /// Set historical data context from the GUI
    pub fn set_historical_context(&mut self, context: Option<String>) {
        self.historical_context = context;
    }

    /// List all available tools
    pub fn list_tools(&self) -> Vec<ToolDefinition> {
        tools::get_all_tool_definitions()
    }

    /// List tools by category
    pub fn list_tools_by_category(&self, category: ToolCategory) -> Vec<ToolDefinition> {
        self.list_tools()
            .into_iter()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Get a tool definition by name
    pub fn get_tool(&self, name: &str) -> Option<ToolDefinition> {
        self.list_tools().into_iter().find(|t| t.name == name)
    }

    /// Call a tool by name with parameters
    pub fn call_tool(&mut self, name: &str, params: serde_json::Value) -> Result<ToolResult> {
        let start = Instant::now();

        let result = match name {
            // System tools
            "get_system_summary" => self.tool_get_system_summary(params),
            "get_system_info" => self.tool_get_system_info(params),
            "get_platform_info" => self.tool_get_platform_info(params),

            // GPU tools
            "get_gpu_status" => self.tool_get_gpu_status(params),
            "get_gpu_list" => self.tool_get_gpu_list(params),
            "get_gpu_details" => self.tool_get_gpu_details(params),
            "get_gpu_processes" => self.tool_get_gpu_processes(params),
            "get_gpu_utilization" => self.tool_get_gpu_utilization(params),
            "get_gpu_memory" => self.tool_get_gpu_memory(params),
            "get_gpu_temperature" => self.tool_get_gpu_temperature(params),
            "get_gpu_power" => self.tool_get_gpu_power(params),

            // CPU tools
            "get_cpu_status" => self.tool_get_cpu_status(params),
            "get_cpu_cores" => self.tool_get_cpu_cores(params),
            "get_cpu_frequency" => self.tool_get_cpu_frequency(params),

            // Memory tools
            "get_memory_status" => self.tool_get_memory_status(params),
            "get_memory_breakdown" => self.tool_get_memory_breakdown(params),
            "get_swap_status" => self.tool_get_swap_status(params),

            // Disk tools
            "get_disk_list" => self.tool_get_disk_list(params),
            "get_disk_details" => self.tool_get_disk_details(params),
            "get_disk_io" => self.tool_get_disk_io(params),
            "get_disk_health" => self.tool_get_disk_health(params),

            // Network tools
            "get_network_interfaces" => self.tool_get_network_interfaces(params),
            "get_network_bandwidth" => self.tool_get_network_bandwidth(params),
            "get_interface_details" => self.tool_get_interface_details(params),

            // Process tools
            "get_process_list" => self.tool_get_process_list(params),
            "get_process_details" => self.tool_get_process_details(params),
            "get_top_cpu_processes" => self.tool_get_top_cpu_processes(params),
            "get_top_memory_processes" => self.tool_get_top_memory_processes(params),
            "get_top_gpu_processes" => self.tool_get_top_gpu_processes(params),
            "search_processes" => self.tool_search_processes(params),

            // Hardware tools
            "get_motherboard_sensors" => self.tool_get_motherboard_sensors(params),
            "get_system_temperatures" => self.tool_get_system_temperatures(params),
            "get_fan_speeds" => self.tool_get_fan_speeds(params),
            "get_voltage_rails" => self.tool_get_voltage_rails(params),
            "get_driver_info" => self.tool_get_driver_info(params),

            // Audio tools
            "get_audio_devices" => self.tool_get_audio_devices(params),
            "get_audio_status" => self.tool_get_audio_status(params),

            // Bluetooth tools
            "get_bluetooth_adapters" => self.tool_get_bluetooth_adapters(params),
            "get_bluetooth_devices" => self.tool_get_bluetooth_devices(params),

            // Display tools
            "get_display_list" => self.tool_get_display_list(params),
            "get_display_details" => self.tool_get_display_details(params),

            // USB tools
            "get_usb_devices" => self.tool_get_usb_devices(params),
            "get_usb_device_details" => self.tool_get_usb_device_details(params),

            // Profile inspector tools
            "list_profile_subsystems" => self.tool_list_profile_subsystems(params),
            "get_profile_settings" => self.tool_get_profile_settings(params),
            "search_profile_settings" => self.tool_search_profile_settings(params),
            "get_active_app_profiles" => self.tool_get_active_app_profiles(params),
            "get_profile_deviations" => self.tool_get_profile_deviations(params),
            "explain_profile_setting" => self.tool_explain_profile_setting(params),
            "benchmark_profile_providers" => self.tool_benchmark_profile_providers(params),
            "list_writable_profile_settings" => self.tool_list_writable_profile_settings(params),
            "apply_profile_setting" => self.tool_apply_profile_setting(params),

            _ => Err(IronError::NotImplemented(format!("Unknown tool: {}", name))),
        };

        let exec_time = start.elapsed().as_millis() as u64;

        match result {
            Ok(mut data) => {
                null_out_absence_words(&mut data);
                Ok(ToolResult::success(name, data, exec_time))
            }
            Err(e) => Ok(ToolResult::error(name, e, exec_time)),
        }
    }

    /// Execute multiple tool calls in sequence
    pub fn call_tools(&mut self, calls: Vec<ToolCall>) -> Vec<ToolResult> {
        calls
            .into_iter()
            .map(|call| {
                self.call_tool(&call.name, call.parameters)
                    .unwrap_or_else(|e| ToolResult::error(&call.name, e, 0))
            })
            .collect()
    }

    /// Get a compact system summary (cached)
    pub fn system_summary(&mut self) -> Result<SystemSummary> {
        // Check cache
        {
            let cache = self.cache.lock().unwrap();
            if let Some((cached_at, ref summary)) = cache.system_summary {
                if cache.is_valid(cached_at) {
                    return Ok(summary.clone());
                }
            }
        }

        // Generate fresh summary
        let summary = self.generate_system_summary()?;

        // Update cache
        {
            let mut cache = self.cache.lock().unwrap();
            cache.system_summary = Some((Instant::now(), summary.clone()));
        }

        Ok(summary)
    }

    /// Generate a fresh system summary
    fn generate_system_summary(&mut self) -> Result<SystemSummary> {
        let mut summary = SystemSummary::default();

        // GPU info
        if let Some(ref gpus) = self.gpus {
            if let Ok(snapshots) = gpus.snapshot_all() {
                summary.gpu_count = snapshots.len();
                for snapshot in snapshots {
                    summary.gpus.push(GpuSummary {
                        name: snapshot.static_info.name.clone(),
                        vendor: format!("{:?}", snapshot.static_info.vendor),
                        utilization_percent: snapshot.dynamic_info.utilization.map(f32::from),
                        memory_used_mb: snapshot.dynamic_info.memory.used.map(|v| v / 1024 / 1024),
                        memory_total_mb: snapshot
                            .dynamic_info
                            .memory
                            .total
                            .map(|v| v / 1024 / 1024),
                        temperature_c: snapshot.dynamic_info.thermal.temperature,
                        power_watts: snapshot.dynamic_info.power.draw.map(|p| p as f32 / 1000.0),
                    });
                }
            }
        }

        // Process info
        if let Some(ref mut proc_mon) = self.process_monitor {
            if let Ok(procs) = proc_mon.processes() {
                summary.process_count = procs.len();

                // Calculate total CPU usage
                summary.total_cpu_percent = procs.iter().map(|p| p.cpu_percent).sum();

                // Get top CPU consumers
                let mut by_cpu = procs.clone();
                by_cpu.sort_by(|a, b| {
                    b.cpu_percent
                        .partial_cmp(&a.cpu_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                summary.top_cpu_processes = by_cpu
                    .iter()
                    .take(5)
                    .map(|p| ProcessSummary {
                        pid: p.pid,
                        name: p.name.clone(),
                        cpu_percent: p.cpu_percent,
                        memory_mb: (p.memory_bytes / 1024 / 1024),
                        gpu_memory_mb: p.total_gpu_memory_bytes.map(|b| b / 1024 / 1024),
                    })
                    .collect();

                // Get top memory consumers
                let mut by_mem = procs.clone();
                by_mem.sort_by_key(|p| std::cmp::Reverse(p.memory_bytes));
                summary.top_memory_processes = by_mem
                    .iter()
                    .take(5)
                    .map(|p| ProcessSummary {
                        pid: p.pid,
                        name: p.name.clone(),
                        cpu_percent: p.cpu_percent,
                        memory_mb: (p.memory_bytes / 1024 / 1024),
                        gpu_memory_mb: p.total_gpu_memory_bytes.map(|b| b / 1024 / 1024),
                    })
                    .collect();
            }

            // GPU processes
            if let Ok(gpu_procs) = proc_mon.gpu_processes() {
                summary.gpu_process_count = gpu_procs.len();
                summary.top_gpu_processes = gpu_procs
                    .iter()
                    .take(5)
                    .map(|p| ProcessSummary {
                        pid: p.pid,
                        name: p.name.clone(),
                        cpu_percent: p.cpu_percent,
                        memory_mb: (p.memory_bytes / 1024 / 1024),
                        gpu_memory_mb: p.total_gpu_memory_bytes.map(|b| b / 1024 / 1024),
                    })
                    .collect();
            }
        }

        // Network info
        if let Some(ref mut net_mon) = self.network_monitor {
            if let Ok(interfaces) = net_mon.active_interfaces() {
                summary.active_network_interfaces = interfaces.len();
                for iface in interfaces {
                    let rate = net_mon.bandwidth_rate(&iface.name, &iface);
                    summary.network_interfaces.push(NetworkSummary {
                        name: iface.name.clone(),
                        rx_bytes_per_sec: rate.map(|(rx, _)| rx as u64),
                        tx_bytes_per_sec: rate.map(|(_, tx)| tx as u64),
                        rx_total_mb: (iface.rx_bytes / 1024 / 1024),
                        tx_total_mb: (iface.tx_bytes / 1024 / 1024),
                        is_up: iface.is_up,
                    });
                }
            }
        }

        // Memory info (platform-specific)
        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::memory;
            if let Ok(mem) = memory::read_memory_stats() {
                summary.memory = Some(MemorySummary {
                    total_mb: (mem.ram.total / 1024),
                    used_mb: (mem.ram.used / 1024),
                    free_mb: (mem.ram.free / 1024),
                    cached_mb: mem.ram.cached.map(|v| v / 1024),
                    swap_total_mb: mem.swap.total.map(|v| v / 1024),
                    swap_used_mb: mem.swap.used.map(|v| v / 1024),
                    usage_percent: if mem.ram.total > 0 {
                        (mem.ram.used as f32 / mem.ram.total as f32) * 100.0
                    } else {
                        0.0
                    },
                });
            }
        }

        // Disk info
        if let Ok(disks) = crate::disk::enumerate_disks() {
            summary.disk_count = disks.len();
            for disk in &disks {
                if let Ok(info) = disk.info() {
                    summary.disks.push(DiskSummary {
                        name: info.name.clone(),
                        model: info.model.clone(),
                        size_gb: (info.capacity / 1024 / 1024 / 1024),
                        disk_type: format!("{:?}", info.disk_type),
                        temperature_c: disk.temperature().ok().flatten(),
                    });
                }
            }
        }

        summary.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(summary)
    }

    /// Format the tool list as a prompt for AI systems
    pub fn tools_as_prompt(&self) -> String {
        let tools = self.list_tools();
        let mut prompt = String::from("# Available System Monitoring Tools\n\n");
        prompt.push_str("You have access to the following tools to query system information:\n\n");

        let mut by_category: HashMap<ToolCategory, Vec<&ToolDefinition>> = HashMap::new();
        for tool in &tools {
            by_category.entry(tool.category).or_default().push(tool);
        }

        for (category, tools) in by_category {
            prompt.push_str(&format!("## {} Tools\n\n", category));
            for tool in tools {
                prompt.push_str(&format!("### `{}`\n", tool.name));
                prompt.push_str(&format!("{}\n\n", tool.description));
                if let Some(ref example) = tool.example {
                    prompt.push_str(&format!("**Example:** `{}`\n\n", example));
                }
            }
        }

        prompt
    }

    /// Get tools in OpenAI function calling format
    pub fn tools_as_openai_functions(&self) -> Vec<serde_json::Value> {
        self.list_tools()
            .into_iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters
                    }
                })
            })
            .collect()
    }

    /// Get tools in Anthropic tool format
    pub fn tools_as_anthropic_tools(&self) -> Vec<serde_json::Value> {
        self.list_tools()
            .into_iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters
                })
            })
            .collect()
    }

    /// Analyze a user query and automatically call relevant tools
    ///
    /// Returns a context string with the results of all relevant tool calls
    /// that can be injected into the AI system prompt.
    pub fn auto_query(&mut self, user_query: &str) -> String {
        let query_lower = user_query.to_lowercase();
        let mut results: Vec<String> = Vec::new();
        let mut tools_called: Vec<&str> = Vec::new();

        // GPU-related queries
        if query_lower.contains("gpu")
            || query_lower.contains("graphics")
            || query_lower.contains("nvidia")
            || query_lower.contains("amd")
            || query_lower.contains("cuda")
            || query_lower.contains("vram")
        {
            tools_called.push("get_gpu_status");
            if let Ok(result) = self.call_tool("get_gpu_status", serde_json::json!({})) {
                if result.success {
                    if let Some(data) = result.data {
                        results.push(format!(
                            "## GPU Status\n```json\n{}\n```",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        ));
                    }
                }
            }

            // If asking about GPU processes or utilization
            if query_lower.contains("process")
                || query_lower.contains("using")
                || query_lower.contains("utiliz")
            {
                tools_called.push("get_gpu_processes");
                if let Ok(result) = self.call_tool("get_gpu_processes", serde_json::json!({})) {
                    if result.success {
                        if let Some(data) = result.data {
                            results.push(format!(
                                "## GPU Processes\n```json\n{}\n```",
                                serde_json::to_string_pretty(&data).unwrap_or_default()
                            ));
                        }
                    }
                }
            }
        }

        // CPU-related queries
        if query_lower.contains("cpu")
            || query_lower.contains("processor")
            || query_lower.contains("core")
            || query_lower.contains("thread")
        {
            tools_called.push("get_cpu_status");
            if let Ok(result) = self.call_tool("get_cpu_status", serde_json::json!({})) {
                if result.success {
                    if let Some(data) = result.data {
                        results.push(format!(
                            "## CPU Status\n```json\n{}\n```",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        ));
                    }
                }
            }
        }

        // Memory-related queries
        if query_lower.contains("memory")
            || query_lower.contains("ram")
            || query_lower.contains("swap")
        {
            tools_called.push("get_memory_status");
            if let Ok(result) = self.call_tool("get_memory_status", serde_json::json!({})) {
                if result.success {
                    if let Some(data) = result.data {
                        results.push(format!(
                            "## Memory Status\n```json\n{}\n```",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        ));
                    }
                }
            }
        }

        // Disk-related queries
        if query_lower.contains("disk")
            || query_lower.contains("storage")
            || query_lower.contains("ssd")
            || query_lower.contains("nvme")
            || query_lower.contains("hdd")
            || query_lower.contains("drive")
        {
            tools_called.push("get_disk_list");
            if let Ok(result) = self.call_tool("get_disk_list", serde_json::json!({})) {
                if result.success {
                    if let Some(data) = result.data {
                        results.push(format!(
                            "## Disk Information\n```json\n{}\n```",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        ));
                    }
                }
            }
        }

        // Network-related queries
        if query_lower.contains("network")
            || query_lower.contains("bandwidth")
            || query_lower.contains("ethernet")
            || query_lower.contains("wifi")
            || query_lower.contains("interface")
            || query_lower.contains("internet")
        {
            tools_called.push("get_network_interfaces");
            if let Ok(result) = self.call_tool("get_network_interfaces", serde_json::json!({})) {
                if result.success {
                    if let Some(data) = result.data {
                        results.push(format!(
                            "## Network Interfaces\n```json\n{}\n```",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        ));
                    }
                }
            }
        }

        // Process-related queries
        if query_lower.contains("process")
            || query_lower.contains("program")
            || query_lower.contains("running")
            || query_lower.contains("application")
        {
            // Determine which process info to get
            if query_lower.contains("cpu") || query_lower.contains("processor") {
                tools_called.push("get_top_cpu_processes");
                if let Ok(result) =
                    self.call_tool("get_top_cpu_processes", serde_json::json!({"count": 10}))
                {
                    if result.success {
                        if let Some(data) = result.data {
                            results.push(format!(
                                "## Top CPU Processes\n```json\n{}\n```",
                                serde_json::to_string_pretty(&data).unwrap_or_default()
                            ));
                        }
                    }
                }
            } else if query_lower.contains("memory") || query_lower.contains("ram") {
                tools_called.push("get_top_memory_processes");
                if let Ok(result) =
                    self.call_tool("get_top_memory_processes", serde_json::json!({"count": 10}))
                {
                    if result.success {
                        if let Some(data) = result.data {
                            results.push(format!(
                                "## Top Memory Processes\n```json\n{}\n```",
                                serde_json::to_string_pretty(&data).unwrap_or_default()
                            ));
                        }
                    }
                }
            } else if query_lower.contains("gpu") {
                tools_called.push("get_top_gpu_processes");
                if let Ok(result) =
                    self.call_tool("get_top_gpu_processes", serde_json::json!({"count": 10}))
                {
                    if result.success {
                        if let Some(data) = result.data {
                            results.push(format!(
                                "## Top GPU Processes\n```json\n{}\n```",
                                serde_json::to_string_pretty(&data).unwrap_or_default()
                            ));
                        }
                    }
                }
            } else {
                // General process list
                tools_called.push("get_process_list");
                if let Ok(result) =
                    self.call_tool("get_process_list", serde_json::json!({"count": 15}))
                {
                    if result.success {
                        if let Some(data) = result.data {
                            results.push(format!(
                                "## Running Processes\n```json\n{}\n```",
                                serde_json::to_string_pretty(&data).unwrap_or_default()
                            ));
                        }
                    }
                }
            }
        }

        // Temperature-related queries
        if query_lower.contains("temp")
            || query_lower.contains("heat")
            || query_lower.contains("hot")
            || query_lower.contains("thermal")
        {
            tools_called.push("get_system_temperatures");
            if let Ok(result) = self.call_tool("get_system_temperatures", serde_json::json!({})) {
                if result.success {
                    if let Some(data) = result.data {
                        results.push(format!(
                            "## System Temperatures\n```json\n{}\n```",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        ));
                    }
                }
            }
        }

        // Power-related queries
        if query_lower.contains("power")
            || query_lower.contains("watt")
            || query_lower.contains("energy")
            || query_lower.contains("consumption")
        {
            tools_called.push("get_gpu_power");
            if let Ok(result) = self.call_tool("get_gpu_power", serde_json::json!({})) {
                if result.success {
                    if let Some(data) = result.data {
                        results.push(format!(
                            "## GPU Power\n```json\n{}\n```",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        ));
                    }
                }
            }
        }

        // System/overview queries - provide general summary
        if query_lower.contains("system")
            || query_lower.contains("overview")
            || query_lower.contains("status")
            || query_lower.contains("summary")
            || query_lower.contains("hardware")
            || query_lower.contains("specs")
            || query_lower.contains("what do i have")
            || query_lower.contains("my computer")
        {
            tools_called.push("get_system_summary");
            if let Ok(result) = self.call_tool("get_system_summary", serde_json::json!({})) {
                if result.success {
                    if let Some(data) = result.data {
                        results.push(format!(
                            "## System Summary\n```json\n{}\n```",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        ));
                    }
                }
            }
        }

        // Fan-related queries
        if query_lower.contains("fan") {
            tools_called.push("get_fan_speeds");
            if let Ok(result) = self.call_tool("get_fan_speeds", serde_json::json!({})) {
                if result.success {
                    if let Some(data) = result.data {
                        results.push(format!(
                            "## Fan Speeds\n```json\n{}\n```",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        ));
                    }
                }
            }
        }

        // If no specific tools matched, provide a general system summary
        if results.is_empty() {
            tools_called.push("get_system_summary");
            if let Ok(result) = self.call_tool("get_system_summary", serde_json::json!({})) {
                if result.success {
                    if let Some(data) = result.data {
                        results.push(format!(
                            "## System Summary\n```json\n{}\n```",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        ));
                    }
                }
            }
        }

        // Historical data queries (injected from GUI if available)
        if (query_lower.contains("minute")
            || query_lower.contains("ago")
            || query_lower.contains("earlier")
            || query_lower.contains("before")
            || query_lower.contains("history")
            || query_lower.contains("historical")
            || query_lower.contains("was"))
            && self.historical_context.is_some()
        {
            if let Some(ref context) = self.historical_context {
                tools_called.push("get_historical_data");
                results.push(format!("## Historical Data\n{}", context));
            }
        }

        // Build the context string
        if results.is_empty() {
            String::from("No system data available.")
        } else {
            let header = format!(
                "# Real-time System Data\n\n*Tools called: {}*\n\n",
                tools_called.join(", ")
            );
            format!("{}{}", header, results.join("\n\n"))
        }
    }

    /// Analyze a user query and automatically call relevant tools
    ///
    /// Returns a tuple of (context string, tools called) with the results of all
    /// relevant tool calls that can be injected into the AI system prompt.
    pub fn auto_query_with_tools(&mut self, user_query: &str) -> (String, Vec<String>) {
        let context = self.auto_query(user_query);

        // Parse tools from the context header
        let tools = if context.contains("*Tools called:") {
            context
                .lines()
                .find(|line| line.contains("*Tools called:"))
                .and_then(|line| {
                    line.trim_start_matches("*Tools called: ")
                        .trim_end_matches("*")
                        .split(", ")
                        .map(|s| s.to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .into()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        (context, tools)
    }
}
