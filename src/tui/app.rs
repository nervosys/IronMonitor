//! Application state management

use crate::agent::{Agent, AgentConfig, AgentResponse};
use crate::silicon::NpuInfo;
use crate::{ProcessMonitor, ProcessMonitorInfo, UnifiedMonitor};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Maximum number of data points to keep in history
const MAX_HISTORY: usize = 60;

/// Maximum number of agent responses to keep
const MAX_AGENT_HISTORY: usize = 10;

/// Build the agent the TUI should use.
///
/// Priority:
/// 1. Local Ollama at http://localhost:11434 (use first installed model from
///    `/api/tags`, with a sensible default if the list is empty).
/// 2. Whatever `AgentConfig::auto_detect` picks (OpenAI / Anthropic / GitHub
///    Models / LM Studio / etc.).
fn build_preferred_agent() -> Option<Agent> {
    let config = build_ollama_config()
        .or_else(|| AgentConfig::auto_detect().ok())
        .map(|c| {
            c.with_caching(true)
                .with_cache_size(50)
                .with_timeout(Duration::from_secs(60))
        })?;
    Agent::new(config).ok()
}

#[cfg(feature = "remote-backends")]
fn build_ollama_config() -> Option<AgentConfig> {
    use crate::agent::backend::BackendConfig;

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(1))
        .timeout(Duration::from_secs(2))
        .build()
        .ok()?;

    let resp = client.get("http://localhost:11434/api/tags").send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().ok()?;
    let models = json.get("models")?.as_array()?;

    // Prefer a small/fast model if one is installed; otherwise take the first.
    const PREFERRED: &[&str] = &[
        "llama3.2:3b",
        "llama3.2",
        "llama3.1:8b",
        "qwen2.5:3b",
        "qwen2.5:7b",
        "mistral:7b",
        "phi3:mini",
    ];
    let installed: Vec<String> = models
        .iter()
        .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect();
    if installed.is_empty() {
        return None;
    }
    let chosen = PREFERRED
        .iter()
        .find(|name| installed.iter().any(|i| i == *name))
        .map(|s| s.to_string())
        .unwrap_or_else(|| installed[0].clone());

    Some(AgentConfig::with_backend(BackendConfig::ollama(&chosen)))
}

#[cfg(not(feature = "remote-backends"))]
fn build_ollama_config() -> Option<AgentConfig> {
    None
}

/// Type of accelerator device
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AcceleratorType {
    /// GPU - Graphics Processing Unit
    #[default]
    Gpu,
    /// NPU - Neural Processing Unit
    Npu,
    /// TPU - Tensor Processing Unit
    Tpu,
    /// FPGA - Field Programmable Gate Array
    Fpga,
    /// DLA - Deep Learning Accelerator (e.g., Jetson DLA)
    Dla,
    /// VPU - Vision Processing Unit (e.g., Intel Movidius)
    Vpu,
    /// IPU - Intelligence Processing Unit (e.g., Graphcore)
    Ipu,
    /// Custom/Other accelerator
    Other,
}

impl std::fmt::Display for AcceleratorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcceleratorType::Gpu => write!(f, "GPU"),
            AcceleratorType::Npu => write!(f, "NPU"),
            AcceleratorType::Tpu => write!(f, "TPU"),
            AcceleratorType::Fpga => write!(f, "FPGA"),
            AcceleratorType::Dla => write!(f, "DLA"),
            AcceleratorType::Vpu => write!(f, "VPU"),
            AcceleratorType::Ipu => write!(f, "IPU"),
            AcceleratorType::Other => write!(f, "ACC"),
        }
    }
}

/// Unified accelerator information structure
#[derive(Clone, Default)]
pub struct AcceleratorInfo {
    /// Device name (e.g., "NVIDIA GeForce RTX 4090", "Intel Neural Compute Stick 2")
    pub name: String,
    /// Vendor name (e.g., "NVIDIA", "AMD", "Intel", "Xilinx")
    pub vendor: String,
    /// Type of accelerator
    pub accel_type: AcceleratorType,
    /// Compute/core utilization (0-100%)
    /// Utilization percentage, or `None` where the device exposes none.
    ///
    /// An NPU's utilization needs a vendor-specific API on Windows and Linux,
    /// and `NpuInfo` says `None` there. Rendering that as `0.0` would put an
    /// idle-looking gauge next to a device nothing can measure.
    pub utilization: Option<f32>,
    /// Temperature in Celsius
    pub temperature: Option<f32>,
    /// Power consumption in Watts
    pub power: Option<f32>,
    /// Power limit in Watts
    pub power_limit: Option<f32>,
    /// Total memory in bytes, or `None` where the device reported none.
    pub memory_total: Option<u64>,
    /// Used memory in bytes, or `None` where the device reported none.
    pub memory_used: Option<u64>,
    /// Core/compute clock in MHz
    pub clock_core: Option<u32>,
    /// Memory clock in MHz
    pub clock_memory: Option<u32>,
    /// Fan speed in RPM
    pub fan_speed_rpm: Option<u32>,
    /// Fan speed percentage (0-100)
    pub fan_speed_percent: Option<f32>,
    /// PCIe link generation (e.g., 4 for Gen4)
    pub pcie_gen: Option<u32>,
    /// PCIe link width (e.g., 16 for x16)
    pub pcie_width: Option<u32>,
    /// PCIe throughput in MB/s
    pub pcie_throughput: Option<f64>,
    /// Encoder utilization (video encoding, 0-100%)
    pub encoder_util: Option<f32>,
    /// Decoder utilization (video decoding, 0-100%)
    pub decoder_util: Option<f32>,
    /// Last time encoder was active
    pub encoder_last_active: Option<Instant>,
    /// Last time decoder was active
    pub decoder_last_active: Option<Instant>,
    /// Device-specific status string (e.g., "P0", "Active", "Idle")
    pub status: Option<String>,
    /// Firmware/driver version
    pub firmware_version: Option<String>,
    /// Serial number or UUID
    pub serial: Option<String>,
    /// PCIe slot info (for PCIe devices)
    pub pcie_slot: Option<String>,
}

/// Process display mode - which device's processes to show
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProcessDisplayMode {
    /// Show CPU processes
    Cpu,
    /// Show GPU processes (with index)
    Gpu(usize),
    /// Show NPU processes (with index)
    Npu(usize),
    /// Show accelerator processes (unified, with index)
    Accelerator(usize),
    /// Show all processes
    #[default]
    All,
}

/// Current view mode for the TUI
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Main dashboard view
    #[default]
    Main,
    /// Process detail view
    ProcessDetail,
    /// Theme selection view
    ThemeSelection,
}

/// Available color themes
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorTheme {
    /// Catppuccin Mocha (default - matches GUI)
    #[default]
    CatppuccinMocha,
    /// Catppuccin Latte (light theme)
    CatppuccinLatte,
    /// Classic Glances style
    Glances,
    /// Nord theme
    Nord,
    /// Dracula theme
    Dracula,
    /// Gruvbox Dark
    GruvboxDark,
}

impl ColorTheme {
    pub fn all() -> &'static [ColorTheme] {
        &[
            ColorTheme::CatppuccinMocha,
            ColorTheme::CatppuccinLatte,
            ColorTheme::Glances,
            ColorTheme::Nord,
            ColorTheme::Dracula,
            ColorTheme::GruvboxDark,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ColorTheme::CatppuccinMocha => "Catppuccin Mocha",
            ColorTheme::CatppuccinLatte => "Catppuccin Latte",
            ColorTheme::Glances => "Glances Classic",
            ColorTheme::Nord => "Nord",
            ColorTheme::Dracula => "Dracula",
            ColorTheme::GruvboxDark => "Gruvbox Dark",
        }
    }

    pub fn next(&self) -> ColorTheme {
        let themes = Self::all();
        let idx = themes.iter().position(|t| t == self).unwrap_or(0);
        themes[(idx + 1) % themes.len()]
    }

    pub fn prev(&self) -> ColorTheme {
        let themes = Self::all();
        let idx = themes.iter().position(|t| t == self).unwrap_or(0);
        if idx == 0 {
            themes[themes.len() - 1]
        } else {
            themes[idx - 1]
        }
    }

    /// Get theme colors
    pub fn colors(&self) -> ThemeColors {
        match self {
            ColorTheme::CatppuccinMocha => ThemeColors {
                ok: (166, 227, 161),       // green
                careful: (137, 180, 250),  // blue
                warning: (249, 226, 175),  // yellow
                critical: (243, 139, 168), // red
                title: (137, 180, 250),    // blue
                separator: (88, 91, 112),  // surface2
                inactive: (108, 112, 134), // overlay0
                surface: (69, 71, 90),     // surface0
                text: (205, 214, 244),     // text
            },
            ColorTheme::CatppuccinLatte => ThemeColors {
                ok: (64, 160, 43),
                careful: (30, 102, 245),
                warning: (223, 142, 29),
                critical: (210, 15, 57),
                title: (30, 102, 245),
                separator: (172, 176, 190),
                inactive: (140, 143, 161),
                surface: (204, 208, 218),
                text: (76, 79, 105),
            },
            ColorTheme::Glances => ThemeColors {
                ok: (0, 255, 0),
                careful: (0, 255, 255),
                warning: (255, 255, 0),
                critical: (255, 0, 0),
                title: (0, 255, 255),
                separator: (105, 105, 105),
                inactive: (105, 105, 105),
                surface: (48, 48, 48),
                text: (255, 255, 255),
            },
            ColorTheme::Nord => ThemeColors {
                ok: (163, 190, 140),
                careful: (129, 161, 193),
                warning: (235, 203, 139),
                critical: (191, 97, 106),
                title: (136, 192, 208),
                separator: (76, 86, 106),
                inactive: (76, 86, 106),
                surface: (59, 66, 82),
                text: (236, 239, 244),
            },
            ColorTheme::Dracula => ThemeColors {
                ok: (80, 250, 123),
                careful: (139, 233, 253),
                warning: (241, 250, 140),
                critical: (255, 85, 85),
                title: (189, 147, 249),
                separator: (68, 71, 90),
                inactive: (98, 114, 164),
                surface: (68, 71, 90),
                text: (248, 248, 242),
            },
            ColorTheme::GruvboxDark => ThemeColors {
                ok: (184, 187, 38),
                careful: (131, 165, 152),
                warning: (250, 189, 47),
                critical: (251, 73, 52),
                title: (250, 189, 47),
                separator: (80, 73, 69),
                inactive: (146, 131, 116),
                surface: (60, 56, 54),
                text: (235, 219, 178),
            },
        }
    }
}

/// Theme color palette
#[derive(Clone, Copy)]
pub struct ThemeColors {
    pub ok: (u8, u8, u8),
    pub careful: (u8, u8, u8),
    pub warning: (u8, u8, u8),
    pub critical: (u8, u8, u8),
    pub title: (u8, u8, u8),
    pub separator: (u8, u8, u8),
    pub inactive: (u8, u8, u8),
    pub surface: (u8, u8, u8),
    pub text: (u8, u8, u8),
}

/// Cached peripheral information to avoid expensive monitor creation on every frame
#[derive(Clone, Default)]
pub struct PeripheralCache {
    /// Audio device info string
    pub audio_info: String,
    /// Bluetooth info string
    pub bluetooth_info: String,
    /// Display info string
    pub display_info: String,
    /// USB device info string
    pub usb_info: String,
    /// Battery info string
    pub battery_info: String,
}

