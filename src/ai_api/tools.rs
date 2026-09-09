//! AI API Tool Implementations
//!
//! This module contains the actual tool implementations that query system data.

use super::types::*;
use super::{AiDataApi, ToolCategory, ToolDefinition};
use crate::error::{IronError, Result};
use serde_json::json;

/// `"WIDTHxHEIGHT"`, or `null` when the display's mode was not read.
///
/// This was `format!("{}x{}", d.width, d.height)` over two `u32`s that were
/// zero when unreadable, so an agent asking about displays was told a monitor
/// was `"0x0"` — a resolution no display has, indistinguishable from a measured
/// one.
fn resolution_text(d: &crate::display::DisplayInfo) -> Option<String> {
    match (d.width, d.height) {
        (Some(w), Some(h)) => Some(format!("{w}x{h}")),
        _ => None,
    }
}

/// Get all tool definitions
pub fn get_all_tool_definitions() -> Vec<ToolDefinition> {
    let mut tools = Vec::new();

    // System tools
    tools.push(ToolDefinition {
        name: "get_system_summary".to_string(),
        description: "Get a comprehensive summary of the entire system including GPUs, CPU, memory, disk, network, and top processes. This is the best tool to start with for an overall view.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::System,
        example: Some("get_system_summary()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_system_info".to_string(),
        description: "Get system identification info: hostname, OS, kernel version, architecture, BIOS, manufacturer.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::System,
        example: Some("get_system_info()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_platform_info".to_string(),
        description:
            "Get platform-specific capabilities and what monitoring features are available."
                .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::System,
        example: Some("get_platform_info()".to_string()),
    });

    // GPU tools
    tools.push(ToolDefinition {
        name: "get_gpu_status".to_string(),
        description: "Get current status of all GPUs: utilization, memory, temperature, power. Best for quick GPU health check.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Gpu,
        example: Some("get_gpu_status()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_gpu_list".to_string(),
        description: "List all detected GPUs with basic info (name, vendor, memory size)."
            .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Gpu,
        example: Some("get_gpu_list()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_gpu_details".to_string(),
        description:
            "Get detailed information about a specific GPU by index, including clocks, processes."
                .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "gpu_index": {
                    "type": "integer",
                    "description": "GPU index (0-based). Use get_gpu_list to see available indices."
                }
            },
            "required": ["gpu_index"]
        }),
        category: ToolCategory::Gpu,
        example: Some("get_gpu_details({\"gpu_index\": 0})".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_gpu_processes".to_string(),
        description: "Get all processes currently using GPU resources with their memory usage."
            .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "gpu_index": {
                    "type": "integer",
                    "description": "Optional: specific GPU index. Omit for all GPUs."
                }
            },
            "required": []
        }),
        category: ToolCategory::Gpu,
        example: Some("get_gpu_processes()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_gpu_utilization".to_string(),
        description: "Get GPU utilization percentages.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "gpu_index": {
                    "type": "integer",
                    "description": "Optional: specific GPU index. Omit for all GPUs."
                }
            },
            "required": []
        }),
        category: ToolCategory::Gpu,
        example: Some("get_gpu_utilization()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_gpu_memory".to_string(),
        description: "Get GPU memory usage (used, free, total) for each GPU.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "gpu_index": {
                    "type": "integer",
                    "description": "Optional: specific GPU index. Omit for all GPUs."
                }
            },
            "required": []
        }),
        category: ToolCategory::Gpu,
        example: Some("get_gpu_memory()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_gpu_temperature".to_string(),
        description: "Get GPU temperatures and thermal status.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "gpu_index": {
                    "type": "integer",
                    "description": "Optional: specific GPU index. Omit for all GPUs."
                }
            },
            "required": []
        }),
        category: ToolCategory::Gpu,
        example: Some("get_gpu_temperature()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_gpu_power".to_string(),
        description: "Get GPU power consumption and limits.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "gpu_index": {
                    "type": "integer",
                    "description": "Optional: specific GPU index. Omit for all GPUs."
                }
            },
            "required": []
        }),
        category: ToolCategory::Gpu,
        example: Some("get_gpu_power()".to_string()),
    });

    // CPU tools
    tools.push(ToolDefinition {
        name: "get_cpu_status".to_string(),
        description: "Get CPU utilization and status for all cores.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Cpu,
        example: Some("get_cpu_status()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_cpu_cores".to_string(),
        description: "Get detailed per-core CPU information.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Cpu,
        example: Some("get_cpu_cores()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_cpu_frequency".to_string(),
        description: "Get CPU frequency information (current, min, max) for each core.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Cpu,
        example: Some("get_cpu_frequency()".to_string()),
    });

    // Memory tools
    tools.push(ToolDefinition {
        name: "get_memory_status".to_string(),
        description: "Get RAM and swap usage summary.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Memory,
        example: Some("get_memory_status()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_memory_breakdown".to_string(),
        description: "Get detailed memory breakdown (buffers, cached, shared, etc.).".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Memory,
        example: Some("get_memory_breakdown()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_swap_status".to_string(),
        description: "Get swap space usage.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Memory,
        example: Some("get_swap_status()".to_string()),
    });

    // Disk tools
    tools.push(ToolDefinition {
        name: "get_disk_list".to_string(),
        description: "List all disk devices in the system.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Disk,
        example: Some("get_disk_list()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_disk_details".to_string(),
        description: "Get detailed info about a specific disk.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "disk_name": {
                    "type": "string",
                    "description": "Disk device name (e.g., 'sda', 'nvme0n1')"
                }
            },
            "required": ["disk_name"]
        }),
        category: ToolCategory::Disk,
        example: Some("get_disk_details({\"disk_name\": \"nvme0n1\"})".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_disk_io".to_string(),
        description: "Get disk I/O statistics (reads, writes).".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "disk_name": {
                    "type": "string",
                    "description": "Optional: specific disk name. Omit for all disks."
                }
            },
            "required": []
        }),
        category: ToolCategory::Disk,
        example: Some("get_disk_io()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_disk_health".to_string(),
        description: "Get disk health status.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "disk_name": {
                    "type": "string",
                    "description": "Optional: specific disk name. Omit for all disks."
                }
            },
            "required": []
        }),
        category: ToolCategory::Disk,
        example: Some("get_disk_health()".to_string()),
    });

    // Network tools
    tools.push(ToolDefinition {
        name: "get_network_interfaces".to_string(),
        description: "List all network interfaces with their status.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "active_only": {
                    "type": "boolean",
                    "description": "Only show active interfaces. Default: true"
                }
            },
            "required": []
        }),
        category: ToolCategory::Network,
        example: Some("get_network_interfaces()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_network_bandwidth".to_string(),
        description: "Get current network bandwidth usage (receive/transmit rates).".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "interface_name": {
                    "type": "string",
                    "description": "Optional: specific interface name. Omit for all interfaces."
                }
            },
            "required": []
        }),
        category: ToolCategory::Network,
        example: Some("get_network_bandwidth()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_interface_details".to_string(),
        description: "Get detailed info about a specific network interface.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "interface_name": {
                    "type": "string",
                    "description": "Interface name (e.g., 'eth0', 'wlan0')"
                }
            },
            "required": ["interface_name"]
        }),
        category: ToolCategory::Network,
        example: Some("get_interface_details({\"interface_name\": \"eth0\"})".to_string()),
    });

    // Process tools
    tools.push(ToolDefinition {
        name: "get_process_list".to_string(),
        description: "Get list of running processes with CPU and memory usage.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of processes to return. Default: 50"
                },
                "sort_by": {
                    "type": "string",
                    "enum": ["cpu", "memory", "gpu_memory", "name", "pid"],
                    "description": "Sort order. Default: cpu"
                }
            },
            "required": []
        }),
        category: ToolCategory::Process,
        example: Some("get_process_list({\"limit\": 20, \"sort_by\": \"memory\"})".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_process_details".to_string(),
        description: "Get detailed information about a specific process.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "pid": {
                    "type": "integer",
                    "description": "Process ID"
                }
            },
            "required": ["pid"]
        }),
        category: ToolCategory::Process,
        example: Some("get_process_details({\"pid\": 1234})".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_top_cpu_processes".to_string(),
        description: "Get top N processes by CPU usage.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "count": {
                    "type": "integer",
                    "description": "Number of processes to return. Default: 10"
                }
            },
            "required": []
        }),
        category: ToolCategory::Process,
        example: Some("get_top_cpu_processes({\"count\": 5})".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_top_memory_processes".to_string(),
        description: "Get top N processes by memory usage.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "count": {
                    "type": "integer",
                    "description": "Number of processes to return. Default: 10"
                }
            },
            "required": []
        }),
        category: ToolCategory::Process,
        example: Some("get_top_memory_processes({\"count\": 5})".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_top_gpu_processes".to_string(),
        description: "Get top N processes by GPU memory usage.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "count": {
                    "type": "integer",
                    "description": "Number of processes to return. Default: 10"
                },
                "gpu_index": {
                    "type": "integer",
                    "description": "Optional: filter to specific GPU"
                }
            },
            "required": []
        }),
        category: ToolCategory::Process,
        example: Some("get_top_gpu_processes({\"count\": 10})".to_string()),
    });

    tools.push(ToolDefinition {
        name: "search_processes".to_string(),
        description: "Search for processes by name pattern.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (case-insensitive substring match)"
                }
            },
            "required": ["pattern"]
        }),
        category: ToolCategory::Process,
        example: Some("search_processes({\"pattern\": \"chrome\"})".to_string()),
    });

    // Hardware tools
    tools.push(ToolDefinition {
        name: "get_motherboard_sensors".to_string(),
        description: "Get all motherboard sensor readings (temperatures, voltages, fans)."
            .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Hardware,
        example: Some("get_motherboard_sensors()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_system_temperatures".to_string(),
        description: "Get all temperature sensors in the system.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Hardware,
        example: Some("get_system_temperatures()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_fan_speeds".to_string(),
        description: "Get all fan speeds and status.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Hardware,
        example: Some("get_fan_speeds()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_voltage_rails".to_string(),
        description: "Get motherboard voltage rail readings.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
        category: ToolCategory::Hardware,
        example: Some("get_voltage_rails()".to_string()),
    });

    tools.push(ToolDefinition {
        name: "get_driver_info".to_string(),
        description: "Get installed driver versions (GPU, network, storage).".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "driver_type": {
                    "type": "string",
                    "enum": ["gpu", "network", "storage", "all"],
                    "description": "Type of drivers to query. Default: all"
                }
            },
            "required": []
        }),
        category: ToolCategory::Hardware,
        example: Some("get_driver_info({\"driver_type\": \"gpu\"})".to_string()),
    });

    // Audio tools
    tools.push(ToolDefinition {
        name: "get_audio_devices".to_string(),
        description: "List all audio devices (speakers, microphones, headphones) with their status.".to_string(),
        parameters: json!({"type": "object", "properties": {"device_type": {"type": "string", "enum": ["output", "input", "all"], "description": "Filter by device type. Default: all"}}, "required": []}),
        category: ToolCategory::Audio,
        example: Some("get_audio_devices()".to_string()),
    });
    tools.push(ToolDefinition {
        name: "get_audio_status".to_string(),
        description: "Get current audio status including master volume and mute state.".to_string(),
        parameters: json!({"type": "object", "properties": {}, "required": []}),
        category: ToolCategory::Audio,
        example: Some("get_audio_status()".to_string()),
    });
    // Bluetooth tools
    tools.push(ToolDefinition {
        name: "get_bluetooth_adapters".to_string(),
        description: "List all Bluetooth adapters in the system.".to_string(),
        parameters: json!({"type": "object", "properties": {}, "required": []}),
        category: ToolCategory::Bluetooth,
        example: Some("get_bluetooth_adapters()".to_string()),
    });
    tools.push(ToolDefinition {
        name: "get_bluetooth_devices".to_string(),
        description: "List all paired/discovered Bluetooth devices with connection status.".to_string(),
        parameters: json!({"type": "object", "properties": {"connected_only": {"type": "boolean", "description": "Only show connected devices. Default: false"}}, "required": []}),
        category: ToolCategory::Bluetooth,
        example: Some("get_bluetooth_devices()".to_string()),
    });
    // Display tools
    tools.push(ToolDefinition {
        name: "get_display_list".to_string(),
        description: "List all connected displays/monitors with resolution and refresh rate."
            .to_string(),
        parameters: json!({"type": "object", "properties": {}, "required": []}),
        category: ToolCategory::Display,
        example: Some("get_display_list()".to_string()),
    });
    tools.push(ToolDefinition {
        name: "get_display_details".to_string(),
        description: "Get detailed information about a specific display.".to_string(),
        parameters: json!({"type": "object", "properties": {"display_id": {"type": "string", "description": "Display identifier"}}, "required": ["display_id"]}),
        category: ToolCategory::Display,
        example: Some("get_display_details()".to_string()),
    });
    // USB tools
    tools.push(ToolDefinition {
        name: "get_usb_devices".to_string(),
        description: "List all connected USB devices with vendor/product info.".to_string(),
        parameters: json!({"type": "object", "properties": {"class": {"type": "string", "enum": ["audio", "hid", "storage", "hub", "video", "all"], "description": "Filter by USB device class. Default: all"}}, "required": []}),
        category: ToolCategory::Usb,
        example: Some("get_usb_devices()".to_string()),
    });
    tools.push(ToolDefinition {
        name: "get_usb_device_details".to_string(),
        description: concat!(
            "Get detailed information about a specific USB device, addressed ",
            "by the `address` field from `get_usb_devices`."
        )
        .to_string(),
        // Was `bus` and `address` as integers. Bus and port do not identify a
        // device -- on Windows every device reports bus 0 port 0 -- so the
        // catalogue was telling agents to ask a question that could only ever
        // return the first device.
        parameters: json!({"type": "object", "properties": {"address": {"type": "string", "description": "Device address, as returned in the `address` field of get_usb_devices"}}, "required": ["address"]}),
        category: ToolCategory::Usb,
        example: Some(
            "get_usb_device_details(address=\"usb_vid_046d_pid_c548_mi_01_9_24c94812_0_0001\")"
                .to_string(),
        ),
    });

    // `get_historical_data` and `compare_metrics` were advertised here with full
    // descriptions and worked examples, and neither had an implementation --- no
    // `tool_*` function and no arm in `call_tool`, so an agent that read the
    // catalogue and called one got "Unknown tool", which reads as the agent's
    // own mistake rather than as ironmon's. The only other mention of the name was
    // a label pushed into a text blob, which is not the tool.
    //
    // They are removed rather than stubbed. Answering "what was the GPU
    // temperature five minutes ago" needs a time-series store, and the only
    // buffer in the crate (`backend::HistoryBuffer`) carries no timestamps and is
    // not reachable from here; `historical_context` is an `Option<String>` a
    // caller sets, not history. Advertising a capability that does not exist is
    // the capability-level version of reporting "Unknown" as a value. What it
    // would take to build properly is in HANDOFF.

    // Profile inspector tools (NVPI / XTU / Ryzen Master / nvme-cli style)
    tools.push(ToolDefinition {
        name: "list_profile_subsystems".to_string(),
        description: "List hardware subsystems with readable driver/firmware profile data (gpu, cpu, nvme, display, memory). Use this to discover what tunable settings can be inspected on this machine.".to_string(),
        parameters: json!({"type": "object", "properties": {}, "required": []}),
        category: ToolCategory::Hardware,
        example: Some("list_profile_subsystems()".to_string()),
    });
    tools.push(ToolDefinition {
        name: "get_profile_settings".to_string(),
        description: "Get all profile groups and settings for one subsystem. Each group is a device (GPU/DIMM/NVMe/display) with its current driver settings. Read-only; mirrors the data shown by NVIDIA Profile Inspector, Intel XTU, AMD Ryzen Master, and nvme-cli.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "subsystem": {
                    "type": "string",
                    "enum": ["gpu", "cpu", "nvme", "display", "memory"],
                    "description": "Subsystem to inspect"
                }
            },
            "required": ["subsystem"]
        }),
        category: ToolCategory::Hardware,
        example: Some(r#"get_profile_settings({"subsystem": "gpu"})"#.to_string()),
    });
    tools.push(ToolDefinition {
        name: "get_profile_deviations".to_string(),
        description: "List every hardware setting whose current value differs from its declared default — the 'what's been changed from stock' report. Sorted with dangerous risk first. Use this to audit a system after a BIOS update, driver reinstall, or to verify a clean baseline.".to_string(),
        parameters: json!({"type": "object", "properties": {}, "required": []}),
        category: ToolCategory::Hardware,
        example: Some("get_profile_deviations()".to_string()),
    });
    tools.push(ToolDefinition {
        name: "benchmark_profile_providers".to_string(),
        description: "Benchmark each profile subsystem provider: wall-clock ms, group/setting counts, throughput. Use this to decide whether to call get_profile_settings(subsystem) selectively when a full snapshot is too slow.".to_string(),
        parameters: json!({"type": "object", "properties": {}, "required": []}),
        category: ToolCategory::Hardware,
        example: Some("benchmark_profile_providers()".to_string()),
    });
    tools.push(ToolDefinition {
        name: "list_writable_profile_settings".to_string(),
        description: "Return the list of profile setting ids that this build can actually write to. Use this before suggesting any apply operation — only ids in this list have a real backing ApplyHandler.".to_string(),
        parameters: json!({"type": "object", "properties": {}, "required": []}),
        category: ToolCategory::Hardware,
        example: Some("list_writable_profile_settings()".to_string()),
    });
    tools.push(ToolDefinition {
        name: "apply_profile_setting".to_string(),
        description: "Write a profile setting (e.g. enable NVIDIA persistence mode). REQUIRES BOTH (a) confirm=true and (b) the environment variable IRONMON_ALLOW_AGENT_WRITES=1 to actually attempt the write. Without those, the call returns a planned-but-not-applied outcome. Every attempt is JSON-logged to the audit log regardless.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "setting_id": {"type": "string", "description": "Exact setting id; must be in list_writable_profile_settings()."},
                "value": {"description": "New value. Booleans use true/false; ints/floats are numeric; otherwise a string."},
                "confirm": {"type": "boolean", "description": "Must be true to attempt the write."}
            },
            "required": ["setting_id", "value"]
        }),
        category: ToolCategory::Hardware,
        example: Some(r#"apply_profile_setting({"setting_id": "scaling_governor", "value": "performance", "confirm": true})"#.to_string()),
    });
    tools.push(ToolDefinition {
        name: "explain_profile_setting".to_string(),
        description: "Return full metadata for a single profile setting by exact id: description, default, range, choices, risk, source, and related settings in the same group. Use this when a user asks 'what does setting X mean'.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "setting_id": {
                    "type": "string",
                    "description": "Exact setting id (e.g. 'power_limit_mw', 'pl1_w', 'feat.write_cache.enabled')."
                }
            },
            "required": ["setting_id"]
        }),
        category: ToolCategory::Hardware,
        example: Some(r#"explain_profile_setting({"setting_id": "pl1_w"})"#.to_string()),
    });
    tools.push(ToolDefinition {
        name: "get_active_app_profiles".to_string(),
        description: "List running processes alongside whether each has a known per-application driver profile (NVIDIA DRS database). Use this to identify apps that could benefit from a tuned profile (DLSS, V-Sync, framerate cap) or to confirm an existing profile applies to the current run.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "matched_only": {
                    "type": "boolean",
                    "description": "If true, return only processes with a known driver profile."
                }
            },
            "required": []
        }),
        category: ToolCategory::Hardware,
        example: Some(r#"get_active_app_profiles({"matched_only": true})"#.to_string()),
    });
    tools.push(ToolDefinition {
        name: "search_profile_settings".to_string(),
        description: "Search all hardware profile settings by case-insensitive substring (matches setting id, display name, description, or value). Useful for questions like 'is XMP enabled', 'what's my CPU power limit', or 'find DLSS setting'.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search term"}
            },
            "required": ["query"]
        }),
        category: ToolCategory::Hardware,
        example: Some(r#"search_profile_settings({"query": "xmp"})"#.to_string()),
    });

    tools
}

