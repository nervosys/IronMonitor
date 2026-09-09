//! Main application state and logic for IronMonitor GUI

use eframe::egui;
use egui::{RichText, ScrollArea, Vec2};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use super::theme::{self, threshold_color, trend_indicator, CyberColors, DeviceTitleColors};
use super::widgets::{
    CyberProgressBar, MetricCard, QuickLookPanel, SectionHeader, SparklineChart, ThresholdLegend,
};

use crate::ai_api::AiDataApi;
use crate::connections::{ConnectionInfo, ConnectionState, Protocol};
use crate::core::cpu::CpuStats;
use crate::core::memory::MemoryStats;
use crate::disk::{self, DiskDevice};
use crate::gpu::{GpuCollection, GpuDynamicInfo, GpuStaticInfo};
use crate::motherboard::{self, DriverInfo, MotherboardDevice, SystemInfo as MBSystemInfo};
use crate::network_monitor::NetworkMonitor;
use crate::network_tools::{self, PortStatus};
#[cfg(target_os = "windows")]
use crate::platform::windows as platform_impl;
use crate::process_monitor::ProcessMonitorInfo;
use crate::system_stats::SystemStats;

const HISTORY_SIZE: usize = 60;
// A full collection tick costs ~500ms on a three-GPU Windows box even with the
// process table decimated, so a 500ms interval asked for more than the hardware can
// deliver and the collector thread never slept. 1s matches what the sources can
// actually sustain, and is the cadence htop and nvtop use.
const DATA_POLL_INTERVAL: Duration = Duration::from_millis(1000); // Data polling rate
const SLOW_UPDATE_INTERVAL: Duration = Duration::from_secs(2); // Slow updates for heavy ops
/// How often the disk refresher thread re-samples every drive.
///
/// SMART and filesystem queries are slow device I/O, but they run on their own
/// thread now, so the cost is one background thread waking occasionally rather than
/// a stalled window.
const DISK_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
/// Largest share of wall time the disk refresher may spend sampling.
const DISK_MAX_DUTY_CYCLE: f32 = 0.25;

/// A disk row as plain data, produced entirely off the UI thread.
///
/// Everything the disk views render lives here, so drawing never calls into a
/// device. It used to hold only the sampled fields while the identity came from a
/// `Box<dyn DiskDevice>` the UI thread kept — which meant the UI thread also did the
/// sampling.
#[derive(Clone)]
struct CachedDiskData {
    /// Device name, carried here so rendering needs no access to the device.
    name: String,
    /// Device classification, likewise.
    disk_type: crate::disk::DiskType,
    info: Option<crate::disk::DiskInfo>,
    io_stats: Option<crate::disk::DiskIoStats>,
    health: Option<crate::disk::DiskHealth>,
    filesystems: Vec<crate::disk::FilesystemInfo>,
    /// Drive temperature, if the device reports one.
    temperature: Option<f32>,
}

impl Default for CachedDiskData {
    fn default() -> Self {
        Self {
            name: String::new(),
            disk_type: crate::disk::DiskType::Unknown,
            info: None,
            io_stats: None,
            health: None,
            filesystems: Vec::new(),
            temperature: None,
        }
    }
}

/// AI agent response from background thread
struct AgentResponse {
    response: String,
    inference_time_ms: u64,
    from_cache: bool,
}

/// Main application state
pub struct IronMonitorApp {
    // Current tab
    current_tab: Tab,

    // Hardware data
    cpu_stats: Option<CpuStats>,
    memory_stats: Option<MemoryStats>,
    #[allow(dead_code)]
    gpu_collection: Option<GpuCollection>,
    gpu_static_info: Vec<GpuStaticInfo>,
    gpu_dynamic_info: Vec<GpuDynamicInfo>,
    network_monitor: Option<NetworkMonitor>,
    process_list: Vec<ProcessMonitorInfo>,

    // Disk data

    // Connection data
    connections: Vec<ConnectionInfo>,
    connection_filter: String,
    connection_protocol_filter: Option<Protocol>,
    connection_state_filter: Option<ConnectionState>,

    // System/Motherboard data
    system_info: Option<MBSystemInfo>,
    system_info_tried: bool,
    motherboard_sensors: Vec<Box<dyn MotherboardDevice>>,
    driver_info: Vec<DriverInfo>,
    pcie_devices: Vec<motherboard::PcieDeviceInfo>,
    sata_devices: Vec<motherboard::SataDeviceInfo>,
    system_temps: Option<motherboard::SystemTemperatures>,
    peripherals: Option<motherboard::PeripheralsInfo>,

    // System-wide stats (Linux/BSD style - load avg, vmstat)
    system_stats: Option<SystemStats>,
    context_switches_history: VecDeque<f32>,
    interrupts_history: VecDeque<f32>,
    prev_context_switches: u64,
    prev_interrupts: u64,

    // Historical data for graphs
    cpu_history: VecDeque<f32>,
    per_core_history: Vec<VecDeque<f32>>,
    memory_history: VecDeque<f32>,
    gpu_history: Vec<VecDeque<f32>>,
    gpu_memory_history: Vec<VecDeque<f32>>,
    gpu_temp_history: Vec<VecDeque<f32>>,
    network_rx_history: VecDeque<f32>,
    network_tx_history: VecDeque<f32>,

    // Network rate tracking (bytes/sec)
    /// Per-interface receive and transmit rates, each absent until a second
    /// sample establishes it. These were bare `f64`s and an unestablished rate
    /// arrived as `0.0`, which the tab drew as an idle link.
    network_rates: std::collections::HashMap<String, (Option<f64>, Option<f64>)>,

    // Timing
    #[allow(dead_code)]
    last_update: Instant,
    last_slow_update: Instant,
    start_time: Instant,

    // System info
    hostname: String,
    os_info: String,

    // Process list state
    process_sort_column: ProcessSortColumn,
    process_sort_ascending: bool,
    process_filter: String,

    // Network Tools state
    nettools_target_host: String,
    nettools_ping_result: Option<crate::network_tools::PingResult>,
    nettools_traceroute_result: Option<crate::network_tools::TracerouteResult>,
    nettools_port_scan_results: Vec<crate::network_tools::PortScanResult>,
    nettools_nmap_result: Option<crate::network_tools::NmapScanResult>,
    nettools_capture_result: Option<crate::network_tools::CaptureResult>,
    nettools_capture_protocol: crate::network_tools::CaptureProtocol,
    nettools_capture_count: u32,
    nettools_port_range_start: u16,
    nettools_port_range_end: u16,
    nettools_is_running: bool,
    nettools_operation: String,
    nettools_dns_results: Vec<std::net::IpAddr>,

    // AI Agent state
    agent: Option<crate::agent::Agent>,
    ai_data_api: Option<AiDataApi>,
    agent_query: String,
    agent_history: VecDeque<AgentChatEntry>,
    agent_is_processing: bool,
    agent_response_receiver: Option<Receiver<Result<AgentResponse, String>>>,

    // AI configuration UI state (reserved for future use)
    #[allow(dead_code)]
    ai_api_key_input: String,
    ai_selected_backend: AiBackendSelection,
    ai_prev_backend: AiBackendSelection,
    ai_selected_model: String,
    /// Models each provider reported, keyed by provider. Filled asynchronously.
    models_by_provider: std::collections::HashMap<AiBackendSelection, Vec<String>>,
    #[allow(dead_code)]
    ai_ollama_starting: bool,
    ai_status_message: Option<(String, bool)>, // (message, is_error)

    // Background loading state
    system_info_receiver: Option<Receiver<SystemInfoUpdate>>,
    system_info_loading: bool,
    disk_receiver: Option<Receiver<Vec<Box<dyn DiskDevice + Send>>>>,
    disk_loading: bool,
    disk_loaded: bool,
    agent_receiver: Option<Receiver<Option<crate::agent::Agent>>>,
    agent_loading: bool,
    agent_loading_start: Instant,
    /// Consecutive auto-detection attempts that found nothing.
    agent_detect_attempts: u32,
    /// When to re-run auto-detection after it came up empty.
    agent_detect_next_retry: Option<Instant>,
    models_receiver: Option<Receiver<(AiBackendSelection, Vec<String>)>>,
    /// Provider whose model listing is currently in flight.
    models_loading_for: Option<AiBackendSelection>,
    ai_data_api_receiver: Option<Receiver<Option<AiDataApi>>>,

    // Cached disk data (refreshed periodically, not on every frame)
    cached_disk_data: Vec<CachedDiskData>,
    disk_rows_receiver: Option<Receiver<Vec<CachedDiskData>>>,

    // Snapshot pipeline
    //
    // Replaces the previous per-poll `std::thread::spawn` + channel machinery. That
    // path re-ran `GpuCollection::auto_detect()` on *every* poll (both branches did,
    // despite the "skip static re-detect" fast path), so NVML was fully
    // re-initialized twice a second. The collector now initializes once and keeps
    // its handles alive.
    /// Background collector. Dropping this stops and joins the thread.
    collector: Option<crate::pipeline::Collector>,
    /// Newest published snapshot.
    snapshot: std::sync::Arc<crate::pipeline::Snapshot>,
    /// Generation already folded into UI state, so repeated frames do no work.
    applied_generation: u64,

    // Settings
    show_settings: bool,
    settings: AppSettings,

    // Alert system
    active_alerts: Vec<Alert>,

    // Historical data for AI queries
    historical_data: Vec<HistoricalDataPoint>,
    last_historical_save: std::time::Instant,

    // Hardware profile inspector (NVPI / XTU / Ryzen Master / nvme-cli style)
    pub(super) profile_snapshot: Option<crate::profile::ProfileSnapshot>,
    pub(super) profile_filter: String,
    pub(super) profile_subsystem_filter: Option<crate::profile::Subsystem>,
    pub(super) profile_snapshot_receiver: Option<Receiver<crate::profile::ProfileSnapshot>>,
    pub(super) profile_snapshot_loading: bool,
    pub(super) profile_deviations_cache: Option<Vec<crate::profile::deviation::Deviation>>,
    pub(super) profile_audit_tail_cache: Vec<String>,
    pub(super) profile_audit_last_read: Option<std::time::Instant>,
    /// True once any profile load (bg or sync) has been attempted. Stops
    /// per-frame respawn after a panicked bg thread; cleared by Refresh.
    pub(super) profile_load_attempted: bool,
    /// True once a sync fallback load has been attempted this generation.
    pub(super) profile_sync_attempted: bool,

    // Theme is expensive to apply (rebuilds font atlas via ctx.set_fonts).
    // Track what we last applied so update() can skip the call when nothing
    // has changed since last frame.
    applied_color_theme: Option<ColorTheme>,

    // Cache for the Processes tab — avoid cloning + sorting the whole
    // process_list on every paint (10 FPS × hundreds of processes is enough
    // to feel laggy).
    process_list_version: u64,
    processes_view_cache: Vec<ProcessMonitorInfo>,
    processes_view_key: Option<(String, ProcessSortColumn, bool, u64)>,
}

/// An instalment of background system info.
///
/// Two messages rather than one, because the seven queries behind this differ in
/// cost by two orders of magnitude. Measured on the development machine:
/// `get_driver_versions` 20.3s and `get_system_temperatures` 2.9s, against
/// `get_peripherals` 0.7s, `get_pcie_devices` 0.6s, `get_system_info` 0.1s and
/// `enumerate_sensors` 0.01s.
///
/// Collecting all seven before sending anything meant the Peripherals tab —
/// which paints USB and PCIe and shows no driver table at all — sat on a spinner
/// for 24 seconds waiting for data it does not display. Headless reads gave up
/// at 30s and returned a bare title, intermittently, depending on how the WMI
/// query happened to go: `every_gui_tab_paints_text` failed roughly three runs
/// in five.
///
/// The split is by cost, not by tab, so [`Slow`](SystemInfoUpdate::Slow) carries
/// exactly the two queries that are expensive and nothing else.
enum SystemInfoUpdate {
    /// Everything cheap enough to wait for. Arrives in about 1.5s.
    ///
    /// Boxed because it is several times the size of `Slow`, and every value of
    /// this enum would otherwise be as large as its biggest variant.
    Fast(Box<FastSystemInfo>),
    /// The two slow queries. Receipt of this ends the load.
    Slow {
        drivers: Vec<DriverInfo>,
        system_temps: Option<motherboard::SystemTemperatures>,
    },
}

/// The payload of [`SystemInfoUpdate::Fast`].
struct FastSystemInfo {
    system_info: Option<MBSystemInfo>,
    sensors: Vec<Box<dyn MotherboardDevice>>,
    pcie_devices: Vec<motherboard::PcieDeviceInfo>,
    sata_devices: Vec<motherboard::SataDeviceInfo>,
    peripherals: Option<motherboard::PeripheralsInfo>,
}

/// Run the seven system-info queries, cheapest first, sending each instalment as
/// it completes.
///
/// One function for both call sites; there were two copies of this thread body
/// and they had already drifted by one `sleep`.
fn collect_system_info(tx: std::sync::mpsc::Sender<SystemInfoUpdate>) {
    let system_info = motherboard::get_system_info().ok();
    let sensors = motherboard::enumerate_sensors().unwrap_or_default();
    let pcie_devices = motherboard::get_pcie_devices().unwrap_or_default();
    let sata_devices = motherboard::get_sata_devices().unwrap_or_default();
    let peripherals = motherboard::get_peripherals().ok();

    // If the receiver is gone the window closed; the slow queries are then
    // 23 seconds of work nobody is waiting for.
    if tx
        .send(SystemInfoUpdate::Fast(Box::new(FastSystemInfo {
            system_info,
            sensors,
            pcie_devices,
            sata_devices,
            peripherals,
        })))
        .is_err()
    {
        return;
    }

    let drivers = motherboard::get_driver_versions().unwrap_or_default();
    let system_temps = motherboard::get_system_temperatures().ok();
    let _ = tx.send(SystemInfoUpdate::Slow {
        drivers,
        system_temps,
    });
}

/// A chat entry in the AI Agent conversation
#[derive(Debug, Clone)]
struct AgentChatEntry {
    role: ChatRole,
    content: String,
    #[allow(dead_code)]
    timestamp: std::time::Instant,
    inference_time_ms: Option<u64>,
    from_cache: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Cpu,
    Accelerators,
    Memory,
    Disk,
    Processes,
    Network,
    NetworkTools,
    Connections,
    SystemInfo,
    Peripherals,
    Profiles,
    AIAssistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessSortColumn {
    Name,
    Pid,
    Cpu,
    Memory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
enum AiBackendSelection {
    /// ironmon's own engine, and the reason it is first: inference stays on the host,
    /// so telemetry never leaves the machine.
    IronWorks,
    #[default]
    Ollama,
    LmStudio,
    Vllm,
    TensorRt,
    OpenAi,
    Anthropic,
    GitHub,
    /// A locally installed CLI tool driven as a subprocess.
    Cli(crate::agent::local::CliProvider),
}

impl AiBackendSelection {
    /// Every provider offered in the dropdown, in preference order.
    ///
    /// This used to list five, while the library supported IronWorks, vLLM,
    /// TensorRT-LLM and four CLI tools as well — so the engine ironmon ships against
    /// could not be selected from ironmon's own UI.
    const ALL: &'static [Self] = &[
        Self::IronWorks,
        Self::Ollama,
        Self::LmStudio,
        Self::Vllm,
        Self::TensorRt,
        Self::OpenAi,
        Self::Anthropic,
        Self::GitHub,
        Self::Cli(crate::agent::local::CliProvider::Claude),
        Self::Cli(crate::agent::local::CliProvider::Codex),
        Self::Cli(crate::agent::local::CliProvider::Gemini),
    ];

    /// Provider name for prose, without the decorative icon.
    fn display_name(self) -> &'static str {
        use crate::agent::local::CliProvider;
        match self {
            Self::IronWorks => "IronWorks",
            Self::Ollama => "Ollama",
            Self::LmStudio => "LM Studio",
            Self::Vllm => "vLLM",
            Self::TensorRt => "TensorRT-LLM",
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::GitHub => "GitHub Models",
            Self::Cli(CliProvider::Claude) => "Claude Code CLI",
            Self::Cli(CliProvider::Codex) => "Codex CLI",
            Self::Cli(CliProvider::Gemini) => "Gemini CLI",
            Self::Cli(CliProvider::Ollama) => "Ollama CLI",
        }
    }

    /// Label with icon, for the dropdown.
    fn menu_label(self) -> &'static str {
        use crate::agent::local::CliProvider;
        match self {
            Self::IronWorks => "⚙ IronWorks (built-in)",
            Self::Ollama => "🦙 Ollama",
            Self::LmStudio => "📦 LM Studio",
            Self::Vllm => "🚀 vLLM",
            Self::TensorRt => "⚡ TensorRT-LLM",
            Self::OpenAi => "🤖 OpenAI",
            Self::Anthropic => "🧠 Anthropic",
            // GitHub shut this service down on 2026-07-30 — catalogue, inference,
            // and BYOK all gone. Say so in the menu rather than offering a provider
            // that cannot answer, which reads to the user as a bug in ironmon.
            Self::GitHub => "🐙 GitHub Models (retired)",
            Self::Cli(CliProvider::Claude) => "🖥 Claude Code CLI",
            Self::Cli(CliProvider::Codex) => "🖥 Codex CLI",
            Self::Cli(CliProvider::Gemini) => "🖥 Gemini CLI",
            Self::Cli(CliProvider::Ollama) => "🖥 Ollama CLI",
        }
    }

    /// Base URL whose `/models` endpoint lists what the provider can serve.
    ///
    /// `None` means the provider exposes no listing IronMonitor can read — a CLI tool
    /// chooses its own model, so there is nothing to enumerate.
    fn models_endpoint(self) -> Option<&'static str> {
        use crate::agent::local::CliProvider;
        match self {
            Self::IronWorks => Some("http://localhost:8080/v1/models"),
            Self::Ollama | Self::Cli(CliProvider::Ollama) => {
                Some("http://localhost:11434/api/tags")
            }
            Self::LmStudio => Some("http://localhost:1234/v1/models"),
            Self::Vllm => Some("http://localhost:8000/v1/models"),
            Self::TensorRt => Some("http://localhost:8001/v1/models"),
            Self::OpenAi => Some("https://api.openai.com/v1/models"),
            Self::Anthropic => Some("https://api.anthropic.com/v1/models"),
            Self::GitHub => Some("https://models.github.ai/catalog/models"),
            Self::Cli(_) => None,
        }
    }

    /// The environment variable a hosted provider reads its credential from.
    ///
    /// The listing endpoints of hosted providers all require authentication, so
    /// without this the live fetch answers 401 and the dropdown silently falls back
    /// to the hardcoded guess below — which is exactly the staleness the fetch
    /// exists to avoid. A key the user exported before launching IronMonitor is just as
    /// usable as one typed into the field.
    fn api_key_env_var(self) -> Option<&'static str> {
        match self {
            Self::OpenAi => Some("OPENAI_API_KEY"),
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            Self::GitHub => Some("GITHUB_TOKEN"),
            _ => None,
        }
    }

    /// Names to offer when the provider could not be asked.
    ///
    /// Only reached when a listing is unavailable — the server is down, or a hosted
    /// provider has no key anywhere IronMonitor can see. A live listing is always
    /// preferred: a hardcoded list is stale the moment a provider ships a model,
    /// which is how this tab came to offer `gpt-3.5-turbo` and `claude-3-sonnet`.
    /// The dropdown labels these "Not listed by provider" for that reason — they are
    /// a guess, not an observation, and they will age.
    fn fallback_models(self) -> &'static [&'static str] {
        match self {
            Self::Anthropic => &["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5"],
            Self::OpenAi => &["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"],
            // GitHub's catalogue is readable without a credential, so the live
            // listing is reached whenever the machine is online. Guessing here
            // would only ever be consulted offline, where the name cannot be used
            // anyway — and GitHub's ids are publisher-qualified, so a guess copied
            // from another provider would be wrong.
            Self::GitHub => &[],
            // Local servers name whatever was loaded into them; there is no
            // meaningful guess, and inventing one would send a request for a model
            // that does not exist.
            _ => &[],
        }
    }
}

/// Color theme options for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorTheme {
    #[default]
    Cyber, // Default neon cyber theme (dark)
    Light,      // Clean light theme
    Ocean,      // Blue/teal oceanic theme
    Forest,     // Green nature theme
    Sunset,     // Orange/red warm theme
    Monochrome, // Grayscale minimalist
}

impl ColorTheme {
    fn name(&self) -> &'static str {
        match self {
            ColorTheme::Cyber => "Cyber (Dark)",
            ColorTheme::Light => "Light",
            ColorTheme::Ocean => "Ocean",
            ColorTheme::Forest => "Forest",
            ColorTheme::Sunset => "Sunset",
            ColorTheme::Monochrome => "Monochrome",
        }
    }

    fn all() -> &'static [ColorTheme] {
        &[
            ColorTheme::Cyber,
            ColorTheme::Light,
            ColorTheme::Ocean,
            ColorTheme::Forest,
            ColorTheme::Sunset,
            ColorTheme::Monochrome,
        ]
    }

    /// Get the primary accent color for this theme
    pub fn accent_color(&self) -> egui::Color32 {
        match self {
            ColorTheme::Cyber => CyberColors::CYAN,
            ColorTheme::Light => egui::Color32::from_rgb(59, 130, 246), // Blue
            ColorTheme::Ocean => egui::Color32::from_rgb(64, 224, 208), // Turquoise
            ColorTheme::Forest => egui::Color32::from_rgb(34, 197, 94), // Green
            ColorTheme::Sunset => egui::Color32::from_rgb(251, 146, 60), // Orange
            ColorTheme::Monochrome => egui::Color32::from_rgb(200, 200, 200), // Light gray
        }
    }

    /// Get the secondary accent color for this theme
    #[allow(dead_code)]
    pub fn secondary_color(&self) -> egui::Color32 {
        match self {
            ColorTheme::Cyber => CyberColors::MAGENTA,
            ColorTheme::Light => egui::Color32::from_rgb(99, 102, 241), // Indigo
            ColorTheme::Ocean => egui::Color32::from_rgb(56, 189, 248), // Sky blue
            ColorTheme::Forest => egui::Color32::from_rgb(74, 222, 128), // Light green
            ColorTheme::Sunset => egui::Color32::from_rgb(248, 113, 113), // Red
            ColorTheme::Monochrome => egui::Color32::from_rgb(150, 150, 150), // Medium gray
        }
    }
}
/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertSeverity {
    Warning,
    Critical,
}

/// An active alert
#[derive(Debug, Clone)]
pub struct Alert {
    #[allow(dead_code)]
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: std::time::Instant,
}

/// Settings for the alert system
#[derive(Debug, Clone)]
pub struct AlertSettings {
    pub enabled: bool,
    pub cpu_warning_threshold: f32,
    pub cpu_critical_threshold: f32,
    pub memory_warning_threshold: f32,
    pub memory_critical_threshold: f32,
    pub gpu_temp_warning: f32,
    pub gpu_temp_critical: f32,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            cpu_warning_threshold: 80.0,
            cpu_critical_threshold: 95.0,
            memory_warning_threshold: 80.0,
            memory_critical_threshold: 95.0,
            gpu_temp_warning: 75.0,
            gpu_temp_critical: 90.0,
        }
    }
}

/// A historical data point for tracking metrics over time
#[derive(Debug, Clone)]
pub struct HistoricalDataPoint {
    pub timestamp: std::time::Instant,
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub gpu_temps: Vec<f32>,
    pub gpu_utils: Vec<f32>,
}

/// Application settings
#[derive(Debug, Clone)]
pub struct AppSettings {
    pub color_theme: ColorTheme,
    pub graph_line_thickness: f32,
    pub show_grid_lines: bool,
    pub animation_speed: f32,
    pub alert_settings: AlertSettings,
    #[allow(dead_code)]
    pub minimize_to_tray: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            color_theme: ColorTheme::Cyber,
            graph_line_thickness: 2.5,
            show_grid_lines: true,
            animation_speed: 1.0,
            alert_settings: AlertSettings::default(),
            minimize_to_tray: false,
        }
    }
}