impl PeripheralCache {
    /// Update all peripheral information (call this periodically, not every frame)
    pub fn refresh(&mut self) {
        use crate::audio::AudioMonitor;
        use crate::battery::BatteryMonitor;
        use crate::bluetooth::BluetoothMonitor;
        use crate::display::DisplayMonitor;
        use crate::usb::UsbMonitor;

        // Audio
        self.audio_info = if let Ok(monitor) = AudioMonitor::new() {
            let devices = monitor.devices();
            if devices.is_empty() {
                "No audio devices detected".to_string()
            } else {
                let mut lines: Vec<String> = Vec::new();
                lines.push(format!(
                    "{} audio device(s) | Volume: {} | Muted: {}",
                    devices.len(),
                    monitor
                        .master_volume()
                        .map_or_else(|| "not read".to_string(), |v| format!("{v}%")),
                    match monitor.is_muted() {
                        Some(true) => "Yes",
                        Some(false) => "No",
                        None => "not read",
                    }
                ));
                for dev in devices.iter().take(4) {
                    let icon = if dev.is_output { "♪" } else { "⚬" };
                    let dflt = if dev.is_default == Some(true) {
                        " [default]"
                    } else {
                        ""
                    };
                    lines.push(format!("  {} {}{}", icon, dev.name, dflt));
                }
                if devices.len() > 4 {
                    lines.push(format!("  ... and {} more", devices.len() - 4));
                }
                lines.join("\n")
            }
        } else {
            "Audio monitoring not available".to_string()
        };

        // Bluetooth
        self.bluetooth_info = if let Ok(monitor) = BluetoothMonitor::new() {
            let adapters = monitor.adapters();
            let devices = monitor.devices();
            if monitor.is_available() {
                let mut lines: Vec<String> = Vec::new();
                for a in adapters {
                    lines.push(format!(
                        "Adapter: {} ({})",
                        a.name,
                        if a.powered { "ON" } else { "OFF" }
                    ));
                }
                if devices.is_empty() {
                    lines.push("No paired devices".to_string());
                } else {
                    for dev in devices.iter().take(6) {
                        let state_str = match dev.state {
                            crate::bluetooth::BluetoothState::Connected => "connected",
                            crate::bluetooth::BluetoothState::Paired => "paired",
                            crate::bluetooth::BluetoothState::Discovered => "discovered",
                            crate::bluetooth::BluetoothState::Disconnected => "disconnected",
                        };
                        let name = dev.name.as_deref().unwrap_or("Unknown");
                        let batt = dev
                            .battery_percent
                            .map(|b| format!(" [{}%]", b))
                            .unwrap_or_default();
                        lines.push(format!("  {} ({}){}", name, state_str, batt));
                    }
                }
                lines.join("\n")
            } else {
                "Bluetooth not available".to_string()
            }
        } else {
            "Bluetooth monitoring not available".to_string()
        };

        // Displays
        self.display_info = if let Ok(monitor) = DisplayMonitor::new() {
            let displays = monitor.displays();
            if displays.is_empty() {
                "No displays detected".to_string()
            } else {
                let info: Vec<String> = displays
                    .iter()
                    .map(|d| {
                        let name = d.name.as_deref().unwrap_or("Display");
                        let conn = format!("{:?}", d.connection);
                        let primary = if d.is_primary { " [primary]" } else { "" };
                        let brightness = d
                            .brightness
                            .map(|b| format!(" | Brightness: {:.0}%", b * 100.0))
                            .unwrap_or_default();
                        let mfr = d
                            .manufacturer
                            .as_deref()
                            .map(|m| format!(" ({})", m))
                            .unwrap_or_default();
                        let mode = match (d.width, d.height) {
                            (Some(w), Some(h)) => format!("{w}x{h}"),
                            _ => "mode not read".to_string(),
                        };
                        let rate = match d.refresh_rate {
                            Some(hz) => format!(" @ {hz:.0}Hz"),
                            None => String::new(),
                        };
                        format!("{name}{mfr}: {mode}{rate} | {conn}{primary}{brightness}")
                    })
                    .collect();
                info.join("\n")
            }
        } else {
            "Display monitoring not available".to_string()
        };

        // USB
        self.usb_info = if let Ok(monitor) = UsbMonitor::new() {
            let devices = monitor.devices();
            if devices.is_empty() {
                "No USB devices detected".to_string()
            } else {
                let mut lines: Vec<String> = Vec::new();
                lines.push(format!("{} USB device(s) connected", devices.len()));
                for dev in devices.iter().take(8) {
                    let name = dev
                        .product
                        .as_deref()
                        .or(dev.description.as_deref())
                        .unwrap_or("USB Device");
                    // This pane already withheld the ids when both were zero,
                    // which is why it never showed [0000:0000]; the CLI did not.
                    let vid_pid = match (dev.vendor_id, dev.product_id) {
                        (None, None) => String::new(),
                        (v, p) => format!(
                            " [{}:{}]",
                            v.map_or("----".to_string(), |v| format!("{v:04X}")),
                            p.map_or("----".to_string(), |p| format!("{p:04X}"))
                        ),
                    };
                    let speed = format!("{:?}", dev.speed);
                    lines.push(format!("  {}{} ({})", name, vid_pid, speed));
                }
                if devices.len() > 8 {
                    lines.push(format!("  ... and {} more", devices.len() - 8));
                }
                lines.join("\n")
            }
        } else {
            "USB monitoring not available".to_string()
        };

        // Battery
        self.battery_info = if let Ok(monitor) = BatteryMonitor::new() {
            if monitor.has_battery() {
                let mut lines: Vec<String> = Vec::new();
                lines.push(format!(
                    "AC: {}",
                    match monitor.ac_connected() {
                        Some(true) => "Connected",
                        Some(false) => "Disconnected",
                        None => "not read",
                    }
                ));
                for bat in monitor.batteries() {
                    let state = format!("{:?}", bat.state);
                    // "0%" for an unread gauge would read as a flat battery.
                    let charge = match bat.charge_percent {
                        Some(p) => format!("{p:.0}%"),
                        None => "charge unread".to_string(),
                    };
                    lines.push(format!("{}: {} ({})", bat.name, charge, state));
                    if let Some(time) = &bat.time_to_empty {
                        lines.push(format!(
                            "  Time remaining: {}:{:02}",
                            time.as_secs() / 3600,
                            (time.as_secs() % 3600) / 60
                        ));
                    }
                    if let Some(health) = bat.wear_level_percent {
                        lines.push(format!("  Health: {:.0}%", health));
                    }
                }
                lines.join("\n")
            } else {
                format!(
                    "AC Power: {}",
                    match monitor.ac_connected() {
                        Some(true) => "Connected",
                        Some(false) => "Disconnected",
                        None => "Unknown",
                    }
                )
            }
        } else {
            "Battery monitoring not available".to_string()
        };
    }
}

/// Application state
pub struct App {
    /// Currently selected tab
    pub selected_tab: usize,
    /// Tab names
    pub tabs: Vec<&'static str>,
    /// CPU history (utilization percentages)
    pub cpu_history: VecDeque<u64>,
    /// Memory history (used percentage)
    pub memory_history: VecDeque<u64>,
    /// GPU histories (one per GPU) - kept for backward compatibility
    pub gpu_histories: Vec<VecDeque<u64>>,
    /// Accelerator histories (one per accelerator)
    pub accelerator_histories: Vec<VecDeque<u64>>,
    /// Network RX rate history
    pub network_rx_history: VecDeque<u64>,
    /// Network TX rate history
    pub network_tx_history: VecDeque<u64>,
    /// Current CPU info
    pub cpu_info: CpuInfo,
    /// Current memory info
    pub memory_info: MemoryInfo,
    /// GPU information (kept for backward compatibility)
    pub gpu_info: Vec<GpuInfo>,
    /// All accelerators (GPUs, NPUs, FPGAs, etc.)
    pub accelerators: Vec<AcceleratorInfo>,
    /// System information
    pub system_info: SystemInfo,
    /// Disk information
    pub disk_info: Vec<DiskInfo>,
    /// Network information
    pub network_info: NetworkInfo,
    /// Update interval
    pub update_interval: Duration,
    /// Last update time
    pub last_update: Instant,
    /// Scroll position for lists
    pub scroll_position: usize,
    /// GPU devices for monitoring
    /// Application configuration
    pub config: crate::config::Config,
    /// Status message to display (cleared after timeout)
    pub status_message: Option<(String, Instant)>,
    /// AI Agent for queries
    pub agent: Option<Agent>,
    /// Agent query input mode
    pub agent_input_mode: bool,
    /// Current agent query being typed
    pub agent_input: String,
    /// Agent response history
    pub agent_history: VecDeque<AgentResponse>,
    /// Agent loading state (true while a query is in flight on the worker)
    pub agent_loading: bool,
    /// Outbound queries to the agent worker thread. `None` until the worker
    /// has been spawned (after agent_init completes).
    agent_query_tx: Option<std::sync::mpsc::Sender<String>>,
    /// Inbound responses from the agent worker thread.
    agent_response_rx: Option<std::sync::mpsc::Receiver<Result<AgentResponse, String>>>,
    /// Monotonic counter incremented every time `agent_history` mutates.
    /// Used by the UI to invalidate its render cache.
    pub agent_history_version: u64,
    /// Process display mode - which device's processes to show
    pub process_display_mode: ProcessDisplayMode,
    /// Process monitor for tracking system and GPU processes
    process_monitor: Option<ProcessMonitor>,
    /// Cached processes from last update
    pub processes: Vec<ProcessMonitorInfo>,
    /// Cached sorted/filtered process indices for fast rendering
    cached_process_order: Vec<usize>,
    /// Cached filtered process count for scroll bounds
    pub filtered_process_count: usize,
    /// Background initialization state
    init_state: InitState,
    /// Receiver for background-initialized GPU devices
    /// Receiver for background-initialized agent
    agent_init_rx: Option<std::sync::mpsc::Receiver<Option<Agent>>>,
    /// Receiver for background-initialized process monitor
    process_init_rx: Option<std::sync::mpsc::Receiver<Option<ProcessMonitor>>>,
    /// Current view mode (Main, ProcessDetail, ThemeSelection)
    pub view_mode: ViewMode,
    /// Currently selected process index in the visible list
    pub selected_process_idx: usize,
    /// PID of the currently selected process (survives re-sorts)
    selected_process_pid: Option<u32>,
    /// Current color theme
    pub color_theme: ColorTheme,
    /// Selected theme index in theme picker
    pub selected_theme_idx: usize,
    /// Cached peripheral data (refreshed periodically, not every frame)
    pub peripheral_cache: PeripheralCache,
    /// Last time peripheral cache was refreshed
    peripheral_cache_last_refresh: Instant,
    /// Cached hardware profile snapshot (lazy on first view, refreshed on demand)
    pub profile_snapshot: Option<crate::profile::ProfileSnapshot>,
    /// Scroll position in the profiles tab
    pub profile_scroll: u16,
    /// Currently selected subsystem index in the profiles tab (0..5)
    pub profile_subsystem_idx: usize,
    /// Whether the deviation + audit overlay is showing
    pub profile_show_deviations: bool,
    /// Background channel for the profile snapshot preload. Drained by
    /// [`Self::check_background_init`].
    profile_init_rx: Option<std::sync::mpsc::Receiver<crate::profile::ProfileSnapshot>>,
    /// True once *any* load (bg or sync) has been attempted, to prevent
    /// per-frame respawn if the bg thread panicked. Cleared by `r` (refresh).
    profile_load_attempted: bool,
    /// True once a sync fallback has been tried this generation.
    profile_sync_attempted: bool,