// Tool implementations for AiDataApi
impl AiDataApi {
    // ============== System Tools ==============

    pub(crate) fn tool_get_system_summary(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let summary = self.system_summary()?;
        Ok(serde_json::to_value(summary)?)
    }

    pub(crate) fn tool_get_system_info(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let info = crate::motherboard::get_system_info()
            .map_err(|e| IronError::NotImplemented(e.to_string()))?;

        let uptime = crate::stats::uptime().ok().map(|d| d.as_secs());

        let details = SystemInfoDetails {
            hostname: info.hostname.clone(),
            os_name: info.os_name.clone(),
            os_version: info.os_version.clone(),
            kernel_version: info.kernel_version.clone(),
            architecture: info.architecture.clone(),
            bios_vendor: info.bios.vendor.clone(),
            bios_version: info.bios.version.clone(),
            manufacturer: info.manufacturer.clone(),
            model: info.product_name.clone(),
            // `read_uptime` has had a platform-specific implementation on
            // Linux, Windows and macOS the whole time; what was missing was a
            // way to call it without building a `IronMonitor`. Boot time follows from
            // uptime, so both are present or both absent -- deriving one from a
            // clock that failed would be worse than reporting neither.
            uptime_seconds: uptime,
            boot_time: uptime.and_then(|u| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|now| now.as_secs().saturating_sub(u))
            }),
        };

        Ok(serde_json::to_value(details)?)
    }

    pub(crate) fn tool_get_platform_info(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut capabilities = serde_json::Map::new();

        capabilities.insert("os".to_string(), json!(std::env::consts::OS));
        capabilities.insert("arch".to_string(), json!(std::env::consts::ARCH));
        capabilities.insert("family".to_string(), json!(std::env::consts::FAMILY));

        // Check GPU support
        capabilities.insert("gpu_nvidia".to_string(), json!(cfg!(feature = "nvidia")));
        capabilities.insert("gpu_amd".to_string(), json!(cfg!(feature = "amd")));
        capabilities.insert("gpu_intel".to_string(), json!(cfg!(feature = "intel")));
        capabilities.insert("gpu_apple".to_string(), json!(cfg!(feature = "apple")));

        // Check what's actually available
        capabilities.insert("gpus_detected".to_string(), json!(self.gpus.is_some()));
        capabilities.insert(
            "gpu_count".to_string(),
            json!(self.gpus.as_ref().map(|g| g.device_count()).unwrap_or(0)),
        );
        capabilities.insert(
            "process_monitor".to_string(),
            json!(self.process_monitor.is_some()),
        );
        capabilities.insert(
            "network_monitor".to_string(),
            json!(self.network_monitor.is_some()),
        );

        Ok(json!(capabilities))
    }

    // ============== GPU Tools ==============

    pub(crate) fn tool_get_gpu_status(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let gpus = self
            .gpus
            .as_ref()
            .ok_or_else(|| IronError::NotImplemented("No GPUs detected".to_string()))?;

        let snapshots = gpus
            .snapshot_all()
            .map_err(|e| IronError::GpuError(e.to_string()))?;

        let status: Vec<_> = snapshots
            .iter()
            .enumerate()
            .map(|(idx, s)| {
                json!({
                    "index": idx,
                    "name": s.static_info.name,
                    "vendor": format!("{:?}", s.static_info.vendor),
                    "utilization_percent": s.dynamic_info.utilization,
                    // `null` rather than `0` for a device that reported no
                    // memory figures. `usage_percent` is the derived field on
                    // `GpuMemory`, which is absent whenever either input is --
                    // the arithmetic used to run against a total that could be
                    // zero and answer `0.0`.
                    "memory": {
                        "used_mb": s.dynamic_info.memory.used.map(|v| v / 1024 / 1024),
                        "total_mb": s.dynamic_info.memory.total.map(|v| v / 1024 / 1024),
                        "free_mb": s.dynamic_info.memory.free.map(|v| v / 1024 / 1024),
                        "usage_percent": s.dynamic_info.memory.utilization,
                    },
                    "temperature_c": s.dynamic_info.thermal.temperature,
                    "power_watts": s.dynamic_info.power.draw.map(|p| p as f32 / 1000.0),
                    "power_limit_watts": s.dynamic_info.power.limit.map(|p| p as f32 / 1000.0),
                })
            })
            .collect();

        Ok(json!({
            "gpu_count": status.len(),
            "gpus": status
        }))
    }

    pub(crate) fn tool_get_gpu_list(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let gpus = self
            .gpus
            .as_ref()
            .ok_or_else(|| IronError::NotImplemented("No GPUs detected".to_string()))?;

        let snapshots = gpus
            .snapshot_all()
            .map_err(|e| IronError::GpuError(e.to_string()))?;

        let list: Vec<_> = snapshots
            .iter()
            .enumerate()
            .map(|(idx, s)| {
                json!({
                    "index": idx,
                    "name": s.static_info.name,
                    "vendor": format!("{:?}", s.static_info.vendor),
                    "total_memory_mb": s.dynamic_info.memory.total.map(|v| v / 1024 / 1024),
                })
            })
            .collect();

        Ok(json!(list))
    }

    pub(crate) fn tool_get_gpu_details(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let gpu_index = params
            .get("gpu_index")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| IronError::InvalidArgument("gpu_index is required".to_string()))?
            as usize;

        let gpus = self
            .gpus
            .as_ref()
            .ok_or_else(|| IronError::NotImplemented("No GPUs detected".to_string()))?;

        let snapshots = gpus
            .snapshot_all()
            .map_err(|e| IronError::GpuError(e.to_string()))?;

        let info = snapshots.get(gpu_index).ok_or_else(|| {
            IronError::InvalidArgument(format!("GPU index {} not found", gpu_index))
        })?;

        Ok(json!({
            "index": gpu_index,
            "name": info.static_info.name,
            "vendor": format!("{:?}", info.static_info.vendor),
            "pci_bus_id": info.static_info.pci_bus_id,
            "uuid": info.static_info.uuid,
            "driver_version": info.static_info.driver_version,
            "utilization": {
                "gpu_percent": info.dynamic_info.utilization,
            },
            "memory": {
                "total_bytes": info.dynamic_info.memory.total,
                "used_bytes": info.dynamic_info.memory.used,
                "free_bytes": info.dynamic_info.memory.free,
                "total_mb": info.dynamic_info.memory.total.map(|v| v / 1024 / 1024),
                "used_mb": info.dynamic_info.memory.used.map(|v| v / 1024 / 1024),
                "usage_percent": info.dynamic_info.memory.utilization,
            },
            "thermal": {
                "temperature_c": info.dynamic_info.thermal.temperature,
                "max_temperature_c": info.dynamic_info.thermal.max_temperature,
                "critical_temperature_c": info.dynamic_info.thermal.critical_temperature,
                "fan_speed_percent": info.dynamic_info.thermal.fan_speed,
                "fan_rpm": info.dynamic_info.thermal.fan_rpm,
            },
            "power": {
                "current_watts": info.dynamic_info.power.draw.map(|p| p as f32 / 1000.0),
                "limit_watts": info.dynamic_info.power.limit.map(|p| p as f32 / 1000.0),
                "default_limit_watts": info.dynamic_info.power.default_limit.map(|p| p as f32 / 1000.0),
            },
            "clocks": {
                "graphics_mhz": info.dynamic_info.clocks.graphics,
                "memory_mhz": info.dynamic_info.clocks.memory,
                "sm_mhz": info.dynamic_info.clocks.sm,
                "max_graphics_mhz": info.dynamic_info.clocks.graphics_max,
                "max_memory_mhz": info.dynamic_info.clocks.memory_max,
            },
            "processes": info.dynamic_info.processes.iter().map(|p| {
                json!({
                    "pid": p.pid,
                    "name": p.name,
                    "memory_bytes": p.memory_usage,
                    "gpu_usage_percent": p.gpu_usage,
                })
            }).collect::<Vec<_>>(),
        }))
    }

    pub(crate) fn tool_get_gpu_processes(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let gpu_index = params.get("gpu_index").and_then(|v| v.as_u64());

        let proc_mon = self.process_monitor.as_mut().ok_or_else(|| {
            IronError::NotImplemented("Process monitor not available".to_string())
        })?;

        let gpu_procs = proc_mon
            .gpu_processes()
            .map_err(|e| IronError::ProcessError(e.to_string()))?;

        let filtered: Vec<_> = gpu_procs
            .iter()
            .filter(|p| {
                gpu_index
                    .map(|idx| p.gpu_indices.contains(&(idx as usize)))
                    .unwrap_or(true)
            })
            .map(|p| {
                json!({
                    "pid": p.pid,
                    "name": p.name,
                    "gpu_indices": p.gpu_indices,
                    "gpu_memory_bytes": p.total_gpu_memory_bytes,
                    "gpu_memory_mb": p.total_gpu_memory_bytes.map(|b| b / 1024 / 1024),
                    "cpu_percent": p.cpu_percent,
                    "system_memory_mb": p.memory_bytes / 1024 / 1024,
                })
            })
            .collect();

        Ok(json!({
            "count": filtered.len(),
            "processes": filtered
        }))
    }

    /// Reject an explicit `gpu_index` that names no device.
    ///
    /// The tools taking an optional index filtered the snapshot list by it and
    /// returned whatever survived, so asking about a GPU that does not exist produced
    /// `{"success": true, "data": []}`. An agent renders that as "nothing to report"
    /// — silence that reads as reassurance — when the truthful answer is that there
    /// is no such device. `get_gpu_details`, which takes a required index, already
    /// errors with this wording; these now match it.
    fn check_gpu_index(gpu_index: Option<u64>, count: usize) -> Result<()> {
        match gpu_index {
            Some(i) if i as usize >= count => Err(IronError::InvalidArgument(format!(
                "GPU index {} not found",
                i
            ))),
            _ => Ok(()),
        }
    }

    pub(crate) fn tool_get_gpu_utilization(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let gpu_index = params.get("gpu_index").and_then(|v| v.as_u64());

        let gpus = self
            .gpus
            .as_ref()
            .ok_or_else(|| IronError::NotImplemented("No GPUs detected".to_string()))?;

        let snapshots = gpus
            .snapshot_all()
            .map_err(|e| IronError::GpuError(e.to_string()))?;
        Self::check_gpu_index(gpu_index, snapshots.len())?;

        let util: Vec<_> = snapshots
            .iter()
            .enumerate()
            .filter(|(idx, _)| gpu_index.map(|i| *idx == i as usize).unwrap_or(true))
            .map(|(idx, s)| {
                json!({
                    "index": idx,
                    "name": s.static_info.name,
                    "gpu_percent": s.dynamic_info.utilization,
                })
            })
            .collect();

        Ok(json!(util))
    }

    pub(crate) fn tool_get_gpu_memory(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let gpu_index = params.get("gpu_index").and_then(|v| v.as_u64());

        let gpus = self
            .gpus
            .as_ref()
            .ok_or_else(|| IronError::NotImplemented("No GPUs detected".to_string()))?;

        let snapshots = gpus
            .snapshot_all()
            .map_err(|e| IronError::GpuError(e.to_string()))?;
        Self::check_gpu_index(gpu_index, snapshots.len())?;

        let mem: Vec<_> = snapshots
            .iter()
            .enumerate()
            .filter(|(idx, _)| gpu_index.map(|i| *idx == i as usize).unwrap_or(true))
            .map(|(idx, s)| {
                json!({
                    "index": idx,
                    "name": s.static_info.name,
                    "total_bytes": s.dynamic_info.memory.total,
                    "used_bytes": s.dynamic_info.memory.used,
                    "free_bytes": s.dynamic_info.memory.free,
                    "total_mb": s.dynamic_info.memory.total.map(|v| v / 1024 / 1024),
                    "used_mb": s.dynamic_info.memory.used.map(|v| v / 1024 / 1024),
                    "free_mb": s.dynamic_info.memory.free.map(|v| v / 1024 / 1024),
                    "usage_percent": s.dynamic_info.memory.utilization
                })
            })
            .collect();

        Ok(json!(mem))
    }

    pub(crate) fn tool_get_gpu_temperature(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let gpu_index = params.get("gpu_index").and_then(|v| v.as_u64());

        let gpus = self
            .gpus
            .as_ref()
            .ok_or_else(|| IronError::NotImplemented("No GPUs detected".to_string()))?;

        let snapshots = gpus
            .snapshot_all()
            .map_err(|e| IronError::GpuError(e.to_string()))?;
        Self::check_gpu_index(gpu_index, snapshots.len())?;

        let temps: Vec<_> = snapshots
            .iter()
            .enumerate()
            .filter(|(idx, _)| gpu_index.map(|i| *idx == i as usize).unwrap_or(true))
            .map(|(idx, s)| {
                json!({
                    "index": idx,
                    "name": s.static_info.name,
                    "temperature_c": s.dynamic_info.thermal.temperature,
                    "max_temperature_c": s.dynamic_info.thermal.max_temperature,
                    "critical_temperature_c": s.dynamic_info.thermal.critical_temperature,
                    "fan_speed_percent": s.dynamic_info.thermal.fan_speed,
                    "fan_rpm": s.dynamic_info.thermal.fan_rpm,
                })
            })
            .collect();

        Ok(json!(temps))
    }

    pub(crate) fn tool_get_gpu_power(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let gpu_index = params.get("gpu_index").and_then(|v| v.as_u64());

        let gpus = self
            .gpus
            .as_ref()
            .ok_or_else(|| IronError::NotImplemented("No GPUs detected".to_string()))?;

        let snapshots = gpus
            .snapshot_all()
            .map_err(|e| IronError::GpuError(e.to_string()))?;
        Self::check_gpu_index(gpu_index, snapshots.len())?;

        let power: Vec<_> = snapshots
            .iter()
            .enumerate()
            .filter(|(idx, _)| gpu_index.map(|i| *idx == i as usize).unwrap_or(true))
            .map(|(idx, s)| {
                json!({
                    "index": idx,
                    "name": s.static_info.name,
                    "current_watts": s.dynamic_info.power.draw.map(|p| p as f32 / 1000.0),
                    "limit_watts": s.dynamic_info.power.limit.map(|p| p as f32 / 1000.0),
                    "default_limit_watts": s.dynamic_info.power.default_limit.map(|p| p as f32 / 1000.0),
                })
            })
            .collect();

        Ok(json!(power))
    }

    // ============== CPU Tools ==============

    pub(crate) fn tool_get_cpu_status(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::cpu;
            let stats = cpu::read_cpu_stats().map_err(|e| IronError::CpuError(e.to_string()))?;

            Ok(json!({
                "core_count": stats.cores.len(),
                "total": {
                    "user_percent": stats.total.user,
                    "system_percent": stats.total.system,
                    "nice_percent": stats.total.nice,
                    "idle_percent": stats.total.idle,
                    "usage_percent": 100.0 - stats.total.idle,
                },
                "model": stats.cores.first().map(|c| &c.model).cloned(),
            }))
        }

        #[cfg(target_os = "windows")]
        {
            use crate::platform::windows;
            let stats =
                windows::read_cpu_stats().map_err(|e| IronError::CpuError(e.to_string()))?;

            Ok(json!({
                "core_count": stats.cores.len(),
                "total": {
                    "user_percent": stats.total.user,
                    "system_percent": stats.total.system,
                    "nice_percent": stats.total.nice,
                    "idle_percent": stats.total.idle,
                    "usage_percent": 100.0 - stats.total.idle,
                },
                "model": stats.cores.first().map(|c| &c.model).cloned(),
            }))
        }

        // macOS. This arm used to shell out to `sysctl vm.loadavg` and publish
        // `load / ncpu * 100` as `usage_percent`. Load average is not
        // utilization: it counts runnable and uninterruptible tasks, so a box
        // blocked on disk reads high while its cores idle, and a saturated one
        // with no queue reads low. It also fell back to 0.0 when the sysctl
        // failed, reporting an idle machine, and named an unreadable CPU
        // literally "CPU".
        //
        // `platform::macos::read_cpu_stats` has existed since 5.2.0 and was
        // wired only into the ontology. This is the fourth consumer found
        // fabricating alongside a real reader it never adopted. Adding a
        // reader is not the same as adopting it.
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let stats = crate::platform::macos::read_cpu_stats()
                .map_err(|e| IronError::CpuError(e.to_string()))?;

            Ok(json!({
                "core_count": stats.cores.len(),
                "total": {
                    "user_percent": stats.total.user,
                    "system_percent": stats.total.system,
                    "nice_percent": stats.total.nice,
                    "idle_percent": stats.total.idle,
                    "usage_percent": 100.0 - stats.total.idle,
                },
                "model": stats.cores.first().map(|c| &c.model).cloned(),
            }))
        }
    }

    pub(crate) fn tool_get_cpu_cores(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::cpu;
            let stats = cpu::read_cpu_stats().map_err(|e| IronError::CpuError(e.to_string()))?;

            let cores: Vec<_> = stats
                .cores
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "online": c.online,
                        "governor": c.governor,
                        "model": c.model,
                        "frequency_mhz": c.frequency.as_ref().map(|f| f.current),
                        "user_percent": c.user,
                        "system_percent": c.system,
                        "idle_percent": c.idle,
                    })
                })
                .collect();

            Ok(json!(cores))
        }

        #[cfg(target_os = "windows")]
        {
            use crate::platform::windows;
            let stats =
                windows::read_cpu_stats().map_err(|e| IronError::CpuError(e.to_string()))?;

            let cores: Vec<_> = stats
                .cores
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "online": c.online,
                        "governor": c.governor,
                        "model": c.model,
                        "frequency_mhz": c.frequency.as_ref().map(|f| f.current),
                        "user_percent": c.user,
                        "system_percent": c.system,
                        "idle_percent": c.idle,
                    })
                })
                .collect();

            Ok(json!(cores))
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            use std::process::Command;
            let sysctl = |key: &str| -> Option<String> {
                Command::new("sysctl")
                    .args(["-n", key])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            };

            // `null`, not the literal string "CPU". A placeholder model name
            // handed to an agent is a name it will repeat back.
            let model = sysctl("machdep.cpu.brand_string");

            // `hw.cpufrequency` does not exist on Apple Silicon -- Apple removed
            // it with the ARM transition -- so this parsed to `None` and
            // `unwrap_or(0)` published `"frequency_mhz": 0` for every core of
            // every M-series Mac. Zero hertz is not a clock, and this is the
            // agent surface, where a fabricated number becomes a premise.
            //
            // It is also one figure repeated across every core, which is the
            // defect `24a7314` fixed on the macOS silicon reader and `f5a54ee`
            // fixed on the Windows one. The value is per-machine, so it is
            // reported once per core only because the shape demands it; when it
            // is absent every core says so rather than saying zero.
            let freq_mhz = sysctl("hw.cpufrequency")
                .and_then(|s| s.parse::<u64>().ok())
                .map(|hz| hz / 1_000_000);

            let ncpu = num_cpus::get();
            let cores: Vec<_> = (0..ncpu)
                .map(|i| {
                    json!({
                        "id": i, "online": true, "model": model,
                        "frequency_mhz": freq_mhz,
                    })
                })
                .collect();
            Ok(json!(cores))
        }
    }

    pub(crate) fn tool_get_cpu_frequency(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::cpu;
            let stats = cpu::read_cpu_stats().map_err(|e| IronError::CpuError(e.to_string()))?;

            let freqs: Vec<_> = stats
                .cores
                .iter()
                .filter_map(|c| {
                    c.frequency.as_ref().map(|f| {
                        json!({
                            "core_id": c.id,
                            "current_mhz": f.current,
                            "min_mhz": f.min,
                            "max_mhz": f.max,
                        })
                    })
                })
                .collect();

            Ok(json!(freqs))
        }

        #[cfg(target_os = "windows")]
        {
            use crate::platform::windows;
            let stats =
                windows::read_cpu_stats().map_err(|e| IronError::CpuError(e.to_string()))?;

            let freqs: Vec<_> = stats
                .cores
                .iter()
                .filter_map(|c| {
                    c.frequency.as_ref().map(|f| {
                        json!({
                            "core_id": c.id,
                            "current_mhz": f.current,
                            "min_mhz": f.min,
                            "max_mhz": f.max,
                        })
                    })
                })
                .collect();

            Ok(json!(freqs))
        }

        // macOS, through the same reader the other two arms use.
        //
        // This shelled out to `hw.cpufrequency`, an Intel-era sysctl that does
        // not exist on Apple Silicon at all, and fell back to `0` -- so every
        // core of every M-series Mac was published to an agent as
        // `current_mhz: 0, min_mhz: 0, max_mhz: 0`. `min` fell back to
        // `freq_hz / 2` and `max` to `freq_hz`, so the zero propagated into all
        // three.
        //
        // `platform::macos::read_cpu_stats` sets `frequency: None` and says why
        // in a comment: "No unelevated source of live per-core frequency on
        // macOS. `None` says that; a nominal figure would repeat the mistake
        // Windows makes with `CurrentMhz`." The tool surface was making exactly
        // that mistake, one function away from the reader that had decided not
        // to. **A reader is not adopted until its callers use it**, and this is
        // the fifth consumer in this crate found fabricating beside one.
        //
        // The `filter_map` is the same as the Linux and Windows arms above, so
        // a platform reporting no per-core frequency yields an empty array --
        // the honest answer, and one an agent can act on.
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let stats = crate::platform::macos::read_cpu_stats()
                .map_err(|e| IronError::CpuError(e.to_string()))?;

            let freqs: Vec<_> = stats
                .cores
                .iter()
                .filter_map(|c| {
                    c.frequency.as_ref().map(|f| {
                        json!({
                            "core_id": c.id,
                            "current_mhz": f.current,
                            "min_mhz": f.min,
                            "max_mhz": f.max,
                        })
                    })
                })
                .collect();

            Ok(json!(freqs))
        }
    }

    // ============== Memory Tools ==============

    pub(crate) fn tool_get_memory_status(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::memory;
            let stats =
                memory::read_memory_stats().map_err(|e| IronError::MemoryError(e.to_string()))?;

            Ok(json!({
                "ram": {
                    "total_mb": stats.ram.total / 1024,
                    "used_mb": stats.ram.used / 1024,
                    "free_mb": stats.ram.free / 1024,
                    "usage_percent": if stats.ram.total > 0 {
                        (stats.ram.used as f64 / stats.ram.total as f64) * 100.0
                    } else { 0.0 },
                },
                // `null`, not `0`. `SwapInfo` carries `Option` since 15a60ab
                // precisely so an unread pagefile is distinguishable from an
                // empty one, and `used_or_zero()` throws that away at the point
                // where an agent reads it. A zero here is the sentence "nothing
                // is paged out", which is a measurement nobody took.
                "swap": {
                    "total_mb": stats.swap.total.map(|v| v / 1024),
                    "used_mb": stats.swap.used.map(|v| v / 1024),
                    "usage_percent": match (stats.swap.total, stats.swap.used) {
                        (Some(t), Some(u)) if t > 0 => json!(u as f64 / t as f64 * 100.0),
                        _ => serde_json::Value::Null,
                    },
                }
            }))
        }

        #[cfg(target_os = "windows")]
        {
            use crate::platform::windows;
            let stats =
                windows::read_memory_stats().map_err(|e| IronError::MemoryError(e.to_string()))?;

            Ok(json!({
                "ram": {
                    "total_mb": stats.ram.total / 1024,
                    "used_mb": stats.ram.used / 1024,
                    "free_mb": stats.ram.free / 1024,
                    "usage_percent": if stats.ram.total > 0 {
                        (stats.ram.used as f64 / stats.ram.total as f64) * 100.0
                    } else { 0.0 },
                },
                // `null`, not `0`. `SwapInfo` carries `Option` since 15a60ab
                // precisely so an unread pagefile is distinguishable from an
                // empty one, and `used_or_zero()` throws that away at the point
                // where an agent reads it. A zero here is the sentence "nothing
                // is paged out", which is a measurement nobody took.
                "swap": {
                    "total_mb": stats.swap.total.map(|v| v / 1024),
                    "used_mb": stats.swap.used.map(|v| v / 1024),
                    "usage_percent": match (stats.swap.total, stats.swap.used) {
                        (Some(t), Some(u)) if t > 0 => json!(u as f64 / t as f64 * 100.0),
                        _ => serde_json::Value::Null,
                    },
                }
            }))
        }

        // Was hand-rolled from `sysctl` and `vm_stat`, with four defects an
        // agent had no way to see: a hardcoded 16384-byte page size ("Apple
        // Silicon default") that reports memory 4x too large on an Intel Mac; a
        // failed `vm_stat` falling to zero pages, which publishes 100% memory
        // used; a failed `hw.memsize` giving a total of 0; and an unreadable
        // `vm.swapusage` reported as no swap -- the exact claim the `SwapInfo`
        // `Option` refactor was done to stop making.
        //
        // `platform::macos::read_memory_stats` has existed since 5.2.0. This is
        // the third consumer found still fabricating around it, after the
        // ontology resolver adopted it and `MonitoringBackend` did not.
        #[cfg(target_os = "macos")]
        {
            let stats = crate::platform::macos::read_memory_stats()
                .map_err(|e| IronError::MemoryError(e.to_string()))?;

            Ok(json!({
                "ram": {
                    "total_mb": stats.ram.total / 1024,
                    "used_mb": stats.ram.used / 1024,
                    "free_mb": stats.ram.free / 1024,
                    "usage_percent": if stats.ram.total > 0 {
                        (stats.ram.used as f64 / stats.ram.total as f64) * 100.0
                    } else { 0.0 },
                },
                // `null` rather than 0 where the platform reported no swap
                // figure. An agent reading 0 cannot tell "no swap configured"
                // from "swap was not readable"; `null` says which.
                "swap": {
                    "total_mb": stats.swap.total.map(|t| t / 1024),
                    "used_mb": stats.swap.used.map(|u| u / 1024),
                    "usage_percent": stats.swap_usage_percent(),
                }
            }))
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            Err(IronError::UnsupportedPlatform(
                "memory is read through per-platform APIs and this platform has none implemented"
                    .into(),
            ))
        }
    }

    pub(crate) fn tool_get_memory_breakdown(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::memory;
            let stats =
                memory::read_memory_stats().map_err(|e| IronError::MemoryError(e.to_string()))?;

            Ok(json!({
                "total_kb": stats.ram.total,
                "used_kb": stats.ram.used,
                "free_kb": stats.ram.free,
                "buffers_kb": stats.ram.buffers,
                "cached_kb": stats.ram.cached,
                "shared_kb": stats.ram.shared,
                "total_mb": stats.ram.total / 1024,
                "used_mb": stats.ram.used / 1024,
                "free_mb": stats.ram.free / 1024,
                "buffers_mb": stats.ram.buffers.map(|v| v / 1024),
                "cached_mb": stats.ram.cached.map(|v| v / 1024),
            }))
        }

        #[cfg(target_os = "windows")]
        {
            use crate::platform::windows;
            let stats =
                windows::read_memory_stats().map_err(|e| IronError::MemoryError(e.to_string()))?;

            Ok(json!({
                "total_kb": stats.ram.total,
                "used_kb": stats.ram.used,
                "free_kb": stats.ram.free,
                "buffers_kb": stats.ram.buffers,
                "cached_kb": stats.ram.cached,
                "shared_kb": stats.ram.shared,
                "total_mb": stats.ram.total / 1024,
                "used_mb": stats.ram.used / 1024,
                "free_mb": stats.ram.free / 1024,
                "buffers_mb": stats.ram.buffers.map(|v| v / 1024),
                "cached_mb": stats.ram.cached.map(|v| v / 1024),
            }))
        }

        // macOS, through the same reader the other two arms use.
        //
        // This arm hand-rolled `sysctl hw.memsize` and `vm_stat` and carried
        // every defect the handoff records fixing in `tool_get_memory_status`,
        // which is the function immediately above it: a hardcoded
        // `page_size = 16384` that reports memory four times too large on an
        // Intel Mac, a failed `hw.memsize` giving a total of 0, and a failed
        // `vm_stat` falling to zero free pages -- which publishes 100% memory
        // used, on a machine nobody measured.
        //
        // It also emitted `buffers_kb: 0` and `shared_kb: 0` where the Linux
        // and Windows arms publish `Option`s, so two absences arrived as
        // measurements of zero.
        //
        // The fix there was to adopt `platform::macos::read_memory_stats`. The
        // fix here is the same, and the fact that it had to be made twice is
        // the point: **the defect was fixed per function rather than per
        // cause**, and the function next door kept it.
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let stats = crate::platform::macos::read_memory_stats()
                .map_err(|e| IronError::MemoryError(e.to_string()))?;

            Ok(json!({
                "total_kb": stats.ram.total,
                "used_kb": stats.ram.used,
                "free_kb": stats.ram.free,
                "buffers_kb": stats.ram.buffers,
                "cached_kb": stats.ram.cached,
                "shared_kb": stats.ram.shared,
                "total_mb": stats.ram.total / 1024,
                "used_mb": stats.ram.used / 1024,
                "free_mb": stats.ram.free / 1024,
                "buffers_mb": stats.ram.buffers.map(|v| v / 1024),
                "cached_mb": stats.ram.cached.map(|v| v / 1024),
            }))
        }
    }

    pub(crate) fn tool_get_swap_status(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::memory;
            let stats =
                memory::read_memory_stats().map_err(|e| IronError::MemoryError(e.to_string()))?;

            Ok(json!({
                "total_kb": stats.swap.total,
                "used_kb": stats.swap.used,
                "cached_kb": stats.swap.cached,
                // See the memory tool: absent rather than zero, because an
                // unread pagefile is not an idle one.
                "total_mb": stats.swap.total.map(|v| v / 1024),
                "used_mb": stats.swap.used.map(|v| v / 1024),
                "usage_percent": match (stats.swap.total, stats.swap.used) {
                    (Some(t), Some(u)) if t > 0 => json!(u as f64 / t as f64 * 100.0),
                    _ => serde_json::Value::Null,
                },
            }))
        }

        #[cfg(target_os = "windows")]
        {
            use crate::platform::windows;
            let stats =
                windows::read_memory_stats().map_err(|e| IronError::MemoryError(e.to_string()))?;

            Ok(json!({
                "total_kb": stats.swap.total,
                "used_kb": stats.swap.used,
                "cached_kb": stats.swap.cached,
                // See the memory tool: absent rather than zero, because an
                // unread pagefile is not an idle one.
                "total_mb": stats.swap.total.map(|v| v / 1024),
                "used_mb": stats.swap.used.map(|v| v / 1024),
                "usage_percent": match (stats.swap.total, stats.swap.used) {
                    (Some(t), Some(u)) if t > 0 => json!(u as f64 / t as f64 * 100.0),
                    _ => serde_json::Value::Null,
                },
            }))
        }

        // macOS, through the same reader the other two arms use.
        //
        // This arm was already honest -- every figure an `Option`, `None` where
        // the sysctl did not answer, after the fix whose comment it carried:
        // "a failed `sysctl` used to land in `unwrap_or_default()`, giving an
        // empty string that both parses read as `0`". What it still had was a
        // **second parser for `vm.swapusage`**, beside `platform::macos`'s
        // `parse_swapusage`, which `read_memory_stats` already calls.
        //
        // Two parsers for one sysctl is how the two Prometheus renderers came
        // to disagree by a factor of 1024 (`367429a`): both were correct when
        // written, and only one of them was corrected. The rule from `a42350d`
        // applies to a *right* duplicate as much as a wrong one -- the question
        // is where else the reader is hand-rolled, not where else it is broken.
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let stats = crate::platform::macos::read_memory_stats()
                .map_err(|e| IronError::MemoryError(e.to_string()))?;

            Ok(json!({
                "total_kb": stats.swap.total,
                "used_kb": stats.swap.used,
                "cached_kb": stats.swap.cached,
                "total_mb": stats.swap.total.map(|v| v / 1024),
                "used_mb": stats.swap.used.map(|v| v / 1024),
                "usage_percent": match (stats.swap.total, stats.swap.used) {
                    (Some(t), Some(u)) if t > 0 => json!(u as f64 / t as f64 * 100.0),
                    // A machine with a zero-sized swap file is not 0% used, it
                    // has no ratio to report.
                    _ => serde_json::Value::Null,
                },
            }))
        }
    }

    // ============== Disk Tools ==============

    pub(crate) fn tool_get_disk_list(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let disks =
            crate::disk::enumerate_disks().map_err(|e| IronError::DiskError(e.to_string()))?;

        let list: Vec<_> = disks
            .iter()
            .filter_map(|d| {
                d.info().ok().map(|info| {
                    json!({
                        "name": info.name,
                        "model": info.model,
                        // Integer division truncated every drive smaller than a
                        // gibibyte to `size_gb: 0`. The USB gadget on the
                        // development machine is 16,450,560 bytes and reported
                        // as sizeless, which is a different claim from small.
                        // The exact figure goes alongside so the answer is
                        // never rounded away.
                        "size_bytes": info.capacity,
                        "size_gb": info.capacity as f64 / 1024.0 / 1024.0 / 1024.0,
                        "disk_type": format!("{:?}", info.disk_type),
                    })
                })
            })
            .collect();

        Ok(json!(list))
    }

    pub(crate) fn tool_get_disk_details(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let disk_name = params
            .get("disk_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IronError::InvalidArgument("disk_name is required".to_string()))?;

        let disks =
            crate::disk::enumerate_disks().map_err(|e| IronError::DiskError(e.to_string()))?;

        let disk = disks
            .iter()
            .find(|d| {
                d.info()
                    .map(|i| i.name.contains(disk_name))
                    .unwrap_or(false)
            })
            .ok_or_else(|| IronError::InvalidArgument(format!("Disk {} not found", disk_name)))?;

        let info = disk
            .info()
            .map_err(|e| IronError::DiskError(e.to_string()))?;

        Ok(json!({
            "name": info.name,
            "model": info.model,
            "serial": info.serial,
            "firmware": info.firmware,
            "size_bytes": info.capacity,
            // See `tool_get_disk_list`: the exact byte count alongside a
            // figure that would otherwise truncate small media to zero.
            "size_bytes": info.capacity,
            "size_gb": info.capacity as f64 / 1024.0 / 1024.0 / 1024.0,
            "disk_type": format!("{:?}", info.disk_type),
            "temperature_c": disk.temperature().ok().flatten(),
        }))
    }

    pub(crate) fn tool_get_disk_io(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let disk_name = params.get("disk_name").and_then(|v| v.as_str());

        let disks =
            crate::disk::enumerate_disks().map_err(|e| IronError::DiskError(e.to_string()))?;

        let io_stats: Vec<_> = disks
            .iter()
            .filter(|d| {
                disk_name
                    .map(|name| d.info().map(|i| i.name.contains(name)).unwrap_or(false))
                    .unwrap_or(true)
            })
            .filter_map(|d| {
                let info = d.info().ok()?;
                let io = d.io_stats().ok()?;
                Some(json!({
                    "name": info.name,
                    "bytes_read": io.read_bytes,
                    "bytes_written": io.write_bytes,
                    "read_ops": io.read_ops,
                    "write_ops": io.write_ops,
                }))
            })
            .collect();

        Ok(json!(io_stats))
    }

    pub(crate) fn tool_get_disk_health(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let disk_name = params.get("disk_name").and_then(|v| v.as_str());

        let disks =
            crate::disk::enumerate_disks().map_err(|e| IronError::DiskError(e.to_string()))?;

        let health: Vec<_> = disks
            .iter()
            .filter(|d| {
                disk_name
                    .map(|name| d.info().map(|i| i.name.contains(name)).unwrap_or(false))
                    .unwrap_or(true)
            })
            .filter_map(|d| {
                let info = d.info().ok()?;
                let h = d.health().ok()?;
                Some(json!({
                    "name": info.name,
                    "overall_health": format!("{:?}", h),
                    "temperature_c": d.temperature().ok().flatten(),
                }))
            })
            .collect();

        Ok(json!(health))
    }

    // ============== Network Tools ==============

    pub(crate) fn tool_get_network_interfaces(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let active_only = params
            .get("active_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let net_mon = self.network_monitor.as_mut().ok_or_else(|| {
            IronError::NotImplemented("Network monitor not available".to_string())
        })?;

        let interfaces = if active_only {
            net_mon
                .active_interfaces()
                .map_err(|e| IronError::Network(e.to_string()))?
        } else {
            net_mon
                .interfaces()
                .map_err(|e| IronError::Network(e.to_string()))?
        };

        let list: Vec<_> = interfaces
            .iter()
            .map(|i| {
                json!({
                    "name": i.name,
                    "is_up": i.is_up,
                    "is_running": i.is_running,
                    "rx_bytes": i.rx_bytes,
                    "tx_bytes": i.tx_bytes,
                    "rx_mb": i.rx_bytes / 1024 / 1024,
                    "tx_mb": i.tx_bytes / 1024 / 1024,
                })
            })
            .collect();

        Ok(json!(list))
    }

    pub(crate) fn tool_get_network_bandwidth(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let interface_name = params.get("interface_name").and_then(|v| v.as_str());

        let net_mon = self.network_monitor.as_mut().ok_or_else(|| {
            IronError::NotImplemented("Network monitor not available".to_string())
        })?;

        let interfaces = net_mon
            .active_interfaces()
            .map_err(|e| IronError::Network(e.to_string()))?;

        let bandwidth: Vec<_> = interfaces
            .iter()
            .filter(|i| interface_name.map(|name| i.name == name).unwrap_or(true))
            .map(|i| {
                // `null` on the first call for an interface: a rate needs two
                // samples. This used to report `0.0`, which reads as an idle
                // link rather than as an unanswered question -- and because the
                // baseline was overwritten before the subtraction, it reported
                // `0.0` on every call, forever.
                let rate = net_mon.bandwidth_rate(&i.name, i);
                json!({
                    "name": i.name,
                    "rx_bytes_per_sec": rate.map(|(rx, _)| rx),
                    "tx_bytes_per_sec": rate.map(|(_, tx)| tx),
                    "rx_mbps": rate.map(|(rx, _)| rx / 1_000_000.0 * 8.0),
                    "tx_mbps": rate.map(|(_, tx)| tx / 1_000_000.0 * 8.0),
                })
            })
            .collect();

        Ok(json!(bandwidth))
    }

    pub(crate) fn tool_get_interface_details(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let interface_name = params
            .get("interface_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IronError::InvalidArgument("interface_name is required".to_string()))?;

        let net_mon = self.network_monitor.as_mut().ok_or_else(|| {
            IronError::NotImplemented("Network monitor not available".to_string())
        })?;

        let iface = net_mon
            .interface_by_name(interface_name)
            .map_err(|e| IronError::Network(e.to_string()))?
            .ok_or_else(|| {
                IronError::InvalidArgument(format!("Interface {} not found", interface_name))
            })?;

        // See `tool_get_network_bandwidth`: absent until a second sample.
        let rate = net_mon.bandwidth_rate(&iface.name, &iface);

        Ok(json!({
            "name": iface.name,
            "is_up": iface.is_up,
            "is_running": iface.is_running,
            "rx_bytes": iface.rx_bytes,
            "tx_bytes": iface.tx_bytes,
            "rx_packets": iface.rx_packets,
            "tx_packets": iface.tx_packets,
            "rx_errors": iface.rx_errors,
            "tx_errors": iface.tx_errors,
            "rx_drops": iface.rx_drops,
            "tx_drops": iface.tx_drops,
            "rx_rate_bytes_sec": rate.map(|(rx, _)| rx),
            "tx_rate_bytes_sec": rate.map(|(_, tx)| tx),
        }))
    }

    // ============== Process Tools ==============

    pub(crate) fn tool_get_process_list(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
        let sort_by = params
            .get("sort_by")
            .and_then(|v| v.as_str())
            .unwrap_or("cpu");

        let proc_mon = self.process_monitor.as_mut().ok_or_else(|| {
            IronError::NotImplemented("Process monitor not available".to_string())
        })?;

        let mut procs = proc_mon
            .processes()
            .map_err(|e| IronError::ProcessError(e.to_string()))?;

        // Sort
        match sort_by {
            "cpu" => procs.sort_by(|a, b| {
                b.cpu_percent
                    .partial_cmp(&a.cpu_percent)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            "memory" => procs.sort_by_key(|p| std::cmp::Reverse(p.memory_bytes)),
            "gpu_memory" => procs.sort_by_key(|p| std::cmp::Reverse(p.total_gpu_memory_bytes)),
            "name" => procs.sort_by(|a, b| a.name.cmp(&b.name)),
            "pid" => procs.sort_by_key(|a| a.pid),
            _ => {}
        }

        let list: Vec<_> = procs
            .iter()
            .take(limit)
            .map(|p| {
                json!({
                    "pid": p.pid,
                    "name": p.name,
                    "cpu_percent": p.cpu_percent,
                    "memory_mb": p.memory_bytes / 1024 / 1024,
                    "gpu_memory_mb": p.total_gpu_memory_bytes.map(|b| b / 1024 / 1024),
                    "thread_count": p.thread_count,
                    "state": p.state,
                })
            })
            .collect();

        Ok(json!({
            "total_count": procs.len(),
            "returned_count": list.len(),
            "sort_by": sort_by,
            "processes": list
        }))
    }

    pub(crate) fn tool_get_process_details(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let pid = params
            .get("pid")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| IronError::InvalidArgument("pid is required".to_string()))?
            as u32;

        let proc_mon = self.process_monitor.as_mut().ok_or_else(|| {
            IronError::NotImplemented("Process monitor not available".to_string())
        })?;

        let proc = proc_mon
            .process_by_pid(pid)
            .map_err(|e| IronError::ProcessError(e.to_string()))?
            .ok_or_else(|| IronError::InvalidArgument(format!("Process {} not found", pid)))?;

        Ok(json!({
            "pid": proc.pid,
            "name": proc.name,
            "state": proc.state,
            "parent_pid": proc.parent_pid,
            "cpu_percent": proc.cpu_percent,
            "memory_bytes": proc.memory_bytes,
            "memory_mb": proc.memory_bytes / 1024 / 1024,
            "virtual_memory_bytes": proc.virtual_memory_bytes,
            "thread_count": proc.thread_count,
            "gpu_indices": proc.gpu_indices,
            "gpu_memory_bytes": proc.total_gpu_memory_bytes,
            "gpu_memory_mb": proc.total_gpu_memory_bytes.map(|b| b / 1024 / 1024),
        }))
    }

    pub(crate) fn tool_get_top_cpu_processes(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

        let proc_mon = self.process_monitor.as_mut().ok_or_else(|| {
            IronError::NotImplemented("Process monitor not available".to_string())
        })?;

        let top = proc_mon
            .processes_by_cpu()
            .map_err(|e| IronError::ProcessError(e.to_string()))?;

        let list: Vec<_> = top
            .iter()
            .take(count)
            .map(|p| {
                json!({
                    "pid": p.pid,
                    "name": p.name,
                    "cpu_percent": p.cpu_percent,
                    "memory_mb": p.memory_bytes / 1024 / 1024,
                })
            })
            .collect();

        Ok(json!(list))
    }

    pub(crate) fn tool_get_top_memory_processes(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

        let proc_mon = self.process_monitor.as_mut().ok_or_else(|| {
            IronError::NotImplemented("Process monitor not available".to_string())
        })?;

        let top = proc_mon
            .processes_by_memory()
            .map_err(|e| IronError::ProcessError(e.to_string()))?;

        let list: Vec<_> = top
            .iter()
            .take(count)
            .map(|p| {
                json!({
                    "pid": p.pid,
                    "name": p.name,
                    "memory_bytes": p.memory_bytes,
                    "memory_mb": p.memory_bytes / 1024 / 1024,
                    "cpu_percent": p.cpu_percent,
                })
            })
            .collect();

        Ok(json!(list))
    }

    pub(crate) fn tool_get_top_gpu_processes(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        let gpu_index = params.get("gpu_index").and_then(|v| v.as_u64());

        let proc_mon = self.process_monitor.as_mut().ok_or_else(|| {
            IronError::NotImplemented("Process monitor not available".to_string())
        })?;

        let top = proc_mon
            .processes_by_gpu_memory()
            .map_err(|e| IronError::ProcessError(e.to_string()))?;

        let list: Vec<_> = top
            .iter()
            .filter(|p| {
                gpu_index
                    .map(|idx| p.gpu_indices.contains(&(idx as usize)))
                    .unwrap_or(true)
            })
            .take(count)
            .map(|p| {
                json!({
                    "pid": p.pid,
                    "name": p.name,
                    "gpu_indices": p.gpu_indices,
                    "gpu_memory_bytes": p.total_gpu_memory_bytes,
                    "gpu_memory_mb": p.total_gpu_memory_bytes.map(|b| b / 1024 / 1024),
                    "cpu_percent": p.cpu_percent,
                    "system_memory_mb": p.memory_bytes / 1024 / 1024,
                })
            })
            .collect();

        Ok(json!(list))
    }

    pub(crate) fn tool_search_processes(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let pattern = params
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IronError::InvalidArgument("pattern is required".to_string()))?
            .to_lowercase();

        let proc_mon = self.process_monitor.as_mut().ok_or_else(|| {
            IronError::NotImplemented("Process monitor not available".to_string())
        })?;

        let procs = proc_mon
            .processes()
            .map_err(|e| IronError::ProcessError(e.to_string()))?;

        let matches: Vec<_> = procs
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&pattern))
            .map(|p| {
                json!({
                    "pid": p.pid,
                    "name": p.name,
                    "cpu_percent": p.cpu_percent,
                    "memory_mb": p.memory_bytes / 1024 / 1024,
                    "gpu_memory_mb": p.total_gpu_memory_bytes.map(|b| b / 1024 / 1024),
                })
            })
            .collect();

        Ok(json!({
            "pattern": pattern,
            "match_count": matches.len(),
            "processes": matches
        }))
    }

    // ============== Hardware Tools ==============

    pub(crate) fn tool_get_motherboard_sensors(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let sensors = crate::motherboard::enumerate_sensors()
            .map_err(|e| IronError::HardwareError(e.to_string()))?;

        let mut readings = Vec::new();

        // Collect temperature sensors
        for sensor in &sensors {
            if let Ok(temps) = sensor.temperature_sensors() {
                for t in temps {
                    readings.push(json!({
                        "name": t.label,
                        "sensor_type": "temperature",
                        "value": t.temperature,
                        "unit": "°C",
                        "max": t.max,
                        "critical": t.critical,
                    }));
                }
            }

            // Collect voltage rails
            if let Ok(volts) = sensor.voltage_rails() {
                for v in volts {
                    readings.push(json!({
                        "name": v.label,
                        "sensor_type": "voltage",
                        "value": v.voltage,
                        "unit": "V",
                        "min": v.min,
                        "max": v.max,
                    }));
                }
            }

            // Collect fan info
            if let Ok(fans) = sensor.fans() {
                for f in fans {
                    readings.push(json!({
                        "name": f.label,
                        "sensor_type": "fan",
                        "value": f.rpm,
                        "unit": "RPM",
                        "min": f.min_rpm,
                        "max": f.max_rpm,
                    }));
                }
            }
        }

        Ok(json!(readings))
    }

    pub(crate) fn tool_get_system_temperatures(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let sensors = crate::motherboard::enumerate_sensors()
            .map_err(|e| IronError::HardwareError(e.to_string()))?;

        let mut temps = Vec::new();

        // Get temperatures from motherboard sensors
        for sensor in &sensors {
            if let Ok(temp_sensors) = sensor.temperature_sensors() {
                for t in temp_sensors {
                    temps.push(json!({
                        "name": t.label,
                        "temperature_c": t.temperature,
                        "max_c": t.max,
                        "critical_c": t.critical,
                        "sensor_type": format!("{:?}", t.sensor_type),
                    }));
                }
            }
        }

        // Add GPU temperatures
        if let Some(ref gpus) = self.gpus {
            if let Ok(snapshots) = gpus.snapshot_all() {
                for (idx, s) in snapshots.iter().enumerate() {
                    if let Some(temp) = s.dynamic_info.thermal.temperature {
                        temps.push(json!({
                            "name": format!("GPU {} ({})", idx, s.static_info.name),
                            "temperature_c": temp,
                        }));
                    }
                }
            }
        }

        // Add disk temperatures
        if let Ok(disks) = crate::disk::enumerate_disks() {
            for disk in &disks {
                if let (Ok(info), Ok(Some(temp))) = (disk.info(), disk.temperature()) {
                    temps.push(json!({
                        "name": format!("Disk ({})", info.name),
                        "temperature_c": temp,
                    }));
                }
            }
        }

        Ok(json!(temps))
    }

    pub(crate) fn tool_get_fan_speeds(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let sensors = crate::motherboard::enumerate_sensors()
            .map_err(|e| IronError::HardwareError(e.to_string()))?;

        let mut fans = Vec::new();

        for sensor in &sensors {
            if let Ok(fan_info) = sensor.fans() {
                for f in fan_info {
                    fans.push(json!({
                        "name": f.label,
                        "rpm": f.rpm,
                        "pwm": f.pwm,
                        "controllable": f.controllable,
                    }));
                }
            }
        }

        Ok(json!(fans))
    }

    pub(crate) fn tool_get_voltage_rails(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let sensors = crate::motherboard::enumerate_sensors()
            .map_err(|e| IronError::HardwareError(e.to_string()))?;

        let mut voltages = Vec::new();

        for sensor in &sensors {
            if let Ok(voltage_rails) = sensor.voltage_rails() {
                for v in voltage_rails {
                    voltages.push(json!({
                        "name": v.label,
                        "voltage_v": v.voltage,
                        "min_v": v.min,
                        "max_v": v.max,
                    }));
                }
            }
        }

        Ok(json!(voltages))
    }

    pub(crate) fn tool_get_driver_info(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let driver_type = params
            .get("driver_type")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let drivers = crate::motherboard::get_driver_versions()
            .map_err(|e| IronError::HardwareError(e.to_string()))?;

        let filtered: Vec<_> = drivers
            .iter()
            .filter(|d| {
                driver_type == "all"
                    || matches!(
                        (&d.driver_type, driver_type),
                        (crate::motherboard::DriverType::Gpu, "gpu")
                            | (crate::motherboard::DriverType::Network, "network")
                            | (crate::motherboard::DriverType::Storage, "storage")
                    )
            })
            .map(|d| {
                json!({
                    "name": d.name,
                    "driver_type": format!("{:?}", d.driver_type),
                    "version": d.version,
                    "vendor": d.vendor,
                    "date": d.date,
                })
            })
            .collect();

        Ok(json!(filtered))
    }

    // ============== Audio Tools ==============
    pub(crate) fn tool_get_audio_devices(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::audio::{AudioDeviceType, AudioMonitor};
        let device_type = params
            .get("device_type")
            .and_then(|v| v.as_str())
            .unwrap_or("all");
        let monitor = AudioMonitor::new().map_err(|e| IronError::HardwareError(e.to_string()))?;
        let devices: Vec<_> = monitor.devices().iter()
            .filter(|d| device_type == "all" || matches!((&d.device_type, device_type), (AudioDeviceType::Output, "output") | (AudioDeviceType::Input, "input") | (AudioDeviceType::Duplex, "output") | (AudioDeviceType::Duplex, "input")))
            .map(|d| json!({"id": d.id, "name": d.name, "type": format!("{:?}", d.device_type), "state": format!("{:?}", d.state), "is_default": d.is_default, "volume": d.volume, "muted": d.muted}))
            .collect();
        Ok(json!(devices))
    }
    pub(crate) fn tool_get_audio_status(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::audio::AudioMonitor;
        let monitor = AudioMonitor::new().map_err(|e| IronError::HardwareError(e.to_string()))?;
        Ok(
            json!({"master_volume": monitor.master_volume(), "is_muted": monitor.is_muted(), "device_count": monitor.devices().len()}),
        )
    }
    // ============== Bluetooth Tools ==============
    pub(crate) fn tool_get_bluetooth_adapters(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::bluetooth::BluetoothMonitor;
        let monitor =
            BluetoothMonitor::new().map_err(|e| IronError::HardwareError(e.to_string()))?;
        let adapters: Vec<_> = monitor
            .adapters()
            .iter()
            .map(
                |a| json!({"id": a.id, "name": a.name, "address": a.address, "powered": a.powered}),
            )
            .collect();
        Ok(json!({"available": monitor.is_available(), "adapters": adapters}))
    }
    pub(crate) fn tool_get_bluetooth_devices(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::bluetooth::{BluetoothMonitor, BluetoothState};
        let connected_only = params
            .get("connected_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let monitor =
            BluetoothMonitor::new().map_err(|e| IronError::HardwareError(e.to_string()))?;
        let devices: Vec<_> = monitor.devices().iter().filter(|d| !connected_only || d.state == BluetoothState::Connected).map(|d| json!({"address": d.address, "name": d.name, "type": format!("{:?}", d.device_type), "state": format!("{:?}", d.state), "battery_percent": d.battery_percent})).collect();
        Ok(json!(devices))
    }
    // ============== Display Tools ==============
    pub(crate) fn tool_get_display_list(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::display::DisplayMonitor;
        let monitor = DisplayMonitor::new().map_err(|e| IronError::HardwareError(e.to_string()))?;
        // `"resolution": "0x0"` went out to agents for any display whose mode
        // was not readable, and `refresh_rate: 0.0` for every display on Linux
        // and macOS, where no reader parses a rate. Both are null now, which an
        // agent can act on; a zero it cannot.
        let displays: Vec<_> = monitor
            .displays()
            .iter()
            .map(|d| {
                json!({
                    "id": d.id,
                    "name": d.name,
                    "manufacturer": d.manufacturer,
                    "connection": format!("{:?}", d.connection),
                    "is_primary": d.is_primary,
                    "resolution": resolution_text(d),
                    "aspect_ratio": d.aspect_ratio(),
                    "refresh_rate": d.refresh_rate,
                    "brightness": d.brightness,
                    "hdr": format!("{:?}", d.hdr),
                })
            })
            .collect();
        Ok(json!({"count": monitor.count(), "displays": displays}))
    }
    pub(crate) fn tool_get_display_details(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::display::DisplayMonitor;
        let display_id = params
            .get("display_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IronError::InvalidArgument("display_id required".to_string()))?;
        let monitor = DisplayMonitor::new().map_err(|e| IronError::HardwareError(e.to_string()))?;
        let display = monitor
            .displays()
            .iter()
            .find(|d| d.id == display_id)
            .ok_or_else(|| {
                IronError::DeviceNotFound(format!("Display {} not found", display_id))
            })?;
        Ok(json!({
            "id": display.id,
            "name": display.name,
            "manufacturer": display.manufacturer,
            "connection": format!("{:?}", display.connection),
            "is_primary": display.is_primary,
            "width": display.width,
            "height": display.height,
            "resolution": resolution_text(display),
            "aspect_ratio": display.aspect_ratio(),
            "refresh_rate": display.refresh_rate,
            "brightness": display.brightness,
            "hdr": format!("{:?}", display.hdr),
        }))
    }
    // ============== USB Tools ==============
    pub(crate) fn tool_get_usb_devices(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::usb::{UsbDeviceClass, UsbMonitor};
        let class_filter = params
            .get("class")
            .and_then(|v| v.as_str())
            .unwrap_or("all");
        let monitor = UsbMonitor::new().map_err(|e| IronError::HardwareError(e.to_string()))?;
        let devices: Vec<_> = monitor.devices().iter().filter(|d| class_filter == "all" || matches!((&d.class, class_filter), (UsbDeviceClass::Audio, "audio") | (UsbDeviceClass::Hid, "hid") | (UsbDeviceClass::MassStorage, "storage") | (UsbDeviceClass::Hub, "hub") | (UsbDeviceClass::Video, "video"))).map(|d| json!({"address": d.address, "vendor_id": d.vendor_id.map(|v| format!("{v:04x}")), "product_id": d.product_id.map(|p| format!("{p:04x}")), "vendor_name": d.manufacturer, "product_name": d.product, "class": format!("{:?}", d.class), "speed": format!("{:?}", d.speed)})).collect();
        Ok(json!(devices))
    }
    pub(crate) fn tool_get_usb_device_details(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::usb::UsbMonitor;

        // Addressed by `UsbDevice::address`, the same string the ontology uses
        // for `usb.{addr}` and the same one `get_usb_devices` returns.
        //
        // This took a `bus` and an `address` and matched them against
        // `bus_number` and `port_number`. When the id scheme moved to the
        // platform device path, the Windows reader stopped filling those two --
        // they are `0` and `0` for every device it enumerates -- and this
        // lookup silently collapsed: **39 devices, one distinct (bus, port)
        // pair**, so the tool could only ever return whichever device came
        // first and every other one on the machine became unreachable.
        //
        // I introduced that when I changed the id scheme, having written the
        // note two commits earlier that says to ask "what else stores this?"
        // after a reader change. Asking it of `bus_number` would have found
        // this immediately.
        let address = params
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                IronError::InvalidArgument(
                    "address required: the device address from get_usb_devices, \
                     e.g. \"usb_vid_046d_pid_c548_mi_01_9_24c94812_0_0001\""
                        .to_string(),
                )
            })?;
        let monitor = UsbMonitor::new().map_err(|e| IronError::HardwareError(e.to_string()))?;
        let device = monitor
            .devices()
            .iter()
            .find(|d| d.address == address)
            .ok_or_else(|| {
                IronError::DeviceNotFound(format!("no USB device with address {address}"))
            })?;
        Ok(
            json!({"address": device.address, "vendor_id": device.vendor_id.map(|v| format!("{v:04x}")), "product_id": device.product_id.map(|p| format!("{p:04x}")), "vendor_name": device.manufacturer, "product_name": device.product, "class": format!("{:?}", device.class), "speed": format!("{:?}", device.speed)}),
        )
    }

    // ============== Profile Inspector Tools ==============
    pub(crate) fn tool_list_profile_subsystems(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::profile::{ProfileInspector, Subsystem};
        let mut inspector = ProfileInspector::new();
        let snapshot = inspector.snapshot_all();
        let subsystems: Vec<_> = Subsystem::ALL
            .iter()
            .map(|s| {
                let groups = snapshot.providers.get(s);
                let (group_count, setting_count) = match groups {
                    Some(g) => (g.len(), g.iter().map(|x| x.settings.len()).sum::<usize>()),
                    None => (0, 0),
                };
                json!({
                    "subsystem": s.as_str(),
                    "groups": group_count,
                    "settings": setting_count,
                })
            })
            .collect();
        Ok(json!({"timestamp": snapshot.timestamp, "subsystems": subsystems}))
    }

    pub(crate) fn tool_get_profile_settings(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::profile::{ProfileInspector, Subsystem};
        let name = params
            .get("subsystem")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IronError::InvalidArgument("subsystem required".into()))?;
        let sub = Subsystem::parse(name)
            .ok_or_else(|| IronError::InvalidArgument(format!("unknown subsystem: {}", name)))?;
        let mut inspector = ProfileInspector::new();
        let groups = inspector.snapshot(sub);
        Ok(serde_json::to_value(groups)?)
    }

    pub(crate) fn tool_get_profile_deviations(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::profile::{deviation::deviations_from_default, ProfileInspector};
        let mut inspector = ProfileInspector::new();
        let snapshot = inspector.snapshot_all();
        let devs = deviations_from_default(&snapshot);
        Ok(json!({"count": devs.len(), "deviations": devs}))
    }

    pub(crate) fn tool_benchmark_profile_providers(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let report = crate::profile::bench::run_bench();
        Ok(serde_json::to_value(report)?)
    }

    pub(crate) fn tool_list_writable_profile_settings(
        &mut self,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let ids = crate::profile::apply::writable_setting_ids();
        Ok(json!({"count": ids.len(), "ids": ids}))
    }

    pub(crate) fn tool_apply_profile_setting(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = params
            .get("setting_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IronError::InvalidArgument("setting_id required".into()))?
            .to_string();
        let confirm = params
            .get("confirm")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let value: crate::profile::SettingValue = match params.get("value") {
            Some(serde_json::Value::Bool(b)) => crate::profile::SettingValue::Bool(*b),
            Some(serde_json::Value::Number(n)) => {
                if let Some(i) = n.as_i64() {
                    crate::profile::SettingValue::Int(i)
                } else if let Some(u) = n.as_u64() {
                    crate::profile::SettingValue::Uint(u)
                } else if let Some(f) = n.as_f64() {
                    crate::profile::SettingValue::Float(f)
                } else {
                    return Err(IronError::InvalidArgument(
                        "unsupported numeric value".into(),
                    ));
                }
            }
            Some(serde_json::Value::String(s)) => crate::profile::SettingValue::Text(s.clone()),
            Some(other) => {
                return Err(IronError::InvalidArgument(format!(
                    "unsupported value type: {}",
                    other
                )))
            }
            None => {
                return Err(IronError::InvalidArgument("value required".into()));
            }
        };

        // Belt-and-suspenders: AI agent writes also require the env-var gate
        // in addition to confirm. Operators must opt-in deliberately.
        let agent_allowed = std::env::var("IRONMON_ALLOW_AGENT_WRITES")
            .map(|v| v == "1")
            .unwrap_or(false);
        let effective_confirm = confirm && agent_allowed;

        let outcome = crate::profile::apply::apply_setting(&id, value, effective_confirm);
        let mut response = serde_json::to_value(&outcome)?;
        if confirm && !agent_allowed {
            if let Some(obj) = response.as_object_mut() {
                obj.insert("agent_write_blocked".to_string(), json!(true));
                obj.insert(
                    "agent_write_message".to_string(),
                    json!("Agent write was suppressed because IRONMON_ALLOW_AGENT_WRITES is not set to 1. Operator must enable this env-var to allow MCP tools to mutate hardware state."),
                );
            }
        }
        Ok(response)
    }

    pub(crate) fn tool_explain_profile_setting(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::profile::{explain, explain::candidates, ProfileInspector};
        let id = params
            .get("setting_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IronError::InvalidArgument("setting_id required".into()))?;
        let mut inspector = ProfileInspector::new();
        let snapshot = inspector.snapshot_all();
        match explain::explain(&snapshot, id) {
            Some(exp) => Ok(serde_json::to_value(exp)?),
            None => Ok(json!({
                "found": false,
                "candidates": candidates(&snapshot, id),
            })),
        }
    }

    pub(crate) fn tool_get_active_app_profiles(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let matched_only = params
            .get("matched_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let entries = if matched_only {
            crate::profile::active::matched_active_profiles()
        } else {
            crate::profile::active::active_profiles_for_processes()
        };
        let total = entries.len();
        let matched = entries.iter().filter(|e| e.has_any_profile()).count();
        Ok(json!({
            "total_processes": total,
            "matched_count": matched,
            "entries": entries,
        }))
    }

    pub(crate) fn tool_search_profile_settings(
        &mut self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use crate::profile::ProfileInspector;
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IronError::InvalidArgument("query required".into()))?;
        let mut inspector = ProfileInspector::new();
        let snapshot = inspector.snapshot_all();
        let hits: Vec<_> = snapshot
            .search(query)
            .into_iter()
            .map(|(sub, group, setting)| {
                json!({
                    "subsystem": sub.as_str(),
                    "device": &group.device,
                    "profile": &group.display_name,
                    "setting": setting,
                })
            })
            .collect();
        Ok(json!({"query": query, "match_count": hits.len(), "matches": hits}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An index naming no device is an error, not an empty success.
    ///
    /// `get_gpu_temperature`, `_memory`, `_power` and `_utilization` take an optional
    /// index and used to filter the snapshot list by it, so `gpu_index: 99` on a
    /// three-GPU machine returned `{"success": true, "data": []}`. An agent reports
    /// that as "nothing to report" rather than "there is no GPU 99". Verified live
    /// against the MCP server before and after.
    #[test]
    fn absent_gpu_index_is_rejected_not_answered_empty() {
        // Present indices pass, including the last valid one.
        assert!(AiDataApi::check_gpu_index(Some(0), 3).is_ok());
        assert!(AiDataApi::check_gpu_index(Some(2), 3).is_ok());
        // Omitted index means "all GPUs" and stays valid.
        assert!(AiDataApi::check_gpu_index(None, 3).is_ok());
        assert!(AiDataApi::check_gpu_index(None, 0).is_ok());

        // One past the end, and far past it.
        for absent in [3u64, 99] {
            let err = AiDataApi::check_gpu_index(Some(absent), 3)
                .expect_err("index {absent} does not exist and must be refused");
            assert!(
                err.to_string()
                    .contains(&format!("GPU index {absent} not found")),
                "unexpected message for index {absent}: {err}"
            );
        }

        // A machine with no GPUs refuses index 0 rather than reporting an empty win.
        assert!(AiDataApi::check_gpu_index(Some(0), 0).is_err());
    }
}