impl IronMonitorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_context(&cc.egui_ctx)
    }

    /// Build the app from a bare [`egui::Context`].
    ///
    /// `CreationContext` was only ever consulted for its context, and requiring one
    /// meant the app could not be constructed without a window — which left every
    /// tab verifiable only by screenshot. That is what made "the AI tab does not
    /// work" impossible to check: on a multi-monitor setup the captures kept
    /// catching other applications, and a capture cannot distinguish a tab that
    /// rendered nothing from one that rendered in an unreadable colour anyway.
    pub fn with_context(egui_ctx: &egui::Context) -> Self {
        let cc = egui_ctx;
        // Apply cyber theme
        theme::apply_cyber_theme(cc);

        // Initialize monitors
        let gpu_collection = GpuCollection::auto_detect().ok();
        let (gpu_static_info, gpu_dynamic_info) = if let Some(ref gpus) = gpu_collection {
            let static_info: Vec<GpuStaticInfo> = gpus
                .gpus()
                .iter()
                .filter_map(|g| g.static_info().ok())
                .collect();
            let dynamic_info: Vec<GpuDynamicInfo> = gpus
                .gpus()
                .iter()
                .filter_map(|g| g.dynamic_info().ok())
                .collect();
            (static_info, dynamic_info)
        } else {
            (vec![], vec![])
        };

        let gpu_count = gpu_static_info.len();

        // Get initial CPU core count using platform-specific implementation
        #[cfg(target_os = "windows")]
        let cpu_core_count = platform_impl::read_cpu_stats()
            .ok()
            .map(|s| s.cores.len())
            .unwrap_or(0);
        #[cfg(not(target_os = "windows"))]
        let cpu_core_count = num_cpus::get();

        // Get initial CPU stats.
        //
        // `CpuStats::new()` is a zero-constructor exactly like `MemoryStats::new()`
        // below — cores empty, idle pinned at 100%. Reaching it means the tab
        // opens claiming a fully idle machine with no cores. Linux has a real
        // reader in `platform::linux::cpu`; it is now called.
        #[cfg(target_os = "windows")]
        let initial_cpu_stats = platform_impl::read_cpu_stats().ok();
        #[cfg(target_os = "linux")]
        let initial_cpu_stats = crate::platform::linux::cpu::read_cpu_stats().ok();
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        let initial_cpu_stats = Some(CpuStats::empty());

        // Get initial memory stats.
        //
        // `MemoryStats::new()` is a zero-constructor, not a reader — it returns a
        // struct of zeros on every platform. Reaching it means the tab opens
        // showing 0 MB of 0 MB until the first refresh replaces it. Linux has had
        // a working `read_memory_stats` in `platform::linux::memory` the whole
        // time and simply was not being called; it is now.
        //
        // macOS still falls through to the zero-constructor. That is left alone
        // deliberately rather than "fixed" blind: there is no
        // `platform::macos::read_memory_stats` to call yet (HANDOFF.md open work
        // 2), and no Mac here to check a replacement against.
        #[cfg(target_os = "windows")]
        let initial_memory_stats = platform_impl::read_memory_stats().ok();
        #[cfg(target_os = "linux")]
        let initial_memory_stats = crate::platform::linux::memory::read_memory_stats().ok();
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        let initial_memory_stats = Some(MemoryStats::empty());

        // Start background loading for AI agent (avoid blocking UI with HTTP timeouts)
        let (agent_tx, agent_rx) = channel();
        std::thread::spawn(move || {
            let agent = crate::agent::AgentConfig::auto_detect()
                .ok()
                .and_then(|config| crate::agent::Agent::new(config).ok());
            let _ = agent_tx.send(agent);
        });

        // Start background loading for AI Data API (avoid blocking on GPU/process init)
        let (ai_api_tx, ai_api_rx) = channel();
        std::thread::spawn(move || {
            let api = AiDataApi::new().ok();
            let _ = ai_api_tx.send(api);
        });

        let mut app = Self {
            current_tab: Tab::Overview,
            cpu_stats: initial_cpu_stats,
            memory_stats: initial_memory_stats,
            gpu_collection,
            gpu_static_info,
            gpu_dynamic_info,
            network_monitor: NetworkMonitor::new().ok(),
            process_list: Vec::new(),
            connections: Vec::new(),
            connection_filter: String::new(),
            connection_protocol_filter: None,
            connection_state_filter: None,
            system_info: None, // Will be fetched lazily on GUI thread
            system_info_tried: false,
            motherboard_sensors: Vec::new(), // Will be fetched lazily
            driver_info: Vec::new(),         // Will be fetched lazily
            pcie_devices: Vec::new(),        // Will be fetched lazily
            sata_devices: Vec::new(),        // Will be fetched lazily
            system_temps: None,              // Will be fetched lazily
            peripherals: None,               // Will be fetched lazily
            system_stats: SystemStats::new().ok(),
            context_switches_history: VecDeque::with_capacity(HISTORY_SIZE),
            interrupts_history: VecDeque::with_capacity(HISTORY_SIZE),
            prev_context_switches: 0,
            prev_interrupts: 0,
            cpu_history: VecDeque::with_capacity(HISTORY_SIZE),
            per_core_history: (0..cpu_core_count)
                .map(|_| VecDeque::with_capacity(HISTORY_SIZE))
                .collect(),
            memory_history: VecDeque::with_capacity(HISTORY_SIZE),
            gpu_history: (0..gpu_count)
                .map(|_| VecDeque::with_capacity(HISTORY_SIZE))
                .collect(),
            gpu_memory_history: (0..gpu_count)
                .map(|_| VecDeque::with_capacity(HISTORY_SIZE))
                .collect(),
            gpu_temp_history: (0..gpu_count)
                .map(|_| VecDeque::with_capacity(HISTORY_SIZE))
                .collect(),
            network_rx_history: VecDeque::with_capacity(HISTORY_SIZE),
            network_tx_history: VecDeque::with_capacity(HISTORY_SIZE),
            network_rates: HashMap::new(),
            last_update: Instant::now(),
            last_slow_update: Instant::now(),
            start_time: Instant::now(),
            hostname: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            os_info: std::env::consts::OS.to_string(),
            process_sort_column: ProcessSortColumn::Cpu,
            process_sort_ascending: false,
            process_filter: String::new(),
            nettools_target_host: "8.8.8.8".to_string(),
            nettools_ping_result: None,
            nettools_traceroute_result: None,
            nettools_port_scan_results: Vec::new(),
            nettools_nmap_result: None,
            nettools_capture_result: None,
            nettools_capture_protocol: crate::network_tools::CaptureProtocol::All,
            nettools_capture_count: 50,
            nettools_port_range_start: 1,
            nettools_port_range_end: 1024,
            nettools_is_running: false,
            nettools_operation: String::new(),
            nettools_dns_results: Vec::new(),

            // AI Agent - loading in background
            agent: None,       // Will be populated when background thread completes
            ai_data_api: None, // Loading in background
            agent_query: String::new(),
            agent_history: VecDeque::with_capacity(50),
            agent_is_processing: false,
            agent_response_receiver: None,

            // AI configuration UI
            ai_api_key_input: String::new(),
            ai_selected_backend: AiBackendSelection::default(),
            ai_prev_backend: AiBackendSelection::default(),
            ai_selected_model: String::new(), // Will be set when ollama models load
            models_by_provider: std::collections::HashMap::new(),
            ai_ollama_starting: false,
            ai_status_message: None,

            // Background loading state
            system_info_receiver: None,
            system_info_loading: false,
            disk_receiver: None,
            disk_loading: false,
            disk_loaded: false,
            agent_receiver: Some(agent_rx),
            agent_loading: true,
            agent_loading_start: Instant::now(),
            agent_detect_attempts: 0,
            agent_detect_next_retry: None,
            models_receiver: None,
            models_loading_for: None,
            ai_data_api_receiver: Some(ai_api_rx),

            // Cached disk data (avoid per-frame I/O)
            cached_disk_data: Vec::new(),
            disk_rows_receiver: None,

            // Snapshot pipeline
            collector: Some(crate::pipeline::Collector::spawn(
                crate::pipeline::CollectorConfig {
                    interval: DATA_POLL_INTERVAL,
                    history_size: HISTORY_SIZE,
                    // The process table costs ~970ms to build (484 processes, each an
                    // OpenProcess plus a SID lookup) and the connection table is not
                    // cheap either, while the GUI only reads both every 2s and only
                    // on their own tabs. Collecting them every tick had the collector
                    // thread running flat out: a full tick cost more than the tick
                    // interval, so it never idled.
                    process_every_n_ticks: 2,
                    connection_every_n_ticks: 2,
                    // Let the collector wake the UI the moment a snapshot lands,
                    // instead of the UI waking on a timer to look for one.
                    on_publish: Some({
                        let ctx = cc.clone();
                        std::sync::Arc::new(move || ctx.request_repaint())
                    }),
                    ..Default::default()
                },
            )),
            snapshot: std::sync::Arc::new(crate::pipeline::Snapshot::default()),
            applied_generation: 0,

            // Settings
            show_settings: false,
            settings: AppSettings::default(),

            // Alert system
            active_alerts: Vec::new(),

            // Historical data for AI queries
            historical_data: Vec::new(),
            last_historical_save: std::time::Instant::now(),

            // Profile inspector
            profile_snapshot: None,
            profile_filter: String::new(),
            profile_subsystem_filter: None,
            profile_snapshot_receiver: None,
            profile_snapshot_loading: false,
            profile_deviations_cache: None,
            profile_audit_tail_cache: Vec::new(),
            profile_audit_last_read: None,
            profile_load_attempted: false,
            profile_sync_attempted: false,
            applied_color_theme: None,
            process_list_version: 0,
            processes_view_cache: Vec::new(),
            processes_view_key: None,
        };

        // Initialize history with zeros
        for _ in 0..HISTORY_SIZE {
            app.cpu_history.push_back(0.0);
            app.memory_history.push_back(0.0);
            app.network_rx_history.push_back(0.0);
            app.network_tx_history.push_back(0.0);
            app.context_switches_history.push_back(0.0);
            app.interrupts_history.push_back(0.0);
            for hist in &mut app.gpu_history {
                hist.push_back(0.0);
            }
            for hist in &mut app.gpu_memory_history {
                hist.push_back(0.0);
            }
            for hist in &mut app.gpu_temp_history {
                hist.push_back(0.0);
            }
            for hist in &mut app.per_core_history {
                hist.push_back(0.0);
            }
        }

        // Initialize previous context switch/interrupt values
        if let Some(ref stats) = app.system_stats {
            if let Some(ref vm) = stats.vm_stats {
                app.prev_context_switches = vm.context_switches;
                app.prev_interrupts = vm.interrupts;
            }
        }

        // Preload heavy per-tab data in background threads so the first click
        // on any tab does not pay a synchronous enumeration cost. Each spawn
        // is fire-and-forget; results land via the receivers drained by
        // check_background_loaders().
        app.start_disk_loading();
        app.start_system_info_loading();
        app.start_profile_load(false);

        app
    }

    /// Fold the newest collector snapshot into UI state.
    ///
    /// Returns `true` when a new generation was applied.
    ///
    /// Performs no hardware I/O — an atomic load plus in-memory mapping — so it
    /// cannot stall a frame. This replaces the previous `start_data_poll` /
    /// `apply_data_update` pair, which spawned a thread per poll, re-ran
    /// `GpuCollection::auto_detect()` on every one of them, and still performed
    /// blocking `NetworkMonitor::interfaces()` calls on the UI thread while
    /// "applying" results.
    fn sync_snapshot(&mut self) -> bool {
        let Some(ref collector) = self.collector else {
            return false;
        };
        let handle = collector.handle();

        // Generation 0 is the placeholder published before the first tick.
        let generation = handle.generation();
        if generation == 0 || generation == self.applied_generation {
            return false;
        }
        self.snapshot = handle.latest();
        self.applied_generation = generation;
        let snapshot = std::sync::Arc::clone(&self.snapshot);

        // === CPU ===
        if let Some(ref stats) = snapshot.cpu {
            let cpu_usage = 100.0 - stats.total.idle;
            self.cpu_history.pop_front();
            self.cpu_history.push_back(cpu_usage);

            for (i, core) in stats.cores.iter().enumerate() {
                if i < self.per_core_history.len() {
                    let util = core.user.unwrap_or(0.0) + core.system.unwrap_or(0.0);
                    self.per_core_history[i].pop_front();
                    self.per_core_history[i].push_back(util);
                }
            }

            self.cpu_stats = Some(stats.clone());
        }

        // === Memory ===
        if let Some(ref stats) = snapshot.memory {
            let usage = stats.ram_usage_percent();
            self.memory_history.pop_front();
            self.memory_history.push_back(usage);
            self.memory_stats = Some(stats.clone());
        }

        // === GPU ===
        // Static descriptors are captured once by the collector and never change at
        // runtime, so a length change means the device set itself changed.
        if !snapshot.gpu_static.is_empty()
            && self.gpu_static_info.len() != snapshot.gpu_static.len()
        {
            let new_count = snapshot.gpu_static.len();
            self.gpu_static_info = snapshot.gpu_static.clone();

            let fresh_series = || {
                let mut v = VecDeque::with_capacity(HISTORY_SIZE);
                for _ in 0..HISTORY_SIZE {
                    v.push_back(0.0);
                }
                v
            };
            self.gpu_history.resize_with(new_count, fresh_series);
            self.gpu_memory_history.resize_with(new_count, fresh_series);
            self.gpu_temp_history.resize_with(new_count, fresh_series);
        }

        // `snapshot.gpu_dynamic` is index-aligned with `gpu_static` and carries `None`
        // for devices whose query failed this tick. Keep the last-known sample for a
        // failed slot rather than dropping it, which would shift every device after it
        // and mislabel the charts.
        if snapshot.gpu_dynamic.len() == self.gpu_dynamic_info.len() {
            for (slot, fresh) in self
                .gpu_dynamic_info
                .iter_mut()
                .zip(snapshot.gpu_dynamic.iter())
            {
                if let Some(fresh) = fresh {
                    *slot = fresh.clone();
                }
            }
        } else {
            self.gpu_dynamic_info = snapshot.gpu_dynamic.iter().flatten().cloned().collect();
        }

        for (i, info) in snapshot.gpu_dynamic.iter().enumerate() {
            let Some(info) = info.as_ref() else { continue };

            if i < self.gpu_history.len() {
                self.gpu_history[i].pop_front();
                if let Some(util) = info.utilization {
                    self.gpu_history[i].push_back(util as f32);
                }
            }

            if i < self.gpu_memory_history.len() {
                // A device that reported no memory contributes no sample: the
                // series holds its last value rather than dropping to zero,
                // which is what the CPU and GPU histories elsewhere do.
                if let Some(mem_pct) = info.memory.utilization {
                    self.gpu_memory_history[i].pop_front();
                    self.gpu_memory_history[i].push_back(mem_pct as f32);
                }
            }

            if i < self.gpu_temp_history.len() {
                self.gpu_temp_history[i].pop_front();
                let temp = info.thermal.temperature.unwrap_or(0) as f32;
                self.gpu_temp_history[i].push_back(temp);
            }
        }

        // === Network ===
        // The previous code charted `(cumulative_bytes / 1MB) % 10000`, which is a
        // sawtooth of a running total rather than a throughput series. These are now
        // actual rates in MB/s, matching the axis label.
        // A tick with no established rate advances neither series: popping and
        // pushing a zero would draw a trough that did not happen, and popping
        // without pushing would shrink a fixed-width history.
        if let Some(rx) = snapshot.total_rx_rate() {
            self.network_rx_history.pop_front();
            self.network_rx_history
                .push_back((rx / 1024.0 / 1024.0) as f32);
        }
        if let Some(tx) = snapshot.total_tx_rate() {
            self.network_tx_history.pop_front();
            self.network_tx_history
                .push_back((tx / 1024.0 / 1024.0) as f32);
        }

        self.network_rates.clear();
        for iface in &snapshot.network {
            self.network_rates
                .insert(iface.name.clone(), (iface.rx_rate, iface.tx_rate));
        }

        true
    }

    /// Slow update for heavy operations (processes, connections)
    fn update_data_slow(&mut self) {
        // Processes, connections and system stats all arrive with the snapshot now.
        // Previously this ran `monitor.processes()`, `all_connections()` and
        // `SystemStats::new()` synchronously on the UI thread every 2s, which is
        // exactly the kind of stall that shows up as a dropped frame.
        //
        // The copies below are still gated on tab visibility: cloning a 300-entry
        // process table for a panel nobody is looking at is pure waste.
        if (self.current_tab == Tab::Processes || self.process_list.is_empty())
            && !self.snapshot.processes.is_empty()
        {
            self.process_list = self.snapshot.processes.clone();
            self.process_list_version = self.process_list_version.wrapping_add(1);
        }

        if self.current_tab == Tab::Connections && !self.snapshot.connections.is_empty() {
            self.connections = self.snapshot.connections.clone();
        }

        // System Stats (load avg, vmstat, etc.)
        if let Some(stats) = self.snapshot.system_stats.clone() {
            // Track context switches and interrupts per interval.
            if let Some(ref vm) = stats.vm_stats {
                let ctx_delta = vm
                    .context_switches
                    .saturating_sub(self.prev_context_switches);
                let int_delta = vm.interrupts.saturating_sub(self.prev_interrupts);

                self.context_switches_history.pop_front();
                self.context_switches_history.push_back(ctx_delta as f32);

                self.interrupts_history.pop_front();
                self.interrupts_history.push_back(int_delta as f32);

                self.prev_context_switches = vm.context_switches;
                self.prev_interrupts = vm.interrupts;
            }

            self.system_stats = Some(stats);
        }
    }

    /// Check for completed background loading operations (non-blocking)
    /// Advance background loaders once, outside the interactive event loop.
    ///
    /// [`Self::update`] calls [`Self::check_background_loaders`] every frame, so
    /// an interactive session picks up finished loads as a matter of course. The
    /// headless renderer draws a tab directly and never runs `update`, so nothing
    /// ever collected the results and four tabs — disk, system, peripherals,
    /// profiles — could only ever render their "Loading …" placeholder. An agent
    /// reading the GUI through `--frame` or `--script` therefore could not see
    /// the tabs carrying the disk, PCI and USB work at all.
    pub fn pump_background_loaders(&mut self, ctx: &egui::Context) {
        self.check_background_loaders(ctx);
    }

    /// Whether a background load *the currently selected tab draws from* is still
    /// in flight.
    ///
    /// Per tab, not global. A global answer made every headless read wait for
    /// every loader: the memory and network tabs render immediately and went from
    /// instant to twelve seconds because they were waiting on a peripherals query
    /// they do not draw.
    ///
    /// The disk case needs both conditions. That tab has three states, not two —
    /// spinner, then "No Disks Detected", then rows — and the middle one is
    /// indistinguishable by text from a machine that genuinely has no disks. So
    /// enumeration finishing is not enough; the rows have to have arrived.
    ///
    /// Only loads that gate painted content count. A pending agent-model probe
    /// does not stop a tab rendering, and waiting on it would make every headless
    /// read as slow as a network timeout.
    pub fn has_pending_load(&self) -> bool {
        match self.current_tab {
            Tab::Disk => {
                self.disk_loading
                    || (self.disk_rows_receiver.is_some() && self.cached_disk_data.is_empty())
            }
            Tab::SystemInfo => self.system_info_loading,
            // Peripherals paints USB and PCIe and no driver table, so it is
            // ready as soon as the fast instalment lands. Waiting on
            // `system_info_loading` meant waiting on `get_driver_versions`,
            // 20 seconds of work whose result this tab never displays.
            Tab::Peripherals => self.peripherals.is_none() && self.system_info_loading,
            Tab::Profiles => self.profile_snapshot_loading,
            _ => false,
        }
    }

    fn check_background_loaders(&mut self, ctx: &egui::Context) {
        // Hardware data no longer arrives through a per-poll channel; it is pulled
        // from the collector's published snapshot in `sync_snapshot`.

        // Check profile snapshot background loading. Handle both the
        // "still loading" (Empty) and "thread died without sending"
        // (Disconnected) cases so the tab never gets stuck on the spinner.
        if let Some(receiver) = self.profile_snapshot_receiver.take() {
            match receiver.try_recv() {
                Ok(snapshot) => {
                    self.profile_snapshot = Some(snapshot);
                    self.profile_snapshot_loading = false;
                    self.profile_deviations_cache = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Still loading; keep the receiver for the next frame.
                    self.profile_snapshot_receiver = Some(receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Background thread exited without sending — most likely
                    // panicked. Clear loading flags so the next visit can
                    // retry (sync fallback) instead of spinning forever.
                    self.profile_snapshot_loading = false;
                }
            }
        }

        // Check AI agent background loading
        if let Some(ref receiver) = self.agent_receiver {
            if let Ok(agent) = receiver.try_recv() {
                self.agent = agent;
                self.agent_loading = false;
                self.agent_receiver = None;

                if self.agent.is_none() {
                    // Detection is five HTTP probes with a 1s connect timeout each,
                    // fired during the startup stampede — GPU enumeration, disk
                    // enumeration and an `ollama list` are all running at that
                    // moment. A probe that loses that race used to latch the tab into
                    // "not connected" permanently, because nothing ever looked again.
                    self.agent_detect_attempts = self.agent_detect_attempts.saturating_add(1);
                    let backoff = match self.agent_detect_attempts {
                        1 => Duration::from_secs(15),
                        2 => Duration::from_secs(30),
                        _ => Duration::from_secs(60),
                    };
                    self.agent_detect_next_retry = Some(Instant::now() + backoff);
                } else {
                    self.agent_detect_attempts = 0;
                    self.agent_detect_next_retry = None;
                }
            }
        }

        // Re-probe on the backoff schedule until something is found.
        if self.agent_receiver.is_none() && self.agent.is_none() {
            if let Some(due) = self.agent_detect_next_retry {
                if Instant::now() >= due {
                    self.agent_detect_next_retry = None;
                    self.spawn_agent_detection();
                }
            }
        }

        // Enumeration finished: hand the devices to a refresher thread, which owns
        // them from here on and publishes rows the UI can draw without touching a
        // device.
        if let Some(ref receiver) = self.disk_receiver {
            if let Ok(disks) = receiver.try_recv() {
                let (tx, rx) = channel();
                self.disk_rows_receiver = Some(rx);
                Self::spawn_disk_refresher(disks, tx, ctx.clone());
                self.disk_loading = false;
                self.disk_loaded = true;
                self.disk_receiver = None;
            }
        }

        // Newest disk sample. `try_iter` drains any backlog so the UI always draws
        // the most recent rows rather than working through stale ones.
        if let Some(ref receiver) = self.disk_rows_receiver {
            if let Some(rows) = receiver.try_iter().last() {
                self.cached_disk_data = rows;
            }
        }

        // Check system info background loading
        if let Some(ref receiver) = self.system_info_receiver {
            // Drain rather than take one: both instalments can land between
            // frames, and stopping after the first would leave the slow half
            // sitting in the channel until the next repaint.
            let mut finished = false;
            while let Ok(update) = receiver.try_recv() {
                match update {
                    SystemInfoUpdate::Fast(fast) => {
                        let FastSystemInfo {
                            system_info,
                            sensors,
                            pcie_devices,
                            sata_devices,
                            peripherals,
                        } = *fast;
                        self.system_info = system_info;
                        self.motherboard_sensors = sensors;
                        self.pcie_devices = pcie_devices;
                        self.sata_devices = sata_devices;
                        self.peripherals = peripherals;
                    }
                    SystemInfoUpdate::Slow {
                        drivers,
                        system_temps,
                    } => {
                        self.driver_info = drivers;
                        self.system_temps = system_temps;
                        finished = true;
                    }
                }
            }
            if finished {
                self.system_info_loading = false;
                self.system_info_tried = true;
                self.system_info_receiver = None;
            }
        }

        // Check AI agent response (non-blocking)
        if let Some(ref receiver) = self.agent_response_receiver {
            if let Ok(result) = receiver.try_recv() {
                match result {
                    Ok(response) => {
                        self.agent_history.push_back(AgentChatEntry {
                            role: ChatRole::Assistant,
                            content: response.response,
                            timestamp: std::time::Instant::now(),
                            inference_time_ms: Some(response.inference_time_ms),
                            from_cache: response.from_cache,
                        });
                    }
                    Err(e) => {
                        self.agent_history.push_back(AgentChatEntry {
                            role: ChatRole::Assistant,
                            content: format!("Error: {}", e),
                            timestamp: std::time::Instant::now(),
                            inference_time_ms: None,
                            from_cache: false,
                        });
                    }
                }
                self.agent_is_processing = false;
                self.agent_response_receiver = None;

                // Limit history size
                while self.agent_history.len() > 100 {
                    self.agent_history.pop_front();
                }
            }
        }

        // A provider answered with its model list.
        if let Some(ref receiver) = self.models_receiver {
            if let Ok((provider, models)) = receiver.try_recv() {
                // Pick a default only for the provider the user is actually on, and
                // only if they have not chosen one.
                if provider == self.ai_selected_backend && self.ai_selected_model.is_empty() {
                    if let Some(first) = models.first() {
                        self.ai_selected_model = first.clone();
                    }
                }
                self.models_by_provider.insert(provider, models);
                self.models_receiver = None;
                self.models_loading_for = None;
            }
        }

        // Load the selected provider's models once, and again whenever the selection
        // changes. Doing this here rather than in the tab's draw code means the list
        // is ready before the tab is first opened.
        if self.models_receiver.is_none()
            && !self
                .models_by_provider
                .contains_key(&self.ai_selected_backend)
        {
            self.fetch_models_async(self.ai_selected_backend, ctx.clone());
        }

        // Check AI Data API background loading
        if let Some(ref receiver) = self.ai_data_api_receiver {
            if let Ok(api) = receiver.try_recv() {
                self.ai_data_api = api;
                self.ai_data_api_receiver = None;
            }
        }
    }

    /// Refresh cached disk data (called periodically, not every frame)
    /// Sample every disk on a worker thread, forever, publishing plain rows.
    ///
    /// This work used to run on the UI thread every 2s. `health()` is a SMART query
    /// and `filesystem_info()` walks volumes — both are device I/O measured in
    /// hundreds of milliseconds — so the window locked up on a fixed 2s beat. No
    /// amount of caching fixes that when the cache is filled by the thread that has
    /// to draw.
    ///
    /// The devices are `Send`, so the worker takes ownership of them and the UI never
    /// holds a `DiskDevice` at all. It cannot block on one by accident again.
    fn spawn_disk_refresher(
        disks: Vec<Box<dyn DiskDevice + Send>>,
        tx: Sender<Vec<CachedDiskData>>,
        ctx: egui::Context,
    ) {
        std::thread::spawn(move || {
            // Disk paths reach WMI on Windows, which needs COM per thread.
            let _com = crate::pipeline::com_guard();

            loop {
                let pass_start = Instant::now();
                let rows: Vec<CachedDiskData> = disks
                    .iter()
                    .map(|disk| CachedDiskData {
                        name: disk.name().to_string(),
                        disk_type: disk.disk_type(),
                        info: disk.info().ok(),
                        io_stats: disk.io_stats().ok(),
                        health: disk.health().ok(),
                        filesystems: disk.filesystem_info().unwrap_or_default(),
                        temperature: disk.temperature().ok().flatten(),
                    })
                    .collect();

                // A closed channel means the app is gone.
                if tx.send(rows).is_err() {
                    return;
                }
                ctx.request_repaint();

                // Sampling every drive measured 1.7s on this machine — `io_stats`
                // alone is 260-550ms per drive — so a fixed 2s beat would keep this
                // thread almost permanently busy. Back off in proportion to what the
                // pass actually cost, so disk polling stays a small fraction of a
                // core no matter how slow the drives are.
                let elapsed = pass_start.elapsed();
                std::thread::sleep(
                    DISK_REFRESH_INTERVAL.max(elapsed.mul_f32(1.0 / DISK_MAX_DUTY_CYCLE)),
                );
            }
        });
    }

    /// Start lazy loading of disk data
    fn start_disk_loading(&mut self) {
        if self.disk_loaded || self.disk_loading {
            return;
        }

        self.disk_loading = true;
        let (tx, rx) = channel();
        self.disk_receiver = Some(rx);

        std::thread::spawn(move || {
            let disks = disk::enumerate_disks().unwrap_or_default();
            // Convert to Send-able type
            let sendable_disks: Vec<Box<dyn DiskDevice + Send>> = disks
                .into_iter()
                .map(|d| d as Box<dyn DiskDevice + Send>)
                .collect();
            let _ = tx.send(sendable_disks);
        });
    }

    /// Start background loading of the hardware profile snapshot.
    /// `force` invalidates the cache (used by the Refresh button); idle calls
    /// short-circuit if a snapshot is already loaded or a load is in flight.
    pub(super) fn start_profile_load(&mut self, force: bool) {
        if self.profile_snapshot_loading || self.profile_snapshot_receiver.is_some() {
            return;
        }
        if !force && self.profile_snapshot.is_some() {
            return;
        }
        // If a previous attempt finished (success or panic) and produced no
        // recoverable result, do not respawn on every frame. The user can
        // force a retry via the Refresh button.
        if !force && self.profile_load_attempted {
            return;
        }
        if force {
            self.profile_sync_attempted = false;
        }
        self.profile_load_attempted = true;
        let (tx, rx) = channel();
        self.profile_snapshot_receiver = Some(rx);
        self.profile_snapshot_loading = true;
        // catch_unwind: a panic in any provider must not leave the loader
        // stuck — drop the sender so the receiver sees Disconnected and the
        // UI can offer a retry.
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut inspector = crate::profile::cache::CachedProfileInspector::new();
                if force {
                    inspector.invalidate(None);
                }
                inspector.snapshot_all()
            }));
            if let Ok(snapshot) = result {
                let _ = tx.send(snapshot);
            }
            // On panic, tx is dropped → receiver gets Disconnected.
        });
    }

    /// Synchronous profile load fallback. Called from the Profiles tab when
    /// no snapshot is available and no background load is in flight (e.g.
    /// the background thread panicked, or returned an empty snapshot). This
    /// blocks the UI for the duration of the scan but guarantees data
    /// appears instead of a blank tab.
    pub(super) fn load_profile_snapshot_sync(&mut self, force: bool) {
        let mut inspector = crate::profile::cache::CachedProfileInspector::new();
        if force {
            inspector.invalidate(None);
        }
        self.profile_snapshot = Some(inspector.snapshot_all());
        self.profile_deviations_cache = None;
    }

    /// Start lazy loading of system info
    fn start_system_info_loading(&mut self) {
        if self.system_info_tried || self.system_info_loading {
            return;
        }

        self.system_info_loading = true;
        let (tx, rx) = channel();
        self.system_info_receiver = Some(rx);

        std::thread::spawn(move || collect_system_info(tx));
    }

    fn cpu_usage(&self) -> f32 {
        self.cpu_stats
            .as_ref()
            .map(|s| 100.0 - s.total.idle)
            .unwrap_or(0.0)
    }

    fn memory_usage(&self) -> f32 {
        self.memory_stats
            .as_ref()
            .map(|s| s.ram_usage_percent())
            .unwrap_or(0.0)
    }
    /// Export current system data to JSON format
    fn export_to_json(&self) -> Result<String, String> {
        use serde_json::json;
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let cpu_data = self.cpu_stats.as_ref().map(|s| {
            let usage = 100.0 - s.total.idle;
            // `unwrap_or(0)` published 0 MHz as a frequency; null says the
            // clock was not read.
            let freq = s
                .cores
                .first()
                .and_then(|c| c.frequency.as_ref())
                .and_then(|f| f.current);
            json!({
                "usage_percent": usage,
                "frequency_mhz": freq,
                "core_count": s.cores.len(),
            })
        });

        let mem_data = self.memory_stats.as_ref().map(|s| {
            json!({
                "total_bytes": s.ram.total,
                "used_bytes": s.ram.used,
                "available_bytes": s.ram.free,
                "usage_percent": s.ram_usage_percent(),
            })
        });

        let gpu_data: Vec<_> = self
            .gpu_static_info
            .iter()
            .zip(self.gpu_dynamic_info.iter())
            .map(|(static_info, dynamic)| {
                json!({
                    "name": static_info.name,
                    "vendor": format!("{:?}", static_info.vendor),
                    "memory_used_mb": dynamic.memory.used.map(|v| v / (1024 * 1024)),
                    "memory_total_mb": dynamic.memory.total.map(|v| v / (1024 * 1024)),
                    "temperature_c": dynamic.thermal.temperature,
                    "utilization_percent": dynamic.utilization,
                    "power_mw": dynamic.power.draw,
                })
            })
            .collect();

        let data = json!({
            "timestamp": timestamp,
            "cpu": cpu_data,
            "memory": mem_data,
            "gpus": gpu_data,
            "processes_count": self.process_list.len(),
            "network_interfaces": self.network_rates.len(),
        });

        serde_json::to_string_pretty(&data).map_err(|e| e.to_string())
    }

    /// Export current system data to CSV format
    fn export_to_csv(&self) -> String {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let mut csv = String::from("metric,value,unit,timestamp\n");

        if let Some(cpu) = &self.cpu_stats {
            let usage = 100.0 - cpu.total.idle;
            csv.push_str(&format!("cpu_usage,{:.1},percent,{}\n", usage, timestamp));
            // No row rather than a row of zero: an export is read later by
            // someone who cannot ask what the zero meant.
            if let Some(mhz) = cpu
                .cores
                .first()
                .and_then(|c| c.frequency.as_ref())
                .and_then(|f| f.current)
            {
                csv.push_str(&format!("cpu_frequency,{mhz},MHz,{timestamp}\n"));
            }
            csv.push_str(&format!(
                "cpu_cores,{},count,{}\n",
                cpu.cores.len(),
                timestamp
            ));
        }

        if let Some(mem) = &self.memory_stats {
            csv.push_str(&format!(
                "memory_total,{},bytes,{}\n",
                mem.ram.total, timestamp
            ));
            csv.push_str(&format!(
                "memory_used,{},bytes,{}\n",
                mem.ram.used, timestamp
            ));
            csv.push_str(&format!(
                "memory_usage,{:.1},percent,{}\n",
                mem.ram_usage_percent(),
                timestamp
            ));
        }

        for (i, (static_info, dynamic)) in self
            .gpu_static_info
            .iter()
            .zip(self.gpu_dynamic_info.iter())
            .enumerate()
        {
            csv.push_str(&format!(
                "gpu{}_name,\"{}\",string,{}\n",
                i, static_info.name, timestamp
            ));
            // An empty CSV field where the device reported nothing, which
            // every reader treats as missing; `0` would be read as a
            // measurement of an empty card.
            csv.push_str(&format!(
                "gpu{}_memory_used,{},MB,{}\n",
                i,
                dynamic
                    .memory
                    .used
                    .map(|v| (v / (1024 * 1024)).to_string())
                    .unwrap_or_default(),
                timestamp
            ));
            csv.push_str(&format!(
                "gpu{}_memory_total,{},MB,{}\n",
                i,
                dynamic
                    .memory
                    .total
                    .map(|v| (v / (1024 * 1024)).to_string())
                    .unwrap_or_default(),
                timestamp
            ));
            if let Some(temp) = dynamic.thermal.temperature {
                csv.push_str(&format!("gpu{}_temperature,{},C,{}\n", i, temp, timestamp));
            }
            csv.push_str(&format!(
                "gpu{}_utilization,{},percent,{}\n",
                i,
                dynamic
                    .utilization
                    .map(|u| u.to_string())
                    .unwrap_or_default(),
                timestamp
            ));
            if let Some(power) = dynamic.power.draw {
                csv.push_str(&format!("gpu{}_power,{:.1},mW,{}\n", i, power, timestamp));
            }
        }

        csv
    }
}