    // === Snapshot pipeline ===
    /// Background collector. Owns every hardware handle; dropping this stops and
    /// joins the thread. Held so the collector lives exactly as long as the App.
    collector: Option<crate::pipeline::Collector>,
    /// Newest snapshot published by the collector. Refreshed by
    /// [`Self::sync_snapshot`]; all `update_*` methods read from this rather than
    /// calling blocking hardware APIs on the render thread.
    snapshot: std::sync::Arc<crate::pipeline::Snapshot>,
    /// Generation of the snapshot already folded into the display fields. Used to
    /// skip redundant re-mapping and redundant redraws.
    applied_generation: u64,
    /// Generation most recently drawn to the terminal, so repeated ticks with no new
    /// data do not trigger identical repaints.
    rendered_generation: u64,
}

/// Background initialization state
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum InitState {
    /// Still loading components
    #[default]
    Loading,
    /// All components ready
    Ready,
}

#[derive(Clone, Default)]
pub struct CpuInfo {
    pub name: String,
    pub cores: usize,
    pub threads: usize,
    pub utilization: f32,
    pub temperature: Option<f32>,
    pub frequency: Option<u64>,
    /// Per-core utilization, `None` for a core whose idle time was not read.
    ///
    /// Was `Vec<f32>`, filled with `100.0 - c.idle.unwrap_or(100.0)` -- so an
    /// unread core arrived as a measured 0% busy. The type could not say
    /// "not read", which is the fault the queued section of the handoff is
    /// about, and it had eight copies.
    pub per_core_usage: Vec<Option<f32>>,
}

#[derive(Clone, Default)]
pub struct MemoryInfo {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    /// Swap total in bytes, or `None` where the pagefile was not read.
    pub swap_total: Option<u64>,
    /// Swap in use in bytes, or `None` where the pagefile was not read.
    pub swap_used: Option<u64>,
}

/// Legacy GPU info struct - kept for backward compatibility
#[derive(Clone, Default)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    /// Utilization percentage, or `None` where the device reports no counter.
    pub utilization: Option<f32>,
    pub temperature: Option<f32>,
    pub power: Option<f32>,
    pub power_limit: Option<f32>,
    pub memory_total: Option<u64>,
    pub memory_used: Option<u64>,
    pub clock_graphics: Option<u32>,
    pub clock_memory: Option<u32>,
    /// Fan speed in RPM
    pub fan_speed_rpm: Option<u32>,
    /// Fan speed percentage (0-100)
    pub fan_speed_percent: Option<f32>,
    /// PCIe generation
    pub pcie_gen: Option<u32>,
    /// PCIe link width (e.g., 16 for x16)
    pub pcie_width: Option<u32>,
    /// Encoder utilization (0-100%)
    pub encoder_util: Option<f32>,
    /// Decoder utilization (0-100%)
    pub decoder_util: Option<f32>,
    /// Last time encoder was active (for auto-hide)
    pub encoder_last_active: Option<Instant>,
    /// Last time decoder was active (for auto-hide)
    pub decoder_last_active: Option<Instant>,
}

impl From<&GpuInfo> for AcceleratorInfo {
    fn from(gpu: &GpuInfo) -> Self {
        AcceleratorInfo {
            name: gpu.name.clone(),
            vendor: gpu.vendor.clone(),
            accel_type: AcceleratorType::Gpu,
            utilization: gpu.utilization,
            temperature: gpu.temperature,
            power: gpu.power,
            power_limit: gpu.power_limit,
            memory_total: gpu.memory_total,
            memory_used: gpu.memory_used,
            clock_core: gpu.clock_graphics,
            clock_memory: gpu.clock_memory,
            fan_speed_rpm: gpu.fan_speed_rpm,
            fan_speed_percent: gpu.fan_speed_percent,
            pcie_gen: gpu.pcie_gen,
            pcie_width: gpu.pcie_width,
            pcie_throughput: None,
            encoder_util: gpu.encoder_util,
            decoder_util: gpu.decoder_util,
            encoder_last_active: gpu.encoder_last_active,
            decoder_last_active: gpu.decoder_last_active,
            status: None,
            firmware_version: None,
            serial: None,
            pcie_slot: None,
        }
    }
}

impl From<&NpuInfo> for AcceleratorInfo {
    fn from(npu: &NpuInfo) -> Self {
        AcceleratorInfo {
            name: npu.name.clone(),
            vendor: npu.vendor.clone(),
            accel_type: AcceleratorType::Npu,
            utilization: npu.utilization.map(f32::from),
            temperature: None,
            power: npu.power_watts,
            power_limit: None,
            memory_total: None,
            memory_used: None,
            clock_core: npu.frequency_mhz,
            clock_memory: None,
            fan_speed_rpm: None,
            fan_speed_percent: None,
            pcie_gen: None,
            pcie_width: None,
            pcie_throughput: None,
            encoder_util: None,
            decoder_util: None,
            encoder_last_active: None,
            decoder_last_active: None,
            status: None,
            firmware_version: None,
            serial: None,
            pcie_slot: None,
        }
    }
}

#[derive(Clone, Default)]
pub struct SystemInfo {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub uptime: Duration,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Default)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total: u64,
    pub used: u64,
    pub filesystem: String,
    /// Read bytes per second
    /// Read throughput, or `None` where no rate was established.
    pub read_rate: Option<f64>,
    /// Write bytes per second
    /// Write throughput. See [`Self::read_rate`].
    pub write_rate: Option<f64>,
}

/// Network interface information for display
#[derive(Clone, Default)]
pub struct NetworkInterfaceInfo {
    /// Interface name (e.g., "eth0", "Ethernet")
    pub name: String,
    /// Is interface up
    pub is_up: bool,
    /// Total bytes received
    pub rx_bytes: u64,
    /// Total bytes transmitted
    pub tx_bytes: u64,
    /// Receive rate (bytes/sec)
    /// Receive rate, or `None` until a second sample establishes one.
    pub rx_rate: Option<f64>,
    /// Transmit rate (bytes/sec)
    /// Transmit rate. See [`Self::rx_rate`].
    pub tx_rate: Option<f64>,
    /// Link speed in Mbps (if available)
    pub speed_mbps: Option<u32>,
}

/// Aggregated network statistics
#[derive(Clone, Default)]
pub struct NetworkInfo {
    /// All network interfaces
    pub interfaces: Vec<NetworkInterfaceInfo>,
    /// Total RX rate across all interfaces (bytes/sec)
    pub total_rx_rate: Option<f64>,
    /// Total TX rate across all interfaces (bytes/sec)
    pub total_tx_rate: Option<f64>,
    /// Total bytes received
    pub total_rx_bytes: u64,
    /// Total bytes transmitted
    pub total_tx_bytes: u64,
}

impl App {
    /// Create a new application instance (blocking - full initialization)
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut app = Self::new_fast()?;
        // Wait for background init to complete
        while app.init_state != InitState::Ready {
            app.check_background_init();
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(app)
    }

    /// Create app with fast startup - slow components initialize in background
    pub fn new_fast() -> Result<Self, Box<dyn std::error::Error>> {
        use std::sync::mpsc;

        // Load config synchronously (fast - just file read)
        let config = crate::config::Config::load().unwrap_or_default();
        let update_interval = Duration::from_millis(config.general.update_interval_ms as u64);

        // Network is collected by the snapshot pipeline; the App no longer owns a
        // NetworkMonitor of its own.

        // GPUs are enumerated once by the snapshot pipeline's collector thread. The
        // TUI used to spawn its own enumeration here and keep a parallel
        // Vec<Box<dyn Device>>, so every vendor driver was initialized and queried
        // twice per refresh for the same numbers.

        // Spawn background thread for agent detection (potentially slow - network checks).
        // Strategy: prefer a local Ollama server at http://localhost:11434
        // — query /api/tags for an installed model and use it directly.
        // If Ollama isn't reachable, fall back to the generic auto_detect
        // path (OpenAI / Anthropic / GitHub / LM Studio / etc.).
        let (agent_tx, agent_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let agent = build_preferred_agent();
            let _ = agent_tx.send(agent);
        });

        // Spawn background thread for process monitor (can be slow on Windows)
        let (proc_tx, proc_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = proc_tx.send(ProcessMonitor::new().ok());
        });

        // Spawn background thread for hardware profile snapshot. This is the
        // slowest single subsystem on Windows (NVMe ioctls + DRS binary scan
        // can take several seconds), so we want it warming up before the
        // user ever opens the Profiles tab.
        let (profile_tx, profile_rx) = mpsc::channel();
        std::thread::spawn(move || {
            // catch_unwind: if any provider panics, drop the sender so the
            // receiver sees Disconnected and the UI can fall back to sync.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut inspector = crate::profile::cache::CachedProfileInspector::new();
                inspector.snapshot_all()
            }));
            if let Ok(snapshot) = result {
                let _ = profile_tx.send(snapshot);
            }
        });

        let mut app = Self {
            selected_tab: 0,
            tabs: vec![
                "Overview",
                "Processes",
                "CPU",
                "Accelerators",
                "Memory",
                "System",
                "Peripherals",
                "Profiles",
                "Agent",
            ],
            cpu_history: VecDeque::with_capacity(MAX_HISTORY),
            memory_history: VecDeque::with_capacity(MAX_HISTORY),
            gpu_histories: Vec::new(),
            accelerator_histories: Vec::new(),
            network_rx_history: VecDeque::with_capacity(MAX_HISTORY),
            network_tx_history: VecDeque::with_capacity(MAX_HISTORY),
            cpu_info: CpuInfo::default(),
            memory_info: MemoryInfo::default(),
            gpu_info: Vec::new(),
            accelerators: Vec::new(),
            system_info: SystemInfo::default(),
            disk_info: Vec::new(),
            network_info: NetworkInfo::default(),
            update_interval,
            last_update: Instant::now(),
            scroll_position: 0,
            config,
            status_message: Some(("Loading...".to_string(), Instant::now())),
            agent: None, // Will be populated from background thread
            agent_input_mode: false,
            agent_input: String::new(),
            agent_history: VecDeque::with_capacity(MAX_AGENT_HISTORY),
            agent_query_tx: None,
            agent_response_rx: None,
            agent_history_version: 0,
            agent_loading: false,
            process_display_mode: ProcessDisplayMode::default(),
            process_monitor: None, // Will be populated from background thread
            processes: Vec::new(),
            cached_process_order: Vec::new(),
            filtered_process_count: 0,
            init_state: InitState::Loading,
            agent_init_rx: Some(agent_rx),
            process_init_rx: Some(proc_rx),
            view_mode: ViewMode::default(),
            selected_process_idx: 0,
            selected_process_pid: None,
            color_theme: ColorTheme::default(),
            selected_theme_idx: 0,
            peripheral_cache: PeripheralCache::default(),
            peripheral_cache_last_refresh: Instant::now() - Duration::from_secs(60), // force initial refresh
            profile_snapshot: None,
            profile_scroll: 0,
            profile_subsystem_idx: 0,
            profile_show_deviations: false,
            profile_init_rx: Some(profile_rx),
            profile_load_attempted: true, // bg load started above
            profile_sync_attempted: false,
            collector: Some(crate::pipeline::Collector::spawn(
                crate::pipeline::CollectorConfig {
                    interval: update_interval,
                    history_size: MAX_HISTORY,
                    ..Default::default()
                },
            )),
            snapshot: std::sync::Arc::new(crate::pipeline::Snapshot::default()),
            applied_generation: 0,
            rendered_generation: 0,
        };

        // System info is static and not part of the per-tick snapshot.
        let _ = app.update_system();

        Ok(app)
    }

    /// Handle one key press in the main view. Returns `true` if the app should quit.
    ///
    /// Extracted from the interactive run loop so that the loop and the headless
    /// script driver share one implementation. While this lived inline in the loop,
    /// nothing could exercise it without a terminal: a driver would have had to
    /// reimplement the bindings and would have drifted from them silently, which is
    /// the same class of problem as three surfaces spelling one domain three ways.
    pub fn handle_main_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};
        match code {
            KeyCode::Char('q') => return true,
            KeyCode::Esc => return true,
            KeyCode::Tab => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    self.previous_tab();
                } else {
                    self.next_tab();
                }
            }
            KeyCode::BackTab => self.previous_tab(),
            // On the Profiles tab, 1–5 selects a subsystem
            // instead of switching tabs (matches the on-screen
            // "[1-5] subsystem" hint). Everywhere else, the
            // number row switches tabs.
            KeyCode::Char(c @ '1'..='5') if self.selected_tab == 7 => {
                let idx = (c as u8 - b'1') as usize;
                if idx < crate::profile::Subsystem::ALL.len() {
                    self.profile_subsystem_idx = idx;
                    self.profile_scroll = 0;
                }
            }
            KeyCode::Char('1') => self.set_tab(0),
            KeyCode::Char('2') => self.set_tab(1),
            KeyCode::Char('3') => self.set_tab(2),
            KeyCode::Char('4') => self.set_tab(3),
            KeyCode::Char('5') => self.set_tab(4),
            KeyCode::Char('6') => self.set_tab(5),
            KeyCode::Char('7') => self.set_tab(6),
            KeyCode::Char('8') => self.set_tab(7),
            KeyCode::Char('9') => self.set_tab(8),
            KeyCode::Left => self.previous_tab(),
            KeyCode::Right => self.next_tab(),
            KeyCode::Up => self.select_process_up(),
            KeyCode::Down => self.select_process_down(),
            KeyCode::PageUp => {
                if self.selected_tab == 7 {
                    self.profile_scroll = self.profile_scroll.saturating_sub(10);
                } else {
                    self.scroll_page_up();
                }
            }
            KeyCode::PageDown => {
                if self.selected_tab == 7 {
                    self.profile_scroll = self.profile_scroll.saturating_add(10);
                } else {
                    self.scroll_page_down();
                }
            }
            KeyCode::Home => self.scroll_to_top(),
            KeyCode::End => self.scroll_to_bottom(),
            KeyCode::Enter => self.open_process_detail(),
            KeyCode::Char('p') | KeyCode::Char('P') => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    self.previous_process_mode();
                } else {
                    self.next_process_mode();
                }
            }
            KeyCode::Char('t') | KeyCode::Char('T') => self.open_theme_picker(),
            KeyCode::Char('r') => {
                if self.selected_tab == 7 {
                    self.refresh_profile_snapshot();
                } else {
                    self.reset_stats();
                }
            }
            KeyCode::Char('[') => {
                if self.selected_tab == 7 {
                    self.profile_prev_subsystem();
                }
            }
            KeyCode::Char(']') => {
                if self.selected_tab == 7 {
                    self.profile_next_subsystem();
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if self.selected_tab == 7 {
                    self.profile_show_deviations = !self.profile_show_deviations;
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A') => self.toggle_agent_input(),
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if self.selected_tab == 8 {
                    self.clear_agent_history();
                }
            }
            KeyCode::F(12) => {
                if let Err(e) = self.save_config() {
                    self.set_status_message(format!("Failed to save config: {}", e));
                }
            }
            _ => {}
        }
        false
    }
    /// Whether new collector data has arrived since the last frame was drawn.
    ///
    /// Consuming: calling this marks the current generation as rendered. The render
    /// loop uses it to avoid repainting identical frames.
    pub fn snapshot_changed_since_render(&mut self) -> bool {
        if self.applied_generation != self.rendered_generation {
            self.rendered_generation = self.applied_generation;
            true
        } else {
            false
        }
    }

    /// Pull the newest snapshot and fold it into the display fields.
    ///
    /// Returns `true` when a *new* generation was applied. The render loop uses this
    /// to skip redraws entirely when the collector has not published since the last
    /// frame — previously every tick forced a redraw whether or not anything changed.
    ///
    /// This never blocks: it is an atomic load plus a pure in-memory mapping. All
    /// hardware access happens on the collector thread.
    pub fn sync_snapshot(&mut self) -> bool {
        let Some(ref collector) = self.collector else {
            return false;
        };
        let handle = collector.handle();

        // Generation 0 is the not-ready placeholder published before the first tick.
        let generation = handle.generation();
        if generation == 0 || generation == self.applied_generation {
            return false;
        }

        self.snapshot = handle.latest();
        self.applied_generation = self.snapshot.generation;

        let _ = self.update_cpu();
        let _ = self.update_memory();
        let _ = self.update_network();
        let _ = self.update_disks();

        true
    }

    /// Check and apply background initialization results
    pub fn check_background_init(&mut self) {
        let mut all_done = true;

        // GPU data arrives with the collector snapshot; there is no separate
        // initialization to wait on.

        // Check agent init. When agent arrives, hand it to a dedicated
        // worker thread that owns the agent + its own monitor and processes
        // queries off the UI thread — agent.ask() can block for tens of
        // seconds on a remote LLM, which used to freeze the TUI.
        if let Some(ref rx) = self.agent_init_rx {
            match rx.try_recv() {
                Ok(Some(agent)) => {
                    self.agent_init_rx = None;
                    self.spawn_agent_worker(agent);
                }
                Ok(None) => {
                    self.agent_init_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    all_done = false;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.agent_init_rx = None;
                }
            }
        }

        // Drain agent responses produced by the worker. Collect first to
        // drop the immutable borrow of self.agent_response_rx before
        // mutating other fields (status messages, history).
        let drained: Vec<Result<AgentResponse, String>> = self
            .agent_response_rx
            .as_ref()
            .map(|rx| {
                let mut out = Vec::new();
                while let Ok(r) = rx.try_recv() {
                    out.push(r);
                }
                out
            })
            .unwrap_or_default();
        for result in drained {
            self.agent_loading = false;
            match result {
                Ok(response) => {
                    self.agent_history.push_back(response);
                    if self.agent_history.len() > MAX_AGENT_HISTORY {
                        self.agent_history.pop_front();
                    }
                    self.agent_history_version = self.agent_history_version.wrapping_add(1);
                }
                Err(e) => {
                    self.set_status_message(format!("Agent error: {}", e));
                }
            }
        }

        // Check process monitor init
        if let Some(ref rx) = self.process_init_rx {
            match rx.try_recv() {
                Ok(monitor) => {
                    self.process_monitor = monitor;
                    self.process_init_rx = None;
                    // Trigger process update
                    let _ = self.update_processes();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    all_done = false;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.process_init_rx = None;
                }
            }
        }

        // Drain profile snapshot bg load. Do NOT gate `Ready` on this — the
        // Profiles tab is opt-in (the user has to switch to it), and the
        // load can take several seconds. Letting the rest of the UI come up
        // immediately is the whole point.
        if let Some(rx) = self.profile_init_rx.take() {
            match rx.try_recv() {
                Ok(snapshot) => {
                    self.profile_snapshot = Some(snapshot);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.profile_init_rx = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Background thread panicked / dropped sender. Leave
                    // snapshot=None so a later sync fallback can fire.
                }
            }
        }

        // Update state when all done
        if all_done
            && self.agent_init_rx.is_none()
            && self.process_init_rx.is_none()
            && self.init_state != InitState::Ready
        {
            self.init_state = InitState::Ready;
            self.status_message = None; // Clear "Loading..." message
        }
    }

    /// Check if app is still initializing
    pub fn is_loading(&self) -> bool {
        self.init_state == InitState::Loading
    }

    /// Query NPU info from the platform-specific silicon monitor.
    fn query_npus() -> std::result::Result<Vec<NpuInfo>, Box<dyn std::error::Error>> {
        use crate::silicon::SiliconMonitor as SiliconMonitorTrait;

        #[cfg(target_os = "linux")]
        {
            let monitor = crate::silicon::linux::LinuxSiliconMonitor::new()?;
            Ok(monitor.npu_info()?)
        }

        #[cfg(target_os = "windows")]
        {
            let monitor = crate::silicon::windows::WindowsSiliconMonitor::new()?;
            Ok(monitor.npu_info()?)
        }

        #[cfg(target_os = "macos")]
        {
            #[cfg(feature = "apple")]
            {
                let monitor = crate::silicon::apple::AppleSiliconMonitor::new()?;
                Ok(monitor.npu_info()?)
            }
            #[cfg(not(feature = "apple"))]
            {
                Ok(Vec::new())
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            Ok(Vec::new())
        }
    }

    /// Update all monitoring data (legacy - calls both fast and slow)
    pub fn update(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.update_fast()?;
        self.update_slow()?;
        Ok(())
    }

    /// Fast updates - CPU, GPU, Memory, Network, Disks.
    ///
    /// Performs no hardware I/O: this is an atomic snapshot load plus an in-memory
    /// mapping, so it cannot stall a frame regardless of how slow a driver is.
    pub fn update_fast(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.sync_snapshot();
        self.update_gpu()?;
        self.last_update = Instant::now();
        Ok(())
    }

    /// Slow updates - system info and peripherals.
    ///
    /// These are not part of the per-tick snapshot: system info is effectively static,
    /// and the peripheral cache has its own background refresh.
    pub fn update_slow(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.update_system()?;
        self.update_processes()?;
        // Refresh peripherals every 10 seconds (they're expensive due to subprocess calls)
        if self.peripheral_cache_last_refresh.elapsed() >= Duration::from_secs(10) {
            self.peripheral_cache.refresh();
            self.peripheral_cache_last_refresh = Instant::now();
        }
        Ok(())
    }

    /// Update processes only (called every 1s)
    pub fn update_processes_only(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.update_processes()
    }

    /// Update disks only (called every 5s - expensive)
    pub fn update_disks_only(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.update_disks()
    }

    fn update_cpu(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Sourced from the collector thread's snapshot rather than a blocking
        // platform call on the render thread. The three per-platform branches that
        // used to live here collapsed into one mapping, because the snapshot
        // already carries a platform-normalized `CpuStats`.
        let Some(stats) = self.snapshot.cpu.as_ref() else {
            // No sample yet (before the first tick). Leave the previous frame's
            // values in place rather than flashing a synthetic zero.
            return Ok(());
        };

        let num_cpus = stats.cores.len();

        self.cpu_info = CpuInfo {
            name: stats
                .cores
                .first()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "CPU".to_string()),
            cores: num_cpus,
            threads: num_cpus,
            utilization: 100.0 - stats.total.idle,
            temperature: None, // Requires admin for WMI thermal zone access
            frequency: stats
                .cores
                .first()
                .and_then(|c| c.frequency.as_ref().and_then(|f| f.current))
                .map(u64::from),
            per_core_usage: stats
                .cores
                .iter()
                .map(|c| c.idle.map(|i| 100.0 - i))
                .collect(),
        };

        self.cpu_history.push_back(self.cpu_info.utilization as u64);
        if self.cpu_history.len() > MAX_HISTORY {
            self.cpu_history.pop_front();
        }

        Ok(())
    }

    fn update_memory(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Sourced from the collector snapshot. The previous fallback branch
        // fabricated a plausible-looking 32 GB/16 GB reading on platforms without a
        // memory collector, which is worse than showing nothing: invented values are
        // indistinguishable from measured ones on screen. An unavailable sample now
        // leaves the panel empty instead.
        let Some(stats) = self.snapshot.memory.as_ref() else {
            return Ok(());
        };

        // Platform collectors report KB; the display layer works in bytes.
        self.memory_info = MemoryInfo {
            total: stats.ram.total * 1024,
            used: stats.ram.used * 1024,
            available: stats.ram.free * 1024,
            swap_total: stats.swap.total.map(|v| v * 1024),
            swap_used: stats.swap.used.map(|v| v * 1024),
        };

        let used_percent = (self.memory_info.used * 100)
            .checked_div(self.memory_info.total)
            .unwrap_or(0);
        self.memory_history.push_back(used_percent);
        if self.memory_history.len() > MAX_HISTORY {
            self.memory_history.pop_front();
        }

        Ok(())
    }

    fn update_gpu(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Sourced from the collector snapshot.
        //
        // The TUI previously kept its own `Vec<Box<dyn Device>>` and queried every
        // vendor driver again here, so each refresh cycle paid for GPU enumeration
        // twice — once on the collector thread and once on the render thread. The
        // snapshot already carries everything this panel displays.
        let snapshot = std::sync::Arc::clone(&self.snapshot);
        if snapshot.gpu_static.is_empty() {
            return Ok(());
        }

        // Retain the previous frame so encoder/decoder activity timestamps survive an
        // idle tick. The old code read `gpu_info.get(gpu_info.len())`, which indexes
        // one past the end and is therefore always `None` — so a timestamp was never
        // actually preserved and the "recently active" indicator reset instantly.
        let previous = std::mem::take(&mut self.gpu_info);
        let now = Instant::now();

        for (idx, static_info) in snapshot.gpu_static.iter().enumerate() {
            let prev = previous.get(idx);
            let dynamic = snapshot.gpu_dynamic.get(idx).and_then(|d| d.as_ref());

            let Some(dynamic) = dynamic else {
                // This device failed its query this tick. Keep the previous reading
                // rather than dropping the GPU out of the list, which would renumber
                // every panel after it.
                if let Some(prev) = prev {
                    self.gpu_info.push(prev.clone());
                }
                continue;
            };

            let encoder_util = dynamic.engines.encoder.map(|e| e as f32);
            let decoder_util = dynamic.engines.decoder.map(|d| d as f32);

            let encoder_last_active = match encoder_util {
                Some(util) if util > 0.0 => Some(now),
                _ => prev.and_then(|p| p.encoder_last_active),
            };
            let decoder_last_active = match decoder_util {
                Some(util) if util > 0.0 => Some(now),
                _ => prev.and_then(|p| p.decoder_last_active),
            };

            self.gpu_info.push(GpuInfo {
                name: static_info.name.clone(),
                vendor: format!("{}", static_info.vendor),
                utilization: dynamic.utilization.map(f32::from),
                temperature: dynamic.thermal.temperature.map(|t| t as f32),
                // Power is reported in milliwatts; the panel displays watts.
                power: dynamic
                    .power
                    .draw
                    .filter(|d| *d > 0)
                    .map(|d| d as f32 / 1000.0),
                power_limit: dynamic
                    .power
                    .limit
                    .filter(|l| *l > 0)
                    .map(|l| l as f32 / 1000.0),
                memory_total: dynamic.memory.total,
                memory_used: dynamic.memory.used,
                clock_graphics: dynamic.clocks.graphics,
                clock_memory: dynamic.clocks.memory,
                fan_speed_rpm: dynamic.thermal.fan_rpm,
                fan_speed_percent: dynamic.thermal.fan_speed.map(|f| f as f32),
                pcie_gen: dynamic.pcie.current_gen.map(|g| g as u32),
                pcie_width: dynamic.pcie.current_width.map(|w| w as u32),
                encoder_util,
                decoder_util,
                encoder_last_active,
                decoder_last_active,
            });
        }

        // Update GPU histories
        while self.gpu_histories.len() < self.gpu_info.len() {
            self.gpu_histories
                .push(VecDeque::with_capacity(MAX_HISTORY));
        }

        for (i, gpu) in self.gpu_info.iter().enumerate() {
            // No reading, no sample: the sparkline holds its last value
            // rather than dropping to a zero nobody measured.
            if let Some(util) = gpu.utilization {
                self.gpu_histories[i].push_back(util as u64);
                if self.gpu_histories[i].len() > MAX_HISTORY {
                    self.gpu_histories[i].pop_front();
                }
            }
        }

        // Update unified accelerators list from GPU info + NPUs
        self.accelerators = self.gpu_info.iter().map(AcceleratorInfo::from).collect();

        // Append NPU accelerators from platform-specific silicon monitor
        if let Ok(npus) = Self::query_npus() {
            self.accelerators
                .extend(npus.iter().map(AcceleratorInfo::from));
        }

        // Update accelerator histories
        while self.accelerator_histories.len() < self.accelerators.len() {
            self.accelerator_histories
                .push(VecDeque::with_capacity(MAX_HISTORY));
        }

        for (i, accel) in self.accelerators.iter().enumerate() {
            // Nothing to plot for a device that reports no utilization.
            if let Some(u) = accel.utilization {
                self.accelerator_histories[i].push_back(u as u64);
            }
            if self.accelerator_histories[i].len() > MAX_HISTORY {
                self.accelerator_histories[i].pop_front();
            }
        }

        Ok(())
    }

    fn update_system(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Get basic system info

        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string());

        let os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);

        // Windows publishes the running build in the registry, revision included.
        // This line used to be the constant "Windows NT", which is not a version of
        // anything — it named the kernel family and was displayed where the version
        // belongs, so a machine on 10.0.26200.8875 reported the same string as one
        // from 1993.
        #[cfg(target_os = "windows")]
        let kernel = windows_kernel_version().unwrap_or_else(|| "Windows NT".to_string());

        // Linux and macOS both answer `uname -r`; they had separate, byte-identical
        // branches here.
        #[cfg(not(target_os = "windows"))]
        let kernel = std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Unknown".to_string());

        // Get uptime - platform-specific
        #[cfg(target_os = "windows")]
        let uptime = crate::platform::windows::get_system_uptime();

        #[cfg(target_os = "linux")]
        let uptime = std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| s.split_whitespace().next().map(|s| s.to_string()))
            .and_then(|s| s.parse::<f64>().ok())
            .map(|secs| Duration::from_secs(secs as u64))
            .unwrap_or(Duration::from_secs(0));

        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        let uptime = Duration::from_secs(0);

        // Get motherboard info if available
        #[cfg(target_os = "windows")]
        let (manufacturer, model) = crate::platform::windows::detect_platform()
            .map(|p| (p.hardware.module.clone(), Some(p.hardware.model.clone())))
            .unwrap_or((None, None));

        #[cfg(not(target_os = "windows"))]
        let (manufacturer, model): (Option<String>, Option<String>) = (None, None);

        self.system_info = SystemInfo {
            hostname,
            os,
            kernel,
            uptime,
            manufacturer,
            model,
        };

        Ok(())
    }

    fn update_disks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Sourced from the collector snapshot, which re-enumerates on a slow cadence
        // (disk topology rarely changes) and, on Windows, uses the GetLogicalDrives
        // bitmask rather than probing all 26 letters with `fs::metadata` — the old
        // path blocked on disconnected network drives.
        //
        // Hold the previous rows until the first enumeration lands, so the panel does
        // not flash "No disks detected" during startup.
        if self.snapshot.disks.is_empty() && !self.disk_info.is_empty() {
            return Ok(());
        }

        self.disk_info.clear();
        self.disk_info
            .extend(self.snapshot.disks.iter().map(|d| DiskInfo {
                name: d.name.clone(),
                mount_point: d.mount_point.clone(),
                total: d.total,
                used: d.used,
                filesystem: d.filesystem.clone(),
                read_rate: d.read_rate,
                write_rate: d.write_rate,
            }));

        if self.disk_info.is_empty() {
            self.disk_info.push(DiskInfo {
                name: "No disks detected".to_string(),
                mount_point: "N/A".to_string(),
                total: 0,
                used: 0,
                filesystem: "N/A".to_string(),
                // A placeholder row for a disk with no readings, so the rates
                // are absent rather than a throughput of zero.
                read_rate: None,
                write_rate: None,
            });
        }

        Ok(())
    }

    fn update_network(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Sourced from the collector snapshot; interface filtering and rate
        // computation happen on the collector thread.
        if self.snapshot.network.is_empty() && !self.network_info.interfaces.is_empty() {
            return Ok(());
        }

        let interfaces: Vec<NetworkInterfaceInfo> = self
            .snapshot
            .network
            .iter()
            .map(|n| NetworkInterfaceInfo {
                name: n.name.clone(),
                is_up: n.is_up,
                rx_bytes: n.rx_bytes,
                tx_bytes: n.tx_bytes,
                rx_rate: n.rx_rate,
                tx_rate: n.tx_rate,
                speed_mbps: n.speed_mbps,
            })
            .collect();

        let total_rx_rate = self.snapshot.total_rx_rate();
        let total_tx_rate = self.snapshot.total_tx_rate();

        self.network_info = NetworkInfo {
            total_rx_bytes: interfaces.iter().map(|i| i.rx_bytes).sum(),
            total_tx_bytes: interfaces.iter().map(|i| i.tx_bytes).sum(),
            interfaces,
            total_rx_rate,
            total_tx_rate,
        };

        // History is stored in KB/s for a readable vertical scale. A tick
        // with no established rate is skipped rather than pushed as zero: the
        // graph would otherwise open with a trough that never happened.
        if let Some(rx) = total_rx_rate {
            self.network_rx_history.push_back((rx / 1024.0) as u64);
        }
        if let Some(tx) = total_tx_rate {
            self.network_tx_history.push_back((tx / 1024.0) as u64);
        }
        if self.network_rx_history.len() > MAX_HISTORY {
            self.network_rx_history.pop_front();
        }
        if self.network_tx_history.len() > MAX_HISTORY {
            self.network_tx_history.pop_front();
        }

        Ok(())
    }

    fn update_processes(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Sourced from the collector snapshot. Process enumeration is the most
        // expensive collector on Windows, so it must never run on the render thread.
        if self.snapshot.processes.is_empty() && !self.processes.is_empty() {
            return Ok(());
        }

        self.processes = self.snapshot.processes.clone();
        self.rebuild_cached_process_order();
        Ok(())
    }

    /// Rebuild the cached process order based on current display mode
    /// This is called when processes are updated or display mode changes
    fn rebuild_cached_process_order(&mut self) {
        use ProcessDisplayMode::*;

        // Build a list of (index, sort_key) tuples, then sort and extract indices
        let mut indexed: Vec<(usize, (u64, u64))> = Vec::with_capacity(self.processes.len());

        match self.process_display_mode {
            All | Cpu => {
                // Check if we have CPU data
                let has_cpu_data = self.processes.iter().any(|p| p.cpu_percent > 0.1);

                for (i, p) in self.processes.iter().enumerate() {
                    if self.process_display_mode == Cpu && !has_cpu_data {
                        // No CPU data, just use memory
                        indexed.push((i, (p.memory_bytes, 0)));
                    } else if self.process_display_mode == Cpu && p.cpu_percent <= 0.1 {
                        // Skip processes with no CPU usage in CPU mode
                        continue;
                    } else {
                        // For All mode: Boost GPU processes to show them first
                        // GPU processes get a large bonus in the sort key
                        let gpu_boost: u64 = if !p.gpu_indices.is_empty() {
                            1_000_000_000 // 1B bonus for GPU processes
                        } else {
                            0
                        };
                        // CPU percent as secondary key (scaled to u64 for comparison)
                        let cpu_key = (p.cpu_percent * 1000.0) as u64;
                        indexed.push((i, (gpu_boost + cpu_key, p.memory_bytes)));
                    }
                }
                // Sort descending - GPU processes first (due to boost), then by CPU
                indexed.sort_by_key(|p| std::cmp::Reverse(p.1));
            }
            Gpu(gpu_idx) | Accelerator(gpu_idx) => {
                for (i, p) in self.processes.iter().enumerate() {
                    if p.gpu_indices.contains(&gpu_idx) {
                        let mem = *p.gpu_memory_per_device.get(&gpu_idx).unwrap_or(&0);
                        indexed.push((i, (mem, 0)));
                    }
                }
                // Sort descending by GPU memory
                indexed.sort_by_key(|p| std::cmp::Reverse(p.1));
            }
            Npu(_) => {
                // No NPU support yet
            }
        }

        self.cached_process_order = indexed.into_iter().map(|(i, _)| i).collect();
        self.filtered_process_count = self.cached_process_order.len();

        // Restore selection to the same PID after re-sort to prevent jitter
        if self.filtered_process_count > 0 {
            if let Some(pid) = self.selected_process_pid {
                // Find where the previously selected PID ended up in the new order
                if let Some(new_idx) = self
                    .cached_process_order
                    .iter()
                    .position(|&i| self.processes.get(i).map(|p| p.pid) == Some(pid))
                {
                    self.selected_process_idx = new_idx;
                } else {
                    // PID disappeared (process exited), clamp to valid range
                    self.selected_process_idx = self
                        .selected_process_idx
                        .min(self.filtered_process_count - 1);
                    // Update tracked PID to whatever is now at this index
                    self.selected_process_pid = self
                        .cached_process_order
                        .get(self.selected_process_idx)
                        .and_then(|&i| self.processes.get(i))
                        .map(|p| p.pid);
                }
            } else {
                self.selected_process_idx = self
                    .selected_process_idx
                    .min(self.filtered_process_count - 1);
                self.selected_process_pid = self
                    .cached_process_order
                    .get(self.selected_process_idx)
                    .and_then(|&i| self.processes.get(i))
                    .map(|p| p.pid);
            }

            // Ensure scroll position keeps selected item visible
            let visible_rows = 25;
            if self.selected_process_idx < self.scroll_position {
                self.scroll_position = self.selected_process_idx;
            } else if self.selected_process_idx >= self.scroll_position + visible_rows {
                self.scroll_position = self.selected_process_idx.saturating_sub(visible_rows - 1);
            }
            // Clamp scroll position
            let max_scroll = self.filtered_process_count.saturating_sub(visible_rows);
            self.scroll_position = self.scroll_position.min(max_scroll);
        } else {
            self.selected_process_idx = 0;
            self.selected_process_pid = None;
            self.scroll_position = 0;
        }
    }

    /// Get processes using cached order (fast - no sorting during render)
    /// Use this during UI rendering for smooth scrolling
    pub fn get_processes_cached(&self) -> Vec<&ProcessMonitorInfo> {
        self.cached_process_order
            .iter()
            .filter_map(|&i| self.processes.get(i))
            .collect()
    }

    /// Get a slice of cached processes for rendering (with pagination)
    /// This is the fastest method for rendering visible processes
    pub fn get_visible_processes(&self, skip: usize, take: usize) -> Vec<&ProcessMonitorInfo> {
        self.cached_process_order
            .iter()
            .skip(skip)
            .take(take)
            .filter_map(|&i| self.processes.get(i))
            .collect()
    }

    /// Get filtered processes based on current display mode
    pub fn get_filtered_processes(&self) -> Vec<&ProcessMonitorInfo> {
        use ProcessDisplayMode::*;

        match self.process_display_mode {
            All => {
                // Show all processes, sorted by CPU usage then memory
                let mut procs: Vec<&ProcessMonitorInfo> = self.processes.iter().collect();
                procs.sort_by(|a, b| {
                    // First compare by CPU, then by memory if CPU is equal
                    match b
                        .cpu_percent
                        .partial_cmp(&a.cpu_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                    {
                        std::cmp::Ordering::Equal => b.memory_bytes.cmp(&a.memory_bytes),
                        other => other,
                    }
                });
                procs
            }
            Cpu => {
                // Show top CPU consumers (or all processes sorted by memory if CPU data unavailable)
                let mut procs: Vec<&ProcessMonitorInfo> = self.processes.iter().collect();

                // Check if we have valid CPU data (at least one process with cpu_percent > 0)
                let has_cpu_data = procs.iter().any(|p| p.cpu_percent > 0.1);

                if has_cpu_data {
                    // Filter to processes actually using CPU
                    procs.retain(|p| p.cpu_percent > 0.1);
                    procs.sort_by(|a, b| {
                        b.cpu_percent
                            .partial_cmp(&a.cpu_percent)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                } else {
                    // No CPU data available (e.g., on Windows), sort by memory instead
                    procs.sort_by_key(|p| std::cmp::Reverse(p.memory_bytes));
                }
                procs
            }
            Gpu(gpu_idx) => {
                // Show processes using this specific GPU
                let mut procs: Vec<&ProcessMonitorInfo> = self
                    .processes
                    .iter()
                    .filter(|p| p.gpu_indices.contains(&gpu_idx))
                    .collect();
                procs.sort_by(|a, b| {
                    let a_mem = a.gpu_memory_per_device.get(&gpu_idx).unwrap_or(&0);
                    let b_mem = b.gpu_memory_per_device.get(&gpu_idx).unwrap_or(&0);
                    b_mem.cmp(a_mem)
                });
                procs
            }
            Npu(npu_idx) => {
                // Filter processes using the nth NPU device
                // NPU processes are tracked via gpu_indices when the accelerator
                // type is NPU. Find the global accelerator index for this NPU.
                let npu_accel_indices: Vec<usize> = self
                    .accelerators
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.accel_type == AcceleratorType::Npu)
                    .map(|(i, _)| i)
                    .collect();

                if let Some(&global_idx) = npu_accel_indices.get(npu_idx) {
                    let mut procs: Vec<&ProcessMonitorInfo> = self
                        .processes
                        .iter()
                        .filter(|p| p.gpu_indices.contains(&global_idx))
                        .collect();
                    procs.sort_by(|a, b| {
                        let a_mem = a.gpu_memory_per_device.get(&global_idx).unwrap_or(&0);
                        let b_mem = b.gpu_memory_per_device.get(&global_idx).unwrap_or(&0);
                        b_mem.cmp(a_mem)
                    });
                    procs
                } else {
                    Vec::new()
                }
            }
            Accelerator(accel_idx) => {
                // Show processes using this specific accelerator (GPU-based for now)
                let mut procs: Vec<&ProcessMonitorInfo> = self
                    .processes
                    .iter()
                    .filter(|p| p.gpu_indices.contains(&accel_idx))
                    .collect();
                procs.sort_by(|a, b| {
                    let a_mem = a.gpu_memory_per_device.get(&accel_idx).unwrap_or(&0);
                    let b_mem = b.gpu_memory_per_device.get(&accel_idx).unwrap_or(&0);
                    b_mem.cmp(a_mem)
                });
                procs
            }
        }
    }

    pub fn set_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.selected_tab = index;
            self.scroll_position = 0;
            if self.selected_tab == 7 {
                self.ensure_profile_snapshot();
            }
        }
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = (self.selected_tab + 1) % self.tabs.len();
        self.scroll_position = 0;
        if self.selected_tab == 7 {
            self.ensure_profile_snapshot();
        }
    }

    pub fn previous_tab(&mut self) {
        if self.selected_tab > 0 {
            self.selected_tab -= 1;
        } else {
            self.selected_tab = self.tabs.len() - 1;
        }
        self.scroll_position = 0;
        if self.selected_tab == 7 {
            self.ensure_profile_snapshot();
        }
    }

    /// Force-refresh the cached profile snapshot. Spawns a background
    /// thread (bound to `r`). Drops any in-flight bg load.
    pub fn refresh_profile_snapshot(&mut self) {
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel();
        self.profile_init_rx = Some(rx);
        self.profile_load_attempted = true;
        self.profile_sync_attempted = false;
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut inspector = crate::profile::cache::CachedProfileInspector::new();
                inspector.invalidate(None);
                inspector.snapshot_all()
            }));
            if let Ok(snapshot) = result {
                let _ = tx.send(snapshot);
            }
        });
        self.profile_scroll = 0;
    }

    /// Called when the user opens the Profiles tab. If the bg preload is
    /// still in flight, do nothing — the UI will show "Loading…" until it
    /// arrives. If the bg load already finished, the snapshot is present.
    /// Only fall back to a synchronous load when (a) no snapshot exists,
    /// (b) no bg load is in flight, and (c) we haven't already tried sync
    /// for this generation — this guarantees we never block the UI more
    /// than once per refresh.
    pub fn ensure_profile_snapshot(&mut self) {
        if self.profile_snapshot.is_some() {
            return;
        }
        if self.profile_init_rx.is_some() {
            return; // bg load still in flight
        }
        if self.profile_sync_attempted {
            return; // already tried sync this generation
        }
        self.profile_sync_attempted = true;
        let mut inspector = crate::profile::cache::CachedProfileInspector::new();
        self.profile_snapshot = Some(inspector.snapshot_all());
        self.profile_scroll = 0;
    }

    pub fn profile_next_subsystem(&mut self) {
        let n = crate::profile::Subsystem::ALL.len();
        self.profile_subsystem_idx = (self.profile_subsystem_idx + 1) % n;
        self.profile_scroll = 0;
    }

    pub fn profile_prev_subsystem(&mut self) {
        let n = crate::profile::Subsystem::ALL.len();
        self.profile_subsystem_idx = (self.profile_subsystem_idx + n - 1) % n;
        self.profile_scroll = 0;
    }

    pub fn scroll_up(&mut self) {
        self.scroll_position = self.scroll_position.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        // Use cached process count to avoid expensive recalculation
        let visible_rows = 25; // Number of processes shown in UI
        let max_scroll = self.filtered_process_count.saturating_sub(visible_rows);
        if self.scroll_position < max_scroll {
            self.scroll_position = self.scroll_position.saturating_add(1);
        }
    }

    /// Get the current scroll position
    pub fn get_scroll_position(&self) -> usize {
        self.scroll_position
    }

    /// Scroll up by one page (25 rows)
    pub fn scroll_page_up(&mut self) {
        self.scroll_position = self.scroll_position.saturating_sub(25);
    }

    /// Scroll down by one page (25 rows)
    pub fn scroll_page_down(&mut self) {
        let visible_rows = 25;
        let max_scroll = self.filtered_process_count.saturating_sub(visible_rows);
        self.scroll_position = (self.scroll_position + 25).min(max_scroll);
    }

    /// Scroll to the top of the process list
    pub fn scroll_to_top(&mut self) {
        self.scroll_position = 0;
    }

    /// Scroll to the bottom of the process list
    pub fn scroll_to_bottom(&mut self) {
        let visible_rows = 25;
        self.scroll_position = self.filtered_process_count.saturating_sub(visible_rows);
    }

    /// Move process selection up
    pub fn select_process_up(&mut self) {
        if self.selected_process_idx > 0 {
            self.selected_process_idx -= 1;
            if self.selected_process_idx < self.scroll_position {
                self.scroll_position = self.selected_process_idx;
            }
            // Track selected PID for stability across re-sorts
            self.selected_process_pid = self
                .cached_process_order
                .get(self.selected_process_idx)
                .and_then(|&i| self.processes.get(i))
                .map(|p| p.pid);
        }
    }

    /// Move process selection down
    pub fn select_process_down(&mut self) {
        let max_idx = self.filtered_process_count.saturating_sub(1);
        if self.selected_process_idx < max_idx {
            self.selected_process_idx += 1;
            let visible_rows = 25;
            if self.selected_process_idx >= self.scroll_position + visible_rows {
                self.scroll_position = self.selected_process_idx.saturating_sub(visible_rows - 1);
            }
            // Track selected PID for stability across re-sorts
            self.selected_process_pid = self
                .cached_process_order
                .get(self.selected_process_idx)
                .and_then(|&i| self.processes.get(i))
                .map(|p| p.pid);
        }
    }

    /// Get the currently selected process
    pub fn get_selected_process(&self) -> Option<&crate::ProcessMonitorInfo> {
        self.cached_process_order
            .get(self.selected_process_idx)
            .and_then(|&idx| self.processes.get(idx))
    }

    /// Open process detail view
    pub fn open_process_detail(&mut self) {
        if self.get_selected_process().is_some() {
            self.view_mode = ViewMode::ProcessDetail;
        }
    }

    /// Close overlay and return to main
    pub fn close_overlay(&mut self) {
        self.view_mode = ViewMode::Main;
    }

    /// Open theme picker
    pub fn open_theme_picker(&mut self) {
        self.selected_theme_idx = ColorTheme::all()
            .iter()
            .position(|t| *t == self.color_theme)
            .unwrap_or(0);
        self.view_mode = ViewMode::ThemeSelection;
    }

    /// Theme picker next
    pub fn theme_picker_next(&mut self) {
        self.selected_theme_idx = (self.selected_theme_idx + 1) % ColorTheme::all().len();
    }

    /// Theme picker prev
    pub fn theme_picker_prev(&mut self) {
        let len = ColorTheme::all().len();
        self.selected_theme_idx = if self.selected_theme_idx == 0 {
            len - 1
        } else {
            self.selected_theme_idx - 1
        };
    }

    /// Apply selected theme
    pub fn apply_selected_theme(&mut self) {
        if let Some(&theme) = ColorTheme::all().get(self.selected_theme_idx) {
            self.color_theme = theme;
        }
        self.view_mode = ViewMode::Main;
        self.set_status_message(format!("Theme: {}", self.color_theme.name()));
    }

    /// Cycle theme directly
    pub fn cycle_theme(&mut self) {
        self.color_theme = self.color_theme.next();
        self.set_status_message(format!("Theme: {}", self.color_theme.name()));
    }

    /// Cycle to next process display mode
    pub fn next_process_mode(&mut self) {
        use ProcessDisplayMode::*;
        self.process_display_mode = match self.process_display_mode {
            All => Cpu,
            Cpu => {
                if !self.accelerators.is_empty() {
                    Accelerator(0)
                } else if !self.gpu_info.is_empty() {
                    Gpu(0)
                } else {
                    All
                }
            }
            Accelerator(idx) => {
                if idx + 1 < self.accelerators.len() {
                    Accelerator(idx + 1)
                } else {
                    All
                }
            }
            Gpu(idx) => {
                if idx + 1 < self.gpu_info.len() {
                    Gpu(idx + 1)
                } else {
                    All
                }
            }
            Npu(idx) => {
                // Cycle through NPU devices
                let npu_count = self
                    .accelerators
                    .iter()
                    .filter(|a| a.accel_type == AcceleratorType::Npu)
                    .count();
                if idx + 1 < npu_count {
                    Npu(idx + 1)
                } else {
                    All
                }
            }
        };
        self.scroll_position = 0;
        self.rebuild_cached_process_order();
    }

    /// Cycle to previous process display mode
    pub fn previous_process_mode(&mut self) {
        use ProcessDisplayMode::*;
        self.process_display_mode = match self.process_display_mode {
            All => {
                if !self.accelerators.is_empty() {
                    Accelerator(self.accelerators.len() - 1)
                } else if !self.gpu_info.is_empty() {
                    Gpu(self.gpu_info.len() - 1)
                } else {
                    Cpu
                }
            }
            Cpu => All,
            Accelerator(idx) => {
                if idx > 0 {
                    Accelerator(idx - 1)
                } else {
                    Cpu
                }
            }
            Gpu(idx) => {
                if idx > 0 {
                    Gpu(idx - 1)
                } else {
                    Cpu
                }
            }
            Npu(idx) => {
                if idx > 0 {
                    Npu(idx - 1)
                } else if !self.accelerators.is_empty() {
                    Accelerator(self.accelerators.len() - 1)
                } else if !self.gpu_info.is_empty() {
                    Gpu(self.gpu_info.len() - 1)
                } else {
                    Cpu
                }
            }
        };
        self.scroll_position = 0;
        self.rebuild_cached_process_order();
    }

    /// Get display name for current process mode
    pub fn process_mode_name(&self) -> String {
        use ProcessDisplayMode::*;
        match self.process_display_mode {
            All => "All Processes".to_string(),
            Cpu => "CPU Processes".to_string(),
            Accelerator(idx) => {
                if let Some(accel) = self.accelerators.get(idx) {
                    format!("{} {} Processes", accel.accel_type, idx)
                } else {
                    format!("Accelerator {} Processes", idx)
                }
            }
            Gpu(idx) => format!("GPU {} Processes", idx),
            Npu(idx) => format!("NPU {} Processes", idx),
        }
    }

    pub fn reset_stats(&mut self) {
        self.cpu_history.clear();
        self.memory_history.clear();
        for history in &mut self.gpu_histories {
            history.clear();
        }
    }

    /// Check if encoder should be shown for a GPU based on timeout
    pub fn should_show_encoder(&self, gpu_index: usize) -> bool {
        if let Some(gpu) = self.gpu_info.get(gpu_index) {
            // If encoder is currently active, always show
            if gpu.encoder_util.is_some() && gpu.encoder_util.unwrap() > 0.0 {
                return true;
            }
            // If encoder was recently active, show based on timeout
            if let Some(last_active) = gpu.encoder_last_active {
                let timeout =
                    Duration::from_secs(self.config.general.encode_decode_hiding_timer as u64);
                return last_active.elapsed() < timeout;
            }
        }
        false
    }

    /// Check if decoder should be shown for a GPU based on timeout
    pub fn should_show_decoder(&self, gpu_index: usize) -> bool {
        if let Some(gpu) = self.gpu_info.get(gpu_index) {
            // If decoder is currently active, always show
            if gpu.decoder_util.is_some() && gpu.decoder_util.unwrap() > 0.0 {
                return true;
            }
            // If decoder was recently active, show based on timeout
            if let Some(last_active) = gpu.decoder_last_active {
                let timeout =
                    Duration::from_secs(self.config.general.encode_decode_hiding_timer as u64);
                return last_active.elapsed() < timeout;
            }
        }
        false
    }

    /// Save current configuration to disk
    pub fn save_config(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.config.save()?;
        self.set_status_message("Configuration saved successfully");
        Ok(())
    }

    /// Set a temporary status message
    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some((message.into(), Instant::now()));
    }

    /// Get current status message if not expired (5 second timeout)
    pub fn get_status_message(&self) -> Option<&str> {
        if let Some((msg, timestamp)) = &self.status_message {
            if timestamp.elapsed() < Duration::from_secs(5) {
                return Some(msg.as_str());
            }
        }
        None
    }

    /// Toggle agent input mode
    pub fn toggle_agent_input(&mut self) {
        self.agent_input_mode = !self.agent_input_mode;
        if self.agent_input_mode {
            self.agent_input.clear();
        }
    }

    /// Add character to agent input
    pub fn agent_input_char(&mut self, c: char) {
        if self.agent_input.len() < 200 {
            // Max 200 chars
            self.agent_input.push(c);
        }
    }

    /// Remove last character from agent input
    pub fn agent_input_backspace(&mut self) {
        self.agent_input.pop();
    }

    /// Submit agent query
    /// Spawn the long-lived agent worker thread. It owns the agent plus a
    /// fresh `UnifiedMonitor` (the UI thread keeps its own), reads queries
    /// off a channel, and sends results back. Called once when agent_init
    /// finishes.
    fn spawn_agent_worker(&mut self, agent: Agent) {
        use std::sync::mpsc;
        let (query_tx, query_rx) = mpsc::channel::<String>();
        let (response_tx, response_rx) = mpsc::channel::<Result<AgentResponse, String>>();
        // Worker owns one Agent handle; the UI keeps a separate clone so
        // `app.agent.is_some()` (used by the UI to gate the input bar) keeps
        // working.
        let ui_agent = agent.clone();
        let mut worker_agent = agent;
        std::thread::spawn(move || {
            // The worker owns its own monitor to avoid sharing state with
            // the UI thread. agent.ask() reads from this monitor at call
            // time, so values reflect the moment the query runs.
            let monitor = match UnifiedMonitor::new() {
                Ok(m) => m,
                Err(e) => {
                    let _ = response_tx.send(Err(format!("worker monitor init failed: {}", e)));
                    return;
                }
            };
            // Receive queries until the UI side hangs up.
            while let Ok(query) = query_rx.recv() {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    worker_agent.ask(&query, &monitor)
                }));
                let outcome = match result {
                    Ok(Ok(r)) => Ok(r),
                    Ok(Err(e)) => Err(e.to_string()),
                    Err(_) => Err("agent panicked while processing query".to_string()),
                };
                if response_tx.send(outcome).is_err() {
                    return; // UI dropped the receiver — shut down.
                }
            }
        });
        self.agent_query_tx = Some(query_tx);
        self.agent_response_rx = Some(response_rx);
        self.agent = Some(ui_agent);
    }

    /// Hand a question to the agent worker thread.
    ///
    /// Took a `&UnifiedMonitor` it never used. Supplying that argument cost the TUI a
    /// blocking `GpuCollection::auto_detect` at startup, and it was fallible: a host
    /// where GPU enumeration errors could not open the terminal dashboard at all,
    /// over a value no code read.
    pub fn submit_agent_query(&mut self) {
        if self.agent_input.is_empty() {
            return;
        }
        let query = self.agent_input.clone();
        self.agent_input.clear();
        self.agent_input_mode = false;

        if self.agent_loading {
            self.set_status_message("Agent is still answering the previous query");
            return;
        }
        let Some(ref tx) = self.agent_query_tx else {
            self.set_status_message("Agent not available");
            return;
        };
        if tx.send(query).is_err() {
            self.set_status_message("Agent worker disconnected");
            self.agent_query_tx = None;
            self.agent_response_rx = None;
            return;
        }
        self.agent_loading = true;
    }

    /// Clear agent history
    pub fn clear_agent_history(&mut self) {
        self.agent_history.clear();
        self.agent_history_version = self.agent_history_version.wrapping_add(1);
        self.set_status_message("Agent history cleared");
    }

    /// Get agent cache statistics
    pub fn agent_cache_stats(&self) -> Option<String> {
        self.agent
            .as_ref()
            .map(|agent| format!("Cache: {} entries", agent.cache_size()))
    }
}