impl eframe::App for IronMonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme only when it actually changes. `apply_*_theme` calls
        // `ctx.set_fonts()` which rebuilds the font atlas — doing that every
        // frame causes ~100ms paint stalls and ruins tab-switch latency.
        let want = self.settings.color_theme;
        if self.applied_color_theme != Some(want) {
            match want {
                ColorTheme::Light => theme::apply_light_theme(ctx),
                _ => theme::apply_cyber_theme(ctx),
            }
            self.applied_color_theme = Some(want);
        }

        // Alerts are derived purely from the latest snapshot, so evaluating them per
        // frame re-derived the same verdict from the same numbers and rebuilt its
        // message strings each time. Moved below, to run only when data changes.

        // Save historical data periodically (every minute)
        if self.last_historical_save.elapsed() >= std::time::Duration::from_secs(60) {
            self.save_historical_data();
            self.last_historical_save = std::time::Instant::now();
        }

        // Check for background loading completions (non-blocking) - must be first!
        self.check_background_loaders(ctx);

        // Fold in the newest snapshot. Cheap enough to attempt every frame: when the
        // collector has not published since the last frame this is a single atomic
        // load and an early return.
        let got_new_data = self.sync_snapshot();

        if got_new_data && self.settings.alert_settings.enabled {
            self.check_alerts();
        }

        // Slow updates (Processes, Connections, System Stats) - every 2s
        if self.last_slow_update.elapsed() >= SLOW_UPDATE_INTERVAL {
            self.update_data_slow();
            self.last_slow_update = Instant::now();
        }

        // Schedule the next repaint against the collector's cadence rather than a
        // fixed 100ms tick. Waking at 10 FPS to redraw identical pixels burns GPU and
        // battery for nothing; the data only changes when a snapshot lands.
        //
        // Frames are driven by the collector's publish hook, not by a timer: each
        // snapshot wakes the UI as it lands, so the displayed cadence is exactly the
        // collection cadence with no sampling jitter.
        //
        // Polling was what made updates look uneven. Waking on a fraction of the tick
        // means a publish is noticed anywhere from immediately to a poll-period late,
        // and that error changes every tick as collection cost varies — so the
        // display advanced twice in quick succession, then seemed to stall.
        //
        // The timer that remains is a safety net for the case where the collector
        // thread has stopped or was never started; at several times the tick it costs
        // nothing when the hook is working.
        let tick = self
            .collector
            .as_ref()
            .map(|c| c.handle().interval())
            .unwrap_or(DATA_POLL_INTERVAL);
        ctx.request_repaint_after(tick * 3);

        // Top panel with title and tabs
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // Logo/Title
                ui.heading(RichText::new("⚡ IronMonitor").color(CyberColors::CYAN));
                ui.separator();

                // Tabs - use local variable to avoid borrow issues
                let current = self.current_tab;
                let tab_color = |tab: Tab| {
                    if current == tab {
                        CyberColors::CYAN
                    } else {
                        CyberColors::TEXT_SECONDARY
                    }
                };

                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::Overview,
                    RichText::new("📊 Overview").color(tab_color(Tab::Overview)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::Cpu,
                    RichText::new("🔲 CPU").color(tab_color(Tab::Cpu)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::Accelerators,
                    RichText::new("⚡ Accelerators").color(tab_color(Tab::Accelerators)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::Memory,
                    RichText::new("💾 Memory").color(tab_color(Tab::Memory)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::Disk,
                    RichText::new("💿 Disk").color(tab_color(Tab::Disk)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::Processes,
                    RichText::new("📋 Processes").color(tab_color(Tab::Processes)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::Network,
                    RichText::new("🌐 Network").color(tab_color(Tab::Network)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::Connections,
                    RichText::new("🔌 Sockets").color(tab_color(Tab::Connections)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::NetworkTools,
                    RichText::new("🔧 Tools").color(tab_color(Tab::NetworkTools)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::Peripherals,
                    RichText::new("🔌 Peripherals").color(tab_color(Tab::Peripherals)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::SystemInfo,
                    RichText::new("🖥️ System").color(tab_color(Tab::SystemInfo)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::Profiles,
                    RichText::new("🛠 Profiles").color(tab_color(Tab::Profiles)),
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::AIAssistant,
                    RichText::new("🤖 AI").color(tab_color(Tab::AIAssistant)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Settings gear icon
                    let settings_btn = ui.add(
                        egui::Button::new(RichText::new("⚙").size(16.0))
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                    );
                    if settings_btn.clicked() {
                        self.show_settings = !self.show_settings;
                    }
                    if settings_btn.hovered() {
                        settings_btn.on_hover_text("Settings");
                    }

                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("{}@{}", self.hostname, self.os_info))
                            .color(CyberColors::TEXT_SECONDARY)
                            .small(),
                    );
                });
            });
            ui.add_space(4.0);
        });

        // Bottom status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                // Quick stats
                let cpu_usage = self.cpu_usage();
                ui.label(
                    RichText::new(format!("CPU: {:.1}%", cpu_usage))
                        .color(theme::utilization_color(cpu_usage)),
                );
                ui.separator();

                let mem_usage = self.memory_usage();
                ui.label(
                    RichText::new(format!("RAM: {:.1}%", mem_usage))
                        .color(theme::utilization_color(mem_usage)),
                );
                ui.separator();

                for (i, gpu) in self.gpu_dynamic_info.iter().enumerate() {
                    ui.label(
                        RichText::new(match gpu.utilization {
                            Some(u) => format!("GPU{i}: {u}%"),
                            None => format!("GPU{i}: -"),
                        })
                        .color(theme::utilization_color(gpu.utilization.unwrap_or(0) as f32)),
                    );
                    if let Some(temp) = gpu.thermal.temperature {
                        ui.label(
                            RichText::new(format!("{}°C", temp))
                                .color(theme::temperature_color(temp as u32)),
                        );
                    }
                    ui.separator();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Export buttons
                    if ui.small_button("CSV").clicked() {
                        let csv = self.export_to_csv();
                        ui.output_mut(|o| o.copied_text = csv);
                    }
                    ui.add_space(4.0);
                    if ui.small_button("JSON").clicked() {
                        if let Ok(json) = self.export_to_json() {
                            ui.output_mut(|o| o.copied_text = json);
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("F1=Help")
                            .color(CyberColors::TEXT_MUTED)
                            .small(),
                    );
                });
            });
            ui.add_space(2.0);
        });

        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| self.draw_current_tab(ui));

        // Settings window (floating)
        self.draw_settings_window(ctx);
    }
}

impl IronMonitorApp {
    /// Draw whichever tab is selected.
    ///
    /// Split out of `update` so that `ironmon gui --frame` and the headless tests can
    /// render one tab without driving a whole eframe cycle. Both paths go through
    /// this, so a frame an agent reads is drawn by the same code a user sees.
    pub fn draw_current_tab(&mut self, ui: &mut egui::Ui) {
        match self.current_tab {
            Tab::Overview => self.draw_overview(ui),
            Tab::Cpu => self.draw_cpu_tab(ui),
            Tab::Accelerators => self.draw_accelerators_tab(ui),
            Tab::Memory => self.draw_memory_tab(ui),
            Tab::Disk => self.draw_disk_tab(ui),
            Tab::Processes => self.draw_processes_tab(ui),
            Tab::Network => self.draw_network_tab(ui),
            Tab::NetworkTools => self.draw_network_tools_tab(ui),
            Tab::Connections => self.draw_connections_tab(ui),
            Tab::SystemInfo => self.draw_system_info_tab(ui),
            Tab::Peripherals => self.draw_peripherals_tab(ui),
            Tab::Profiles => self.draw_profiles_tab(ui),
            Tab::AIAssistant => self.draw_ai_assistant_tab(ui),
        }
    }

    /// Select a tab by name, as an agent reading the tab bar would refer to it.
    ///
    /// Returns the accepted names when the input matches none, so a caller is told
    /// what it may ask for rather than silently getting the default.
    pub fn select_tab_by_name(&mut self, name: &str) -> Result<(), Vec<&'static str>> {
        let all = [
            ("overview", Tab::Overview),
            ("cpu", Tab::Cpu),
            ("accelerators", Tab::Accelerators),
            ("memory", Tab::Memory),
            ("disk", Tab::Disk),
            ("processes", Tab::Processes),
            ("network", Tab::Network),
            ("tools", Tab::NetworkTools),
            ("connections", Tab::Connections),
            ("system", Tab::SystemInfo),
            ("peripherals", Tab::Peripherals),
            ("profiles", Tab::Profiles),
            ("ai", Tab::AIAssistant),
        ];
        match all.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some((_, tab)) => {
                self.current_tab = *tab;
                Ok(())
            }
            None => Err(all.iter().map(|(n, _)| *n).collect()),
        }
    }
}

impl IronMonitorApp {
    /// Check for threshold violations and generate alerts
    fn check_alerts(&mut self) {
        let settings = self.settings.alert_settings.clone();
        let mut new_alerts: Vec<(AlertSeverity, String)> = Vec::new();

        // Check CPU usage
        let cpu_usage = self.cpu_usage();
        if cpu_usage >= settings.cpu_critical_threshold {
            new_alerts.push((
                AlertSeverity::Critical,
                format!("CPU usage critical: {:.1}%", cpu_usage),
            ));
        } else if cpu_usage >= settings.cpu_warning_threshold {
            new_alerts.push((
                AlertSeverity::Warning,
                format!("CPU usage high: {:.1}%", cpu_usage),
            ));
        }

        // Check memory usage
        if let Some(ref mem) = self.memory_stats {
            let mem_percent = mem.ram_usage_percent();
            if mem_percent >= settings.memory_critical_threshold {
                new_alerts.push((
                    AlertSeverity::Critical,
                    format!("Memory usage critical: {:.1}%", mem_percent),
                ));
            } else if mem_percent >= settings.memory_warning_threshold {
                new_alerts.push((
                    AlertSeverity::Warning,
                    format!("Memory usage high: {:.1}%", mem_percent),
                ));
            }
        }

        // Check GPU temperatures
        for (i, gpu) in self.gpu_dynamic_info.iter().enumerate() {
            if let Some(temp) = gpu.thermal.temperature {
                if temp as f32 >= settings.gpu_temp_critical {
                    new_alerts.push((
                        AlertSeverity::Critical,
                        format!("GPU {} temperature critical: {}°C", i, temp),
                    ));
                } else if temp as f32 >= settings.gpu_temp_warning {
                    new_alerts.push((
                        AlertSeverity::Warning,
                        format!("GPU {} temperature high: {}°C", i, temp),
                    ));
                }
            }
        }

        // Add collected alerts
        for (severity, message) in new_alerts {
            self.add_alert(severity, message);
        }

        // Remove old alerts (older than 30 seconds)
        let now = std::time::Instant::now();
        self.active_alerts
            .retain(|a| now.duration_since(a.timestamp).as_secs() < 30);
    }

    /// Add an alert if not already present
    fn add_alert(&mut self, severity: AlertSeverity, message: String) {
        if !self.active_alerts.iter().any(|a| a.message == message) {
            self.active_alerts.push(Alert {
                severity,
                message,
                timestamp: std::time::Instant::now(),
            });
        }
    }

    /// Save current metrics to historical data for AI queries
    fn save_historical_data(&mut self) {
        let cpu_usage = self.cpu_usage();
        let memory_usage = self
            .memory_stats
            .as_ref()
            .map(|m| m.ram_usage_percent())
            .unwrap_or(0.0);
        let gpu_temps: Vec<f32> = self
            .gpu_dynamic_info
            .iter()
            .filter_map(|g| g.thermal.temperature.map(|t| t as f32))
            .collect();
        // Devices that reported a counter. One that did not contributes no
        // point rather than a zero, which would read as an idle card.
        let gpu_utils: Vec<f32> = self
            .gpu_dynamic_info
            .iter()
            .filter_map(|g| g.utilization)
            .map(f32::from)
            .collect();

        self.historical_data.push(HistoricalDataPoint {
            timestamp: std::time::Instant::now(),
            cpu_usage,
            memory_usage,
            gpu_temps,
            gpu_utils,
        });

        // Keep only last 60 minutes of data
        if self.historical_data.len() > 60 {
            self.historical_data.remove(0);
        }
    }

    /// Get historical summary for AI agent queries
    #[allow(dead_code)]
    pub fn get_historical_summary(&self, minutes_ago: u32) -> Option<String> {
        let now = std::time::Instant::now();
        let target_duration = std::time::Duration::from_secs(minutes_ago as u64 * 60);
        for point in self.historical_data.iter().rev() {
            let age = now.duration_since(point.timestamp);
            if age >= target_duration {
                let mut summary = format!(
                    "Historical data from {} minutes ago:\n- CPU: {:.1}%\n- Memory: {:.1}%",
                    age.as_secs() / 60,
                    point.cpu_usage,
                    point.memory_usage
                );
                if !point.gpu_temps.is_empty() {
                    summary.push_str(&format!("\n- GPU temps: {:?}°C", point.gpu_temps));
                }
                if !point.gpu_utils.is_empty() {
                    summary.push_str(&format!("\n- GPU utils: {:?}%", point.gpu_utils));
                }
                return Some(summary);
            }
        }
        None
    }

    /// Whether a question can actually be sent right now.
    ///
    /// Mirrors the AI tab's gate: what stops a send is having no model to send to,
    /// not what auto-detection happened to find. A CLI provider picks its own model,
    /// so an empty selection is fine there.
    pub(super) fn agent_can_answer(&self) -> bool {
        if matches!(self.ai_selected_backend, AiBackendSelection::Cli(_)) {
            return true;
        }
        if !self.ai_selected_model.is_empty() {
            return true;
        }
        self.models_by_provider
            .get(&self.ai_selected_backend)
            .is_some_and(|m| !m.is_empty())
            || !self.ai_selected_backend.fallback_models().is_empty()
    }