/// The running Windows build, revision included (e.g. "10.0.26200.8875").
///
/// Returns `None` when the registry does not answer, so the caller can fall back
/// rather than print a partial version as if it were complete.
#[cfg(target_os = "windows")]
fn windows_kernel_version() -> Option<String> {
    use crate::platform::windows as plat;

    const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

    let major = plat::read_registry_u32(CURRENT_VERSION, "CurrentMajorVersionNumber")?;
    let minor = plat::read_registry_u32(CURRENT_VERSION, "CurrentMinorVersionNumber")?;
    let build = plat::read_registry_string(CURRENT_VERSION, "CurrentBuildNumber")?;

    // The update revision moves between patch Tuesdays; it is appended only when
    // present, rather than defaulted to 0.
    Some(match plat::read_registry_u32(CURRENT_VERSION, "UBR") {
        Some(ubr) => format!("{major}.{minor}.{build}.{ubr}"),
        None => format!("{major}.{minor}.{build}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end check of the snapshot wiring: the collector thread must publish
    /// real hardware data, and `sync_snapshot` must fold it into the display fields.
    ///
    /// This covers the refactor that moved every `update_*` method off blocking
    /// platform calls and onto the published snapshot. A regression here means the
    /// TUI renders stale or empty panels.
    #[test]
    fn sync_snapshot_populates_display_state_from_collector() {
        let mut app = match App::new() {
            Ok(app) => app,
            // Constructing the App loads config and spawns collectors; if that is not
            // possible in this environment there is nothing meaningful to assert.
            Err(e) => {
                eprintln!("skipping: App::new failed: {e}");
                return;
            }
        };

        // Cold start is dominated by GPU enumeration, so allow generous headroom.
        let deadline = Instant::now() + Duration::from_secs(45);
        let mut applied = false;
        while Instant::now() < deadline {
            if app.sync_snapshot() {
                applied = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        assert!(
            applied,
            "collector published no snapshot within 45s; the pipeline is not running"
        );

        // CPU and memory statistics have Linux and Windows readers and no macOS
        // ones, so the display state legitimately has nothing to show there.
        // Asserting otherwise would be asserting that readers exist which do not;
        // the gap is recorded in `stats::macos_stats` rather than papered over here.
        //
        // What the test still checks on every platform is the part it is actually
        // about: that the collector publishes a snapshot, that applying it is
        // idempotent, and that the render guard consumes the generation exactly
        // once. Those hold regardless of which readers a platform has.
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            assert!(
                app.cpu_info.cores > 0,
                "CPU core count did not reach the display state"
            );
            assert!(
                !app.cpu_info.per_core_usage.is_empty(),
                "per-core usage did not reach the display state"
            );
            assert!(
                app.memory_info.total > 0,
                "memory total did not reach the display state"
            );
        }

        // This one holds everywhere: on a platform with no reader both are zero,
        // and zero does not exceed zero. A used figure above total would be wrong
        // on any platform.
        assert!(
            app.memory_info.used <= app.memory_info.total,
            "memory used {} exceeds total {}",
            app.memory_info.used,
            app.memory_info.total
        );

        // The generation guard must make a second immediate sync a no-op, otherwise
        // the render loop would repaint identical frames.
        assert!(
            !app.sync_snapshot(),
            "sync_snapshot re-applied the same generation"
        );

        // First call reports the new generation, second reports nothing new.
        assert!(app.snapshot_changed_since_render());
        assert!(
            !app.snapshot_changed_since_render(),
            "render guard did not consume the generation"
        );
    }

    #[test]
    fn test_color_theme_all_count() {
        assert_eq!(ColorTheme::all().len(), 6);
    }

    #[test]
    fn test_color_theme_default() {
        let theme = ColorTheme::default();
        assert_eq!(theme, ColorTheme::CatppuccinMocha);
    }

    #[test]
    fn test_color_theme_names() {
        assert_eq!(ColorTheme::CatppuccinMocha.name(), "Catppuccin Mocha");
        assert_eq!(ColorTheme::CatppuccinLatte.name(), "Catppuccin Latte");
        assert_eq!(ColorTheme::Glances.name(), "Glances Classic");
        assert_eq!(ColorTheme::Nord.name(), "Nord");
        assert_eq!(ColorTheme::Dracula.name(), "Dracula");
        assert_eq!(ColorTheme::GruvboxDark.name(), "Gruvbox Dark");
    }

    #[test]
    fn test_next_wraps() {
        let last = *ColorTheme::all().last().unwrap();
        let first = ColorTheme::all()[0];
        assert_eq!(last.next(), first);
    }

    #[test]
    fn test_next_sequence() {
        let t = ColorTheme::CatppuccinMocha;
        assert_eq!(t.next(), ColorTheme::CatppuccinLatte);
    }

    #[test]
    fn test_prev_wraps() {
        let first = ColorTheme::all()[0];
        let last = *ColorTheme::all().last().unwrap();
        assert_eq!(first.prev(), last);
    }

    #[test]
    fn test_prev_sequence() {
        let t = ColorTheme::CatppuccinLatte;
        assert_eq!(t.prev(), ColorTheme::CatppuccinMocha);
    }

    #[test]
    fn test_next_prev_roundtrip() {
        for theme in ColorTheme::all() {
            assert_eq!(theme.next().prev(), *theme);
            assert_eq!(theme.prev().next(), *theme);
        }
    }

    #[test]
    fn test_colors_valid_rgb() {
        for theme in ColorTheme::all() {
            let c = theme.colors();
            // Just verify we can access all color fields (they're u8 so always valid)
            let _ok = c.ok;
            let _warn = c.warning;
            let _crit = c.critical;
            let _title = c.title;
            let _text = c.text;
        }
    }

    #[test]
    fn test_view_mode_default() {
        assert_eq!(ViewMode::default(), ViewMode::Main);
    }

    #[test]
    fn test_process_display_mode_default() {
        assert_eq!(ProcessDisplayMode::default(), ProcessDisplayMode::All);
    }

    #[test]
    fn test_accelerator_type_default() {
        assert_eq!(AcceleratorType::default(), AcceleratorType::Gpu);
    }

    #[test]
    fn test_accelerator_type_display() {
        assert_eq!(format!("{}", AcceleratorType::Gpu), "GPU");
        assert_eq!(format!("{}", AcceleratorType::Npu), "NPU");
        assert_eq!(format!("{}", AcceleratorType::Tpu), "TPU");
        assert_eq!(format!("{}", AcceleratorType::Fpga), "FPGA");
        assert_eq!(format!("{}", AcceleratorType::Dla), "DLA");
        assert_eq!(format!("{}", AcceleratorType::Vpu), "VPU");
        assert_eq!(format!("{}", AcceleratorType::Ipu), "IPU");
        assert_eq!(format!("{}", AcceleratorType::Other), "ACC");
    }

    #[test]
    fn test_process_display_mode_npu_variant() {
        let mode = ProcessDisplayMode::Npu(0);
        assert_ne!(mode, ProcessDisplayMode::All);
        assert_ne!(mode, ProcessDisplayMode::Cpu);
        // Different NPU indices are different modes
        assert_ne!(ProcessDisplayMode::Npu(0), ProcessDisplayMode::Npu(1));
        assert_eq!(ProcessDisplayMode::Npu(2), ProcessDisplayMode::Npu(2));
    }

    #[test]
    fn test_accelerator_info_default() {
        let info = AcceleratorInfo::default();
        assert_eq!(info.accel_type, AcceleratorType::Gpu);
        assert!(info.name.is_empty());
        assert_eq!(
            info.utilization, None,
            "a default accelerator has no utilization reading, not one of zero"
        );
        // The same sentence as the utilization assertion above, which sat
        // directly on top of these two while they asserted a zero.
        assert_eq!(
            info.memory_total, None,
            "a default accelerator has no memory reading, not one of zero"
        );
        assert_eq!(info.memory_used, None);
        assert!(info.temperature.is_none());
        assert!(info.power.is_none());
    }
}

#[cfg(test)]
mod ontology_vocabulary_tests {
    /// Tabs that name an ontology domain must use the ontology's spelling.
    ///
    /// Not every tab is a domain — "Overview", "Processes" and "Agent" are views,
    /// not entity namespaces — but the ones that are have to match, or the three
    /// surfaces drift apart on whether it is "Cpu", "cpu" or "CPU" and an agent
    /// cannot map a tab a user is describing onto ids it can query.
    #[test]
    fn tab_names_that_are_domains_use_the_ontology_spelling() {
        use crate::ontology::labels;

        // Mirrors the `tabs` vec in `TuiApp::new`.
        let tabs = [
            "Overview",
            "Processes",
            "CPU",
            "Accelerators",
            "Memory",
            "System",
            "Peripherals",
            "Profiles",
            "Agent",
        ];

        for tab in tabs {
            let lowered = tab.to_ascii_lowercase();
            if labels::is_known_domain(&lowered) {
                assert_eq!(
                    tab,
                    labels::domain_label(&lowered),
                    "TUI tab {tab:?} names an ontology domain but spells it \
                     differently from the ontology"
                );
            }
        }
    }

    /// At least some tabs must be domains, or the assertion above is vacuous.
    #[test]
    fn the_vocabulary_check_is_not_vacuous() {
        use crate::ontology::labels;
        let domain_tabs = ["CPU", "Memory", "System"]
            .iter()
            .filter(|t| labels::is_known_domain(&t.to_ascii_lowercase()))
            .count();
        assert!(
            domain_tabs >= 2,
            "expected several TUI tabs to name ontology domains; found {domain_tabs}"
        );
    }
}