    /// Compact "ask about this machine" bar pinned to the top of the Overview tab.
    ///
    /// Shares `agent_query`, `agent_history` and `send_agent_query` with the AI tab,
    /// so this is a second entry point into one conversation rather than a second
    /// assistant: a question asked here appears in the AI tab's transcript and vice
    /// versa, and both honour the backend and model selected there.
    pub(super) fn draw_overview_chat_bar(&mut self, ui: &mut egui::Ui) {
        let can_answer = self.agent_can_answer();
        let mut submit = false;

        egui::Frame::none()
            .fill(CyberColors::SURFACE)
            .rounding(6.0)
            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Ask").color(CyberColors::CYAN).strong());

                    // Leave room for the button and status text on the right.
                    let field_width = (ui.available_width() - 190.0).max(160.0);
                    let response = ui.add_sized(
                        [field_width, 22.0],
                        egui::TextEdit::singleline(&mut self.agent_query)
                            .hint_text("e.g. why is my GPU hot?")
                            .interactive(can_answer && !self.agent_is_processing),
                    );
                    submit = response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !self.agent_query.trim().is_empty();

                    let button_enabled = can_answer
                        && !self.agent_is_processing
                        && !self.agent_query.trim().is_empty();
                    if ui
                        .add_enabled(button_enabled, egui::Button::new("Send"))
                        .clicked()
                    {
                        submit = true;
                    }

                    if self.agent_is_processing {
                        ui.spinner();
                    } else if !can_answer {
                        // Say which condition is unmet rather than "unavailable":
                        // the backend may be perfectly reachable and simply have no
                        // model chosen yet.
                        ui.label(
                            RichText::new("no model selected — see the AI tab")
                                .small()
                                .color(CyberColors::TEXT_SECONDARY),
                        );
                    }
                });

                // Most recent exchange, so an answer is visible without leaving the
                // tab. The full transcript stays in the AI tab.
                let last_answer = self
                    .agent_history
                    .iter()
                    .rev()
                    .find(|e| e.role == ChatRole::Assistant);
                if let Some(entry) = last_answer {
                    ui.add_space(4.0);
                    ui.separator();
                    let text = entry.content.trim();
                    // Keep the bar a bar: long answers are truncated here and read in
                    // full on the AI tab.
                    const MAX: usize = 400;
                    let shown: String = if text.chars().count() > MAX {
                        let mut s: String = text.chars().take(MAX).collect();
                        s.push('…');
                        s
                    } else {
                        text.to_string()
                    };
                    ui.label(RichText::new(shown).color(CyberColors::TEXT_PRIMARY));
                    if let Some(ms) = entry.inference_time_ms {
                        ui.label(
                            RichText::new(format!(
                                "{}ms{}",
                                ms,
                                if entry.from_cache { " · cached" } else { "" }
                            ))
                            .small()
                            .color(CyberColors::TEXT_SECONDARY),
                        );
                    }
                }
            });

        if submit {
            self.send_agent_query();
        }
    }

    fn draw_overview(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            self.draw_overview_chat_bar(ui);
            ui.add_space(6.0);

            // Glances-style QuickLook panel at the top
            let cpu_usage = self.cpu_usage();
            let mem_usage = self
                .memory_stats
                .as_ref()
                .map(|m| m.ram_usage_percent())
                .unwrap_or(0.0);
            // `swap_usage_percent()` already returns `Option` and already
            // handles a zero total; this recomputed it from `*_or_zero()` and
            // answered 0% for an unread pagefile.
            let swap_usage = self
                .memory_stats
                .as_ref()
                .and_then(|m| m.swap_usage_percent());
            let load_avg = self
                .system_stats
                .as_ref()
                .and_then(|s| s.load_average.as_ref())
                .map(|l| l.one as f32)
                .unwrap_or(0.0);

            // Calculate trends from history
            let cpu_trend = self
                .cpu_history
                .iter()
                .rev()
                .nth(1)
                .map(|&prev| trend_indicator(cpu_usage, prev).0)
                .unwrap_or("→");
            let mem_trend = self
                .memory_history
                .iter()
                .rev()
                .nth(1)
                .map(|&prev| trend_indicator(mem_usage, prev).0)
                .unwrap_or("→");

            ui.add(
                QuickLookPanel::new(cpu_usage, mem_usage, swap_usage, load_avg)
                    .with_trends(cpu_trend, mem_trend),
            );

            ui.add_space(4.0);

            // Threshold legend
            ui.add(ThresholdLegend);

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // System status bar (htop-style)
            ui.horizontal(|ui| {
                // Uptime
                let uptime = self.start_time.elapsed();
                let hours = uptime.as_secs() / 3600;
                let minutes = (uptime.as_secs() % 3600) / 60;
                let seconds = uptime.as_secs() % 60;
                ui.label(
                    RichText::new(format!("⏱ {:02}:{:02}:{:02}", hours, minutes, seconds))
                        .color(CyberColors::CYAN),
                );
                ui.separator();

                // Process state summary (htop-style: Tasks: X, Y thr; 1 running)
                let running_count = self.process_list.iter().filter(|p| p.state == 'R').count();
                let sleeping_count = self.process_list.iter().filter(|p| p.state == 'S').count();
                let zombie_count = self.process_list.iter().filter(|p| p.state == 'Z').count();
                let disk_wait_count = self.process_list.iter().filter(|p| p.state == 'D').count();
                let stopped_count = self.process_list.iter().filter(|p| p.state == 'T').count();

                ui.label(
                    RichText::new(format!("Tasks: {}", self.process_list.len()))
                        .color(CyberColors::TEXT_PRIMARY),
                );
                // Platforms that do not report per-process scheduling state mark it
                // 'U'. Showing "0R 0S" there would be as wrong as the "420R 0S" this
                // replaced — say the breakdown is unavailable instead.
                if running_count + sleeping_count + zombie_count + disk_wait_count + stopped_count
                    > 0
                {
                    ui.label(
                        RichText::new(format!("{}R", running_count)).color(CyberColors::NEON_GREEN),
                    );
                    ui.label(
                        RichText::new(format!("{}S", sleeping_count)).color(CyberColors::CYAN),
                    );
                }
                if zombie_count > 0 {
                    ui.label(
                        RichText::new(format!("{}Z", zombie_count)).color(CyberColors::NEON_RED),
                    );
                }
                if disk_wait_count > 0 {
                    ui.label(
                        RichText::new(format!("{}D", disk_wait_count))
                            .color(CyberColors::NEON_YELLOW),
                    );
                }
                if stopped_count > 0 {
                    ui.label(
                        RichText::new(format!("{}T", stopped_count))
                            .color(CyberColors::NEON_ORANGE),
                    );
                }
                ui.separator();

                // Connections count
                ui.label(
                    RichText::new(format!("🔌 {} Conn", self.connections.len()))
                        .color(CyberColors::NEON_PURPLE),
                );
                ui.separator();

                // Accelerator count
                if !self.gpu_static_info.is_empty() {
                    ui.label(
                        RichText::new(format!("⚡ {} Accel", self.gpu_static_info.len()))
                            .color(CyberColors::NEON_ORANGE),
                    );
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // Top metric cards
            ui.horizontal_wrapped(|ui| {
                // CPU Card
                let cpu_usage = self.cpu_usage();
                ui.add(
                    MetricCard::new("CPU Usage", format!("{:.1}", cpu_usage))
                        .unit("%")
                        .color(theme::cpu_color(cpu_usage)),
                );

                // Memory Card
                if let Some(ref mem) = self.memory_stats {
                    let usage = mem.ram_usage_percent();
                    // `RamInfo` is in KiB, so this is gibibytes — the card was
                    // labelled MB and read "78.6 MB" next to a bar showing 84% of
                    // 93.6 GB.
                    let used_gb = mem.ram.used as f64 / 1024.0 / 1024.0;
                    ui.add(
                        MetricCard::new("Memory", format!("{:.1}", used_gb))
                            .unit("GB")
                            .color(theme::memory_color(usage)),
                    );
                }

                // Accelerator Cards
                for (i, (static_info, dynamic_info)) in self
                    .gpu_static_info
                    .iter()
                    .zip(self.gpu_dynamic_info.iter())
                    .enumerate()
                {
                    use crate::gpu::GpuVendor;
                    let accel_type = match static_info.vendor {
                        GpuVendor::Nvidia
                        | GpuVendor::Amd
                        | GpuVendor::Intel
                        | GpuVendor::Apple => "GPU",
                    };
                    ui.add(
                        MetricCard::new(
                            &format!("{} {}", accel_type, i),
                            dynamic_info
                                .utilization
                                .map_or_else(|| "-".to_string(), |u| u.to_string()),
                        )
                        .unit("%")
                        .color(theme::accel_color(
                            dynamic_info.utilization.unwrap_or(0) as f32,
                        )),
                    );

                    if let Some(temp) = dynamic_info.thermal.temperature {
                        ui.add(
                            MetricCard::new(
                                &format!(
                                    "{} Temp",
                                    &static_info.name[..static_info.name.len().min(10)]
                                ),
                                temp,
                            )
                            .unit("°C")
                            .color(theme::temperature_color(temp as u32)),
                        );
                    }

                    // GPU Memory. A dash where the device reported none,
                    // rather than a card reading 0%.
                    let mem_label = dynamic_info
                        .memory
                        .utilization
                        .map_or_else(|| "-".to_string(), |p| format!("{p}"));
                    ui.add(
                        MetricCard::new(&format!("GPU{} Mem", i), mem_label)
                            .unit("%")
                            // No reading, no threshold colour: 0% would paint
                            // the card the "plenty free" colour for a device
                            // whose memory nobody could read.
                            .color(theme::memory_color(
                                dynamic_info.memory.utilization.unwrap_or(0) as f32,
                            )),
                    );
                }
            });

            ui.add_space(16.0);

            // Charts section
            ui.columns(2, |columns| {
                // CPU Chart
                columns[0].add(
                    SparklineChart::new(self.cpu_history.iter().cloned().collect())
                        .color(DeviceTitleColors::CPU)
                        .height(100.0)
                        .title("CPU Usage")
                        .unit("%")
                        .max_value(100.0)
                        .show_scale(true)
                        .show_min_max(true),
                );

                // Memory Chart
                columns[1].add(
                    SparklineChart::new(self.memory_history.iter().cloned().collect())
                        .color(DeviceTitleColors::MEMORY)
                        .height(100.0)
                        .title("Memory Usage")
                        .unit("%")
                        .max_value(100.0)
                        .show_scale(true)
                        .show_min_max(true),
                );
            });

            ui.add_space(16.0);

            // GPU Charts
            if !self.gpu_history.is_empty() {
                let gpu_title = super::widgets::domain_section_title("gpu", "Utilization");
                ui.add(SectionHeader::new(&gpu_title).icon("🎮"));
                let num_cols = self.gpu_history.len().min(4);
                ui.columns(num_cols.max(1), |columns| {
                    for (i, hist) in self.gpu_history.iter().enumerate() {
                        if i < columns.len() {
                            columns[i].add(
                                SparklineChart::new(hist.iter().cloned().collect())
                                    .color(DeviceTitleColors::ACCEL)
                                    .height(80.0)
                                    .title(format!("GPU {}", i))
                                    .unit("%")
                                    .max_value(100.0)
                                    .show_scale(true),
                            );
                        }
                    }
                });
            }

            ui.add_space(16.0);

            // Linux/BSD style System Stats (like htop/vmstat)
            // Not Linux/BSD-only: uptime and CPU count render on every platform, and
            // the entries that are Linux-sourced hide themselves when absent.
            let t = super::widgets::domain_section_title("system", "Stats");
            ui.add(SectionHeader::new(&t).icon("📈"));

            // System info row
            ui.horizontal(|ui| {
                // Load Average (htop/uptime style)
                if let Some(ref stats) = self.system_stats {
                    if let Some(ref load) = stats.load_average {
                        ui.label(
                            RichText::new(format!(
                                "⚖ Load: {:.2}, {:.2}, {:.2}",
                                load.one, load.five, load.fifteen
                            ))
                            .color(CyberColors::CYAN),
                        );
                        ui.separator();
                    }

                    // System uptime (from OS, not app)
                    if let Some(uptime) = stats.uptime_seconds {
                        let days = uptime / 86400;
                        let hours = (uptime % 86400) / 3600;
                        let mins = (uptime % 3600) / 60;
                        let uptime_str = if days > 0 {
                            format!("🖥 Uptime: {}d {:02}h {:02}m", days, hours, mins)
                        } else {
                            format!("🖥 Uptime: {:02}h {:02}m", hours, mins)
                        };
                        ui.label(RichText::new(uptime_str).color(CyberColors::NEON_GREEN));
                        ui.separator();
                    }

                    // Running/Total processes. Each half is shown only if it
                    // was read: Windows reports a total and no runnable count,
                    // macOS reports neither.
                    let tasks = match (stats.running_processes, stats.total_processes) {
                        (Some(running), Some(total)) => {
                            Some(format!("🔄 Tasks: {running} running, {total} total"))
                        }
                        (None, Some(total)) => Some(format!("🔄 Tasks: {total} total")),
                        (Some(running), None) => Some(format!("🔄 Tasks: {running} running")),
                        (None, None) => None,
                    };
                    if let Some(tasks) = tasks {
                        ui.label(RichText::new(tasks).color(CyberColors::NEON_PURPLE));
                        ui.separator();
                    }

                    // CPUs
                    if stats.num_cpus > 0 {
                        ui.label(
                            RichText::new(format!("💻 {} CPUs", stats.num_cpus))
                                .color(CyberColors::NEON_ORANGE),
                        );
                    }
                }
            });

            ui.add_space(8.0);

            // CPU Time breakdown (like vmstat/top)
            if let Some(ref stats) = self.system_stats {
                if let Some(ref cpu_time) = stats.cpu_time {
                    let total = cpu_time.total() as f32;
                    if total > 0.0 {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("CPU Time: ").color(CyberColors::TEXT_PRIMARY));
                            ui.label(
                                RichText::new(format!(
                                    "us:{:.1}%",
                                    (cpu_time.user as f32 / total) * 100.0
                                ))
                                .color(CyberColors::NEON_GREEN)
                                .small(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "sy:{:.1}%",
                                    (cpu_time.system as f32 / total) * 100.0
                                ))
                                .color(CyberColors::NEON_ORANGE)
                                .small(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "ni:{:.1}%",
                                    (cpu_time.nice as f32 / total) * 100.0
                                ))
                                .color(CyberColors::CYAN)
                                .small(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "id:{:.1}%",
                                    (cpu_time.idle as f32 / total) * 100.0
                                ))
                                .color(CyberColors::TEXT_MUTED)
                                .small(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "wa:{:.1}%",
                                    (cpu_time.iowait as f32 / total) * 100.0
                                ))
                                .color(CyberColors::NEON_RED)
                                .small(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "hi:{:.1}%",
                                    (cpu_time.irq as f32 / total) * 100.0
                                ))
                                .color(CyberColors::MAGENTA)
                                .small(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "si:{:.1}%",
                                    (cpu_time.softirq as f32 / total) * 100.0
                                ))
                                .color(CyberColors::NEON_PURPLE)
                                .small(),
                            );
                            if cpu_time.steal > 0 {
                                ui.label(
                                    RichText::new(format!(
                                        "st:{:.1}%",
                                        (cpu_time.steal as f32 / total) * 100.0
                                    ))
                                    .color(CyberColors::NEON_YELLOW)
                                    .small(),
                                );
                            }
                        });
                    }
                }

                // VMstat info (context switches, interrupts, etc.)
                if let Some(ref vm) = stats.vm_stats {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("VMstat: ").color(CyberColors::TEXT_PRIMARY));
                        // Show rates per second
                        let ctx_rate = self.context_switches_history.back().unwrap_or(&0.0);
                        let int_rate = self.interrupts_history.back().unwrap_or(&0.0);
                        ui.label(
                            RichText::new(format!("ctx/s:{:.0}", ctx_rate))
                                .color(CyberColors::CYAN)
                                .small(),
                        );
                        ui.label(
                            RichText::new(format!("int/s:{:.0}", int_rate))
                                .color(CyberColors::NEON_GREEN)
                                .small(),
                        );
                        if vm.processes_blocked > 0 {
                            ui.label(
                                RichText::new(format!("blocked:{}", vm.processes_blocked))
                                    .color(CyberColors::NEON_RED)
                                    .small(),
                            );
                        }
                        ui.label(
                            RichText::new(format!("pgpgin:{}", vm.pages_in))
                                .color(CyberColors::NEON_PURPLE)
                                .small(),
                        );
                        ui.label(
                            RichText::new(format!("pgpgout:{}", vm.pages_out))
                                .color(CyberColors::NEON_ORANGE)
                                .small(),
                        );
                        if vm.swap_in > 0 || vm.swap_out > 0 {
                            ui.label(
                                RichText::new(format!("swin:{} swout:{}", vm.swap_in, vm.swap_out))
                                    .color(CyberColors::NEON_YELLOW)
                                    .small(),
                            );
                        }
                    });
                }
            }

            ui.add_space(8.0);

            // Context Switches and Interrupts charts (vmstat-style).
            //
            // Only drawn where the counters exist. They come from /proc/stat, so on
            // Windows and macOS `vm_stats` is None and the histories keep the zeros
            // they were seeded with — which rendered as two flat lines reading
            // "0.0/s", asserting the machine performs no context switches and takes
            // no interrupts.
            if self
                .system_stats
                .as_ref()
                .is_some_and(|s| s.vm_stats.is_some())
            {
                ui.columns(2, |columns| {
                    columns[0].add(
                        SparklineChart::new(
                            self.context_switches_history.iter().cloned().collect(),
                        )
                        .color(CyberColors::CYAN)
                        .height(70.0)
                        .title("Context Switches")
                        .unit("/s")
                        .show_scale(true),
                    );

                    columns[1].add(
                        SparklineChart::new(self.interrupts_history.iter().cloned().collect())
                            .color(CyberColors::NEON_GREEN)
                            .height(70.0)
                            .title("Interrupts")
                            .unit("/s")
                            .show_scale(true),
                    );
                });
            } else {
                ui.label(
                    RichText::new(
                        "Context switch and interrupt counters are not exposed by this platform",
                    )
                    .color(CyberColors::TEXT_SECONDARY)
                    .small(),
                );
            }

            ui.add_space(16.0);

            // Network Charts
            let net_title = super::widgets::domain_section_title("network", "I/O");
            ui.add(SectionHeader::new(&net_title).icon("🌐"));
            ui.columns(2, |columns| {
                columns[0].add(
                    SparklineChart::new(self.network_rx_history.iter().cloned().collect())
                        .color(DeviceTitleColors::NETWORK)
                        .height(70.0)
                        .title("Download")
                        .unit("KB/s")
                        .show_scale(true),
                );

                columns[1].add(
                    SparklineChart::new(self.network_tx_history.iter().cloned().collect())
                        .color(DeviceTitleColors::NETWORK)
                        .height(70.0)
                        .title("Upload")
                        .unit("KB/s")
                        .show_scale(true),
                );
            });
        });
    }

    fn draw_cpu_tab(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            let cpu_title = super::widgets::domain_section_title("cpu", "Overview");
            ui.add(SectionHeader::new(&cpu_title).icon("🔲"));

            if let Some(ref cpu) = self.cpu_stats {
                // Overall utilization with Glances-style threshold colors
                let cpu_usage = 100.0 - cpu.total.idle;

                // Get trend from history
                let cpu_trend = self
                    .cpu_history
                    .iter()
                    .rev()
                    .nth(1)
                    .map(|&prev| trend_indicator(cpu_usage, prev).0)
                    .unwrap_or("→");

                ui.add(
                    CyberProgressBar::new(cpu_usage / 100.0)
                        .with_threshold_color()
                        .with_trend(cpu_trend)
                        .label("Total CPU")
                        .height(28.0),
                );

                ui.add_space(8.0);

                // CPU History
                ui.add(
                    SparklineChart::new(self.cpu_history.iter().cloned().collect())
                        .color(DeviceTitleColors::CPU)
                        .height(120.0)
                        .title("CPU History")
                        .unit("%")
                        .max_value(100.0)
                        .show_scale(true)
                        .show_min_max(true),
                );

                // Per-core sparklines (if available)
                if !self.per_core_history.is_empty() {
                    ui.add_space(16.0);
                    ui.add(SectionHeader::new("Per-Core History").icon("⚡"));

                    let num_cols =
                        (self.per_core_history.len().min(8) as f32).sqrt().ceil() as usize;
                    let num_cols = num_cols.clamp(2, 4);

                    ui.columns(num_cols, |columns| {
                        for (i, hist) in self.per_core_history.iter().enumerate() {
                            let col = i % num_cols;
                            if col < columns.len() {
                                let core_usage = hist.back().copied().unwrap_or(0.0);
                                columns[col].add(
                                    SparklineChart::new(hist.iter().cloned().collect())
                                        .color(theme::cpu_color(core_usage))
                                        .height(60.0)
                                        .title(format!("Core {}", i))
                                        .unit("%")
                                        .max_value(100.0)
                                        .show_scale(true),
                                );
                            }
                        }
                    });
                }

                // CPU Info
                ui.add_space(16.0);
                let t = super::widgets::domain_section_title("cpu", "Information");
                ui.add(SectionHeader::new(&t).icon("ℹ️"));

                let cores = &cpu.cores;
                egui::Grid::new("cpu_info_grid")
                    .num_columns(2)
                    .spacing([40.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Cores:").color(CyberColors::TEXT_SECONDARY));
                        ui.label(
                            RichText::new(format!("{}", cores.len())).color(DeviceTitleColors::CPU),
                        );
                        ui.end_row();

                        ui.label(RichText::new("Online:").color(CyberColors::TEXT_SECONDARY));
                        ui.label(
                            RichText::new(format!("{}", cpu.online_count()))
                                .color(DeviceTitleColors::CPU),
                        );
                        ui.end_row();

                        if let Some(core) = cores.first() {
                            if let Some(ref freq) = core.frequency {
                                ui.label(
                                    RichText::new("Frequency:").color(CyberColors::TEXT_SECONDARY),
                                );
                                ui.label(
                                    RichText::new(match freq.current {
                                        Some(mhz) => format!("{mhz} MHz"),
                                        None => "not read".to_string(),
                                    })
                                    .color(DeviceTitleColors::CPU),
                                );
                                ui.end_row();
                            }

                            if !core.model.is_empty() {
                                ui.label(
                                    RichText::new("Model:").color(CyberColors::TEXT_SECONDARY),
                                );
                                ui.label(RichText::new(&core.model).color(DeviceTitleColors::CPU));
                                ui.end_row();
                            }
                        }
                    });
            } else {
                ui.label(RichText::new("Unable to read CPU statistics").color(CyberColors::ERROR));
            }
        });
    }

    fn draw_accelerators_tab(&mut self, ui: &mut egui::Ui) {
        if self.gpu_static_info.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.label(RichText::new("⚡").size(48.0));
                ui.label(
                    RichText::new("No Accelerators Detected")
                        .color(CyberColors::TEXT_SECONDARY)
                        .size(24.0),
                );
                ui.label(
                    RichText::new("No GPUs, NPUs, FPGAs, or other accelerators found.\nInstall drivers or check hardware connection.")
                        .color(CyberColors::TEXT_MUTED),
                );
            });
            return;
        }

        // Auto-scale layout based on device count
        let device_count = self.gpu_static_info.len();
        let available_width = ui.available_width();

        // Scale elements based on device count
        let chart_height = if device_count == 1 {
            100.0
        } else if device_count == 2 {
            80.0
        } else {
            65.0
        };
        let bar_height = if device_count <= 2 { 18.0 } else { 14.0 };
        let _font_scale = if device_count <= 2 {
            1.0
        } else if device_count <= 4 {
            0.9
        } else {
            0.8
        };

        ScrollArea::vertical().show(ui, |ui| {
            for (i, (static_info, dynamic_info)) in self
                .gpu_static_info
                .iter()
                .zip(self.gpu_dynamic_info.iter())
                .enumerate()
            {
                let accel_color = DeviceTitleColors::ACCEL;

                // Device pane frame - compact
                let frame = egui::Frame::none()
                    .fill(CyberColors::SURFACE)
                    .stroke(egui::Stroke::new(1.0_f32, CyberColors::BORDER))
                    .rounding(4.0)
                    .inner_margin(8.0);

                frame.show(ui, |ui| {
                    ui.set_width(available_width - 16.0);

                    // Header row: Icon + Name + Live Metrics
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⚡").color(accel_color).size(20.0));
                        ui.label(
                            RichText::new(&static_info.name)
                                .color(CyberColors::TEXT_PRIMARY)
                                .strong()
                                .size(18.0),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Live metrics on the right
                            if let Some(clock) = dynamic_info.clocks.graphics {
                                ui.label(
                                    RichText::new(format!("{} MHz", clock))
                                        .color(CyberColors::NEON_BLUE)
                                        .size(15.0),
                                );
                            }
                            if let Some(power) = dynamic_info.power.draw {
                                ui.label(
                                    RichText::new(format!("{:.0}W", power as f64 / 1000.0))
                                        .color(CyberColors::NEON_ORANGE)
                                        .size(15.0),
                                );
                            }
                            if let Some(temp) = dynamic_info.thermal.temperature {
                                ui.label(
                                    RichText::new(format!("{}°C", temp))
                                        .color(theme::temperature_color(temp as u32))
                                        .size(15.0),
                                );
                            }
                        });
                    });

                    ui.add_space(4.0);

                    // Main content: Progress bars + Charts side by side
                    ui.horizontal(|ui| {
                        // Left side: Progress bars
                        ui.vertical(|ui| {
                            ui.set_width(220.0);

                            // Utilization bar
                            ui.label(
                                RichText::new(match dynamic_info.utilization {
                                    Some(u) => format!("Utilization {u}%"),
                                    None => "Utilization -".to_string(),
                                })
                                .color(CyberColors::TEXT_SECONDARY)
                                .size(13.0),
                            );
                            // No reading, no bar: an empty bar and a bar at
                            // zero look the same, and only one is a reading.
                            if let Some(util) = dynamic_info.utilization {
                                ui.add(
                                    CyberProgressBar::new(util as f32 / 100.0)
                                        .color(accel_color)
                                        .height(bar_height),
                                );
                            }

                            ui.add_space(4.0);

                            // VRAM bar
                            let mb = |v: Option<u64>| {
                                v.map_or_else(|| "-".to_string(), |b| (b / 1024 / 1024).to_string())
                            };
                            ui.label(
                                RichText::new(format!(
                                    "VRAM {}/{}MB",
                                    mb(dynamic_info.memory.used),
                                    mb(dynamic_info.memory.total)
                                ))
                                .color(CyberColors::TEXT_SECONDARY)
                                .size(13.0),
                            );
                            // The bar is drawn only when there is a fraction to
                            // draw. An empty bar and a bar at zero look the
                            // same, and only one of them is a reading.
                            if let Some(pct) = dynamic_info.memory.utilization {
                                ui.add(
                                    CyberProgressBar::new(pct as f32 / 100.0)
                                        .color(DeviceTitleColors::MEMORY)
                                        .height(bar_height),
                                );
                            }

                            // Vendor/Driver info at bottom
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("{:?}", static_info.vendor))
                                    .color(accel_color)
                                    .size(12.0),
                            );
                            if let Some(ref driver) = static_info.driver_version {
                                ui.label(
                                    RichText::new(format!("Driver: {}", driver))
                                        .color(CyberColors::TEXT_MUTED)
                                        .size(12.0),
                                );
                            }
                        });

                        ui.add_space(12.0);

                        // Right side: Charts (expand to fill)
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width());

                            if i < self.gpu_history.len() {
                                ui.add(
                                    SparklineChart::new(
                                        self.gpu_history[i].iter().cloned().collect(),
                                    )
                                    .color(accel_color)
                                    .height(chart_height)
                                    .title("Utilization")
                                    .unit("%")
                                    .max_value(100.0)
                                    .show_scale(true),
                                );
                            }

                            ui.add_space(2.0);

                            if i < self.gpu_temp_history.len() {
                                ui.add(
                                    SparklineChart::new(
                                        self.gpu_temp_history[i].iter().cloned().collect(),
                                    )
                                    .color(CyberColors::NEON_YELLOW)
                                    .height(chart_height)
                                    .title("Temperature")
                                    .unit("°C")
                                    .max_value(100.0)
                                    .show_scale(true),
                                );
                            }
                        });
                    });
                });

                // Minimal gap between device panes
                if i < device_count - 1 {
                    ui.add_space(2.0);
                }
            }
        });
    }

    fn draw_memory_tab(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            if let Some(ref mem) = self.memory_stats {
                let usage = mem.ram_usage_percent();
                let total_mb = mem.ram.total as f64 / 1024.0;
                let used_mb = mem.ram.used as f64 / 1024.0;
                let free_mb = mem.ram.free as f64 / 1024.0;
                // Absent on platforms that publish no such figure -- Windows
                // and macOS have no "buffers" at all. The cards below show an
                // em dash for those rather than a plausible `0`.
                let buffers_mb = mem.ram.buffers.map(|v| v as f64 / 1024.0);
                let cached_mb = mem.ram.cached.map(|v| v as f64 / 1024.0);
                let shared_mb = mem.ram.shared.map(|v| v as f64 / 1024.0);
                let card = |v: Option<f64>| match v {
                    Some(x) => format!("{x:.0}"),
                    None => "—".to_string(),
                };
                // Available = free + buffers + cached (like `free -h`), over
                // whichever of the two this platform reports.
                let available_mb = free_mb + buffers_mb.unwrap_or(0.0) + cached_mb.unwrap_or(0.0);

                ui.add(SectionHeader::new("Physical Memory").icon("💾"));

                // Get trend from history
                let mem_trend = self
                    .memory_history
                    .iter()
                    .rev()
                    .nth(1)
                    .map(|&prev| trend_indicator(usage, prev).0)
                    .unwrap_or("→");

                ui.add(
                    CyberProgressBar::new(usage / 100.0)
                        .with_threshold_color()
                        .with_trend(mem_trend)
                        .label(format!("{:.1} MB / {:.1} MB", used_mb, total_mb))
                        .height(32.0),
                );

                ui.add_space(8.0);

                // Memory breakdown like `free -h` output
                let t = super::widgets::domain_section_title("memory", "Breakdown (free -h style)");
                ui.add(SectionHeader::new(&t).icon("📈"));

                // Main row: total, used, free, shared, buff/cache, available
                ui.horizontal(|ui| {
                    ui.add(
                        MetricCard::new("Total", format!("{:.0}", total_mb))
                            .unit("MB")
                            .color(DeviceTitleColors::MEMORY),
                    );
                    ui.add(
                        MetricCard::new("Used", format!("{:.0}", used_mb))
                            .unit("MB")
                            .color(threshold_color(usage)),
                    );
                    ui.add(
                        MetricCard::new("Free", format!("{:.0}", free_mb))
                            .unit("MB")
                            .color(CyberColors::THRESHOLD_OK),
                    );
                    ui.add(
                        MetricCard::new("Shared", card(shared_mb))
                            .unit("MB")
                            .color(CyberColors::NEON_PURPLE),
                    );
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.add(
                        MetricCard::new("Buffers", card(buffers_mb))
                            .unit("MB")
                            .color(CyberColors::NEON_ORANGE),
                    );
                    ui.add(
                        MetricCard::new("Cached", card(cached_mb))
                            .unit("MB")
                            .color(CyberColors::NEON_YELLOW),
                    );
                    ui.add(
                        MetricCard::new("Available", format!("{:.0}", available_mb))
                            .unit("MB")
                            .color(CyberColors::THRESHOLD_OK),
                    );
                    ui.add(
                        MetricCard::new("Usage", format!("{:.1}", usage))
                            .unit("%")
                            .color(threshold_color(usage)),
                    );
                });

                // Visual breakdown bar (stacked)
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Memory Map: ").color(CyberColors::TEXT_PRIMARY));
                    // Show proportional bar
                    if total_mb > 0.0 {
                        // The bar segments an unreported quantity at zero
                        // width, which is what "not shown" looks like in a
                        // stacked bar -- it is not a claim about the value.
                        let buffers_bar = buffers_mb.unwrap_or(0.0);
                        let cached_bar = cached_mb.unwrap_or(0.0);
                        let used_pct = (used_mb - buffers_bar - cached_bar).max(0.0) / total_mb;
                        let buffers_pct = buffers_bar / total_mb;
                        let cached_pct = cached_bar / total_mb;
                        let free_pct = free_mb / total_mb;

                        let _bar_width = ui.available_width() - 100.0;

                        ui.label(
                            RichText::new(format!("█{:.0}%", used_pct * 100.0))
                                .color(CyberColors::MAGENTA)
                                .small(),
                        );
                        ui.label(
                            RichText::new(format!("█{:.0}%", buffers_pct * 100.0))
                                .color(CyberColors::NEON_ORANGE)
                                .small(),
                        );
                        ui.label(
                            RichText::new(format!("█{:.0}%", cached_pct * 100.0))
                                .color(CyberColors::NEON_YELLOW)
                                .small(),
                        );
                        ui.label(
                            RichText::new(format!("█{:.0}%", free_pct * 100.0))
                                .color(CyberColors::THRESHOLD_OK)
                                .small(),
                        );

                        // Legend
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new("(used/buffers/cache/free)")
                                    .color(CyberColors::TEXT_MUTED)
                                    .small(),
                            );
                        });
                    }
                });

                ui.add_space(16.0);

                // Memory history
                ui.add(
                    SparklineChart::new(self.memory_history.iter().cloned().collect())
                        .color(DeviceTitleColors::MEMORY)
                        .height(150.0)
                        .title("Memory Usage History")
                        .unit("%")
                        .max_value(100.0)
                        .show_scale(true)
                        .show_min_max(true),
                );

                // Swap info
                ui.add_space(16.0);
                ui.add(SectionHeader::new("Swap Memory").icon("🔄"));

                // `total_or_zero()` discarded the `Option` that `SwapInfo`
                // carries specifically so an unread pagefile is distinguishable
                // from an absent one -- and the `else` branch below then told
                // the user "No swap configured", which is a statement about the
                // machine rather than about the reading. Windows swap is read
                // through a WMI query that can fail.
                let swap_usage = mem.swap_usage_percent();
                let swap_total_mb = mem.swap.total.map(|v| v as f64 / 1024.0);
                let swap_used_mb = mem.swap.used.map(|v| v as f64 / 1024.0);
                let swap_free_mb = match (swap_total_mb, swap_used_mb) {
                    (Some(total), Some(used)) => Some(total - used),
                    _ => None,
                };
                let swap_cached_mb = mem.swap.cached.map(|v| v as f64 / 1024.0);

                let mb = |v: Option<f64>| v.map_or_else(|| "-".to_string(), |x| format!("{x:.0}"));

                if swap_total_mb.is_some_and(|t| t > 0.0) {
                    ui.add(
                        CyberProgressBar::new(swap_usage.unwrap_or(0.0) / 100.0)
                            .color(CyberColors::NEON_PURPLE)
                            .label(format!(
                                "Swap: {} MB / {} MB",
                                mb(swap_used_mb),
                                mb(swap_total_mb)
                            ))
                            .height(24.0),
                    );

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            MetricCard::new("Swap Total", mb(swap_total_mb))
                                .unit("MB")
                                .color(DeviceTitleColors::MEMORY),
                        );
                        ui.add(
                            MetricCard::new("Swap Used", mb(swap_used_mb))
                                .unit("MB")
                                .color(CyberColors::MAGENTA),
                        );
                        ui.add(
                            MetricCard::new("Swap Free", mb(swap_free_mb))
                                .unit("MB")
                                .color(CyberColors::NEON_GREEN),
                        );
                        if swap_cached_mb.is_some_and(|c| c > 0.0) {
                            ui.add(
                                MetricCard::new("Swap Cached", mb(swap_cached_mb))
                                    .unit("MB")
                                    .color(CyberColors::NEON_YELLOW),
                            );
                        }
                    });
                } else if swap_total_mb.is_some() {
                    ui.label(RichText::new("No swap configured").color(CyberColors::TEXT_MUTED));
                } else {
                    // Not the same sentence. One is a fact about the machine,
                    // the other about this reading of it.
                    ui.label(
                        RichText::new("Swap not reported by this platform")
                            .color(CyberColors::TEXT_MUTED),
                    );
                }
            } else {
                ui.label(
                    RichText::new("Unable to read memory statistics").color(CyberColors::ERROR),
                );
            }
        });
    }

    fn draw_processes_tab(&mut self, ui: &mut egui::Ui) {
        ui.add(SectionHeader::new("Running Processes (htop-style)").icon("📋"));

        // Task summary (htop-style: "Tasks: 150, 43 thr; 1 running")
        let running = self.process_list.iter().filter(|p| p.state == 'R').count();
        let sleeping = self
            .process_list
            .iter()
            .filter(|p| p.state == 'S' || p.state == 'I')
            .count();
        let disk_wait = self.process_list.iter().filter(|p| p.state == 'D').count();
        let zombie = self.process_list.iter().filter(|p| p.state == 'Z').count();
        let stopped = self.process_list.iter().filter(|p| p.state == 'T').count();
        let gpu_procs = self
            .process_list
            .iter()
            .filter(|p| p.total_gpu_memory_bytes.is_some_and(|b| b > 0))
            .count();

        ui.horizontal(|ui| {
            ui.label(RichText::new("Tasks:").color(CyberColors::TEXT_PRIMARY));
            ui.label(
                RichText::new(format!("{}", self.process_list.len())).color(CyberColors::CYAN),
            );
            ui.separator();
            ui.label(RichText::new(format!("{} running", running)).color(CyberColors::NEON_GREEN));
            ui.label(
                RichText::new(format!("{} sleeping", sleeping)).color(CyberColors::TEXT_MUTED),
            );
            if disk_wait > 0 {
                ui.label(
                    RichText::new(format!("{} D-wait", disk_wait)).color(CyberColors::NEON_ORANGE),
                );
            }
            if zombie > 0 {
                ui.label(RichText::new(format!("{} zombie", zombie)).color(CyberColors::NEON_RED));
            }
            if stopped > 0 {
                ui.label(
                    RichText::new(format!("{} stopped", stopped)).color(CyberColors::NEON_PURPLE),
                );
            }
            ui.separator();
            if gpu_procs > 0 {
                ui.label(
                    RichText::new(format!("🎮 {} GPU", gpu_procs)).color(CyberColors::NEON_ORANGE),
                );
            }
        });

        ui.add_space(4.0);

        // Filter bar
        ui.horizontal(|ui| {
            ui.label(RichText::new("🔍").color(CyberColors::TEXT_SECONDARY));
            ui.add(
                egui::TextEdit::singleline(&mut self.process_filter)
                    .hint_text("Filter processes...")
                    .desired_width(200.0),
            );

            ui.separator();

            // Sort options
            ui.label(RichText::new("Sort by:").color(CyberColors::TEXT_SECONDARY));
            if ui
                .selectable_label(self.process_sort_column == ProcessSortColumn::Name, "Name")
                .clicked()
            {
                self.process_sort_column = ProcessSortColumn::Name;
            }
            if ui
                .selectable_label(self.process_sort_column == ProcessSortColumn::Cpu, "CPU")
                .clicked()
            {
                self.process_sort_column = ProcessSortColumn::Cpu;
            }
            if ui
                .selectable_label(
                    self.process_sort_column == ProcessSortColumn::Memory,
                    "Memory",
                )
                .clicked()
            {
                self.process_sort_column = ProcessSortColumn::Memory;
            }
            if ui
                .selectable_label(self.process_sort_column == ProcessSortColumn::Pid, "PID")
                .clicked()
            {
                self.process_sort_column = ProcessSortColumn::Pid;
            }

            if ui
                .button(if self.process_sort_ascending {
                    "↑"
                } else {
                    "↓"
                })
                .clicked()
            {
                self.process_sort_ascending = !self.process_sort_ascending;
            }

            ui.label(
                RichText::new(format!("Total: {}", self.process_list.len()))
                    .color(CyberColors::TEXT_MUTED),
            );
        });

        ui.add_space(8.0);

        // Rebuild the sorted/filtered view only when something that affects
        // it has actually changed. Per-frame the cached view is reused.
        let want_key = (
            self.process_filter.clone(),
            self.process_sort_column,
            self.process_sort_ascending,
            self.process_list_version,
        );
        if self.processes_view_key.as_ref() != Some(&want_key) {
            let mut view = self.process_list.clone();
            if !self.process_filter.is_empty() {
                let filter = self.process_filter.to_lowercase();
                view.retain(|p| p.name.to_lowercase().contains(&filter));
            }
            let ascending = self.process_sort_ascending;
            match self.process_sort_column {
                ProcessSortColumn::Name => view.sort_by(|a, b| {
                    if ascending {
                        a.name.cmp(&b.name)
                    } else {
                        b.name.cmp(&a.name)
                    }
                }),
                ProcessSortColumn::Pid => view.sort_by(|a, b| {
                    if ascending {
                        a.pid.cmp(&b.pid)
                    } else {
                        b.pid.cmp(&a.pid)
                    }
                }),
                ProcessSortColumn::Cpu => view.sort_by(|a, b| {
                    let cmp = a
                        .cpu_percent
                        .partial_cmp(&b.cpu_percent)
                        .unwrap_or(std::cmp::Ordering::Equal);
                    if ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                }),
                ProcessSortColumn::Memory => view.sort_by(|a, b| {
                    let cmp = a.memory_bytes.cmp(&b.memory_bytes);
                    if ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                }),
            }
            self.processes_view_cache = view;
            self.processes_view_key = Some(want_key);
        }

        // Process table
        ScrollArea::vertical().show(ui, |ui| {
            let processes = &self.processes_view_cache;

            // Table header (htop-style)
            ui.horizontal(|ui| {
                ui.set_min_height(24.0);
                ui.label(RichText::new("PID").color(CyberColors::CYAN).strong());
                ui.add_space(40.0);
                ui.label(RichText::new("ST").color(CyberColors::CYAN).strong()); // State
                ui.add_space(8.0);
                ui.label(RichText::new("PRI").color(CyberColors::CYAN).strong()); // Priority
                ui.add_space(8.0);
                ui.label(RichText::new("Name").color(CyberColors::CYAN).strong());
                ui.add_space(160.0);
                ui.label(RichText::new("CPU %").color(CyberColors::CYAN).strong());
                ui.add_space(30.0);
                ui.label(RichText::new("Memory").color(CyberColors::CYAN).strong());
                ui.add_space(40.0);
                ui.label(RichText::new("GPU Mem").color(CyberColors::CYAN).strong());
            });
            ui.separator();

            // Process rows (htop-style with state and priority)
            for process in processes.iter().take(100) {
                let cpu_color = theme::utilization_color(process.cpu_percent);
                let mem_mb = process.memory_bytes as f64 / 1024.0 / 1024.0;
                let gpu_mem_mb = process
                    .total_gpu_memory_bytes
                    .map(|b| b as f64 / 1024.0 / 1024.0);

                // State color coding like htop
                let state_color = match process.state {
                    'R' => CyberColors::NEON_GREEN,        // Running
                    'S' | 'I' => CyberColors::TEXT_MUTED,  // Sleeping/Idle
                    'D' => CyberColors::NEON_ORANGE,       // Disk wait (uninterruptible)
                    'Z' => CyberColors::NEON_RED,          // Zombie
                    'T' | 't' => CyberColors::NEON_PURPLE, // Stopped/Traced
                    _ => CyberColors::TEXT_SECONDARY,
                };

                ui.horizontal(|ui| {
                    ui.set_min_height(20.0);
                    ui.label(
                        RichText::new(format!("{:>6}", process.pid))
                            .color(CyberColors::TEXT_MUTED)
                            .monospace(),
                    );
                    ui.add_space(20.0);
                    // State column
                    ui.label(
                        RichText::new(format!("{}", process.state))
                            .color(state_color)
                            .monospace(),
                    );
                    ui.add_space(8.0);
                    // Priority/nice column
                    let pri_str = process
                        .priority
                        .map(|p| format!("{:>3}", p))
                        .unwrap_or_else(|| "  -".to_string());
                    ui.label(
                        RichText::new(pri_str)
                            .color(CyberColors::TEXT_MUTED)
                            .monospace(),
                    );
                    ui.add_space(8.0);
                    // Name
                    ui.add_sized(
                        Vec2::new(200.0, 20.0),
                        egui::Label::new(
                            RichText::new(&process.name).color(CyberColors::TEXT_PRIMARY),
                        ),
                    );
                    // CPU
                    ui.label(
                        RichText::new(format!("{:>5.1}%", process.cpu_percent))
                            .color(cpu_color)
                            .monospace(),
                    );
                    ui.add_space(10.0);
                    // Memory
                    ui.label(
                        RichText::new(format!("{:>8.1} MB", mem_mb))
                            .color(CyberColors::MAGENTA)
                            .monospace(),
                    );
                    ui.add_space(20.0);
                    // GPU Memory (if using GPU)
                    if let Some(mb) = gpu_mem_mb.filter(|m| *m > 0.1) {
                        ui.label(
                            RichText::new(format!("{mb:>6.0} MB"))
                                .color(CyberColors::NEON_ORANGE)
                                .monospace(),
                        );
                    } else {
                        ui.label(
                            RichText::new("     -   ")
                                .color(CyberColors::TEXT_MUTED)
                                .monospace(),
                        );
                    }
                });
            }
        });
    }

    fn draw_network_tab(&mut self, ui: &mut egui::Ui) {
        // Clone the rates to avoid borrow conflict
        let rates = self.network_rates.clone();

        ScrollArea::vertical().show(ui, |ui| {
            let t = super::widgets::domain_section_title("network", "Interfaces");
            ui.add(SectionHeader::new(&t).icon("🌐"));

            // Show total bandwidth rates at the top
            // Summed over the interfaces that have a rate.
            let total_rx_rate: f64 = rates.values().filter_map(|(rx, _)| *rx).sum();
            let total_tx_rate: f64 = rates.values().filter_map(|(_, tx)| *tx).sum();

            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(RichText::new("Total Bandwidth:").color(CyberColors::TEXT_MUTED));
                ui.label(
                    RichText::new(format!("↓ {}/s", format_bytes(total_rx_rate)))
                        .color(DeviceTitleColors::NETWORK)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("↑ {}/s", format_bytes(total_tx_rate)))
                        .color(DeviceTitleColors::NETWORK)
                        .strong(),
                );
            });
            ui.add_space(8.0);

            // Network charts - stacked vertically, left-aligned
            ui.add(
                SparklineChart::new(self.network_rx_history.iter().cloned().collect())
                    .color(DeviceTitleColors::NETWORK)
                    .height(80.0)
                    .title("Download (Total MB)")
                    .unit("MB")
                    .show_scale(true)
                    .show_min_max(true),
            );

            ui.add_space(4.0);

            ui.add(
                SparklineChart::new(self.network_tx_history.iter().cloned().collect())
                    .color(DeviceTitleColors::NETWORK)
                    .height(80.0)
                    .title("Upload (Total MB)")
                    .unit("MB")
                    .show_scale(true)
                    .show_min_max(true),
            );

            ui.add_space(16.0);

            // Interface details
            if let Some(ref mut monitor) = self.network_monitor {
                if let Ok(interfaces) = monitor.interfaces() {
                    for iface in interfaces {
                        let iface_color = if iface.name.contains("eth")
                            || iface.name.contains("en")
                            || iface.name.contains("Ethernet")
                        {
                            CyberColors::NEON_BLUE
                        } else if iface.name.contains("wl") || iface.name.contains("Wi") {
                            CyberColors::NEON_PURPLE
                        } else {
                            CyberColors::CYAN
                        };

                        // Get bandwidth rates for this interface
                        let (rx_rate, tx_rate) =
                            rates.get(&iface.name).copied().unwrap_or((None, None));
                        // An em dash where no rate has been established yet.
                        let fmt = |r: Option<f64>| match r {
                            Some(v) => format!("{}/s", format_bytes(v)),
                            None => "—".to_string(),
                        };

                        ui.add(SectionHeader::new(&iface.name).icon("📡"));

                        // Bandwidth rate row
                        ui.horizontal(|ui| {
                            ui.add_space(20.0);
                            ui.label(RichText::new("Rate:").color(CyberColors::TEXT_MUTED));
                            ui.label(
                                RichText::new(format!("↓ {}", fmt(rx_rate)))
                                    .color(CyberColors::NEON_GREEN)
                                    .monospace(),
                            );
                            ui.label(
                                RichText::new(format!("↑ {}", fmt(tx_rate)))
                                    .color(CyberColors::NEON_ORANGE)
                                    .monospace(),
                            );
                            if let Some(speed) = iface.speed_mbps {
                                ui.separator();
                                ui.label(RichText::new("Link:").color(CyberColors::TEXT_MUTED));
                                ui.label(
                                    RichText::new(format!("{} Mbps", speed))
                                        .color(iface_color)
                                        .monospace(),
                                );
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.add(
                                MetricCard::new(
                                    "Received",
                                    format!("{:.1}", iface.rx_bytes as f64 / 1024.0 / 1024.0),
                                )
                                .unit("MB")
                                .color(CyberColors::NEON_GREEN),
                            );

                            ui.add(
                                MetricCard::new(
                                    "Sent",
                                    format!("{:.1}", iface.tx_bytes as f64 / 1024.0 / 1024.0),
                                )
                                .unit("MB")
                                .color(CyberColors::NEON_ORANGE),
                            );

                            ui.add(
                                MetricCard::new("Packets In", iface.rx_packets).color(iface_color),
                            );

                            ui.add(
                                MetricCard::new("Packets Out", iface.tx_packets).color(iface_color),
                            );
                        });

                        // Status and IP addresses
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Status:").color(CyberColors::TEXT_SECONDARY));
                            ui.label(
                                RichText::new(if iface.is_up { "UP" } else { "DOWN" }).color(
                                    if iface.is_up {
                                        CyberColors::NEON_GREEN
                                    } else {
                                        CyberColors::ERROR
                                    },
                                ),
                            );

                            if !iface.ipv4_addresses.is_empty() {
                                ui.separator();
                                ui.label(RichText::new("IPv4:").color(CyberColors::TEXT_SECONDARY));
                                for ip in &iface.ipv4_addresses {
                                    ui.label(RichText::new(ip).color(iface_color).monospace());
                                }
                            }
                        });

                        ui.add_space(8.0);
                    }
                }
            } else {
                ui.label(
                    RichText::new("Unable to read network information").color(CyberColors::ERROR),
                );
            }
        });
    }

    fn draw_disk_tab(&mut self, ui: &mut egui::Ui) {
        // Trigger lazy loading of disk data
        self.start_disk_loading();

        // Show loading indicator
        if self.disk_loading {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.spinner();
                ui.label(
                    RichText::new("Loading disk information...")
                        .color(CyberColors::TEXT_SECONDARY)
                        .size(18.0),
                );
            });
            return;
        }

        if self.cached_disk_data.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.label(RichText::new("💿").size(64.0));
                ui.add_space(16.0);
                ui.label(
                    RichText::new("No Disks Detected")
                        .color(CyberColors::TEXT_SECONDARY)
                        .size(24.0),
                );
                ui.label(
                    RichText::new("Unable to enumerate storage devices")
                        .color(CyberColors::TEXT_MUTED),
                );
            });
            return;
        }

        ui.add(SectionHeader::new("Storage Devices").icon("💿"));
        ui.add_space(8.0);

        // Column widths - must match draw_disk_row exactly
        const COL_MODEL: f32 = 420.0;
        const COL_INTERFACE: f32 = 110.0;
        const COL_CAPACITY: f32 = 90.0;
        const COL_READ: f32 = 110.0;
        const COL_WRITE: f32 = 110.0;
        const COL_SPACING: f32 = 30.0;
        const HEADER_HEIGHT: f32 = 20.0;

        // Header row - use exact same allocation method as data rows for perfect alignment
        // Must account for: 1px stroke + 12px inner_margin from each card's Frame
        const CARD_LEFT_OFFSET: f32 = 13.0;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = COL_SPACING;
            ui.add_space(CARD_LEFT_OFFSET);

            // Column 1: Device (left-aligned to match data)
            let (model_rect, _) =
                ui.allocate_exact_size(egui::vec2(COL_MODEL, HEADER_HEIGHT), egui::Sense::hover());
            if ui.is_rect_visible(model_rect) {
                ui.painter().text(
                    model_rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    "Device",
                    egui::FontId::proportional(13.0),
                    CyberColors::TEXT_MUTED,
                );
            }

            // Column 2: Interface (centered to match data)
            let (iface_rect, _) = ui.allocate_exact_size(
                egui::vec2(COL_INTERFACE, HEADER_HEIGHT),
                egui::Sense::hover(),
            );
            if ui.is_rect_visible(iface_rect) {
                ui.painter().text(
                    iface_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Interface",
                    egui::FontId::proportional(13.0),
                    CyberColors::TEXT_MUTED,
                );
            }

            // Column 3: Capacity (centered to match data)
            let (cap_rect, _) = ui.allocate_exact_size(
                egui::vec2(COL_CAPACITY, HEADER_HEIGHT),
                egui::Sense::hover(),
            );
            if ui.is_rect_visible(cap_rect) {
                ui.painter().text(
                    cap_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Capacity",
                    egui::FontId::proportional(13.0),
                    CyberColors::TEXT_MUTED,
                );
            }

            // Column 4: Read (centered to match data)
            let (read_rect, _) =
                ui.allocate_exact_size(egui::vec2(COL_READ, HEADER_HEIGHT), egui::Sense::hover());
            if ui.is_rect_visible(read_rect) {
                ui.painter().text(
                    read_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "📥 Read",
                    egui::FontId::proportional(13.0),
                    CyberColors::TEXT_MUTED,
                );
            }

            // Column 5: Write (centered to match data)
            let (write_rect, _) =
                ui.allocate_exact_size(egui::vec2(COL_WRITE, HEADER_HEIGHT), egui::Sense::hover());
            if ui.is_rect_visible(write_rect) {
                ui.painter().text(
                    write_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "📤 Write",
                    egui::FontId::proportional(13.0),
                    CyberColors::TEXT_MUTED,
                );
            }

            // Health column (right side)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(CARD_LEFT_OFFSET);
                ui.label(
                    RichText::new("Health")
                        .color(CyberColors::TEXT_MUTED)
                        .size(13.0),
                );
            });
        });
        ui.add_space(4.0);

        // Disk list with scroll
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, cached) in self.cached_disk_data.iter().enumerate() {
                    Self::draw_disk_row_cached(ui, i, cached);
                    ui.add_space(6.0);
                }
            });
    }

    fn draw_disk_row_cached(ui: &mut egui::Ui, _index: usize, cached: &CachedDiskData) {
        let disk_color = DeviceTitleColors::DISK;
        let disk_name = cached.name.clone();
        let disk_type = cached.disk_type;

        let type_icon = match disk_type {
            crate::disk::DiskType::NvmeSsd => "⚡",
            crate::disk::DiskType::SataSsd => "💾",
            crate::disk::DiskType::SataHdd => "🔘",
            crate::disk::DiskType::Usb => "🔌",
            crate::disk::DiskType::Scsi => "📀",
            crate::disk::DiskType::Virtual => "☁",
            crate::disk::DiskType::Unknown => "?",
        };

        let format_bytes = |bytes: u64| -> String {
            let b = bytes as f64;
            if b >= 1e12 {
                format!("{:.2} TB", b / 1e12)
            } else if b >= 1e9 {
                format!("{:.1} GB", b / 1e9)
            } else if b >= 1e6 {
                format!("{:.0} MB", b / 1e6)
            } else if b >= 1e3 {
                format!("{:.0} KB", b / 1e3)
            } else {
                format!("{} B", bytes)
            }
        };

        // Use cached data instead of making I/O calls
        let info = &cached.info;
        let io_stats = &cached.io_stats;
        let health = &cached.health;

        let model_name = info
            .as_ref()
            .map(|i| {
                if i.model.len() > 40 {
                    format!("{}…", &i.model[..38])
                } else {
                    i.model.clone()
                }
            })
            .unwrap_or_else(|| disk_name.clone());
        let interface = info
            .as_ref()
            .and_then(|i| i.interface_type.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let capacity = info
            .as_ref()
            .map(|i| format_bytes(i.capacity))
            .unwrap_or_else(|| "N/A".to_string());
        let read_bytes = io_stats
            .as_ref()
            .map(|io| format_bytes(io.read_bytes))
            .unwrap_or_else(|| "N/A".to_string());
        let write_bytes = io_stats
            .as_ref()
            .map(|io| format_bytes(io.write_bytes))
            .unwrap_or_else(|| "N/A".to_string());

        egui::Frame::none()
            .fill(CyberColors::SURFACE)
            .stroke(egui::Stroke::new(1.0_f32, disk_color.gamma_multiply(0.4)))
            .rounding(6.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                // Use fixed column positions via exact sizing
                const COL_MODEL: f32 = 420.0;
                const COL_INTERFACE: f32 = 110.0;
                const COL_CAPACITY: f32 = 90.0;
                const COL_READ: f32 = 110.0;
                const COL_WRITE: f32 = 110.0;
                const ROW_HEIGHT: f32 = 45.0;

                // Row 1: Use exact size allocation to guarantee column widths
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 30.0;

                    // Column 1: Icon + Model - use exact size
                    let (model_rect, _) = ui.allocate_exact_size(
                        egui::vec2(COL_MODEL, ROW_HEIGHT),
                        egui::Sense::hover(),
                    );
                    if ui.is_rect_visible(model_rect) {
                        // Draw icon
                        let icon_pos = model_rect.left_center() + egui::vec2(14.0, 0.0);
                        ui.painter().text(
                            icon_pos,
                            egui::Align2::LEFT_CENTER,
                            type_icon,
                            egui::FontId::proportional(28.0),
                            CyberColors::TEXT_PRIMARY,
                        );

                        // Draw model name
                        let name_pos = model_rect.left_center() + egui::vec2(50.0, -8.0);
                        ui.painter().text(
                            name_pos,
                            egui::Align2::LEFT_CENTER,
                            &model_name,
                            egui::FontId::proportional(18.0),
                            disk_color,
                        );

                        // Draw disk name
                        let disk_name_pos = model_rect.left_center() + egui::vec2(50.0, 10.0);
                        ui.painter().text(
                            disk_name_pos,
                            egui::Align2::LEFT_CENTER,
                            &disk_name,
                            egui::FontId::monospace(13.0),
                            CyberColors::TEXT_MUTED,
                        );
                    }

                    // Column 2: Interface - exact size, centered
                    let (iface_rect, _) = ui.allocate_exact_size(
                        egui::vec2(COL_INTERFACE, ROW_HEIGHT),
                        egui::Sense::hover(),
                    );
                    if ui.is_rect_visible(iface_rect) {
                        ui.painter().text(
                            iface_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &interface,
                            egui::FontId::proportional(16.0),
                            CyberColors::NEON_PURPLE,
                        );
                    }

                    // Column 3: Capacity - exact size, centered
                    let (cap_rect, _) = ui.allocate_exact_size(
                        egui::vec2(COL_CAPACITY, ROW_HEIGHT),
                        egui::Sense::hover(),
                    );
                    if ui.is_rect_visible(cap_rect) {
                        ui.painter().text(
                            cap_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &capacity,
                            egui::FontId::proportional(18.0),
                            CyberColors::CYAN,
                        );
                    }

                    // Column 4: Read - exact size, centered
                    let (read_rect, _) = ui.allocate_exact_size(
                        egui::vec2(COL_READ, ROW_HEIGHT),
                        egui::Sense::hover(),
                    );
                    if ui.is_rect_visible(read_rect) {
                        ui.painter().text(
                            read_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &read_bytes,
                            egui::FontId::proportional(16.0),
                            CyberColors::NEON_GREEN,
                        );
                    }

                    // Column 5: Write - exact size, centered
                    let (write_rect, _) = ui.allocate_exact_size(
                        egui::vec2(COL_WRITE, ROW_HEIGHT),
                        egui::Sense::hover(),
                    );
                    if ui.is_rect_visible(write_rect) {
                        ui.painter().text(
                            write_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &write_bytes,
                            egui::FontId::proportional(16.0),
                            CyberColors::NEON_ORANGE,
                        );
                    }

                    // Column 6: Health - right aligned, use remaining space
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(h) = &health {
                            let (text, color) = match h {
                                crate::disk::DiskHealth::Healthy => {
                                    ("✓ Healthy", CyberColors::NEON_GREEN)
                                }
                                crate::disk::DiskHealth::Warning => {
                                    ("⚠ Warning", CyberColors::NEON_ORANGE)
                                }
                                crate::disk::DiskHealth::Critical
                                | crate::disk::DiskHealth::Failed => {
                                    ("✗ Critical", CyberColors::NEON_RED)
                                }
                                crate::disk::DiskHealth::Unknown => {
                                    ("Unknown", CyberColors::TEXT_MUTED)
                                }
                            };
                            egui::Frame::none()
                                .fill(color.gamma_multiply(0.15))
                                .rounding(4.0)
                                .inner_margin(egui::vec2(10.0, 4.0))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(text).color(color).size(14.0));
                                });
                        }
                    });
                });

                // Row 2: Partitions with aligned columns (use cached filesystem data)
                let filesystems = &cached.filesystems;
                if !filesystems.is_empty() {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);

                    for fs in filesystems.iter().take(3) {
                        let pct = fs.usage_percent();
                        let color = if pct > 90.0 {
                            CyberColors::NEON_RED
                        } else if pct > 75.0 {
                            CyberColors::NEON_ORANGE
                        } else {
                            CyberColors::NEON_GREEN
                        };

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 20.0; // Even spacing

                            // Mount point (fixed width 50px)
                            ui.allocate_ui(egui::vec2(50.0, 22.0), |ui| {
                                let mount = fs.mount_point.to_string_lossy();
                                ui.label(
                                    RichText::new(mount.as_ref())
                                        .color(CyberColors::TEXT_PRIMARY)
                                        .size(15.0)
                                        .monospace(),
                                );
                            });

                            // Filesystem type (fixed width 60px)
                            ui.allocate_ui(egui::vec2(60.0, 22.0), |ui| {
                                ui.label(
                                    RichText::new(&fs.fs_type)
                                        .color(CyberColors::TEXT_MUTED)
                                        .size(13.0),
                                );
                            });

                            // Progress bar (fixed width 180px)
                            let bar_w = 180.0;
                            let bar_h = 18.0;
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(bar_w, bar_h),
                                egui::Sense::hover(),
                            );
                            if ui.is_rect_visible(rect) {
                                ui.painter()
                                    .rect_filled(rect, 3.0, CyberColors::BACKGROUND_DARK);
                                let w = rect.width() * pct / 100.0;
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(
                                        rect.min,
                                        egui::vec2(w, rect.height()),
                                    ),
                                    3.0,
                                    color.gamma_multiply(0.8),
                                );
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    format!("{:.0}%", pct),
                                    egui::FontId::proportional(12.0),
                                    CyberColors::TEXT_PRIMARY,
                                );
                            }

                            // Used / Total (fixed width)
                            ui.allocate_ui(egui::vec2(180.0, 22.0), |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} / {}",
                                        format_bytes(fs.used_size),
                                        format_bytes(fs.total_size)
                                    ))
                                    .color(CyberColors::TEXT_SECONDARY)
                                    .size(14.0),
                                );
                            });
                        });
                        ui.add_space(2.0); // Space between partition rows
                    }
                    if filesystems.len() > 3 {
                        ui.label(
                            RichText::new(format!("+{} more partitions", filesystems.len() - 3))
                                .color(CyberColors::TEXT_MUTED)
                                .size(12.0),
                        );
                    }
                }
            });
    }

    fn draw_connections_tab(&mut self, ui: &mut egui::Ui) {
        let t = super::widgets::domain_section_title("network", "Connections (netstat)");
        ui.add(SectionHeader::new(&t).icon("📡"));

        // Filter bar
        ui.horizontal(|ui| {
            ui.label(RichText::new("🔍").color(CyberColors::TEXT_SECONDARY));
            ui.add(
                egui::TextEdit::singleline(&mut self.connection_filter)
                    .hint_text("Filter by address or process...")
                    .desired_width(200.0),
            );

            ui.separator();

            // Protocol filter
            ui.label(RichText::new("Protocol:").color(CyberColors::TEXT_SECONDARY));
            if ui
                .selectable_label(self.connection_protocol_filter.is_none(), "All")
                .clicked()
            {
                self.connection_protocol_filter = None;
            }
            if ui
                .selectable_label(
                    self.connection_protocol_filter == Some(Protocol::Tcp),
                    "TCP",
                )
                .clicked()
            {
                self.connection_protocol_filter = Some(Protocol::Tcp);
            }
            if ui
                .selectable_label(
                    self.connection_protocol_filter == Some(Protocol::Udp),
                    "UDP",
                )
                .clicked()
            {
                self.connection_protocol_filter = Some(Protocol::Udp);
            }

            ui.separator();

            // State filter
            ui.label(RichText::new("State:").color(CyberColors::TEXT_SECONDARY));
            if ui
                .selectable_label(self.connection_state_filter.is_none(), "All")
                .clicked()
            {
                self.connection_state_filter = None;
            }
            if ui
                .selectable_label(
                    self.connection_state_filter == Some(ConnectionState::Established),
                    "ESTABLISHED",
                )
                .clicked()
            {
                self.connection_state_filter = Some(ConnectionState::Established);
            }
            if ui
                .selectable_label(
                    self.connection_state_filter == Some(ConnectionState::Listen),
                    "LISTEN",
                )
                .clicked()
            {
                self.connection_state_filter = Some(ConnectionState::Listen);
            }

            ui.label(
                RichText::new(format!("Total: {}", self.connections.len()))
                    .color(CyberColors::TEXT_MUTED),
            );
        });

        ui.add_space(8.0);

        // Connection table
        ScrollArea::vertical().show(ui, |ui| {
            let mut connections = self.connections.clone();

            // Apply protocol filter
            if let Some(proto) = self.connection_protocol_filter {
                connections.retain(|c| {
                    c.protocol == proto
                        || (proto == Protocol::Tcp && c.protocol == Protocol::Tcp6)
                        || (proto == Protocol::Udp && c.protocol == Protocol::Udp6)
                });
            }

            // Apply state filter
            if let Some(state) = self.connection_state_filter {
                connections.retain(|c| c.state == state);
            }

            // Apply text filter
            if !self.connection_filter.is_empty() {
                let filter = self.connection_filter.to_lowercase();
                connections.retain(|c| {
                    c.local_address.to_lowercase().contains(&filter)
                        || c.remote_address
                            .as_ref()
                            .map(|r| r.to_lowercase().contains(&filter))
                            .unwrap_or(false)
                        || c.process_name
                            .as_ref()
                            .map(|p| p.to_lowercase().contains(&filter))
                            .unwrap_or(false)
                });
            }

            // Table header
            ui.horizontal(|ui| {
                ui.set_min_height(24.0);
                ui.add_sized(
                    Vec2::new(60.0, 20.0),
                    egui::Label::new(RichText::new("Proto").color(CyberColors::CYAN).strong()),
                );
                ui.add_sized(
                    Vec2::new(200.0, 20.0),
                    egui::Label::new(
                        RichText::new("Local Address")
                            .color(CyberColors::CYAN)
                            .strong(),
                    ),
                );
                ui.add_sized(
                    Vec2::new(200.0, 20.0),
                    egui::Label::new(
                        RichText::new("Remote Address")
                            .color(CyberColors::CYAN)
                            .strong(),
                    ),
                );
                ui.add_sized(
                    Vec2::new(100.0, 20.0),
                    egui::Label::new(RichText::new("State").color(CyberColors::CYAN).strong()),
                );
                ui.add_sized(
                    Vec2::new(60.0, 20.0),
                    egui::Label::new(RichText::new("PID").color(CyberColors::CYAN).strong()),
                );
                ui.label(RichText::new("Process").color(CyberColors::CYAN).strong());
            });
            ui.separator();

            // Connection rows
            for conn in connections.iter().take(200) {
                let proto_color = match conn.protocol {
                    Protocol::Tcp | Protocol::Tcp6 => CyberColors::NEON_BLUE,
                    Protocol::Udp | Protocol::Udp6 => CyberColors::NEON_PURPLE,
                };

                let state_color = match conn.state {
                    ConnectionState::Established => CyberColors::NEON_GREEN,
                    ConnectionState::Listen => CyberColors::CYAN,
                    ConnectionState::TimeWait | ConnectionState::CloseWait => {
                        CyberColors::NEON_YELLOW
                    }
                    ConnectionState::Stateless => CyberColors::TEXT_MUTED,
                    _ => CyberColors::NEON_ORANGE,
                };

                ui.horizontal(|ui| {
                    ui.set_min_height(18.0);

                    // Protocol
                    ui.add_sized(
                        Vec2::new(60.0, 18.0),
                        egui::Label::new(
                            RichText::new(format!("{}", conn.protocol))
                                .color(proto_color)
                                .monospace(),
                        ),
                    );

                    // Local Address
                    ui.add_sized(
                        Vec2::new(200.0, 18.0),
                        egui::Label::new(
                            RichText::new(&conn.local_address)
                                .color(CyberColors::TEXT_PRIMARY)
                                .monospace(),
                        ),
                    );

                    // Remote Address
                    let remote = conn.remote_address.as_deref().unwrap_or("*");
                    ui.add_sized(
                        Vec2::new(200.0, 18.0),
                        egui::Label::new(
                            RichText::new(remote)
                                .color(CyberColors::TEXT_SECONDARY)
                                .monospace(),
                        ),
                    );

                    // State
                    ui.add_sized(
                        Vec2::new(100.0, 18.0),
                        egui::Label::new(
                            RichText::new(format!("{}", conn.state))
                                .color(state_color)
                                .monospace(),
                        ),
                    );

                    // PID
                    let pid_str = conn
                        .pid
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    ui.add_sized(
                        Vec2::new(60.0, 18.0),
                        egui::Label::new(
                            RichText::new(pid_str)
                                .color(CyberColors::TEXT_MUTED)
                                .monospace(),
                        ),
                    );

                    // Process name
                    let proc_name = conn.process_name.as_deref().unwrap_or("-");
                    ui.label(RichText::new(proc_name).color(CyberColors::MAGENTA));
                });
            }

            if connections.len() > 200 {
                ui.label(
                    RichText::new(format!(
                        "... and {} more connections",
                        connections.len() - 200
                    ))
                    .color(CyberColors::TEXT_MUTED),
                );
            }
        });
    }

    fn draw_system_info_tab(&mut self, ui: &mut egui::Ui) {
        // Trigger lazy loading of system info data
        self.start_system_info_loading();

        ScrollArea::vertical().show(ui, |ui| {
            let t = super::widgets::domain_section_title("system", "Information");
            ui.add(SectionHeader::new(&t));

            // Show loading indicator if still loading
            if self.system_info_loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new("Loading detailed system information...")
                            .color(CyberColors::TEXT_MUTED),
                    );
                });
                ui.add_space(8.0);
            }

            // Always show basic system info from environment
            egui::Grid::new("basic_system_info_grid")
                .num_columns(2)
                .spacing([40.0, 8.0])
                .show(ui, |ui| {
                    // Hostname
                    ui.label(RichText::new("Hostname:").color(CyberColors::TEXT_MUTED));
                    ui.label(RichText::new(&self.hostname).color(CyberColors::CYAN));
                    ui.end_row();

                    // OS from environment
                    ui.label(RichText::new("Platform:").color(CyberColors::TEXT_MUTED));
                    ui.label(RichText::new(std::env::consts::OS).color(CyberColors::NEON_GREEN));
                    ui.end_row();

                    // Architecture
                    ui.label(RichText::new("Architecture:").color(CyberColors::TEXT_MUTED));
                    ui.label(RichText::new(std::env::consts::ARCH).color(CyberColors::NEON_GREEN));
                    ui.end_row();

                    // Uptime
                    let uptime = self.start_time.elapsed();
                    let hours = uptime.as_secs() / 3600;
                    let mins = (uptime.as_secs() % 3600) / 60;
                    let secs = uptime.as_secs() % 60;
                    ui.label(RichText::new("App Uptime:").color(CyberColors::TEXT_MUTED));
                    ui.label(
                        RichText::new(format!("{:02}:{:02}:{:02}", hours, mins, secs))
                            .color(CyberColors::TEXT_PRIMARY),
                    );
                    ui.end_row();
                });

            // System Info Section from WMI (if available)
            if let Some(ref info) = self.system_info {
                ui.add_space(16.0);
                ui.add(SectionHeader::new("Operating System"));

                egui::Grid::new("system_info_grid")
                    .num_columns(2)
                    .spacing([40.0, 8.0])
                    .show(ui, |ui| {
                        // OS Information
                        ui.label(RichText::new("Operating System:").color(CyberColors::TEXT_MUTED));
                        ui.label(
                            RichText::new(format!("{} {}", info.os_name, info.os_version))
                                .color(CyberColors::CYAN),
                        );
                        ui.end_row();

                        if let Some(ref kernel) = info.kernel_version {
                            ui.label(RichText::new("Kernel:").color(CyberColors::TEXT_MUTED));
                            ui.label(RichText::new(kernel).color(CyberColors::TEXT_PRIMARY));
                            ui.end_row();
                        }

                        if let Some(ref hostname) = info.hostname {
                            ui.label(
                                RichText::new("Computer Name:").color(CyberColors::TEXT_MUTED),
                            );
                            ui.label(RichText::new(hostname).color(CyberColors::TEXT_PRIMARY));
                            ui.end_row();
                        }
                    });

                ui.add_space(16.0);
                ui.add(SectionHeader::new("Hardware"));

                egui::Grid::new("hardware_info_grid")
                    .num_columns(2)
                    .spacing([40.0, 8.0])
                    .show(ui, |ui| {
                        if let Some(ref manufacturer) = info.manufacturer {
                            ui.label(RichText::new("Manufacturer:").color(CyberColors::TEXT_MUTED));
                            ui.label(RichText::new(manufacturer).color(CyberColors::NEON_PURPLE));
                            ui.end_row();
                        }

                        if let Some(ref product) = info.product_name {
                            ui.label(RichText::new("Product:").color(CyberColors::TEXT_MUTED));
                            ui.label(RichText::new(product).color(CyberColors::TEXT_PRIMARY));
                            ui.end_row();
                        }

                        if let Some(ref serial) = info.serial_number {
                            ui.label(RichText::new("Serial:").color(CyberColors::TEXT_MUTED));
                            ui.label(RichText::new(serial).color(CyberColors::TEXT_MUTED));
                            ui.end_row();
                        }

                        // Motherboard
                        if let Some(ref vendor) = info.board_vendor {
                            ui.label(RichText::new("Board Vendor:").color(CyberColors::TEXT_MUTED));
                            ui.label(RichText::new(vendor).color(CyberColors::NEON_ORANGE));
                            ui.end_row();
                        }

                        if let Some(ref name) = info.board_name {
                            ui.label(RichText::new("Board Model:").color(CyberColors::TEXT_MUTED));
                            ui.label(RichText::new(name).color(CyberColors::TEXT_PRIMARY));
                            ui.end_row();
                        }

                        if let Some(ref version) = info.board_version {
                            ui.label(
                                RichText::new("Board Version:").color(CyberColors::TEXT_MUTED),
                            );
                            ui.label(RichText::new(version).color(CyberColors::TEXT_SECONDARY));
                            ui.end_row();
                        }

                        // CPU
                        if let Some(ref cpu_name) = info.cpu_name {
                            ui.label(RichText::new("CPU:").color(CyberColors::TEXT_MUTED));
                            ui.label(RichText::new(cpu_name).color(CyberColors::CYAN));
                            ui.end_row();
                        }

                        if let (Some(cores), Some(threads)) = (info.cpu_cores, info.cpu_threads) {
                            ui.label(RichText::new("CPU Config:").color(CyberColors::TEXT_MUTED));
                            ui.label(
                                RichText::new(format!("{} Cores / {} Threads", cores, threads))
                                    .color(CyberColors::TEXT_PRIMARY),
                            );
                            ui.end_row();
                        }
                    });

                // BIOS/UEFI Section
                ui.add_space(16.0);
                ui.add(SectionHeader::new("BIOS / UEFI"));

                egui::Grid::new("bios_info_grid")
                    .num_columns(2)
                    .spacing([40.0, 8.0])
                    .show(ui, |ui| {
                        let bios = &info.bios;

                        ui.label(RichText::new("Firmware Type:").color(CyberColors::TEXT_MUTED));
                        ui.label(
                            RichText::new(format!("{:?}", bios.firmware_type))
                                .color(CyberColors::NEON_GREEN),
                        );
                        ui.end_row();

                        if let Some(ref vendor) = bios.vendor {
                            ui.label(RichText::new("BIOS Vendor:").color(CyberColors::TEXT_MUTED));
                            ui.label(RichText::new(vendor).color(CyberColors::NEON_PURPLE));
                            ui.end_row();
                        }

                        if let Some(ref version) = bios.version {
                            ui.label(RichText::new("BIOS Version:").color(CyberColors::TEXT_MUTED));
                            ui.label(RichText::new(version).color(CyberColors::TEXT_PRIMARY));
                            ui.end_row();
                        }

                        if let Some(ref date) = bios.release_date {
                            ui.label(RichText::new("Release Date:").color(CyberColors::TEXT_MUTED));
                            ui.label(RichText::new(date).color(CyberColors::TEXT_SECONDARY));
                            ui.end_row();
                        }

                        if let Some(secure_boot) = bios.secure_boot {
                            ui.label(RichText::new("Secure Boot:").color(CyberColors::TEXT_MUTED));
                            let (text, color) = if secure_boot {
                                ("Enabled", CyberColors::NEON_GREEN)
                            } else {
                                ("Disabled", CyberColors::NEON_ORANGE)
                            };
                            ui.label(RichText::new(text).color(color));
                            ui.end_row();
                        }
                    });
            } else if !self.system_info_loading {
                // Only show error if we're done loading and still have no data
                ui.add_space(16.0);
                ui.label(
                    RichText::new("⚠ Detailed system information not available (WMI query failed)")
                        .color(CyberColors::NEON_ORANGE),
                );
            }

            // Motherboard Sensors Section
            ui.add_space(16.0);
            ui.add(SectionHeader::new("🌡️ System Temperatures"));

            // Collect all available temperatures from various sources
            let mut all_temps: Vec<(String, f32, &str)> = Vec::new();

            // Get motherboard sensor data
            let mut has_mb_sensors = false;
            for sensor_device in &self.motherboard_sensors {
                let temps = sensor_device.temperature_sensors().unwrap_or_default();
                if !temps.is_empty() {
                    has_mb_sensors = true;
                    for temp in temps {
                        all_temps.push((temp.label.clone(), temp.temperature, "Motherboard"));
                    }
                }
            }

            // Get GPU temperatures
            for (i, info) in self.gpu_dynamic_info.iter().enumerate() {
                if let Some(temp) = info.thermal.temperature {
                    let gpu_name = self
                        .gpu_static_info
                        .get(i)
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| format!("GPU {}", i));
                    all_temps.push((gpu_name, temp as f32, "GPU"));
                }
            }

            // Disk temperatures come from the refresher thread, already sampled.
            for disk in &self.cached_disk_data {
                if let Some(temp) = disk.temperature {
                    all_temps.push((disk.name.clone(), temp, "Storage"));
                }
            }

            if all_temps.is_empty() {
                // No temperatures available at all
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ℹ️").size(16.0));
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("No temperature sensors detected")
                                .color(CyberColors::TEXT_MUTED),
                        );
                        ui.label(
                            RichText::new(
                                "Windows WMI doesn't expose CPU temperatures on most systems.",
                            )
                            .color(CyberColors::TEXT_MUTED)
                            .small(),
                        );
                    });
                });
                ui.add_space(8.0);

                // LHM download link
                ui.horizontal(|ui| {
                    ui.label(RichText::new("💡").size(14.0));
                    ui.label(
                        RichText::new("For full sensor support, run ")
                            .color(CyberColors::TEXT_SECONDARY),
                    );
                    ui.hyperlink_to(
                        RichText::new("LibreHardwareMonitor").color(CyberColors::CYAN),
                        "https://github.com/LibreHardwareMonitor/LibreHardwareMonitor",
                    );
                });
                ui.label(
                    RichText::new("IronMonitor will auto-detect LHM sensors when it's running.")
                        .color(CyberColors::TEXT_MUTED)
                        .small(),
                );
            } else {
                // Show all temperatures in a nice grid
                egui::Grid::new("all_temps_grid")
                    .num_columns(3)
                    .spacing([40.0, 8.0])
                    .show(ui, |ui| {
                        for (name, temp, source) in &all_temps {
                            ui.label(RichText::new(name).color(CyberColors::TEXT_SECONDARY));
                            ui.label(
                                RichText::new(format!("{:.1}°C", temp))
                                    .color(theme::temperature_color(*temp as u32))
                                    .strong(),
                            );
                            ui.label(
                                RichText::new(*source)
                                    .color(CyberColors::TEXT_MUTED)
                                    .small(),
                            );
                            ui.end_row();
                        }
                    });

                // Show note if we only have GPU temps (no motherboard)
                if !has_mb_sensors {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("💡").size(14.0));
                        ui.label(
                            RichText::new("CPU temps: Install ")
                                .color(CyberColors::TEXT_MUTED)
                                .small(),
                        );
                        ui.hyperlink_to(
                            RichText::new("LibreHardwareMonitor")
                                .color(CyberColors::CYAN)
                                .small(),
                            "https://github.com/LibreHardwareMonitor/LibreHardwareMonitor",
                        );
                    });
                }
            }

            // Show voltages and fans if available (from motherboard sensors)
            for sensor_device in &self.motherboard_sensors {
                let voltages = sensor_device.voltage_rails().unwrap_or_default();
                let fans = sensor_device.fans().unwrap_or_default();

                if !voltages.is_empty() {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("⚡ Voltages")
                            .color(CyberColors::TEXT_MUTED)
                            .strong(),
                    );
                    egui::Grid::new(format!("volts_{}", sensor_device.name()))
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            for volt in &voltages {
                                ui.label(
                                    RichText::new(&volt.label).color(CyberColors::TEXT_SECONDARY),
                                );
                                ui.label(
                                    RichText::new(format!("{:.3}V", volt.voltage))
                                        .color(CyberColors::NEON_YELLOW),
                                );
                                ui.end_row();
                            }
                        });
                }

                if !fans.is_empty() {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("🌀 Fans")
                            .color(CyberColors::TEXT_MUTED)
                            .strong(),
                    );
                    egui::Grid::new(format!("fans_{}", sensor_device.name()))
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            for fan in &fans {
                                ui.label(
                                    RichText::new(&fan.label).color(CyberColors::TEXT_SECONDARY),
                                );
                                let (rpm_text, rpm_color) = match fan.rpm {
                                    Some(0) => ("Stopped".to_string(), CyberColors::TEXT_MUTED),
                                    Some(rpm) => (format!("{} RPM", rpm), CyberColors::NEON_GREEN),
                                    None => ("N/A".to_string(), CyberColors::TEXT_MUTED),
                                };
                                ui.label(RichText::new(rpm_text).color(rpm_color));
                                ui.end_row();
                            }
                        });
                }
            }

            // Storage Devices (SATA/NVMe) Section
            if !self.sata_devices.is_empty() {
                ui.add_space(16.0);
                ui.add(SectionHeader::new("💾 Storage Devices"));

                egui::Grid::new("sata_devices_grid")
                    .num_columns(5)
                    .spacing([15.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        // Header
                        ui.label(RichText::new("Device").color(CyberColors::CYAN).strong());
                        ui.label(RichText::new("Model").color(CyberColors::CYAN).strong());
                        ui.label(RichText::new("Capacity").color(CyberColors::CYAN).strong());
                        ui.label(RichText::new("Interface").color(CyberColors::CYAN).strong());
                        ui.label(RichText::new("Type").color(CyberColors::CYAN).strong());
                        ui.end_row();

                        for device in &self.sata_devices {
                            // Device name
                            ui.label(RichText::new(&device.name).color(CyberColors::TEXT_PRIMARY));

                            // Model
                            ui.label(
                                RichText::new(device.model.as_deref().unwrap_or("-"))
                                    .color(CyberColors::TEXT_SECONDARY),
                            );

                            // Capacity
                            let capacity = device
                                .capacity_gb
                                .map(|gb| {
                                    if gb >= 1000.0 {
                                        format!("{:.1} TB", gb / 1000.0)
                                    } else {
                                        format!("{:.0} GB", gb)
                                    }
                                })
                                .unwrap_or_else(|| "-".to_string());
                            ui.label(RichText::new(capacity).color(CyberColors::NEON_YELLOW));

                            // Interface
                            ui.label(
                                RichText::new(device.interface_speed.as_deref().unwrap_or("-"))
                                    .color(CyberColors::NEON_BLUE),
                            );

                            // Media type
                            let (type_str, type_color) = match device.media_type {
                                motherboard::SataMediaType::Ssd => ("SSD", CyberColors::NEON_GREEN),
                                motherboard::SataMediaType::Hdd => {
                                    ("HDD", CyberColors::NEON_ORANGE)
                                }
                                motherboard::SataMediaType::Unknown => {
                                    ("Unknown", CyberColors::TEXT_MUTED)
                                }
                            };
                            ui.label(RichText::new(type_str).color(type_color));
                            ui.end_row();
                        }
                    });
            }

            // PCIe Devices Section
            if !self.pcie_devices.is_empty() {
                ui.add_space(16.0);
                ui.add(SectionHeader::new("🔌 PCIe Devices"));

                egui::Grid::new("pcie_devices_grid")
                    .num_columns(3)
                    .spacing([20.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        // Header
                        ui.label(RichText::new("Class").color(CyberColors::CYAN).strong());
                        ui.label(RichText::new("Device").color(CyberColors::CYAN).strong());
                        ui.label(RichText::new("Vendor").color(CyberColors::CYAN).strong());
                        ui.end_row();

                        for device in &self.pcie_devices {
                            // Device class with color coding
                            let (class_str, class_color) = match device.device_class.as_deref() {
                                Some("Display") => ("Display", CyberColors::NEON_GREEN),
                                Some("Network") => ("Network", CyberColors::NEON_BLUE),
                                Some("Storage") => ("Storage", CyberColors::NEON_PURPLE),
                                Some("Audio") => ("Audio", CyberColors::NEON_ORANGE),
                                Some("USB") => ("USB", CyberColors::NEON_YELLOW),
                                Some(other) => (other, CyberColors::TEXT_SECONDARY),
                                None => ("Other", CyberColors::TEXT_MUTED),
                            };
                            ui.label(RichText::new(class_str).color(class_color));

                            // Device name
                            ui.label(RichText::new(&device.name).color(CyberColors::TEXT_PRIMARY));

                            // Vendor
                            ui.label(
                                RichText::new(device.vendor.as_deref().unwrap_or("-"))
                                    .color(CyberColors::TEXT_SECONDARY),
                            );
                            ui.end_row();
                        }
                    });
            }

            // Drivers Section
            if !self.driver_info.is_empty() {
                ui.add_space(16.0);
                ui.add(SectionHeader::new("Installed Drivers"));

                egui::Grid::new("drivers_grid")
                    .num_columns(4)
                    .spacing([20.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        // Header
                        ui.label(RichText::new("Type").color(CyberColors::CYAN).strong());
                        ui.label(RichText::new("Name").color(CyberColors::CYAN).strong());
                        ui.label(RichText::new("Version").color(CyberColors::CYAN).strong());
                        ui.label(RichText::new("Vendor").color(CyberColors::CYAN).strong());
                        ui.end_row();

                        for driver in &self.driver_info {
                            let type_color = match driver.driver_type {
                                crate::motherboard::DriverType::Gpu => CyberColors::NEON_GREEN,
                                crate::motherboard::DriverType::Network => CyberColors::NEON_BLUE,
                                crate::motherboard::DriverType::Storage => CyberColors::NEON_PURPLE,
                                _ => CyberColors::TEXT_SECONDARY,
                            };

                            ui.label(
                                RichText::new(format!("{}", driver.driver_type)).color(type_color),
                            );
                            ui.label(RichText::new(&driver.name).color(CyberColors::TEXT_PRIMARY));
                            ui.label(
                                RichText::new(&driver.version).color(CyberColors::NEON_YELLOW),
                            );
                            ui.label(
                                RichText::new(driver.vendor.as_deref().unwrap_or("-"))
                                    .color(CyberColors::TEXT_MUTED),
                            );
                            ui.end_row();
                        }
                    });
            }
        });
    }

    fn draw_peripherals_tab(&mut self, ui: &mut egui::Ui) {
        // Trigger background loading if not started (same as System tab)
        // Check for results from background loading
        if let Some(receiver) = self.system_info_receiver.take() {
            let mut finished = false;
            while let Ok(update) = receiver.try_recv() {
                match update {
                    SystemInfoUpdate::Fast(fast) => {
                        let FastSystemInfo {
                            system_info,
                            sensors,
                            pcie_devices,
                            sata_devices,
                            peripherals,
                        } = *fast;
                        self.system_info = system_info;
                        self.motherboard_sensors = sensors;
                        self.pcie_devices = pcie_devices;
                        self.sata_devices = sata_devices;
                        self.peripherals = peripherals;
                    }
                    SystemInfoUpdate::Slow {
                        drivers,
                        system_temps,
                    } => {
                        self.driver_info = drivers;
                        self.system_temps = system_temps;
                        finished = true;
                    }
                }
            }
            if finished {
                self.system_info_loading = false;
            } else {
                // Put it back: the slow half has not arrived yet.
                self.system_info_receiver = Some(receiver);
            }
        }

        // Start background loading if not started yet
        if !self.system_info_tried && !self.system_info_loading {
            self.system_info_tried = true;
            self.system_info_loading = true;

            let (tx, rx) = channel();
            self.system_info_receiver = Some(rx);

            std::thread::spawn(move || collect_system_info(tx));
        }

        ScrollArea::vertical().show(ui, |ui| {
            ui.add(SectionHeader::new("🔌 Peripherals & Buses"));

            // Show loading indicator if peripherals not yet loaded
            if self.peripherals.is_none() && self.system_info_loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        RichText::new("Loading peripheral information...")
                            .color(CyberColors::TEXT_MUTED),
                    );
                });
                ui.add_space(8.0);
            }

            if let Some(ref peripherals) = self.peripherals {
                // USB Devices Section
                if !peripherals.usb_devices.is_empty() {
                    ui.add_space(8.0);
                    ui.add(SectionHeader::new("🔗 USB Devices"));

                    // Group by USB version
                    let mut usb3_devices: Vec<_> = peripherals
                        .usb_devices
                        .iter()
                        .filter(|d| {
                            matches!(
                                d.usb_version,
                                motherboard::UsbVersion::Usb3_0
                                    | motherboard::UsbVersion::Usb3_1
                                    | motherboard::UsbVersion::Usb3_2
                                    | motherboard::UsbVersion::Usb4
                            )
                        })
                        .collect();
                    let mut usb2_devices: Vec<_> = peripherals
                        .usb_devices
                        .iter()
                        .filter(|d| matches!(d.usb_version, motherboard::UsbVersion::Usb2_0))
                        .collect();
                    let mut other_usb: Vec<_> = peripherals
                        .usb_devices
                        .iter()
                        .filter(|d| {
                            matches!(
                                d.usb_version,
                                motherboard::UsbVersion::Usb1_1 | motherboard::UsbVersion::Unknown
                            )
                        })
                        .collect();

                    usb3_devices.sort_by(|a, b| a.name.cmp(&b.name));
                    usb2_devices.sort_by(|a, b| a.name.cmp(&b.name));
                    other_usb.sort_by(|a, b| a.name.cmp(&b.name));

                    egui::Grid::new("usb_devices_grid")
                        .num_columns(4)
                        .spacing([15.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            // Header
                            ui.label(RichText::new("Version").color(CyberColors::CYAN).strong());
                            ui.label(RichText::new("Device").color(CyberColors::CYAN).strong());
                            ui.label(RichText::new("Class").color(CyberColors::CYAN).strong());
                            ui.label(RichText::new("Status").color(CyberColors::CYAN).strong());
                            ui.end_row();

                            // USB 3.x devices first (fastest)
                            for device in &usb3_devices {
                                let version_color = match device.usb_version {
                                    motherboard::UsbVersion::Usb4 => CyberColors::NEON_PURPLE,
                                    motherboard::UsbVersion::Usb3_2 => CyberColors::NEON_GREEN,
                                    motherboard::UsbVersion::Usb3_1 => CyberColors::NEON_GREEN,
                                    motherboard::UsbVersion::Usb3_0 => CyberColors::NEON_BLUE,
                                    _ => CyberColors::TEXT_SECONDARY,
                                };
                                ui.label(
                                    RichText::new(format!("{}", device.usb_version))
                                        .color(version_color),
                                );
                                ui.label(
                                    RichText::new(&device.name).color(CyberColors::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new(device.device_class.as_deref().unwrap_or("-"))
                                        .color(CyberColors::TEXT_SECONDARY),
                                );
                                let status_color = if device.status.as_deref() == Some("OK") {
                                    CyberColors::NEON_GREEN
                                } else {
                                    CyberColors::NEON_ORANGE
                                };
                                ui.label(
                                    RichText::new(device.status.as_deref().unwrap_or("-"))
                                        .color(status_color),
                                );
                                ui.end_row();
                            }

                            // USB 2.0 devices
                            for device in &usb2_devices {
                                ui.label(
                                    RichText::new(format!("{}", device.usb_version))
                                        .color(CyberColors::NEON_YELLOW),
                                );
                                ui.label(
                                    RichText::new(&device.name).color(CyberColors::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new(device.device_class.as_deref().unwrap_or("-"))
                                        .color(CyberColors::TEXT_SECONDARY),
                                );
                                let status_color = if device.status.as_deref() == Some("OK") {
                                    CyberColors::NEON_GREEN
                                } else {
                                    CyberColors::NEON_ORANGE
                                };
                                ui.label(
                                    RichText::new(device.status.as_deref().unwrap_or("-"))
                                        .color(status_color),
                                );
                                ui.end_row();
                            }

                            // Other USB devices
                            for device in &other_usb {
                                ui.label(
                                    RichText::new(format!("{}", device.usb_version))
                                        .color(CyberColors::TEXT_MUTED),
                                );
                                ui.label(
                                    RichText::new(&device.name).color(CyberColors::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new(device.device_class.as_deref().unwrap_or("-"))
                                        .color(CyberColors::TEXT_SECONDARY),
                                );
                                let status_color = if device.status.as_deref() == Some("OK") {
                                    CyberColors::NEON_GREEN
                                } else {
                                    CyberColors::NEON_ORANGE
                                };
                                ui.label(
                                    RichText::new(device.status.as_deref().unwrap_or("-"))
                                        .color(status_color),
                                );
                                ui.end_row();
                            }
                        });

                    // USB summary
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Total: {} devices ({} USB 3.x, {} USB 2.0, {} other)",
                                peripherals.usb_devices.len(),
                                usb3_devices.len(),
                                usb2_devices.len(),
                                other_usb.len()
                            ))
                            .color(CyberColors::TEXT_MUTED)
                            .small(),
                        );
                    });
                }

                // Display Outputs Section
                if !peripherals.display_outputs.is_empty() {
                    ui.add_space(16.0);
                    ui.add(SectionHeader::new("🖥️ Display Outputs"));

                    egui::Grid::new("display_outputs_grid")
                        .num_columns(4)
                        .spacing([20.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            // Header
                            ui.label(RichText::new("Type").color(CyberColors::CYAN).strong());
                            ui.label(RichText::new("Name").color(CyberColors::CYAN).strong());
                            ui.label(
                                RichText::new("Resolution")
                                    .color(CyberColors::CYAN)
                                    .strong(),
                            );
                            ui.label(RichText::new("Refresh").color(CyberColors::CYAN).strong());
                            ui.end_row();

                            for output in &peripherals.display_outputs {
                                let type_color = match output.output_type {
                                    motherboard::DisplayOutputType::Hdmi => {
                                        CyberColors::NEON_PURPLE
                                    }
                                    motherboard::DisplayOutputType::DisplayPort => {
                                        CyberColors::NEON_GREEN
                                    }
                                    motherboard::DisplayOutputType::Thunderbolt => {
                                        CyberColors::NEON_YELLOW
                                    }
                                    motherboard::DisplayOutputType::UsbC => CyberColors::NEON_BLUE,
                                    _ => CyberColors::TEXT_SECONDARY,
                                };
                                ui.label(
                                    RichText::new(format!("{}", output.output_type))
                                        .color(type_color),
                                );
                                ui.label(
                                    RichText::new(&output.name).color(CyberColors::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new(output.resolution.as_deref().unwrap_or("-"))
                                        .color(CyberColors::NEON_YELLOW),
                                );
                                let refresh = output
                                    .refresh_rate
                                    .map(|r| format!("{} Hz", r))
                                    .unwrap_or_else(|| "-".to_string());
                                ui.label(RichText::new(refresh).color(CyberColors::TEXT_SECONDARY));
                                ui.end_row();
                            }
                        });
                }

                // Audio Devices Section
                if !peripherals.audio_devices.is_empty() {
                    ui.add_space(16.0);
                    ui.add(SectionHeader::new("🔊 Audio Devices"));

                    egui::Grid::new("audio_devices_grid")
                        .num_columns(4)
                        .spacing([20.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            // Header
                            ui.label(RichText::new("Type").color(CyberColors::CYAN).strong());
                            ui.label(RichText::new("Device").color(CyberColors::CYAN).strong());
                            ui.label(
                                RichText::new("Manufacturer")
                                    .color(CyberColors::CYAN)
                                    .strong(),
                            );
                            ui.label(RichText::new("Status").color(CyberColors::CYAN).strong());
                            ui.end_row();

                            for device in &peripherals.audio_devices {
                                let type_color = match device.device_type {
                                    motherboard::AudioDeviceType::Output => CyberColors::NEON_GREEN,
                                    motherboard::AudioDeviceType::Input => CyberColors::NEON_BLUE,
                                    motherboard::AudioDeviceType::OutputInput => {
                                        CyberColors::NEON_PURPLE
                                    }
                                    motherboard::AudioDeviceType::Unknown => {
                                        CyberColors::TEXT_MUTED
                                    }
                                };
                                ui.label(
                                    RichText::new(format!("{}", device.device_type))
                                        .color(type_color),
                                );
                                ui.label(
                                    RichText::new(&device.name).color(CyberColors::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new(device.manufacturer.as_deref().unwrap_or("-"))
                                        .color(CyberColors::TEXT_SECONDARY),
                                );
                                let status_color = if device.status.as_deref() == Some("OK") {
                                    CyberColors::NEON_GREEN
                                } else {
                                    CyberColors::NEON_ORANGE
                                };
                                ui.label(
                                    RichText::new(device.status.as_deref().unwrap_or("-"))
                                        .color(status_color),
                                );
                                ui.end_row();
                            }
                        });
                }

                // Network Ports Section
                if !peripherals.network_ports.is_empty() {
                    ui.add_space(16.0);
                    ui.add(SectionHeader::new("🌐 Network Ports"));

                    egui::Grid::new("network_ports_grid")
                        .num_columns(4)
                        .spacing([20.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            // Header
                            ui.label(RichText::new("Type").color(CyberColors::CYAN).strong());
                            ui.label(RichText::new("Adapter").color(CyberColors::CYAN).strong());
                            ui.label(RichText::new("Speed").color(CyberColors::CYAN).strong());
                            ui.label(RichText::new("MAC").color(CyberColors::CYAN).strong());
                            ui.end_row();

                            for port in &peripherals.network_ports {
                                let type_color = match port.port_type {
                                    motherboard::NetworkPortType::Ethernet => {
                                        CyberColors::NEON_BLUE
                                    }
                                    motherboard::NetworkPortType::WiFi => CyberColors::NEON_GREEN,
                                    motherboard::NetworkPortType::Bluetooth => {
                                        CyberColors::NEON_PURPLE
                                    }
                                    motherboard::NetworkPortType::Thunderbolt => {
                                        CyberColors::NEON_YELLOW
                                    }
                                    motherboard::NetworkPortType::Other => CyberColors::TEXT_MUTED,
                                };
                                ui.label(
                                    RichText::new(format!("{}", port.port_type)).color(type_color),
                                );
                                ui.label(
                                    RichText::new(&port.name).color(CyberColors::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new(port.speed.as_deref().unwrap_or("-"))
                                        .color(CyberColors::NEON_YELLOW),
                                );
                                ui.label(
                                    RichText::new(port.mac_address.as_deref().unwrap_or("-"))
                                        .color(CyberColors::TEXT_MUTED)
                                        .small(),
                                );
                                ui.end_row();
                            }
                        });
                }

                // Bluetooth Devices Section (if any)
                if !peripherals.bluetooth_devices.is_empty() {
                    ui.add_space(16.0);
                    ui.add(SectionHeader::new("📶 Bluetooth Devices"));

                    egui::Grid::new("bluetooth_devices_grid")
                        .num_columns(3)
                        .spacing([20.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(RichText::new("Device").color(CyberColors::CYAN).strong());
                            ui.label(RichText::new("Address").color(CyberColors::CYAN).strong());
                            ui.label(RichText::new("Status").color(CyberColors::CYAN).strong());
                            ui.end_row();

                            for device in &peripherals.bluetooth_devices {
                                ui.label(
                                    RichText::new(&device.name).color(CyberColors::TEXT_PRIMARY),
                                );
                                ui.label(
                                    RichText::new(device.address.as_deref().unwrap_or("-"))
                                        .color(CyberColors::TEXT_MUTED),
                                );
                                let status = if device.connected {
                                    "Connected"
                                } else if device.paired {
                                    "Paired"
                                } else {
                                    "Available"
                                };
                                let status_color = if device.connected {
                                    CyberColors::NEON_GREEN
                                } else {
                                    CyberColors::TEXT_SECONDARY
                                };
                                ui.label(RichText::new(status).color(status_color));
                                ui.end_row();
                            }
                        });
                }
            } else if !self.system_info_loading {
                ui.add_space(16.0);
                ui.label(
                    RichText::new("⚠ Peripheral information not available")
                        .color(CyberColors::NEON_ORANGE),
                );
            }
        });
    }

    fn draw_network_tools_tab(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                // Header
                ui.add(SectionHeader::new("🔧 Network Diagnostic Tools"));
                ui.label(
                    RichText::new("nmap • traceroute • ping • netcat style utilities")
                        .color(CyberColors::TEXT_SECONDARY),
                );
                ui.add_space(10.0);

                // Target Host Input
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Target:").color(CyberColors::CYAN));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.nettools_target_host)
                            .desired_width(200.0)
                            .hint_text("hostname or IP"),
                    );
                    ui.add_space(20.0);

                    // Ping button
                    if ui
                        .button(RichText::new("🔔 Ping").color(CyberColors::NEON_GREEN))
                        .clicked()
                        && !self.nettools_is_running
                    {
                        let host = self.nettools_target_host.clone();
                        match network_tools::ping(&host, 4) {
                            Ok(result) => {
                                self.nettools_ping_result = Some(result);
                                self.nettools_operation = "Ping complete".to_string();
                            }
                            Err(e) => {
                                self.nettools_operation = format!("Ping failed: {}", e);
                            }
                        }
                    }

                    // Traceroute button
                    if ui
                        .button(RichText::new("🗺️ Traceroute").color(CyberColors::NEON_BLUE))
                        .clicked()
                        && !self.nettools_is_running
                    {
                        let host = self.nettools_target_host.clone();
                        match network_tools::traceroute(&host, 30) {
                            Ok(result) => {
                                self.nettools_traceroute_result = Some(result);
                                self.nettools_operation = "Traceroute complete".to_string();
                            }
                            Err(e) => {
                                self.nettools_operation = format!("Traceroute failed: {}", e);
                            }
                        }
                    }

                    // DNS Lookup button
                    if ui
                        .button(RichText::new("📖 DNS").color(CyberColors::NEON_YELLOW))
                        .clicked()
                        && !self.nettools_is_running
                    {
                        let host = self.nettools_target_host.clone();
                        match network_tools::dns_lookup(&host) {
                            Ok(addrs) => {
                                self.nettools_dns_results = addrs;
                                self.nettools_operation = "DNS lookup complete".to_string();
                            }
                            Err(e) => {
                                self.nettools_operation = format!("DNS lookup failed: {}", e);
                            }
                        }
                    }
                });

                // Port Scan Section
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Port Range:").color(CyberColors::CYAN));
                    let mut start = self.nettools_port_range_start as i32;
                    let mut end = self.nettools_port_range_end as i32;
                    ui.add(egui::DragValue::new(&mut start).range(1..=65535).prefix("Start: "));
                    ui.add(egui::DragValue::new(&mut end).range(1..=65535).prefix("End: "));
                    self.nettools_port_range_start = start as u16;
                    self.nettools_port_range_end = end as u16;

                    // Common ports button (parallel scan)
                    if ui
                        .button(RichText::new("🔍 Scan Common").color(CyberColors::NEON_PURPLE))
                        .clicked()
                        && !self.nettools_is_running
                    {
                        let host = self.nettools_target_host.clone();
                        let ports = network_tools::common_ports();
                        match network_tools::parallel_scan(&host, &ports, std::time::Duration::from_secs(1), 50) {
                            Ok(results) => {
                                self.nettools_port_scan_results = results;
                                self.nettools_operation = "Port scan complete".to_string();
                            }
                            Err(e) => {
                                self.nettools_operation = format!("Port scan failed: {}", e);
                            }
                        }
                    }

                    // Scan range button (parallel)
                    if ui
                        .button(RichText::new("🔎 Scan Range").color(CyberColors::NEON_ORANGE))
                        .clicked()
                        && !self.nettools_is_running
                    {
                        let host = self.nettools_target_host.clone();
                        let start = self.nettools_port_range_start;
                        let end = self.nettools_port_range_end;
                        let ports: Vec<u16> = (start..=end).collect();
                        match network_tools::parallel_scan(&host, &ports, std::time::Duration::from_secs(1), 100) {
                            Ok(results) => {
                                self.nettools_port_scan_results = results;
                                self.nettools_operation = "Port scan complete".to_string();
                            }
                            Err(e) => {
                                self.nettools_operation = format!("Port scan failed: {}", e);
                            }
                        }
                    }
                });

                // Nmap-style scan section
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Nmap-Style Scans:").color(CyberColors::CYAN));

                    // Quick scan button
                    if ui
                        .button(RichText::new("⚡ Quick Scan").color(CyberColors::NEON_GREEN))
                        .clicked()
                        && !self.nettools_is_running
                    {
                        let host = self.nettools_target_host.clone();
                        match network_tools::quick_scan(&host) {
                            Ok(result) => {
                                self.nettools_nmap_result = Some(result);
                                self.nettools_operation = "Nmap scan complete".to_string();
                            }
                            Err(e) => {
                                self.nettools_operation = format!("Nmap scan failed: {}", e);
                            }
                        }
                    }

                    // Full scan button
                    if ui
                        .button(RichText::new("🔬 Full Scan").color(CyberColors::NEON_BLUE))
                        .clicked()
                        && !self.nettools_is_running
                    {
                        let host = self.nettools_target_host.clone();
                        match network_tools::full_scan(&host, std::time::Duration::from_millis(500)) {
                            Ok(result) => {
                                let duration = result.scan_duration_secs;
                                self.nettools_nmap_result = Some(result);
                                self.nettools_operation = format!("Nmap scan complete in {:.2}s", duration);
                            }
                            Err(e) => {
                                self.nettools_operation = format!("Nmap scan failed: {}", e);
                            }
                        }
                    }
                });

                // Status line
                if !self.nettools_operation.is_empty() {
                    ui.add_space(5.0);
                    ui.label(
                        RichText::new(&self.nettools_operation)
                            .color(CyberColors::TEXT_SECONDARY)
                            .italics(),
                    );
                }

                ui.add_space(15.0);
                ui.separator();

                // Results Section
                ui.columns(2, |columns| {
                    // Left column: Ping & DNS results
                    columns[0].add(SectionHeader::new("📡 Ping Results"));
                    if let Some(ref result) = self.nettools_ping_result {
                        columns[0].horizontal(|ui| {
                            let status_color = if result.is_reachable {
                                CyberColors::NEON_GREEN
                            } else {
                                CyberColors::NEON_RED
                            };
                            let status_text = if result.is_reachable {
                                "✓ REACHABLE"
                            } else {
                                "✗ UNREACHABLE"
                            };
                            ui.label(RichText::new(&result.host).color(CyberColors::CYAN));
                            ui.label(RichText::new(status_text).color(status_color).strong());
                        });

                        if let Some(ref ip) = result.ip_address {
                            columns[0].label(
                                RichText::new(format!("  IP: {}", ip)).color(CyberColors::TEXT_SECONDARY),
                            );
                        }

                        columns[0].label(
                            RichText::new(format!(
                                "  Packets: {} sent, {} received, {:.0}% loss",
                                result.packets_sent, result.packets_received, result.packet_loss_percent
                            ))
                            .color(CyberColors::TEXT_PRIMARY),
                        );

                        if result.is_reachable {
                            columns[0].label(
                                RichText::new(format!(
                                    "  RTT: min={:.2}ms avg={:.2}ms max={:.2}ms",
                                    result.rtt_min_ms, result.rtt_avg_ms, result.rtt_max_ms
                                ))
                                .color(CyberColors::NEON_YELLOW),
                            );

                            // RTT visualization
                            columns[0].add_space(5.0);
                            let rtt_data: Vec<f32> = result.ping_times.iter()
                                .filter_map(|t| t.map(|v| v as f32))
                                .collect();
                            if !rtt_data.is_empty() {
                                columns[0].add(SparklineChart::new(rtt_data).color(CyberColors::CYAN));
                            }
                        }
                    } else {
                        columns[0].label(
                            RichText::new("No ping results yet").color(CyberColors::TEXT_MUTED),
                        );
                    }

                    // DNS Results
                    columns[0].add_space(10.0);
                    columns[0].add(SectionHeader::new("📖 DNS Results"));
                    if !self.nettools_dns_results.is_empty() {
                        for addr in &self.nettools_dns_results {
                            let addr_color = if addr.is_ipv4() {
                                CyberColors::NEON_GREEN
                            } else {
                                CyberColors::NEON_BLUE
                            };
                            columns[0].label(
                                RichText::new(format!("  → {}", addr)).color(addr_color),
                            );
                        }
                    } else {
                        columns[0].label(
                            RichText::new("No DNS results yet").color(CyberColors::TEXT_MUTED),
                        );
                    }

                    // Right column: Traceroute results
                    columns[1].add(SectionHeader::new("🗺️ Traceroute Results"));
                    if let Some(ref result) = self.nettools_traceroute_result {
                        columns[1].label(
                            RichText::new(format!(
                                "Route to {} ({} hops)",
                                result.target,
                                result.hops.len()
                            ))
                            .color(CyberColors::CYAN),
                        );

                        let status_color = if result.destination_reached {
                            CyberColors::NEON_GREEN
                        } else {
                            CyberColors::NEON_YELLOW
                        };
                        let status_text = if result.destination_reached {
                            "✓ Destination reached"
                        } else {
                            "⚠ Destination not reached"
                        };
                        columns[1].label(RichText::new(status_text).color(status_color));

                        columns[1].add_space(5.0);
                        ScrollArea::vertical()
                            .id_salt("traceroute_scroll")
                            .max_height(200.0)
                            .show(&mut columns[1], |ui| {
                                for hop in &result.hops {
                                    let addr = hop.address.as_deref().unwrap_or("*");
                                    let rtt = hop
                                        .rtt_ms
                                        .map(|r| format!("{:.2}ms", r))
                                        .unwrap_or_else(|| "*".to_string());

                                    let addr_color = if hop.responded {
                                        CyberColors::NEON_GREEN
                                    } else {
                                        CyberColors::TEXT_MUTED
                                    };

                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!("{:>2}", hop.ttl))
                                                .color(CyberColors::CYAN),
                                        );
                                        ui.label(
                                            RichText::new(format!("{:>15}", addr))
                                                .color(addr_color)
                                                .monospace(),
                                        );
                                        ui.label(
                                            RichText::new(format!("{:>10}", rtt))
                                                .color(CyberColors::NEON_YELLOW)
                                                .monospace(),
                                        );
                                        if let Some(ref hostname) = hop.hostname {
                                            ui.label(
                                                RichText::new(hostname)
                                                    .color(CyberColors::TEXT_SECONDARY),
                                            );
                                        }
                                    });
                                }
                            });
                    } else {
                        columns[1].label(
                            RichText::new("No traceroute results yet").color(CyberColors::TEXT_MUTED),
                        );
                    }
                });

                // Port Scan Results
                ui.add_space(15.0);
                ui.separator();
                ui.add(SectionHeader::new("🔍 Port Scan Results"));

                if !self.nettools_port_scan_results.is_empty() {
                    // Summary
                    let open_count = self.nettools_port_scan_results.iter()
                        .filter(|p| p.status == PortStatus::Open)
                        .count();
                    let closed_count = self.nettools_port_scan_results.iter()
                        .filter(|p| p.status == PortStatus::Closed)
                        .count();
                    let filtered_count = self.nettools_port_scan_results.iter()
                        .filter(|p| p.status == PortStatus::Filtered)
                        .count();

                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Scanned {} ports: ", self.nettools_port_scan_results.len()))
                                .color(CyberColors::TEXT_PRIMARY),
                        );
                        ui.label(
                            RichText::new(format!("{} open", open_count))
                                .color(CyberColors::NEON_GREEN),
                        );
                        ui.label(
                            RichText::new(format!("{} closed", closed_count))
                                .color(CyberColors::NEON_RED),
                        );
                        ui.label(
                            RichText::new(format!("{} filtered", filtered_count))
                                .color(CyberColors::NEON_YELLOW),
                        );
                    });

                    ui.add_space(5.0);

                    // Show open/filtered ports in a grid (nmap style output)
                    ScrollArea::vertical()
                        .id_salt("port_scan_scroll")
                        .max_height(250.0)
                        .show(ui, |ui| {
                            egui::Grid::new("port_scan_grid")
                                .num_columns(4)
                                .spacing([20.0, 4.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    // Header
                                    ui.label(RichText::new("PORT").color(CyberColors::CYAN).strong());
                                    ui.label(RichText::new("STATE").color(CyberColors::CYAN).strong());
                                    ui.label(RichText::new("SERVICE").color(CyberColors::CYAN).strong());
                                    ui.label(RichText::new("CONNECT").color(CyberColors::CYAN).strong());
                                    ui.end_row();

                                    for result in &self.nettools_port_scan_results {
                                        // Only show open/filtered ports (like nmap default)
                                        if result.status != PortStatus::Open && result.status != PortStatus::Filtered {
                                            continue;
                                        }

                                        let status_color = match result.status {
                                            PortStatus::Open => CyberColors::NEON_GREEN,
                                            PortStatus::Closed => CyberColors::NEON_RED,
                                            PortStatus::Filtered => CyberColors::NEON_YELLOW,
                                            PortStatus::Error => CyberColors::TEXT_MUTED,
                                        };

                                        ui.label(
                                            RichText::new(format!("{}/tcp", result.port))
                                                .color(CyberColors::TEXT_PRIMARY),
                                        );
                                        ui.label(
                                            RichText::new(format!("{}", result.status))
                                                .color(status_color),
                                        );
                                        ui.label(
                                            RichText::new(result.service.as_deref().unwrap_or("-"))
                                                .color(CyberColors::TEXT_SECONDARY),
                                        );
                                        ui.label(
                                            RichText::new(
                                                result
                                                    .connect_time_ms
                                                    .map(|t| format!("{:.1}ms", t))
                                                    .unwrap_or_else(|| "-".to_string()),
                                            )
                                            .color(CyberColors::NEON_YELLOW),
                                        );
                                        ui.end_row();
                                    }
                                });
                        });
                } else {
                    ui.label(
                        RichText::new("No port scan results yet. Use 'Scan Common' or 'Scan Range' to scan ports.")
                            .color(CyberColors::TEXT_MUTED),
                    );
                }

                // Nmap Scan Results Section
                ui.add_space(15.0);
                ui.separator();
                ui.add(SectionHeader::new("🎯 Nmap-Style Scan Results"));

                if let Some(ref result) = self.nettools_nmap_result {
                    // Host info
                    ui.horizontal(|ui| {
                        let status_color = if result.is_up {
                            CyberColors::NEON_GREEN
                        } else {
                            CyberColors::NEON_RED
                        };
                        let status_text = if result.is_up { "UP" } else { "DOWN" };

                        ui.label(RichText::new(&result.host).color(CyberColors::CYAN));
                        ui.label(RichText::new(format!("({})", status_text)).color(status_color));
                        if let Some(latency) = result.latency_ms {
                            ui.label(RichText::new(format!("{:.2}ms latency", latency)).color(CyberColors::NEON_YELLOW));
                        }
                    });

                    // IP addresses
                    if !result.ip_addresses.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("IP(s):").color(CyberColors::TEXT_MUTED));
                            for ip in &result.ip_addresses {
                                ui.label(RichText::new(ip).color(CyberColors::NEON_GREEN));
                            }
                        });
                    }

                    // Hostname
                    if let Some(ref hostname) = result.hostname {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Hostname:").color(CyberColors::TEXT_MUTED));
                            ui.label(RichText::new(hostname).color(CyberColors::TEXT_PRIMARY));
                        });
                    }

                    // OS fingerprint
                    if let Some(ref os) = result.os_fingerprint {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("OS Guess:").color(CyberColors::TEXT_MUTED));
                            let os_text = match (&os.os_family, &os.os_gen) {
                                (Some(family), Some(gen)) => format!("{} {}", family, gen),
                                (Some(family), None) => family.clone(),
                                _ => "Unknown".to_string(),
                            };
                            ui.label(RichText::new(os_text).color(CyberColors::NEON_PURPLE));
                            ui.label(RichText::new(format!("({}% confidence)", os.confidence)).color(CyberColors::TEXT_SECONDARY));
                        });
                    }

                    ui.add_space(5.0);
                    ui.label(RichText::new(format!("Scan completed in {:.2}s", result.scan_duration_secs)).color(CyberColors::TEXT_SECONDARY));

                    // Services table
                    if !result.services.is_empty() {
                        ui.add_space(10.0);
                        ui.label(RichText::new(format!("{} open port(s) detected:", result.services.len())).color(CyberColors::CYAN));

                        ScrollArea::vertical()
                            .id_salt("nmap_services_scroll")
                            .max_height(200.0)
                            .show(ui, |ui| {
                                egui::Grid::new("nmap_services_grid")
                                    .num_columns(4)
                                    .spacing([20.0, 4.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        // Header
                                        ui.label(RichText::new("PORT").color(CyberColors::CYAN).strong());
                                        ui.label(RichText::new("SERVICE").color(CyberColors::CYAN).strong());
                                        ui.label(RichText::new("VERSION").color(CyberColors::CYAN).strong());
                                        ui.label(RichText::new("BANNER").color(CyberColors::CYAN).strong());
                                        ui.end_row();

                                        for svc in &result.services {
                                            ui.label(RichText::new(format!("{}/tcp", svc.port)).color(CyberColors::TEXT_PRIMARY));
                                            ui.label(RichText::new(&svc.service).color(CyberColors::NEON_GREEN));

                                            let version = match (&svc.product, &svc.version) {
                                                (Some(p), Some(v)) => format!("{} {}", p, v),
                                                (Some(p), None) => p.clone(),
                                                (None, Some(v)) => v.clone(),
                                                _ => "-".to_string(),
                                            };
                                            ui.label(RichText::new(version).color(CyberColors::NEON_PURPLE));

                                            let banner = svc.banner.as_ref()
                                                .map(|b| if b.len() > 40 { format!("{}...", &b[..40]) } else { b.clone() })
                                                .unwrap_or_else(|| "-".to_string());
                                            ui.label(RichText::new(banner).color(CyberColors::TEXT_SECONDARY));
                                            ui.end_row();
                                        }
                                    });
                            });
                    } else {
                        ui.label(RichText::new("No open ports found on scanned target.").color(CyberColors::TEXT_MUTED));
                    }
                } else {
                    ui.label(
                        RichText::new("No nmap scan results yet. Use 'Quick Scan' or 'Full Scan' for service detection.")
                            .color(CyberColors::TEXT_MUTED),
                    );
                }

                // Packet Capture (tcpdump-style) Section
                ui.add_space(15.0);
                ui.separator();
                ui.add(SectionHeader::new("📦 Packet Capture (tcpdump)"));

                // Check if capture tools are available
                let capture_available = network_tools::is_capture_available();

                if !capture_available {
                    ui.label(
                        RichText::new("⚠️ No packet capture tool found. Install Wireshark (tshark) or tcpdump.")
                            .color(CyberColors::WARNING),
                    );
                } else {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Protocol:").color(CyberColors::CYAN));
                        egui::ComboBox::from_id_salt("capture_protocol")
                            .selected_text(format!("{}", self.nettools_capture_protocol))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.nettools_capture_protocol, network_tools::CaptureProtocol::All, "All");
                                ui.selectable_value(&mut self.nettools_capture_protocol, network_tools::CaptureProtocol::Tcp, "TCP");
                                ui.selectable_value(&mut self.nettools_capture_protocol, network_tools::CaptureProtocol::Udp, "UDP");
                                ui.selectable_value(&mut self.nettools_capture_protocol, network_tools::CaptureProtocol::Icmp, "ICMP");
                                ui.selectable_value(&mut self.nettools_capture_protocol, network_tools::CaptureProtocol::Http, "HTTP");
                                ui.selectable_value(&mut self.nettools_capture_protocol, network_tools::CaptureProtocol::Https, "HTTPS");
                                ui.selectable_value(&mut self.nettools_capture_protocol, network_tools::CaptureProtocol::Dns, "DNS");
                                ui.selectable_value(&mut self.nettools_capture_protocol, network_tools::CaptureProtocol::Ssh, "SSH");
                            });

                        ui.add_space(10.0);
                        ui.label(RichText::new("Packets:").color(CyberColors::CYAN));
                        let mut count = self.nettools_capture_count as i32;
                        ui.add(egui::DragValue::new(&mut count).range(10..=1000));
                        self.nettools_capture_count = count as u32;

                        ui.add_space(10.0);
                        if ui
                            .button(RichText::new("📡 Capture").color(CyberColors::NEON_GREEN))
                            .clicked()
                            && !self.nettools_is_running
                        {
                            let config = network_tools::CaptureConfig {
                                protocol: self.nettools_capture_protocol,
                                host_filter: if self.nettools_target_host.is_empty() || self.nettools_target_host == "8.8.8.8" {
                                    None
                                } else {
                                    Some(self.nettools_target_host.clone())
                                },
                                packet_count: self.nettools_capture_count,
                                timeout_secs: 30,
                                ..Default::default()
                            };

                            match network_tools::capture_packets(&config) {
                                Ok(result) => {
                                    self.nettools_capture_result = Some(result);
                                    self.nettools_operation = "Capture complete".to_string();
                                }
                                Err(e) => {
                                    self.nettools_operation = format!("Capture failed: {}", e);
                                }
                            }
                        }
                    });

                    // Capture results
                    if let Some(ref result) = self.nettools_capture_result {
                        ui.add_space(10.0);

                        // Summary stats
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} packets", result.total_packets)).color(CyberColors::NEON_GREEN));
                            ui.label(RichText::new(format!("({:.1} pkt/s)", result.packets_per_sec)).color(CyberColors::TEXT_SECONDARY));
                            ui.label(RichText::new(format!("{} bytes", result.total_bytes)).color(CyberColors::CYAN));
                            ui.label(RichText::new(format!("in {:.2}s", result.duration_secs)).color(CyberColors::TEXT_MUTED));
                        });

                        // Protocol breakdown
                        if !result.protocol_stats.is_empty() {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Protocols:").color(CyberColors::TEXT_MUTED));
                                for (proto, count) in &result.protocol_stats {
                                    ui.label(RichText::new(format!("{}: {}", proto, count)).color(CyberColors::NEON_YELLOW));
                                }
                            });
                        }

                        // Top talkers
                        ui.columns(2, |cols| {
                            cols[0].label(RichText::new("Top Sources:").color(CyberColors::CYAN).small());
                            for (addr, count) in result.top_sources.iter().take(5) {
                                cols[0].label(RichText::new(format!("  {} ({})", addr, count)).color(CyberColors::TEXT_SECONDARY).small());
                            }

                            cols[1].label(RichText::new("Top Destinations:").color(CyberColors::CYAN).small());
                            for (addr, count) in result.top_destinations.iter().take(5) {
                                cols[1].label(RichText::new(format!("  {} ({})", addr, count)).color(CyberColors::TEXT_SECONDARY).small());
                            }
                        });

                        // Packet table
                        ui.add_space(5.0);
                        ScrollArea::vertical()
                            .id_salt("capture_packets_scroll")
                            .max_height(200.0)
                            .show(ui, |ui| {
                                egui::Grid::new("capture_packets_grid")
                                    .num_columns(6)
                                    .spacing([10.0, 2.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        // Header
                                        ui.label(RichText::new("#").color(CyberColors::CYAN).small());
                                        ui.label(RichText::new("TIME").color(CyberColors::CYAN).small());
                                        ui.label(RichText::new("SOURCE").color(CyberColors::CYAN).small());
                                        ui.label(RichText::new("DEST").color(CyberColors::CYAN).small());
                                        ui.label(RichText::new("PROTO").color(CyberColors::CYAN).small());
                                        ui.label(RichText::new("LEN").color(CyberColors::CYAN).small());
                                        ui.end_row();

                                        for pkt in result.packets.iter().take(100) {
                                            ui.label(RichText::new(format!("{}", pkt.number)).color(CyberColors::TEXT_MUTED).small());
                                            ui.label(RichText::new(&pkt.timestamp).color(CyberColors::TEXT_SECONDARY).small());
                                            ui.label(RichText::new(&pkt.source).color(CyberColors::NEON_GREEN).small());
                                            ui.label(RichText::new(&pkt.destination).color(CyberColors::NEON_BLUE).small());
                                            ui.label(RichText::new(&pkt.protocol).color(CyberColors::NEON_YELLOW).small());
                                            ui.label(RichText::new(format!("{}", pkt.length)).color(CyberColors::TEXT_PRIMARY).small());
                                            ui.end_row();
                                        }
                                    });
                            });
                    } else {
                        ui.label(
                            RichText::new("No capture results yet. Click 'Capture' to start packet capture.")
                                .color(CyberColors::TEXT_MUTED),
                        );
                        ui.label(
                            RichText::new("Note: Requires administrator/root privileges.")
                                .color(CyberColors::WARNING)
                                .small(),
                        );
                    }
                }

                // Help/Info section
                ui.add_space(15.0);
                ui.separator();
                ui.collapsing(RichText::new("ℹ️ About Network Tools").color(CyberColors::CYAN), |ui| {
                    ui.label(RichText::new(
                        "This tab provides network diagnostic tools similar to popular CLI utilities:\n\n\
                        • Ping - ICMP echo test (like 'ping' command)\n\
                        • Traceroute - Path tracing with hop-by-hop latency (like 'traceroute/tracert')\n\
                        • DNS - Domain name resolution (like 'nslookup/dig')\n\
                        • Port Scan - TCP connect scan (like 'nmap -sT')\n\
                        • Nmap Scan - Service detection with banner grabbing\n\
                        • Packet Capture - Network traffic capture (like 'tcpdump/tshark')\n\n\
                        Note: Some operations may require administrator privileges or be blocked by firewalls."
                    ).color(CyberColors::TEXT_SECONDARY));
                });
            });
    }

    pub(super) fn draw_ai_assistant_tab(&mut self, ui: &mut egui::Ui) {
        // Show loading state while agent is being initialized in background
        // But timeout after 3 seconds to show the UI anyway
        let loading_timeout =
            self.agent_loading && self.agent_loading_start.elapsed().as_secs() < 3;

        if loading_timeout {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.spinner();
                ui.add_space(10.0);
                ui.label(
                    RichText::new("🔍 Detecting AI backends...")
                        .color(CyberColors::CYAN)
                        .size(18.0),
                );
                ui.add_space(10.0);
                ui.label(
                    RichText::new("Checking for Ollama, OpenAI, Anthropic, LM Studio...")
                        .color(CyberColors::TEXT_SECONDARY),
                );
            });
            return;
        }

        // Past the spinner budget, show the controls rather than making the user wait
        // on a blank tab — but detection is still running, which is not the same thing
        // as having finished and found nothing.
        if self.agent_loading && self.agent_loading_start.elapsed().as_secs() >= 3 {
            self.agent_loading = false;
        }

        // `agent_receiver` is dropped when the detection thread's result is taken, so
        // this is the truthful "still looking" signal. Backend discovery probes five
        // local ports and used to take 4.7s, comfortably past the 3s spinner budget,
        // so the tab asserted "AI backend not connected" about a backend it was in
        // the middle of finding — and then connected a second later.
        let detection_in_flight = self.agent_receiver.is_some();

        // Whether a question can actually be asked — which is not the same as what
        // auto-detection found.
        //
        // `send_agent_query` builds its own `AgentConfig` from the selections on this
        // tab and constructs its own `Agent` and `UnifiedMonitor` inside the worker
        // thread. It never read `self.agent`, nor the `UnifiedMonitor` the app used to
        // build synchronously at startup purely to gate this banner. Testing those
        // announced "AI backend not connected" over a tab that worked perfectly well,
        // whenever a startup probe missed or that second GPU enumeration failed. The
        // monitor field is gone; it cost a blocking `GpuCollection::auto_detect` on
        // the UI thread and was read nowhere else.
        //
        // What genuinely prevents a send is having no model to send to.
        // A CLI tool picks its own model, so an empty selection is fine there.
        let needs_model = self.ai_selected_model.is_empty()
            && !matches!(self.ai_selected_backend, AiBackendSelection::Cli(_))
            && self
                .models_by_provider
                .get(&self.ai_selected_backend)
                .is_some_and(|m| m.is_empty())
            && self.ai_selected_backend.fallback_models().is_empty();
        let agent_available = !needs_model;

        // Detection did run, and found nothing. Worth saying — it means the default
        // selection is a guess rather than something observed — but it does not stop
        // the user asking a question, and the answer path reports its own failures.
        let detection_found_nothing =
            !detection_in_flight && self.agent.is_none() && self.agent_detect_attempts > 0;

        // Wrap content in ScrollArea like other tabs
        ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(8.0);

            // Header with provider/model selection
            let mut refresh_models = false;
            let mut retry_detection = false;

            ui.horizontal(|ui| {
                ui.add(SectionHeader::new("🤖 AI System Assistant"));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Model dropdown - use detected models for Ollama, predefined for others
                    let display_model = if self.ai_selected_model.is_empty() {
                        "Select model...".to_string()
                    } else {
                        self.ai_selected_model.clone()
                    };

                // Re-ask the provider what it serves.
                if ui
                    .button(RichText::new("🔄").size(14.0))
                    .on_hover_text("Refresh model list")
                    .clicked()
                {
                    refresh_models = true;
                }

                egui::ComboBox::from_id_salt("ai_model_select")
                    .selected_text(RichText::new(&display_model).color(CyberColors::NEON_GREEN))
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        let provider = self.ai_selected_backend;
                        let loading = self.models_loading_for == Some(provider);
                        let listed = self.models_by_provider.get(&provider);

                        match listed {
                            Some(models) if !models.is_empty() => {
                                for model in models.clone() {
                                    ui.selectable_value(
                                        &mut self.ai_selected_model,
                                        model.clone(),
                                        &model,
                                    );
                                }
                            }
                            _ if loading => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(
                                        RichText::new("Loading models…")
                                            .color(CyberColors::TEXT_MUTED),
                                    );
                                });
                            }
                            _ => {
                                // The provider could not be asked. Offer known names
                                // where they exist, and say plainly that these were
                                // not read from the provider.
                                let fallback = provider.fallback_models();
                                if fallback.is_empty() {
                                    ui.label(
                                        RichText::new(match provider {
                                            AiBackendSelection::Cli(_) => {
                                                "This tool selects its own model"
                                            }
                                            _ => "No models listed — is the server running?",
                                        })
                                        .color(CyberColors::TEXT_MUTED),
                                    );
                                } else {
                                    ui.label(
                                        RichText::new("Not listed by provider:")
                                            .color(CyberColors::TEXT_MUTED)
                                            .small(),
                                    );
                                    for model in fallback {
                                        ui.selectable_value(
                                            &mut self.ai_selected_model,
                                            model.to_string(),
                                            *model,
                                        );
                                    }
                                }
                            }
                        }
                    });

                ui.label(RichText::new("Model:").color(CyberColors::TEXT_SECONDARY).size(12.0));

                ui.add_space(12.0);

                // Provider dropdown
                egui::ComboBox::from_id_salt("ai_provider_select")
                    .selected_text(
                        RichText::new(self.ai_selected_backend.menu_label())
                            .color(CyberColors::CYAN),
                    )
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for provider in AiBackendSelection::ALL {
                            ui.selectable_value(
                                &mut self.ai_selected_backend,
                                *provider,
                                provider.menu_label(),
                            );
                        }
                    });

                ui.label(RichText::new("Provider:").color(CyberColors::TEXT_SECONDARY).size(12.0));
            });
        });

        // Handle deferred refresh after the UI block
        if refresh_models {
            self.models_by_provider.remove(&self.ai_selected_backend);
            self.fetch_models_async(self.ai_selected_backend, ui.ctx().clone());
        }
        if retry_detection {
            self.spawn_agent_detection();
        }

        // Detect backend change and reset model selection to match new provider
        if self.ai_selected_backend != self.ai_prev_backend {
            self.ai_prev_backend = self.ai_selected_backend;
            // Adopt whatever the new provider reported. If it has not been asked yet,
            // leave the model empty: the fetch started by `check_background_loaders`
            // fills it in, and an empty box is honest about not knowing yet where a
            // guessed name would produce a request for a model that may not exist.
            self.ai_selected_model = self
                .models_by_provider
                .get(&self.ai_selected_backend)
                .and_then(|m| m.first().cloned())
                .unwrap_or_default();
            // Reconnect agent with new backend/model selection
            self.retry_agent_connection();
        }

        ui.add_space(8.0);

        // Show connection status when no agent is available yet. While detection is
        // still running this must report that, not a failure it has not established.
        if !agent_available || detection_in_flight || detection_found_nothing {
            let accent = if agent_available {
                CyberColors::CYAN
            } else {
                CyberColors::NEON_YELLOW
            };
            egui::Frame::none()
                .fill(CyberColors::SURFACE)
                .stroke(egui::Stroke::new(1.0_f32, accent))
                .rounding(6.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if detection_in_flight {
                            ui.spinner();
                            ui.label(RichText::new("Detecting AI backends…").color(accent));
                            return;
                        }
                        if !agent_available {
                            ui.label(RichText::new("⚠").color(accent).size(16.0));
                            ui.label(RichText::new(
                                "No Ollama models installed. Run: ollama pull llama3.2",
                            ).color(accent));
                        } else {
                            // Detection came up empty, but nothing is broken: sending
                            // uses the provider selected above and will report its own
                            // error if that provider is not actually there.
                            ui.label(RichText::new("ⓘ").color(accent).size(16.0));
                            ui.label(RichText::new(format!(
                                "No backend detected on the usual local ports. Sending will still try {}.",
                                self.ai_selected_backend.display_name(),
                            )).color(accent));
                        }
                        if ui.button(RichText::new("🔄 Retry").color(CyberColors::CYAN)).clicked() {
                            refresh_models = true;
                            retry_detection = true;
                        }
                    });
                });
            ui.add_space(8.0);
        }

        // Chat history area - fills available space
        let chat_height = ui.available_height() - 80.0; // Leave room for input area

        egui::Frame::none()
            .fill(CyberColors::BACKGROUND)
            .stroke(egui::Stroke::new(1.0_f32, CyberColors::BORDER))
            .rounding(6.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_min_size(egui::vec2(ui.available_width(), chat_height.max(300.0)));

                if self.agent_history.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label(
                            RichText::new("👋 Welcome to the AI Assistant!")
                                .color(CyberColors::CYAN)
                                .size(20.0),
                        );
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new("Ask questions about your system's performance, GPU status, or get optimization suggestions.")
                                .color(CyberColors::TEXT_SECONDARY)
                                .size(14.0),
                        );
                        ui.add_space(24.0);

                        // Example questions
                        egui::Frame::none()
                            .fill(CyberColors::SURFACE)
                            .rounding(6.0)
                            .inner_margin(12.0)
                            .show(ui, |ui| {
                                ui.label(RichText::new("💡 Try asking:").color(CyberColors::TEXT_PRIMARY).size(13.0));
                                ui.add_space(8.0);
                                let examples = [
                                    "What is my GPU utilization?",
                                    "Is my system running hot?",
                                    "How can I optimize performance?",
                                    "What's using my GPU memory?",
                                ];
                                for example in examples {
                                    ui.label(RichText::new(format!("  • {}", example)).color(CyberColors::CYAN_DIM).size(12.0));
                                }
                            });
                    });
                } else {
                    // Scroll area for messages
                    let scroll_height = chat_height - 30.0;
                    egui::ScrollArea::vertical()
                        .max_height(scroll_height.max(250.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                    // Calculate max bubble width - allow wider bubbles
                    let max_bubble_width = (ui.available_width() - 60.0).min(800.0);

                    for entry in self.agent_history.iter() {
                        let is_user = entry.role == ChatRole::User;

                        let (bg_color, border_color, text_color, icon) = if is_user {
                            (CyberColors::SURFACE, CyberColors::CYAN_DIM, CyberColors::TEXT_PRIMARY, "👤")
                        } else {
                            (CyberColors::BACKGROUND_DARK, CyberColors::NEON_GREEN, CyberColors::TEXT_PRIMARY, "🤖")
                        };

                        // Message bubble - left aligned for assistant, right padding for user
                        if is_user {
                            ui.add_space(40.0); // Indent user messages
                        }

                        egui::Frame::none()
                            .fill(bg_color)
                            .stroke(egui::Stroke::new(1.0_f32, border_color))
                            .inner_margin(10.0)
                            .rounding(8.0)
                            .show(ui, |ui| {
                                ui.set_max_width(max_bubble_width);

                                // Header with icon and role
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(icon).size(14.0));
                                    ui.label(
                                        RichText::new(if is_user { "You" } else { "Assistant" })
                                            .color(if is_user { CyberColors::CYAN } else { CyberColors::NEON_GREEN })
                                            .strong()
                                            .size(12.0),
                                    );

                                    // Metadata on the right
                                    if !is_user {
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let meta = if entry.from_cache {
                                                "⚡ cached".to_string()
                                            } else if let Some(ms) = entry.inference_time_ms {
                                                format!("⏱ {}ms", ms)
                                            } else {
                                                String::new()
                                            };
                                            if !meta.is_empty() {
                                                ui.label(RichText::new(meta).color(CyberColors::TEXT_MUTED).size(10.0));
                                            }
                                        });
                                    }
                                });

                                ui.add_space(4.0);

                                // Message content with text wrapping
                                ui.label(
                                    RichText::new(&entry.content)
                                        .color(text_color)
                                        .size(13.0),
                                );
                            });

                        ui.add_space(8.0);
                    }
                    }); // End ScrollArea
                }
            });

        ui.add_space(8.0);

        // Input area with improved styling
        egui::Frame::none()
            .fill(CyberColors::SURFACE)
            .stroke(egui::Stroke::new(1.0_f32, CyberColors::BORDER))
            .rounding(6.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.agent_query)
                            .hint_text("Ask about your system...")
                            .desired_width(ui.available_width() - 100.0)
                            .font(egui::TextStyle::Body),
                    );

                    let enter_pressed =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    let send_enabled = !self.agent_is_processing && !self.agent_query.trim().is_empty();
                    let send_btn = ui.add_enabled(
                        send_enabled,
                        egui::Button::new(
                            RichText::new(if self.agent_is_processing { "⏳" } else { "➤ Send" })
                                .color(if send_enabled { CyberColors::CYAN } else { CyberColors::TEXT_MUTED })
                                .size(14.0)
                        )
                        .min_size(Vec2::new(70.0, 28.0)),
                    );

                    if (enter_pressed || send_btn.clicked())
                        && !self.agent_is_processing
                        && !self.agent_query.trim().is_empty()
                    {
                        self.send_agent_query();
                    }
                });
            });

        // Bottom toolbar
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(RichText::new("🗑️ Clear").color(CyberColors::TEXT_MUTED).size(12.0)).clicked() {
                self.agent_history.clear();
            }

            ui.add_space(8.0);

            if self.agent_is_processing {
                ui.spinner();
                ui.label(
                    RichText::new("Thinking...")
                        .color(CyberColors::CYAN)
                        .italics()
                        .size(12.0),
                );
            } else {
                ui.label(
                    RichText::new(format!("{} messages", self.agent_history.len()))
                        .color(CyberColors::TEXT_MUTED)
                        .size(11.0),
                );
            }
        });
        }); // End ScrollArea
    }

    fn send_agent_query(&mut self) {
        let query = self.agent_query.trim().to_string();
        if query.is_empty() || self.agent_is_processing {
            return;
        }

        // Add user message to history
        self.agent_history.push_back(AgentChatEntry {
            role: ChatRole::User,
            content: query.clone(),
            timestamp: std::time::Instant::now(),
            inference_time_ms: None,
            from_cache: false,
        });

        self.agent_query.clear();
        self.agent_is_processing = true;

        // Build config from current GUI selections (not from stored agent which may be stale)
        let backend_config = self.build_backend_config();
        let config = crate::agent::AgentConfig::with_backend(backend_config);

        let (tx, rx) = channel();
        self.agent_response_receiver = Some(rx);

        // Spawn background thread for agent query (all heavy work off UI thread)
        std::thread::spawn(move || {
            let result = (|| -> Result<AgentResponse, String> {
                // Create fresh agent and monitor in background thread
                let mut agent = crate::agent::Agent::new(config)
                    .map_err(|e| format!("Failed to create agent: {}", e))?;
                let monitor = crate::UnifiedMonitor::new()
                    .map_err(|e| format!("Failed to create monitor: {}", e))?;

                // Get tool context in background thread (avoids blocking UI)
                let tool_context = AiDataApi::new()
                    .ok()
                    .map(|mut api| api.auto_query(&query))
                    .unwrap_or_default();

                // Enhance the query with tool context if available
                let enhanced_query = if !tool_context.is_empty() {
                    format!("{}\n\n---\n\n## User Question\n{}", tool_context, query)
                } else {
                    query
                };

                let response = agent
                    .ask(&enhanced_query, &monitor)
                    .map_err(|e| format!("{}", e))?;

                Ok(AgentResponse {
                    response: response.response,
                    inference_time_ms: response.inference_time_ms,
                    from_cache: response.from_cache,
                })
            })();

            let _ = tx.send(result);
        });
    }

    /// Draw the AI setup panel when no backend is available
    #[allow(dead_code)]
    fn draw_ai_setup_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);

        // Show status message if any
        if let Some((msg, is_error)) = &self.ai_status_message {
            let color = if *is_error {
                CyberColors::NEON_RED
            } else {
                CyberColors::NEON_GREEN
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(if *is_error { "❌" } else { "✓" }).color(color));
                ui.label(RichText::new(msg.as_str()).color(color));
            });
            ui.add_space(10.0);
        }

        // Detected backends section
        let available = crate::agent::AgentConfig::list_available_backends();
        if !available.is_empty() {
            egui::Frame::none()
                .fill(CyberColors::SURFACE)
                .rounding(8.0)
                .inner_margin(15.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("✓ Available Backends")
                            .color(CyberColors::NEON_GREEN)
                            .size(16.0),
                    );
                    ui.add_space(5.0);
                    for backend in &available {
                        ui.label(
                            RichText::new(format!("  • {:?}", backend)).color(CyberColors::CYAN),
                        );
                    }
                    ui.add_space(10.0);
                    if ui
                        .button(RichText::new("🔄 Retry Connection").color(CyberColors::CYAN))
                        .clicked()
                    {
                        self.retry_agent_connection();
                    }
                });
            ui.add_space(15.0);
        }

        // Setup options
        egui::Frame::none()
            .fill(CyberColors::SURFACE)
            .rounding(8.0)
            .inner_margin(15.0)
            .show(ui, |ui| {
                ui.label(
                    RichText::new("🔧 Configure AI Backend")
                        .color(CyberColors::CYAN)
                        .size(18.0),
                );
                ui.add_space(15.0);

                // Backend selection tabs - read current value first to avoid borrow issues
                let current_backend = self.ai_selected_backend;
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.ai_selected_backend,
                        AiBackendSelection::Ollama,
                        RichText::new("🦙 Ollama").color(
                            if current_backend == AiBackendSelection::Ollama {
                                CyberColors::CYAN
                            } else {
                                CyberColors::TEXT_SECONDARY
                            },
                        ),
                    );
                    ui.selectable_value(
                        &mut self.ai_selected_backend,
                        AiBackendSelection::OpenAi,
                        RichText::new("🤖 OpenAI").color(
                            if current_backend == AiBackendSelection::OpenAi {
                                CyberColors::CYAN
                            } else {
                                CyberColors::TEXT_SECONDARY
                            },
                        ),
                    );
                    ui.selectable_value(
                        &mut self.ai_selected_backend,
                        AiBackendSelection::Anthropic,
                        RichText::new("🧠 Anthropic").color(
                            if current_backend == AiBackendSelection::Anthropic {
                                CyberColors::CYAN
                            } else {
                                CyberColors::TEXT_SECONDARY
                            },
                        ),
                    );
                    ui.selectable_value(
                        &mut self.ai_selected_backend,
                        AiBackendSelection::GitHub,
                        RichText::new("🐙 GitHub").color(
                            if current_backend == AiBackendSelection::GitHub {
                                CyberColors::CYAN
                            } else {
                                CyberColors::TEXT_SECONDARY
                            },
                        ),
                    );
                    ui.selectable_value(
                        &mut self.ai_selected_backend,
                        AiBackendSelection::LmStudio,
                        RichText::new("📦 LM Studio").color(
                            if current_backend == AiBackendSelection::LmStudio {
                                CyberColors::CYAN
                            } else {
                                CyberColors::TEXT_SECONDARY
                            },
                        ),
                    );
                });
                ui.add_space(15.0);
                ui.separator();
                ui.add_space(10.0);

                match self.ai_selected_backend {
                    AiBackendSelection::Ollama => {
                        ui.label(
                            RichText::new("Ollama - Local AI (Recommended)")
                                .color(CyberColors::TEXT_PRIMARY)
                                .size(14.0),
                        );
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new(
                                "Run AI models locally on your machine. Free and private.",
                            )
                            .color(CyberColors::TEXT_SECONDARY),
                        );
                        ui.add_space(15.0);

                        ui.horizontal(|ui| {
                            if self.ai_ollama_starting {
                                ui.spinner();
                                ui.label(
                                    RichText::new("Starting Ollama...").color(CyberColors::CYAN),
                                );
                            } else {
                                if ui
                                    .button(
                                        RichText::new("▶ Start Ollama")
                                            .color(CyberColors::NEON_GREEN)
                                            .size(14.0),
                                    )
                                    .clicked()
                                {
                                    self.start_ollama();
                                }
                                if ui
                                    .button(
                                        RichText::new("📥 Install Ollama")
                                            .color(CyberColors::TEXT_SECONDARY),
                                    )
                                    .clicked()
                                {
                                    let _ = open::that("https://ollama.com/download");
                                }
                            }
                        });
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("After starting Ollama, click 'Retry Connection' above.")
                                .color(CyberColors::TEXT_MUTED)
                                .small(),
                        );
                    }
                    AiBackendSelection::OpenAi => {
                        ui.label(
                            RichText::new("OpenAI API")
                                .color(CyberColors::TEXT_PRIMARY)
                                .size(14.0),
                        );
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new("Use GPT models via OpenAI API. Requires API key.")
                                .color(CyberColors::TEXT_SECONDARY),
                        );
                        ui.add_space(15.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("API Key:").color(CyberColors::TEXT_SECONDARY));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.ai_api_key_input)
                                    .password(true)
                                    .hint_text("sk-...")
                                    .desired_width(300.0),
                            );
                        });
                        ui.add_space(10.0);
                        if ui
                            .button(RichText::new("💾 Set API Key").color(CyberColors::CYAN))
                            .clicked()
                        {
                            self.set_api_key("OPENAI_API_KEY");
                        }
                        ui.add_space(5.0);
                        if ui
                            .link(
                                RichText::new("Get an API key from OpenAI →")
                                    .color(CyberColors::TEXT_MUTED)
                                    .small(),
                            )
                            .clicked()
                        {
                            let _ = open::that("https://platform.openai.com/api-keys");
                        }
                    }
                    AiBackendSelection::Anthropic => {
                        ui.label(
                            RichText::new("Anthropic Claude API")
                                .color(CyberColors::TEXT_PRIMARY)
                                .size(14.0),
                        );
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new("Use Claude models via Anthropic API. Requires API key.")
                                .color(CyberColors::TEXT_SECONDARY),
                        );
                        ui.add_space(15.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("API Key:").color(CyberColors::TEXT_SECONDARY));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.ai_api_key_input)
                                    .password(true)
                                    .hint_text("sk-ant-...")
                                    .desired_width(300.0),
                            );
                        });
                        ui.add_space(10.0);
                        if ui
                            .button(RichText::new("💾 Set API Key").color(CyberColors::CYAN))
                            .clicked()
                        {
                            self.set_api_key("ANTHROPIC_API_KEY");
                        }
                        ui.add_space(5.0);
                        if ui
                            .link(
                                RichText::new("Get an API key from Anthropic →")
                                    .color(CyberColors::TEXT_MUTED)
                                    .small(),
                            )
                            .clicked()
                        {
                            let _ = open::that("https://console.anthropic.com/settings/keys");
                        }
                    }
                    AiBackendSelection::GitHub => {
                        ui.label(
                            RichText::new("GitHub Models")
                                .color(CyberColors::TEXT_PRIMARY)
                                .size(14.0),
                        );
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new("Use AI models via GitHub. Requires GitHub token.")
                                .color(CyberColors::TEXT_SECONDARY),
                        );
                        ui.add_space(15.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Token:").color(CyberColors::TEXT_SECONDARY));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.ai_api_key_input)
                                    .password(true)
                                    .hint_text("ghp_...")
                                    .desired_width(300.0),
                            );
                        });
                        ui.add_space(10.0);
                        if ui
                            .button(RichText::new("💾 Set Token").color(CyberColors::CYAN))
                            .clicked()
                        {
                            self.set_api_key("GITHUB_TOKEN");
                        }
                        ui.add_space(5.0);
                        if ui
                            .link(
                                RichText::new("Create a GitHub token →")
                                    .color(CyberColors::TEXT_MUTED)
                                    .small(),
                            )
                            .clicked()
                        {
                            let _ = open::that("https://github.com/settings/tokens");
                        }
                    }
                    AiBackendSelection::LmStudio => {
                        ui.label(
                            RichText::new("LM Studio - Local AI")
                                .color(CyberColors::TEXT_PRIMARY)
                                .size(14.0),
                        );
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new(
                                "Run local models with LM Studio's OpenAI-compatible API.",
                            )
                            .color(CyberColors::TEXT_SECONDARY),
                        );
                        ui.add_space(15.0);

                        ui.label(
                            RichText::new("1. Download and install LM Studio")
                                .color(CyberColors::TEXT_SECONDARY),
                        );
                        ui.label(
                            RichText::new("2. Download a model (e.g., Llama 3.2, Mistral)")
                                .color(CyberColors::TEXT_SECONDARY),
                        );
                        ui.label(
                            RichText::new("3. Start the local server on port 1234")
                                .color(CyberColors::TEXT_SECONDARY),
                        );
                        ui.add_space(10.0);

                        if ui
                            .button(RichText::new("📥 Download LM Studio").color(CyberColors::CYAN))
                            .clicked()
                        {
                            let _ = open::that("https://lmstudio.ai/");
                        }
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new(
                                "After starting the server, click 'Retry Connection' above.",
                            )
                            .color(CyberColors::TEXT_MUTED)
                            .small(),
                        );
                    }
                    // The remaining providers need no walkthrough beyond where they
                    // listen, or what to install.
                    other => {
                        ui.label(
                            RichText::new(other.display_name())
                                .color(CyberColors::TEXT_PRIMARY)
                                .size(14.0),
                        );
                        ui.add_space(5.0);
                        let detail = match other {
                            AiBackendSelection::IronWorks => {
                                "ironmon's built-in engine. Serve a model on \
                                 http://localhost:8080 and it is used automatically. \
                                 Inference stays on this machine."
                            }
                            AiBackendSelection::Vllm => {
                                "Start vLLM with an OpenAI-compatible server on \
                                 http://localhost:8000."
                            }
                            AiBackendSelection::TensorRt => {
                                "Start TensorRT-LLM / Triton on http://localhost:8001."
                            }
                            AiBackendSelection::Cli(_) => {
                                "Driven as a subprocess. Install the tool, sign in with \
                                 it once, and IronMonitor will call it — the tool chooses its \
                                 own model, so the model list stays empty."
                            }
                            _ => "",
                        };
                        if !detail.is_empty() {
                            ui.label(RichText::new(detail).color(CyberColors::TEXT_SECONDARY));
                        }
                    }
                }
            });
    }

    /// Start Ollama in the background
    #[allow(dead_code)]
    fn start_ollama(&mut self) {
        self.ai_ollama_starting = true;
        self.ai_status_message = Some(("Starting Ollama...".to_string(), false));

        // Try to start Ollama
        #[cfg(target_os = "windows")]
        {
            // On Windows, try to start Ollama from common locations
            let ollama_paths = [
                std::env::var("LOCALAPPDATA")
                    .ok()
                    .map(|p| format!("{}\\Ollama\\ollama.exe", p)),
                Some("C:\\Program Files\\Ollama\\ollama.exe".to_string()),
                Some("ollama".to_string()), // Try PATH
            ];

            for path in ollama_paths.into_iter().flatten() {
                if std::process::Command::new(&path)
                    .arg("serve")
                    .spawn()
                    .is_ok()
                {
                    self.ai_status_message = Some((
                        "Ollama started! Wait a few seconds and click 'Retry Connection'."
                            .to_string(),
                        false,
                    ));
                    self.ai_ollama_starting = false;
                    return;
                }
            }
            self.ai_status_message = Some((
                "Could not start Ollama. Please install it from ollama.com".to_string(),
                true,
            ));
        }

        #[cfg(not(target_os = "windows"))]
        {
            if std::process::Command::new("ollama")
                .arg("serve")
                .spawn()
                .is_ok()
            {
                self.ai_status_message = Some((
                    "Ollama started! Wait a few seconds and click 'Retry Connection'.".to_string(),
                    false,
                ));
            } else {
                self.ai_status_message = Some((
                    "Could not start Ollama. Please install it from ollama.com".to_string(),
                    true,
                ));
            }
        }

        self.ai_ollama_starting = false;
    }

    /// Set an API key as environment variable and retry connection
    #[allow(dead_code)]
    fn set_api_key(&mut self, env_var: &str) {
        let key = self.ai_api_key_input.trim();
        if key.is_empty() {
            self.ai_status_message = Some(("Please enter an API key".to_string(), true));
            return;
        }

        // Set environment variable for this process
        std::env::set_var(env_var, key);
        self.ai_api_key_input.clear();
        // The listing this provider gave before the key existed was an
        // unauthenticated one — almost certainly empty. Drop it so the loader asks
        // again, now that there is something to authenticate with; otherwise the
        // dropdown keeps showing the hardcoded guess for the rest of the session.
        self.models_by_provider.remove(&self.ai_selected_backend);
        self.ai_status_message = Some((format!("{} set! Retrying connection...", env_var), false));

        // Retry connection
        self.retry_agent_connection();
    }

    /// Build a BackendConfig from the current GUI backend/model selection
    fn build_backend_config(&self) -> crate::agent::BackendConfig {
        use crate::agent::BackendConfig;

        // Prefer the explicit choice; otherwise the first model the provider itself
        // reported; otherwise a known name for that provider. Nothing is invented for
        // a local server, because a made-up name there produces a request for a model
        // that is not loaded.
        let model = if !self.ai_selected_model.is_empty() {
            self.ai_selected_model.clone()
        } else {
            self.models_by_provider
                .get(&self.ai_selected_backend)
                .and_then(|m| m.first().cloned())
                .or_else(|| {
                    self.ai_selected_backend
                        .fallback_models()
                        .first()
                        .map(|s| s.to_string())
                })
                .unwrap_or_default()
        };

        let api_key = if self.ai_api_key_input.trim().is_empty() {
            None
        } else {
            Some(self.ai_api_key_input.trim().to_string())
        };

        match self.ai_selected_backend {
            AiBackendSelection::IronWorks => BackendConfig::ironworks(&model),
            AiBackendSelection::Ollama => BackendConfig::ollama(&model),
            AiBackendSelection::LmStudio => BackendConfig::lm_studio(&model),
            AiBackendSelection::Vllm => BackendConfig::vllm(&model),
            AiBackendSelection::TensorRt => BackendConfig::tensorrt(&model),
            AiBackendSelection::OpenAi => BackendConfig::openai(&model, api_key),
            AiBackendSelection::Anthropic => BackendConfig::anthropic(&model, api_key),
            AiBackendSelection::GitHub => BackendConfig::github_models(&model, api_key),
            AiBackendSelection::Cli(provider) => BackendConfig::cli(provider, &model),
        }
    }

    /// The credential to authenticate a listing request with, if there is one.
    ///
    /// The field the user just typed into wins, because that is the key they are
    /// currently trying; otherwise the process environment, which is where a key
    /// exported before launch lives. Reading only the field meant a user with
    /// `ANTHROPIC_API_KEY` already set got an unauthenticated request, a 401, an
    /// empty list, and the stale hardcoded names — with nothing on screen saying
    /// the provider had never actually been asked.
    fn resolve_api_key(&self, provider: AiBackendSelection) -> String {
        let typed = self.ai_api_key_input.trim();
        if !typed.is_empty() {
            return typed.to_string();
        }
        provider
            .api_key_env_var()
            .and_then(|var| std::env::var(var).ok())
            .map(|key| key.trim().to_string())
            .unwrap_or_default()
    }

    /// Ask a provider what models it serves, on a background thread.
    ///
    /// Every provider that exposes a listing is read live, so the dropdown shows what
    /// is actually loadable rather than a list baked in when the file was written.
    /// Ollama was previously enumerated by spawning `ollama list` and parsing its
    /// table; this uses the HTTP API instead, which needs no subprocess.
    ///
    /// Runs entirely off the UI thread — the request has a timeout and hosted
    /// providers are a round trip away.
    fn fetch_models_async(&mut self, provider: AiBackendSelection, ctx: egui::Context) {
        let Some(endpoint) = provider.models_endpoint() else {
            // Nothing to enumerate; record that so the UI stops saying "loading".
            self.models_by_provider.insert(provider, Vec::new());
            return;
        };

        let api_key = self.resolve_api_key(provider);
        let (tx, rx) = channel();
        self.models_receiver = Some(rx);
        self.models_loading_for = Some(provider);

        std::thread::spawn(move || {
            let models = Self::fetch_models_blocking(provider, endpoint, &api_key);
            let _ = tx.send((provider, models));
            ctx.request_repaint();
        });
    }

    /// Perform the model listing request. Runs on a worker thread.
    fn fetch_models_blocking(
        provider: AiBackendSelection,
        endpoint: &str,
        api_key: &str,
    ) -> Vec<String> {
        let Ok(client) = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(2))
            .build()
        else {
            return Vec::new();
        };

        let mut request = client.get(endpoint);
        if !api_key.is_empty() {
            request = match provider {
                // Anthropic authenticates with `x-api-key` and requires a version.
                AiBackendSelection::Anthropic => request
                    .header("x-api-key", api_key)
                    .header("anthropic-version", "2023-06-01"),
                _ => request.bearer_auth(api_key),
            };
        }

        let Ok(response) = request.send() else {
            return Vec::new();
        };
        if !response.status().is_success() {
            return Vec::new();
        }
        let Ok(json) = response.json::<serde_json::Value>() else {
            return Vec::new();
        };

        // Ollama answers `{"models":[{"name":...}]}`; everything else here speaks the
        // OpenAI shape, `{"data":[{"id":...}]}`.
        let mut names: Vec<String> = json
            .get("models")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
                    .map(str::to_string)
                    .collect()
            })
            .or_else(|| {
                json.get("data").and_then(|d| d.as_array()).map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            m.get("id")
                                .or_else(|| m.get("name"))
                                .and_then(|n| n.as_str())
                        })
                        .map(str::to_string)
                        .collect()
                })
            })
            .unwrap_or_default();

        names.sort();
        names.dedup();
        names
    }

    /// Run backend auto-detection on a background thread.
    fn spawn_agent_detection(&mut self) {
        let (tx, rx) = channel();
        self.agent_receiver = Some(rx);
        self.agent_loading = true;
        self.agent_loading_start = Instant::now();
        std::thread::spawn(move || {
            let agent = crate::agent::AgentConfig::auto_detect()
                .ok()
                .and_then(|config| crate::agent::Agent::new(config).ok());
            let _ = tx.send(agent);
        });
    }

    /// Retry agent connection with current configuration
    fn retry_agent_connection(&mut self) {
        self.agent_loading = true;
        self.ai_status_message = None;

        let backend_config = self.build_backend_config();
        let (tx, rx) = channel();
        self.agent_receiver = Some(rx);

        std::thread::spawn(move || {
            let config = crate::agent::AgentConfig::with_backend(backend_config);
            let agent = crate::agent::Agent::new(config).ok();
            let _ = tx.send(agent);
        });
    }

    /// Draw the settings window
    fn draw_settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }

        egui::Window::new("⚙ Settings")
            .collapsible(true)
            .resizable(true)
            .default_width(350.0)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 50.0))
            .show(ctx, |ui| {
                ui.add_space(8.0);

                // Close button in corner
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("Appearance").color(CyberColors::CYAN));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✕").clicked() {
                            self.show_settings = false;
                        }
                    });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Color Theme Selection
                ui.label(RichText::new("Color Theme").color(CyberColors::TEXT_PRIMARY));
                ui.add_space(4.0);

                egui::ComboBox::from_id_salt("theme_selector")
                    .selected_text(self.settings.color_theme.name())
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for theme in ColorTheme::all() {
                            let is_selected = self.settings.color_theme == *theme;
                            ui.horizontal(|ui| {
                                // Color preview swatch
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(16.0, 16.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 2.0, theme.accent_color());

                                if ui.selectable_label(is_selected, theme.name()).clicked() {
                                    self.settings.color_theme = *theme;
                                }
                            });
                        }
                    });

                ui.add_space(16.0);

                // Graph Line Thickness
                ui.label(RichText::new("Graph Line Thickness").color(CyberColors::TEXT_PRIMARY));
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut self.settings.graph_line_thickness, 1.0..=5.0)
                            .step_by(0.5)
                            .suffix(" px"),
                    );
                });

                // Preview line
                ui.add_space(8.0);
                let preview_rect = ui.available_rect_before_wrap();
                let preview_height = 30.0;
                let preview_width = preview_rect.width().min(200.0);
                let (response, painter) = ui.allocate_painter(
                    egui::vec2(preview_width, preview_height),
                    egui::Sense::hover(),
                );
                let rect = response.rect;

                // Draw preview background
                painter.rect_filled(rect, 4.0, CyberColors::SURFACE);

                // Draw sample sine wave with current thickness
                let points: Vec<egui::Pos2> = (0..50)
                    .map(|i| {
                        let t = i as f32 / 49.0;
                        let x = rect.left() + t * rect.width();
                        let y = rect.center().y + (t * 6.0 * std::f32::consts::PI).sin() * 10.0;
                        egui::Pos2::new(x, y)
                    })
                    .collect();

                painter.add(egui::Shape::line(
                    points,
                    egui::Stroke::new(
                        self.settings.graph_line_thickness,
                        self.settings.color_theme.accent_color(),
                    ),
                ));

                ui.add_space(16.0);

                // Show Grid Lines
                ui.checkbox(
                    &mut self.settings.show_grid_lines,
                    RichText::new("Show Grid Lines").color(CyberColors::TEXT_PRIMARY),
                );

                ui.add_space(8.0);

                // Animation Speed
                ui.label(RichText::new("Animation Speed").color(CyberColors::TEXT_PRIMARY));
                ui.add_space(4.0);
                ui.add(
                    egui::Slider::new(&mut self.settings.animation_speed, 0.5..=2.0)
                        .step_by(0.1)
                        .suffix("x"),
                );

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                // Reset to defaults button
                ui.horizontal(|ui| {
                    if ui.button("Reset to Defaults").clicked() {
                        self.settings = AppSettings::default();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            self.show_settings = false;
                        }
                    });
                });

                ui.add_space(8.0);
            });
    }
}

/// Format bytes as human-readable string (B, KB, MB, GB)
fn format_bytes(bytes: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if bytes >= GB {
        format!("{:.2} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes / KB)
    } else {
        format!("{:.0} B", bytes)
    }
}

#[cfg(test)]
mod model_listing_tests {
    use super::AiBackendSelection;

    /// The stale-model bug in one assertion.
    ///
    /// A hardcoded fallback list is only ever reached when the live listing failed,
    /// and for a hosted provider the overwhelmingly common cause is that IronMonitor had
    /// no credential to authenticate the listing request with. If such a provider
    /// has nowhere to look for a key, its dropdown can *never* show live names — it
    /// is permanently pinned to whatever was true the day the list was written.
    /// That is exactly how this tab came to offer `gpt-3.5-turbo`.
    #[test]
    fn a_provider_with_hardcoded_models_can_find_a_credential_to_replace_them() {
        for provider in AiBackendSelection::ALL {
            if provider.fallback_models().is_empty() {
                continue;
            }
            assert!(
                provider.api_key_env_var().is_some(),
                "{:?} ships hardcoded model names but has no environment variable to \
                 read a key from, so its listing request is always unauthenticated \
                 and those names can never be replaced by live ones",
                provider
            );
        }
    }

    /// A key is only useful if there is somewhere to spend it.
    #[test]
    fn every_provider_with_a_credential_has_a_listing_to_authenticate() {
        for provider in AiBackendSelection::ALL {
            if provider.api_key_env_var().is_some() {
                assert!(
                    provider.models_endpoint().is_some(),
                    "{:?} reads an API key but exposes no listing endpoint",
                    provider
                );
            }
        }
    }

    /// Guessing a name for a local server produces a request for a model that was
    /// never loaded — a confusing failure the user cannot act on. Only providers
    /// serving a fixed hosted catalogue may guess.
    #[test]
    fn local_providers_never_guess_a_model_name() {
        for provider in AiBackendSelection::ALL {
            let is_local = provider
                .models_endpoint()
                .is_some_and(|url| url.contains("localhost"));
            if is_local {
                assert!(
                    provider.fallback_models().is_empty(),
                    "{:?} guesses a model name, but a local server serves only what \
                     was loaded into it",
                    provider
                );
            }
        }
    }
}
