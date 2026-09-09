//! CLI tool for IronMonitor (ironmon)

#[cfg(feature = "cli")]
use clap::{CommandFactory, Parser, Subcommand};
#[cfg(feature = "cli")]
use colored::Colorize;
use std::path::PathBuf;
use std::time::Duration;

#[cfg(feature = "cli")]
#[derive(Parser)]
#[command(name = "ironmon")]
#[command(about = "IronMonitor: Comprehensive hardware monitoring for CPUs, GPUs, NPUs, memory, I/O, and network silicon", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Deny every data-collection category for this session. IronMonitor collects and
    /// transmits nothing today, so this records a denial rather than stopping
    /// anything; see `ironmon privacy status`
    #[arg(long, global = true)]
    no_telemetry: bool,

    /// Keep this machine's details on it: refuse AI backends that would relay
    /// the question to a vendor, and deny every data-collection category.
    /// Backends served locally still work
    #[arg(long, global = true)]
    offline: bool,
}

#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum Commands {
    /// Launch Graphical User Interface (GUI) - desktop application
    #[cfg(feature = "gui")]
    Gui {
        /// Render one tab's text content to stdout and exit, instead of opening a
        /// window
        ///
        /// The GUI is the surface an agent otherwise cannot see at all: it draws
        /// into a window, and a screenshot cannot distinguish text that was never
        /// drawn from text drawn in an unreadable colour. This renders the same
        /// widget tree headlessly and prints what was painted.
        #[arg(long)]
        frame: bool,

        /// Tab to render with --frame (overview, cpu, memory, disk, processes,
        /// network, system, peripherals, profiles, ai, ...)
        #[arg(long, default_value = "overview")]
        tab: String,

        /// Inspect the GUI from a script and report the result; `-` reads stdin
        ///
        /// Steps: `goto <tab>`, `capture`, `assert <text>`, `refute <text>`. There is
        /// no `key` step: GUI tabs are addressable by name, so unlike the TUI there
        /// is no navigation state to drive. Exits 1 if any assertion fails, 2 if the
        /// script does not parse.
        #[arg(long)]
        script: Option<String>,
    },
    /// Launch Terminal User Interface (TUI) - interactive dashboard
    Tui {
        /// Render one frame to stdout and exit, instead of taking over the terminal
        ///
        /// The TUI normally draws into an alternate screen under raw mode, which an
        /// agent with no TTY cannot read. This renders the same frame into a buffer
        /// and prints it, so what a human would see is inspectable and diffable.
        #[arg(long)]
        frame: bool,

        /// Tab to render with --frame, by name (case-insensitive) or index
        #[arg(long)]
        tab: Option<String>,

        /// Frame width in columns for --frame
        #[arg(long, default_value = "200")]
        width: u16,

        /// Frame height in rows for --frame
        #[arg(long, default_value = "50")]
        height: u16,

        /// Drive the TUI from a script and report the result; `-` reads stdin
        ///
        /// Steps: `goto <tab>`, `key <name>`, `refresh`, `capture`, `assert <text>`,
        /// `refute <text>`. Key steps go through the same handler the interactive
        /// loop uses, so an assertion is evidence about the real TUI. Exits 1 if any
        /// assertion fails.
        #[arg(long)]
        script: Option<String>,
    },
    /// Command-line interface for monitoring hardware
    Cli {
        #[command(subcommand)]
        action: CliSubcommand,

        /// Update interval in seconds
        #[arg(short, long, default_value = "1.0", global = true)]
        interval: f64,

        /// Output format (json or text)
        #[arg(short, long, default_value = "text", global = true)]
        format: String,

        /// Watch mode: continuously refresh output (press 'q' to quit)
        #[arg(short, long, global = true)]
        watch: bool,
    },
    /// AI agent features: query system, export manifests, start MCP server
    Ai {
        #[command(subcommand)]
        action: AiSubcommand,
    },
    /// Record system metrics to a time-series database for analysis
    Record {
        #[command(subcommand)]
        action: RecordSubcommand,
    },
    /// Manage privacy settings and data collection consent
    Privacy {
        #[command(subcommand)]
        action: PrivacySubcommand,
    },
    /// Inspect hardware profiles and tunable driver settings (NVPI/XTU/Ryzen Master/nvme-cli style)
    Profile {
        #[command(subcommand)]
        action: ProfileSubcommand,
    },
    /// Detect what the machine is being used for and recommend a hardware profile
    ///
    /// Reports the use case — AI training, AI inference, gaming, interactive,
    /// idle — with the evidence behind it, then the settings that would suit it.
    ///
    /// **Recommends by default and writes nothing.** Every proposed value comes
    /// from what the driver itself declared: an entry in the setting's own choice
    /// list, or its reported default. Applying requires both `--apply` and
    /// `--confirm`, goes through the audited apply layer, and is capped below the
    /// risk tier that covers power, thermal and voltage writes.
    Tune {
        /// Re-evaluate every N seconds instead of once — the automatic server.
        /// Still recommend-only unless `--apply` is also given.
        #[arg(long, value_name = "SECONDS")]
        watch: Option<u64>,

        /// Apply the recommendations rather than only printing them. Requires
        /// `--confirm`; without it every write is refused by the apply layer.
        #[arg(long)]
        apply: bool,

        /// Explicit confirmation for writes, as `ironmon profile set` requires.
        #[arg(long)]
        confirm: bool,

        /// Highest risk tier to apply unattended: safe or moderate. `dangerous`
        /// is not accepted — those settings can destabilize hardware and no
        /// unattended loop in ironmon writes one.
        #[arg(long, value_name = "TIER", default_value = "safe")]
        max_risk: String,

        /// Assume this use case instead of detecting one — for testing a profile
        /// without waiting for the workload
        #[arg(long, value_name = "CASE")]
        r#as: Option<String>,

        /// Output format (json or text)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Print the entity ontology: every value IronMonitor can report, with its unit and
    /// provenance
    ///
    /// The schema an agent reads before querying anything. Pure — touches no
    /// hardware, so the output is identical on every machine and can be fetched
    /// ahead of time. `--format json` is the machine-facing form.
    Describe {
        /// Emit the command surface instead of the entity surface: every
        /// subcommand, its arguments and its help, generated from the parser so it
        /// cannot drift from what the binary actually accepts
        #[arg(long)]
        commands: bool,

        /// List only settings that can be written, with the `ironmon profile set`
        /// id that writes each
        #[arg(long)]
        writable: bool,

        /// Restrict to one domain (cpu, gpu, memory, disk, network, power, thermal,
        /// process, system, board)
        #[arg(long)]
        domain: Option<String>,

        /// Substring match against ids and descriptions
        #[arg(long)]
        search: Option<String>,

        /// Look up a single entity by exact id
        #[arg(long)]
        id: Option<String>,

        /// Output format (json or text)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Read one ontology entity by id
    ///
    /// `ironmon get gpu.0.thermal.temperature`. Exits 1 when the id is unknown and 2
    /// when it is known but unavailable here, so a caller can tell "no such thing"
    /// from "nothing to report".
    Get {
        /// Entity id, e.g. `memory.total` or `gpu.0.power.draw`
        id: String,

        /// Output format (json or text)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// One-screen summary of this machine
    ///
    /// Reads through the same resolver as `snapshot`, so the two cannot disagree
    /// about one machine. Anything unreadable prints the reason rather than a
    /// dash: "no GPU detected" and "GPU enumeration failed" call for different
    /// responses, and this is the surface people paste into issues.
    Status {
        /// Draw the OS logo in ASCII, neofetch-style
        #[arg(long)]
        ascii: bool,

        /// Never colourise, even on a terminal
        #[arg(long)]
        no_color: bool,

        /// Output format (text or json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Intrusion detection over listening sockets and watched files
    ///
    /// Observes and reports. It never blocks a connection, kills a process or
    /// quarantines a file.
    ///
    /// A first run records a baseline and reports `no_baseline`, never `clean`:
    /// a baseline taken after a compromise records the compromise. The baseline
    /// path is explicit so there is no hidden state deciding what "changed"
    /// means.
    Ids {
        /// Where the baseline lives. Recorded if absent, compared against if
        /// present.
        #[arg(long)]
        baseline: std::path::PathBuf,

        /// Files to watch, in addition to the listening sockets always checked.
        #[arg(long)]
        watch: Vec<std::path::PathBuf>,

        /// Output format (json or text)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Read every resolvable entity at once, with provenance
    ///
    /// The bulk form of `get`. Entities IronMonitor cannot read here are still listed,
    /// each with the reason, so an absent device stays distinguishable from an
    /// unimplemented reader.
    Snapshot {
        /// Restrict to one domain
        #[arg(long)]
        domain: Option<String>,

        /// Omit entities that could not be read
        #[arg(long)]
        resolved_only: bool,

        /// Check every reading against the ontology's declared ranges; exit
        /// non-zero if any is physically impossible
        #[arg(long)]
        validate: bool,

        /// Output format: text, json, or json-ld
        ///
        /// `json-ld` emits a linked-data document whose `@context` resolves
        /// ironmon's terms to IRIs and its units to QUDT, so an agent that has
        /// never seen IronMonitor can interpret the graph without documentation.
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Run as a headless daemon configured from a TOML file
    ///
    /// Like `serve`, but reads host/port/interval from config and writes a PID
    /// file. Refuses to bind a routable address without an api_key.
    Daemon {
        /// Path to the TOML configuration file
        #[arg(short, long)]
        config: Option<String>,
        /// Print a sample configuration file and exit
        #[arg(long)]
        sample_config: bool,
    },
    /// Serve the REST API and Prometheus metrics over HTTP
    ///
    /// Prometheus metrics are at /api/v1/metrics/prometheus. Prometheus's
    /// default /metrics is not served at all, so scrape_configs must set
    /// metrics_path explicitly; grafana/README.md has the stanza.
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value_t = 9100)]
        port: u16,
        /// Address to bind. Defaults to loopback; set 0.0.0.0 to expose on the
        /// network, which also exposes full hardware telemetry to it.
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        /// Log each request to stderr
        #[arg(long)]
        log_requests: bool,
    },
}

/// Profile subcommands for hardware setting inspection
#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum ProfileSubcommand {
    /// List subsystems with profile data
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show all settings for a subsystem (gpu, cpu, nvme, display, memory)
    Show {
        /// Subsystem name
        subsystem: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Search settings across all subsystems
    Search {
        /// Case-insensitive search term
        query: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Dump the complete profile snapshot
    Dump {
        /// Output as JSON (default: text)
        #[arg(long)]
        json: bool,
        /// Write to file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Write a setting value (requires --confirm). Only ids with a registered handler are writable.
    Set {
        /// Exact setting id (e.g. "persistence_mode", "scaling_governor")
        setting_id: String,
        /// New value. Bool: true/false/on/off; numbers parsed as int/float; otherwise treated as text.
        value: String,
        /// Required to actually perform the write.
        #[arg(long)]
        confirm: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// List available Windows power schemes (GUID + friendly name)
    Schemes {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// List all writable setting ids on this build
    Writable {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Benchmark each provider — wall-clock time and throughput
    Bench {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Watch for changes: snapshot at --interval and print added/removed/changed entries
    Watch {
        /// Refresh interval in seconds
        #[arg(short, long, default_value = "5")]
        interval: f64,
        /// Maximum number of ticks before exit (0 = run until Ctrl-C)
        #[arg(short, long, default_value = "0")]
        ticks: u32,
        /// Restrict watching to one subsystem (gpu, cpu, nvme, display, memory)
        #[arg(short, long)]
        subsystem: Option<String>,
    },
    /// Tail the apply audit log
    Audit {
        /// Number of trailing lines to show
        #[arg(short, long, default_value = "20")]
        lines: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// List settings whose current value differs from their declared default
    Deviations {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show full metadata for a single setting by id
    Explain {
        /// Exact setting id (e.g. "power_limit_mw", "pl1_w", "feat.write_cache.enabled")
        setting_id: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// List running processes and whether each has a known driver profile
    Active {
        /// Show only processes with a known profile
        #[arg(short, long)]
        matched: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Compare a saved JSON snapshot against the current state
    Diff {
        /// Path to baseline snapshot JSON (created via `ironmon profile dump --json -o file.json`)
        baseline: PathBuf,
        /// Compare against another saved snapshot instead of current state
        #[arg(short, long)]
        against: Option<PathBuf>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

/// AI subcommands for agent integration
#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum AiSubcommand {
    /// Ask AI agent about system state (interactive if no query provided)
    Query {
        /// Question to ask the AI agent
        question: Option<String>,
    },
    /// Export tool manifests for AI agents (OpenAI, Claude, Gemini, etc.)
    Manifest {
        /// Output format: openai, anthropic, gemini, grok, llama, mistral, deepseek, jsonld, mcp, or json
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Start MCP (Model Context Protocol) server for Claude Desktop integration
    Server,
    /// List models held in the local IronVault vault (read-only; never unlocks it)
    #[cfg(feature = "vault")]
    Models {
        /// Output format: table or json
        #[arg(short, long, default_value = "table")]
        format: String,
    },
}

/// CLI subcommands for hardware monitoring
#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum CliSubcommand {
    /// Show board information
    Board,
    /// Monitor GPU statistics
    Gpu,
    /// Monitor CPU statistics
    Cpu,
    /// Monitor memory statistics
    Memory,
    /// Monitor power statistics
    Power,
    /// Monitor temperature statistics
    Temperature,
    /// Monitor processes with smart categorization
    Processes,
    /// Monitor Tegra/Jetson engine clocks (APE, DLA, NVENC). Linux only —
    /// reads /sys/kernel/debug/clk, which does not exist on other platforms.
    /// For desktop GPU encoder/decoder utilization, use `gpu`.
    Engines,
    /// Monitor audio devices
    Audio,
    /// Monitor Bluetooth devices
    Bluetooth,
    /// Monitor displays
    Display,
    /// List USB devices
    Usb,
    /// Show all statistics
    All,
    /// Interactive real-time monitoring mode
    Monitor,
    /// Ask AI agent about system state
    Ai {
        /// Question to ask the AI agent (if not provided, enters interactive mode)
        query: Option<String>,
    },
    /// NVIDIA Jetson-specific utilities (requires Linux + Jetson hardware)
    Jetson {
        #[command(subcommand)]
        action: JetsonSubcommand,
    },
}

/// Jetson-specific subcommands (Linux + NVIDIA Jetson hardware only)
#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum JetsonSubcommand {
    /// Jetson Clocks - Maximize performance by locking clocks at max frequency
    Clocks {
        #[command(subcommand)]
        action: JetsonClocksAction,
    },
    /// NVPModel - Power mode management (MAXN, 15W, 30W, etc.)
    Powermode {
        #[command(subcommand)]
        action: NvpmodelAction,
    },
    /// Swap file management (create, enable, disable, remove)
    Swap {
        #[command(subcommand)]
        action: SwapAction,
    },
}

#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum JetsonClocksAction {
    /// Enable jetson_clocks (maximize performance)
    Enable,
    /// Disable jetson_clocks (restore settings)
    Disable,
    /// Show jetson_clocks status
    Status,
    /// Store current configuration
    Store,
}

#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum NvpmodelAction {
    /// Show current power mode
    Show,
    /// List all available power modes
    List,
    /// Set power mode by ID
    Set {
        /// Mode ID
        mode_id: u32,
        /// Force mode change
        #[arg(short, long)]
        force: bool,
    },
    /// Set power mode by name
    SetName {
        /// Mode name
        name: String,
        /// Force mode change
        #[arg(short, long)]
        force: bool,
    },
}

#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum SwapAction {
    /// Show current swap status
    Status,
    /// Create a new swap file
    Create {
        /// Swap file path
        #[arg(short, long, default_value = "/swapfile")]
        path: PathBuf,
        /// Size in GB
        #[arg(short, long, default_value = "8")]
        size: u32,
        /// Enable on boot
        #[arg(short, long)]
        auto: bool,
    },
    /// Enable swap file
    Enable {
        /// Swap file path
        path: PathBuf,
    },
    /// Disable swap file
    Disable {
        /// Swap file path
        path: PathBuf,
    },
    /// Remove swap file
    Remove {
        /// Swap file path
        path: PathBuf,
    },
}

/// Record subcommands for time-series database operations
#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum RecordSubcommand {
    /// Start recording system metrics to the database
    Start {
        /// Database file path
        #[arg(short, long, default_value = "ironmon_metrics.db")]
        database: PathBuf,

        /// Maximum database size (e.g., 100MB, 1GB)
        #[arg(long, default_value = "100MB")]
        max_size: String,

        /// Recording interval in seconds
        #[arg(short, long, default_value = "1.0")]
        interval: f64,

        /// Maximum number of processes to record per snapshot
        #[arg(long, default_value = "50")]
        max_processes: usize,

        /// Output format (json or text)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Show database statistics and information
    Info {
        /// Database file path
        #[arg(short, long, default_value = "ironmon_metrics.db")]
        database: PathBuf,
    },
    /// Query recorded metrics
    Query {
        /// Database file path
        #[arg(short, long, default_value = "ironmon_metrics.db")]
        database: PathBuf,

        /// Start time (minutes ago, or Unix timestamp)
        #[arg(long)]
        start: Option<String>,

        /// End time (minutes ago, or Unix timestamp)
        #[arg(long)]
        end: Option<String>,

        /// Output format (json or text)
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Limit number of results
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// Export recorded data to JSON or CSV
    Export {
        /// Database file path
        #[arg(short, long, default_value = "ironmon_metrics.db")]
        database: PathBuf,

        /// Output file path
        #[arg(short, long)]
        output: PathBuf,

        /// Export format (json or csv)
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Delete the database file
    Clear {
        /// Database file path
        #[arg(short, long, default_value = "ironmon_metrics.db")]
        database: PathBuf,

        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}

/// Privacy subcommands for managing data collection consent
#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum PrivacySubcommand {
    /// Show current consent and data collection status
    Status,
    /// Opt out of all remote data collection (revoke all consents)
    OptOut,
    /// Review and change consent settings interactively
    Review,
    /// Opt in to all data collection categories
    OptIn,
    /// Show what data would be collected for each consent category
    Info,
}

#[cfg(feature = "cli")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    env_logger::init();

    // Handle global privacy flags early
    if cli.no_telemetry || cli.offline {
        // Set environment variable to signal all components to disable telemetry
        std::env::set_var("IRONMON_NO_TELEMETRY", "1");
        if cli.offline {
            std::env::set_var("IRONMON_OFFLINE", "1");
        }
        if std::env::var("IRONMON_VERBOSE").is_ok() {
            eprintln!(
                "{} {}",
                "[INFO]".cyan(),
                if cli.offline {
                    "Running in offline mode - all network features and data collection disabled"
                } else {
                    "Telemetry disabled for this session"
                }
            );
        }
    }

    match &cli.command {
        // GUI command - Graphical User Interface (default if no command given)
        #[cfg(feature = "gui")]
        Some(Commands::Gui { frame, tab, script }) => {
            if let Some(source) = script {
                handle_gui_script_command(source)?;
            } else if *frame {
                handle_gui_frame_command(tab)?;
            } else {
                ironmonlib::gui::run().map_err(|e| format!("GUI error: {}", e))?;
            }
        }
        #[cfg(feature = "gui")]
        None => {
            ironmonlib::gui::run().map_err(|e| format!("GUI error: {}", e))?;
        }

        // TUI command - Terminal User Interface
        Some(Commands::Tui {
            frame,
            tab,
            width,
            height,
            script,
        }) => {
            if let Some(source) = script {
                handle_tui_script_command(source, *width, *height)?;
            } else if *frame {
                handle_tui_frame_command(tab.as_deref(), *width, *height)?;
            } else {
                ironmonlib::tui::run()?;
            }
        }

        Some(Commands::Tune {
            watch,
            apply,
            confirm,
            max_risk,
            r#as,
            format,
        }) => {
            handle_tune_command(*watch, *apply, *confirm, max_risk, r#as.as_deref(), format)?;
        }

        // CLI commands - use shared MonitoringBackend
        Some(Commands::Cli {
            action,
            interval,
            format,
            watch,
        }) => {
            handle_cli_command(action, *interval, format, *watch)?;
        }

        // AI subcommands - query, manifest, server
        Some(Commands::Ai { action }) => match action {
            AiSubcommand::Query { question } => {
                handle_ai_command(question.as_deref())?;
            }
            AiSubcommand::Manifest { format, output } => {
                handle_ai_manifest(format, output.as_ref())?;
            }
            AiSubcommand::Server => {
                handle_mcp_server()?;
            }
            #[cfg(feature = "vault")]
            AiSubcommand::Models { format } => {
                handle_vault_models(format)?;
            }
        },

        // Record command - time-series database operations
        Some(Commands::Record { action }) => {
            handle_record_command(action)?;
        }

        // Privacy command - manage data collection consent
        Some(Commands::Privacy { action }) => {
            handle_privacy_command(action)?;
        }

        // Profile command - hardware profile inspector
        Some(Commands::Profile { action }) => {
            handle_profile_command(action)?;
        }
        Some(Commands::Describe {
            commands,
            writable,
            domain,
            search,
            id,
            format,
        }) => {
            handle_describe_command(
                *commands,
                *writable,
                domain.as_deref(),
                search.as_deref(),
                id.as_deref(),
                format,
            )?;
        }
        Some(Commands::Get { id, format }) => {
            handle_get_command(id, format)?;
        }
        Some(Commands::Status {
            ascii,
            no_color,
            format,
        }) => {
            handle_status_command(*ascii, *no_color, format)?;
        }
        Some(Commands::Ids {
            baseline,
            watch,
            format,
        }) => {
            handle_ids_command(baseline, watch, format)?;
        }
        Some(Commands::Snapshot {
            domain,
            resolved_only,
            validate,
            format,
        }) => {
            handle_snapshot_command(domain.as_deref(), *resolved_only, *validate, format)?;
        }
        Some(Commands::Daemon {
            config,
            sample_config,
        }) => {
            handle_daemon_command(config.as_deref(), *sample_config)?;
        }
        Some(Commands::Serve {
            port,
            bind,
            log_requests,
        }) => {
            handle_serve_command(*port, bind, *log_requests)?;
        }

        // Default: launch GUI if available, otherwise TUI
        #[cfg(not(feature = "gui"))]
        None => {
            ironmonlib::tui::run()?;
        }
    }

    Ok(())
}

/// Handle top-level AI command (shortcut for 'cli ai')
#[cfg(feature = "cli")]
fn handle_ai_command(query: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    // Delegate to the same AI handler used by 'cli ai'
    handle_ai_query(query)
}

/// Handle MCP server command
#[cfg(feature = "cli")]
fn handle_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::ai_api::McpServer;

    eprintln!("[*] Starting MCP server on stdio...");
    eprintln!(
        "[*] Protocol version: {}",
        ironmonlib::ai_api::MCP_PROTOCOL_VERSION
    );

    let mut server = McpServer::new()?;
    server.run_stdio()?;
    Ok(())
}

/// Handle AI manifest export command
#[cfg(feature = "cli")]
fn handle_ai_manifest(
    format: &str,
    output: Option<&PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::ai_api::{AgentManifest, ExportFormat};

    let manifest = AgentManifest::new();

    let export_format = match format.to_lowercase().as_str() {
        "openai" | "chatgpt" | "gpt" | "gpt4" | "o1" | "o3" => ExportFormat::OpenAI,
        "anthropic" | "claude" => ExportFormat::Anthropic,
        "gemini" | "google" => ExportFormat::Gemini,
        "grok" | "xai" => ExportFormat::Grok,
        "llama" | "meta" => ExportFormat::Llama,
        "mistral" | "mixtral" => ExportFormat::Mistral,
        "deepseek" | "r1" => ExportFormat::DeepSeek,
        "jsonld" | "json-ld" | "schema" => ExportFormat::JsonLd,
        "mcp" => ExportFormat::Mcp,
        "json" | "simple" => ExportFormat::SimpleJson,
        _ => {
            eprintln!("Unknown format '{}'. Supported: openai, anthropic, gemini, grok, llama, mistral, deepseek, jsonld, mcp, json", format);
            return Err("Invalid format".into());
        }
    };

    let exported = manifest.export(export_format);
    let json_output = serde_json::to_string_pretty(&exported)?;

    if let Some(path) = output {
        std::fs::write(path, &json_output)?;
        eprintln!("[+] Manifest written to: {}", path.display());
    } else {
        println!("{}", json_output);
    }

    Ok(())
}

/// Handle CLI subcommands using the shared MonitoringBackend
#[cfg(feature = "cli")]
fn handle_cli_command(
    action: &CliSubcommand,
    interval: f64,
    format: &str,
    watch: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::backend::{BackendConfig, MonitoringBackend};
    use ironmonlib::IronMonitor;

    // Create backend config with the specified interval
    let config = BackendConfig {
        update_interval: Duration::from_secs_f64(interval),
        ..Default::default()
    };

    // Helper closure to run in watch mode
    let run_watch_mode = |display_fn: &dyn Fn() -> Result<(), Box<dyn std::error::Error>>| -> Result<(), Box<dyn std::error::Error>> {
        use crossterm::{
            event::{self, Event, KeyCode},
            terminal::{disable_raw_mode, enable_raw_mode},
        };

        println!("Watch mode - Press 'q' to quit, refreshing every {:.1}s", interval);
        enable_raw_mode()?;

        loop {
            // Clear screen
            print!("\x1B[2J\x1B[1;1H");

            // Display the data
            if let Err(e) = display_fn() {
                disable_raw_mode()?;
                return Err(e);
            }

            println!("\n[Press 'q' to quit]");

            // Check for quit key with timeout
            if event::poll(Duration::from_secs_f64(interval))? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q') || key.code == KeyCode::Char('Q') {
                        break;
                    }
                }
            }
        }

        disable_raw_mode()?;
        Ok(())
    };
    match action {
        CliSubcommand::Board => {
            let stats = IronMonitor::with_interval(interval)?;
            let board = stats.board_info();
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(board)?);
            } else {
                print_board_info(board);
            }
        }
        CliSubcommand::Gpu => {
            // Use MonitoringBackend for GPU data
            let backend = MonitoringBackend::with_config(config)?;
            if format == "json" {
                let gpu_info: Vec<_> = backend
                    .gpu_static_info()
                    .iter()
                    .zip(backend.gpu_dynamic_info().iter())
                    .map(|(s, d)| {
                        serde_json::json!({
                            "name": s.name,
                            "vendor": s.vendor.to_string(),
                            "utilization": d.utilization,
                            "memory_used": d.memory.used,
                            "memory_total": d.memory.total,
                            "temperature": d.thermal.temperature,
                            "power_draw": d.power.draw,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&gpu_info)?);
            } else {
                print_gpu_info_backend(&backend);
            }
        }
        CliSubcommand::Cpu => {
            let backend = MonitoringBackend::with_config(config)?;
            if format == "json" {
                if let Some(cpu) = backend.cpu_stats() {
                    println!("{}", serde_json::to_string_pretty(cpu)?);
                }
            } else {
                print_cpu_info_backend(&backend);
            }
        }
        CliSubcommand::Memory => {
            let backend = MonitoringBackend::with_config(config)?;
            if format == "json" {
                if let Some(mem) = backend.memory_stats() {
                    println!("{}", serde_json::to_string_pretty(mem)?);
                }
            } else {
                print_memory_info_backend(&backend);
            }
        }
        CliSubcommand::Power => {
            // Same shape as Temperature below: `PowerStats.rails` is hwmon
            // rails, which Windows does not expose, so this printed "No power
            // rails exposed by this platform" while the ontology resolved two
            // GPU power draws, both power limits, the battery percentage and
            // the active power profile. The rails block is kept for the
            // platforms that have them.
            let mut stats = IronMonitor::with_interval(interval)?;
            let snapshot = stats.snapshot()?;
            let readings = ironmonlib::ontology::resolve::snapshot();
            let power = ironmonlib::fetch::power(&readings);

            if format == "json" {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "rails": &snapshot.power,
                        "readings": power,
                    }))?
                );
            } else {
                print_power_info(&snapshot.power, &power);
            }
        }
        CliSubcommand::Temperature => {
            // `IronMonitor::snapshot().temperature` covers CPU and motherboard
            // sensors only — its Windows reader skips GPUs with the comment
            // "GPU temps come from NVML", which is true of the crate and not
            // of this command. On a desktop with no OpenHardwareMonitor
            // installed that map is empty, so `ironmon cli temperature` printed
            // "No temperature sensors detected" while `ironmon snapshot` read
            // two GPUs and three NVMe drives, seconds apart, in the same
            // binary.
            //
            // The ontology already resolves every temperature in the machine
            // with a provenance and, where one is missing, a reason. Read it
            // rather than a subset of it.
            let readings = ironmonlib::ontology::resolve::snapshot();
            let temps = ironmonlib::fetch::temperatures(&readings);

            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&temps)?);
            } else {
                print_temperature_readings(&temps);
            }
        }
        CliSubcommand::Processes => {
            let backend = MonitoringBackend::with_config(config)?;
            if format == "json" {
                let procs = backend.processes();
                println!("{}", serde_json::to_string_pretty(&procs)?);
            } else {
                print_process_info_backend(&backend)?;
            }
        }
        CliSubcommand::Engines => {
            let mut stats = IronMonitor::with_interval(interval)?;
            let snapshot = stats.snapshot()?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&snapshot.engines)?);
            } else {
                print_engine_info(&snapshot.engines);
            }
        }
        CliSubcommand::All => {
            let backend = MonitoringBackend::with_config(config)?;
            if format == "json" {
                let state = backend.get_full_system_state();
                println!("{}", serde_json::to_string_pretty(&state)?);
            } else {
                print_all_info(&backend)?;
            }
        }
        CliSubcommand::Monitor => {
            let stats = IronMonitor::with_interval(interval)?;
            run_interactive_mode(stats)?;
        }
        CliSubcommand::Ai { query } => {
            handle_ai_query(query.as_deref())?;
        }
        CliSubcommand::Audio => {
            use ironmonlib::audio::AudioMonitor;

            let display_audio = || -> Result<(), Box<dyn std::error::Error>> {
                let monitor = AudioMonitor::new()?;
                if format == "json" {
                    println!("{}", serde_json::to_string_pretty(monitor.devices())?);
                } else {
                    println!("{}", "═══ Audio Devices ═══".cyan().bold());
                    // Both of these were constructor defaults that no reader
                    // ever assigns, printed as "100%" and "No" on every
                    // machine. `unwrap_or(100)` made the absence unreachable
                    // even once the field could express it.
                    match monitor.master_volume() {
                        Some(v) => println!("  Master Volume: {v}%"),
                        None => println!(
                            "  Master Volume: not read — IronMonitor has no mixer binding on this platform"
                        ),
                    }
                    match monitor.is_muted() {
                        Some(m) => println!("  Muted: {}", if m { "Yes" } else { "No" }),
                        None => println!("  Muted: not read"),
                    }
                    for device in monitor.devices() {
                        // `{:?}` on an `Option` prints `Some(Active)`, which
                        // shows the wrapper to the user. The state is either
                        // known or it is not.
                        let state = match &device.state {
                            Some(s) => format!("{s:?}"),
                            None => "state not read".to_string(),
                        };
                        println!("  {} ({:?}) - {}", device.name, device.device_type, state);
                    }
                }
                Ok(())
            };

            if watch {
                run_watch_mode(&display_audio)?;
            } else {
                display_audio()?;
            }
        }
        CliSubcommand::Bluetooth => {
            use ironmonlib::bluetooth::BluetoothMonitor;

            let display_bluetooth = || -> Result<(), Box<dyn std::error::Error>> {
                let monitor = BluetoothMonitor::new()?;
                if format == "json" {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "available": monitor.is_available(),
                            "adapters": monitor.adapters(),
                            "devices": monitor.devices(),
                        }))?
                    );
                } else {
                    println!("{}", "═══ Bluetooth ═══".cyan().bold());
                    println!(
                        "  Available: {}",
                        if monitor.is_available() { "Yes" } else { "No" }
                    );
                    println!("  Adapters: {}", monitor.adapters().len());
                    for adapter in monitor.adapters() {
                        println!("    {} ({})", adapter.name, adapter.address);
                    }
                    println!("  Devices: {}", monitor.devices().len());
                    for device in monitor.devices() {
                        println!(
                            "    {} - {:?}",
                            device.name.as_deref().unwrap_or("Unknown"),
                            device.state
                        );
                    }
                }
                Ok(())
            };

            if watch {
                run_watch_mode(&display_bluetooth)?;
            } else {
                display_bluetooth()?;
            }
        }
        CliSubcommand::Display => {
            use ironmonlib::display::DisplayMonitor;

            let display_displays = || -> Result<(), Box<dyn std::error::Error>> {
                let monitor = DisplayMonitor::new()?;
                if format == "json" {
                    println!("{}", serde_json::to_string_pretty(monitor.displays())?);
                } else {
                    println!("{}", "═══ Displays ═══".cyan().bold());
                    println!("  Count: {}", monitor.count());
                    for display in monitor.displays() {
                        // "RX-A740 0x0 @ 0Hz Dvi" was this line. The ontology
                        // already refused those zeros, in a reason that reads
                        // "zero is not a resolution"; the fields carry the
                        // absence now, so this can say the same thing.
                        let mode = match (display.width, display.height) {
                            (Some(w), Some(h)) => format!("{w}x{h}"),
                            _ => "mode not readable".to_string(),
                        };
                        let rate = match display.refresh_rate {
                            Some(hz) => format!(" @ {hz:.0}Hz"),
                            None => String::new(),
                        };
                        let conn = match display.connection {
                            ironmonlib::display::DisplayConnection::Unknown => {
                                "connection unknown".to_string()
                            }
                            other => format!("{other:?}"),
                        };
                        println!(
                            "  {} {}{} {}",
                            display.name.as_deref().unwrap_or("unnamed"),
                            mode,
                            rate,
                            conn
                        );
                    }
                }
                Ok(())
            };

            if watch {
                run_watch_mode(&display_displays)?;
            } else {
                display_displays()?;
            }
        }
        CliSubcommand::Usb => {
            use ironmonlib::usb::UsbMonitor;

            let display_usb = || -> Result<(), Box<dyn std::error::Error>> {
                let monitor = UsbMonitor::new()?;
                if format == "json" {
                    println!("{}", serde_json::to_string_pretty(monitor.devices())?);
                } else {
                    println!("{}", "═══ USB Devices ═══".cyan().bold());
                    println!("  Count: {}", monitor.devices().len());
                    for device in monitor.devices() {
                        let name = device.product.as_deref().unwrap_or("unnamed");
                        // A root hub's PnP id has no VID_ or PID_ at all, and
                        // this printed [0000:0000] for each of them.
                        let ids = match (device.vendor_id, device.product_id) {
                            (None, None) => "[no usb ids]".to_string(),
                            (v, p) => format!(
                                "[{}:{}]",
                                v.map_or("----".to_string(), |v| format!("{v:04x}")),
                                p.map_or("----".to_string(), |p| format!("{p:04x}"))
                            ),
                        };
                        // The class, which the device declares and which 24 of
                        // this machine's 39 USB nodes report -- it was not shown
                        // at all. And the speed, which is not read on Windows:
                        // printing `Unknown` shows an enum variant where the
                        // honest answer is that nobody asked. See the absence
                        // reason on `usb.{addr}.speed`.
                        let class = match device.class {
                            ironmonlib::usb::UsbDeviceClass::Unknown => {
                                "class undeclared".to_string()
                            }
                            ref c => format!("{c:?}"),
                        };
                        let speed = match device.speed {
                            ironmonlib::usb::UsbSpeed::Unknown => "speed not read".to_string(),
                            ref s => format!("{s:?}"),
                        };
                        println!("  {ids} {name} ({class}, {speed})");
                    }
                }
                Ok(())
            };

            if watch {
                run_watch_mode(&display_usb)?;
            } else {
                display_usb()?;
            }
        }
        CliSubcommand::Jetson { action } => {
            handle_jetson_command(action)?;
        }
    }

    Ok(())
}

#[cfg(feature = "cli")]
fn print_board_info(board: &ironmonlib::core::platform_info::BoardInfo) {
    println!("{}", "═══ Board Information ═══".cyan().bold());
    println!(
        "  {} {}",
        "Model:".white().bold(),
        board.hardware.model.yellow()
    );
    println!(
        "  {} {} {}",
        "System:".white().bold(),
        board.platform.system.green(),
        board.platform.machine.green()
    );
    if let Some(dist) = &board.platform.distribution {
        println!("  {} {}", "Distribution:".white().bold(), dist.green());
    }
    // Omitted when the platform reported no release, rather than printing an empty
    // label that reads as "the kernel version is blank".
    if !board.platform.release.is_empty() {
        println!(
            "  {} {}",
            "Kernel:".white().bold(),
            board.platform.release.green()
        );
    }

    if let Some(l4t) = &board.hardware.l4t {
        println!("  {} {}", "L4T:".white().bold(), l4t.magenta());
    }
    if let Some(cuda) = &board.libraries.cuda {
        println!("  {} {}", "CUDA:".white().bold(), cuda.magenta());
    }
}

#[cfg(feature = "cli")]
fn print_gpu_info(gpus: &std::collections::HashMap<String, ironmonlib::core::gpu::GpuInfo>) {
    println!("{}", "═══ GPU Information ═══".cyan().bold());
    if gpus.is_empty() {
        println!("  {}", "No GPUs detected".yellow());
        return;
    }
    // `GpuStats` is a HashMap, whose iteration order changes between calls. In the
    // interactive monitor that made the device list jump around between frames.
    let mut names: Vec<&String> = gpus.keys().collect();
    names.sort();

    for name in names {
        let gpu = &gpus[name];
        println!(
            "\n  {} {} ({})",
            "▶".green(),
            name.white().bold(),
            format!("{:?}", gpu.gpu_type).magenta()
        );

        // Color-code load based on usage. A device with no utilization
        // counter prints "not read" rather than a green 0.0%, which is the
        // same treatment the clocks below already get.
        let load_colored = match gpu.status.load {
            Some(load) => {
                let load_str = format!("{load:.1}%");
                if load > 90.0 {
                    load_str.red().bold()
                } else if load > 70.0 {
                    load_str.yellow()
                } else {
                    load_str.green()
                }
            }
            None => "not read".dimmed(),
        };
        println!("    {} {}", "Load:".white(), load_colored);

        // Clocks are unavailable on several adapters (the Windows AMD path reads
        // none). Printing "0 MHz (0-0 MHz)" states a clock that was never read.
        if gpu.frequency.current > 0 || gpu.frequency.max > 0 {
            println!(
                "    {} {} {} ({}-{} MHz)",
                "Frequency:".white(),
                format!("{} MHz", gpu.frequency.current).cyan(),
                "range:".dimmed(),
                gpu.frequency.min,
                gpu.frequency.max
            );
        }
        // Only Linux DVFS drivers expose a governor.
        if !gpu.frequency.governor.is_empty() {
            println!(
                "    {} {}",
                "Governor:".white(),
                gpu.frequency.governor.blue()
            );
        }

        if let (Some(used), Some(total)) = (gpu.status.memory_used, gpu.status.memory_total) {
            println!(
                "    {} {} / {} MB",
                "Memory:".white(),
                (used / 1024 / 1024).to_string().cyan(),
                total / 1024 / 1024
            );
        }

        if let Some(temp) = gpu.status.temperature {
            let temp_str = format!("{:.1}°C", temp);
            let temp_colored = if temp > 85.0 {
                temp_str.red().bold()
            } else if temp > 70.0 {
                temp_str.yellow()
            } else {
                temp_str.green()
            };
            println!("    {} {}", "Temperature:".white(), temp_colored);
        }
        if let Some(power) = gpu.status.power_draw {
            println!(
                "    {} {}",
                "Power:".white(),
                format!("{:.1}W", power).yellow()
            );
        }
    }
}

/// Print the CPU's model and clock, skipping either if it was not measured.
///
/// Neither line appeared at all while the Windows reader shelled out to `wmic`,
/// because on any build since 11 24H2 the tool is gone and there was nothing real to
/// print. Both are read from the registry and `CallNtPowerInformation` now.
#[cfg(feature = "cli")]
fn print_cpu_identity(cpu: &ironmonlib::core::cpu::CpuStats) {
    let Some(core) = cpu.cores.first() else {
        return;
    };
    if !core.model.is_empty() && core.model != "Unknown CPU" {
        println!("  {} {}", "Model:".white().bold(), core.model.green());
    }
    if let Some(freq) = &core.frequency {
        // "Clock: 0 MHz" was this line, reading a sentinel the Windows reader
        // wrote and only the ontology resolver understood.
        let mut clock = match freq.current {
            Some(mhz) => format!("{mhz} MHz"),
            None => "not read — Windows reports the nominal clock, not the current one".to_string(),
        };
        // Only worth stating a ceiling when it differs from what the core is doing.
        if let Some(max) = freq.max.filter(|m| *m > 0 && Some(*m) != freq.current) {
            clock.push_str(&format!(" (max {max} MHz)"));
        }
        println!("  {} {}", "Clock:".white().bold(), clock.green());
    }
}

#[cfg(feature = "cli")]
fn print_cpu_info(cpu: &ironmonlib::core::cpu::CpuStats) {
    println!("{}", "═══ CPU Information ═══".cyan().bold());

    print_cpu_identity(cpu);
    println!(
        "  {} {} (Online: {})",
        // `core_count()` is `cores.len()` — the logical processor list. This
        // said "Cores: 24" one line under "AMD Ryzen 9 9900X 12-Core
        // Processor", the same contradiction `ironmon status` carried.
        "Threads:".white().bold(),
        cpu.core_count().to_string().green(),
        cpu.online_count().to_string().green()
    );

    let usage = 100.0 - cpu.total.idle;
    let usage_str = format!("{:.1}%", usage);
    let usage_colored = if usage > 90.0 {
        usage_str.red().bold()
    } else if usage > 70.0 {
        usage_str.yellow()
    } else {
        usage_str.green()
    };
    println!(
        "  {} {} ({}: {:.1}%, {}: {:.1}%, {}: {:.1}%)",
        "Usage:".white().bold(),
        usage_colored,
        "user".dimmed(),
        cpu.total.user,
        "system".dimmed(),
        cpu.total.system,
        "idle".dimmed(),
        cpu.total.idle
    );

    println!("\n  {}", "Per-Core Usage:".white().bold());

    // Display cores in a compact grid format
    let cores_per_row = 4;
    for chunk in cpu.cores.chunks(cores_per_row) {
        print!("    ");
        for core in chunk {
            if core.online {
                let usage = 100.0 - core.idle.unwrap_or(0.0);
                let bar = create_usage_bar(usage, 10);
                let usage_str = format!("{:>5.1}%", usage);
                let usage_colored = if usage > 90.0 {
                    usage_str.red()
                } else if usage > 70.0 {
                    usage_str.yellow()
                } else {
                    usage_str.green()
                };
                print!(
                    "{}{:>2}{} {} {} ",
                    "CPU".dimmed(),
                    core.id,
                    ":".dimmed(),
                    bar,
                    usage_colored
                );
            } else {
                print!(
                    "{}{:>2}{} {}     ",
                    "CPU".dimmed(),
                    core.id,
                    ":".dimmed(),
                    "OFFLINE".red()
                );
            }
        }
        println!();
    }
}

/// Create a colored usage bar
#[cfg(feature = "cli")]
fn create_usage_bar(usage: f32, width: usize) -> String {
    let filled = ((usage / 100.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);

    let bar_char = "█";
    let empty_char = "░";

    let filled_str = bar_char.repeat(filled);
    let empty_str = empty_char.repeat(empty);

    let colored_filled = if usage > 90.0 {
        filled_str.red()
    } else if usage > 70.0 {
        filled_str.yellow()
    } else {
        filled_str.green()
    };

    format!("{}{}", colored_filled, empty_str.dimmed())
}

#[cfg(feature = "cli")]
fn print_memory_info(memory: &ironmonlib::core::memory::MemoryStats) {
    println!("{}", "═══ Memory Information ═══".cyan().bold());

    let ram_usage = memory.ram_usage_percent();
    let ram_bar = create_usage_bar(ram_usage, 20);
    let ram_used_gb = memory.ram.used as f64 / 1024.0 / 1024.0;
    let ram_total_gb = memory.ram.total as f64 / 1024.0 / 1024.0;

    let ram_pct_str = format!("{:.1}%", ram_usage);
    let ram_pct_colored = if ram_usage > 90.0 {
        ram_pct_str.red().bold()
    } else if ram_usage > 75.0 {
        ram_pct_str.yellow()
    } else {
        ram_pct_str.green()
    };

    println!(
        "  {} {} {:.2} / {:.2} GB {}",
        "RAM:".white().bold(),
        ram_bar,
        ram_used_gb,
        ram_total_gb,
        ram_pct_colored
    );

    // Three cases, and the old `total_or_zero() > 0` collapsed two of them:
    // a pagefile in use, a machine with none configured, and a pagefile this
    // platform did not report. The last one used to vanish from the output
    // entirely, which reads as the second.
    if memory.swap.total.is_none() {
        println!("  {} not reported by this platform", "SWAP:".white().bold());
    } else if memory.swap.total_or_zero() > 0 {
        // Guarded above, so swap was reported.
        let swap_usage = memory.swap_usage_percent_or_zero();
        let swap_bar = create_usage_bar(swap_usage, 20);
        let swap_used_gb = memory.swap.used_or_zero() as f64 / 1024.0 / 1024.0;
        let swap_total_gb = memory.swap.total_or_zero() as f64 / 1024.0 / 1024.0;

        let swap_pct_str = format!("{:.1}%", swap_usage);
        let swap_pct_colored = if swap_usage > 90.0 {
            swap_pct_str.red().bold()
        } else if swap_usage > 75.0 {
            swap_pct_str.yellow()
        } else {
            swap_pct_str.green()
        };

        println!(
            "  {} {} {:.2} / {:.2} GB {}",
            "SWAP:".white().bold(),
            swap_bar,
            swap_used_gb,
            swap_total_gb,
            swap_pct_colored
        );
    }
}

#[cfg(feature = "cli")]
fn print_power_info(
    power: &ironmonlib::core::power::PowerStats,
    readings: &[&ironmonlib::ontology::resolve::Reading],
) {
    println!("{}", "═══ Power Information ═══".cyan().bold());

    print_readings_with_reasons(readings, "no power entity is readable here");

    // With no rails the total is a fixed zero, and "Total Power: 0.00W" reads as a
    // measurement of an idle machine rather than the absence of any power sensor.
    //
    // Returning here used to end the command, which is why a Windows desktop
    // saw nothing at all. The readings above are printed first now, so an
    // absent rail set only means the rails are absent.
    if power.rails.is_empty() {
        println!();
        println!(
            "  {}",
            "No hwmon power rails on this platform (Linux exposes these; Windows does not)"
                .yellow()
        );
        return;
    }

    let total = power.total_watts();
    let total_str = format!("{:.2}W", total);
    let total_colored = if total > 100.0 {
        total_str.red().bold()
    } else if total > 50.0 {
        total_str.yellow()
    } else {
        total_str.green()
    };
    println!("  {} {}", "Total Power:".white().bold(), total_colored);

    if !power.rails.is_empty() {
        println!("\n  {}", "Power Rails:".white().bold());
        for (name, rail) in &power.rails {
            if rail.online {
                let power_w = rail.power as f64 / 1000.0;
                let power_str = format!("{:.2}W", power_w);
                let power_colored = if power_w > 30.0 {
                    power_str.yellow()
                } else {
                    power_str.green()
                };
                println!(
                    "    {} {} ({:.1}V, {:.1}mA)",
                    format!("{}:", name).white(),
                    power_colored,
                    rail.voltage as f64 / 1000.0,
                    rail.current as f64
                );
            }
        }
    }
}

/// Print a set of ontology readings, then the reason for each one that has no
/// value.
///
/// Both `ironmon cli temperature` and `ironmon cli power` read a narrow platform
/// struct and printed "No temperature sensors detected" / "No power rails
/// exposed by this platform" on a desktop where the ontology resolved five
/// temperatures and four power figures. The distinction this renderer keeps —
/// a value, or a reason there is none — is the one those two messages
/// collapsed.
#[cfg(feature = "cli")]
fn print_readings_with_reasons(
    readings: &[&ironmonlib::ontology::resolve::Reading],
    nothing_known: &str,
) {
    use ironmonlib::ontology::Provenance;

    if readings.is_empty() {
        println!("  {}", nothing_known.yellow());
        return;
    }

    let mut unread = Vec::new();
    for r in readings {
        let Some(value) = r.value.as_ref() else {
            unread.push(*r);
            continue;
        };

        // A reading carries its own unit, so render the number and name it
        // rather than assuming degrees or watts.
        let rendered = match value.as_f64() {
            // A drive reporting 40.850006103515625 °C is reporting one tenth
            // of a degree it cannot resolve; the extra digits are the float,
            // not the sensor. Integers stay integers so a core count does not
            // grow a decimal point.
            Some(n) => {
                let number = if n.fract() == 0.0 {
                    format!("{n}")
                } else {
                    format!("{n:.1}")
                };
                match &r.unit {
                    Some(u) => format!("{number} {u:?}").to_lowercase(),
                    None => number,
                }
            }
            None => value.to_string().trim_matches('"').to_string(),
        };
        let qualifier = match r.provenance {
            Provenance::Measured => String::new(),
            other => format!(" [{other:?}]").to_lowercase(),
        };
        println!(
            "  {:<44} {}{}",
            format!("{}:", r.id).white(),
            rendered.green(),
            qualifier
        );
    }

    if !unread.is_empty() {
        println!();
        println!("{}", "  Not readable here:".white().bold());
        for r in unread {
            let reason = r.note.as_deref().unwrap_or("no reason recorded");
            println!("    {:<42} {}", r.id.dimmed(), reason.dimmed());
        }
    }
}

/// Print every temperature the ontology resolved, and the reason for each one
/// it could not.
#[cfg(feature = "cli")]
fn print_temperature_readings(temps: &[&ironmonlib::ontology::resolve::Reading]) {
    println!("{}", "═══ Temperature Information ═══".cyan().bold());
    print_readings_with_reasons(
        temps,
        "IronMonitor knows of no temperature entity on this platform",
    );
}

#[cfg(feature = "cli")]
fn run_interactive_mode(
    mut stats: ironmonlib::IronMonitor,
) -> Result<(), Box<dyn std::error::Error>> {
    use crossterm::{
        event::{self, Event, KeyCode},
        terminal::{disable_raw_mode, enable_raw_mode},
    };
    use std::time::Duration;

    println!("Interactive monitoring mode - Press 'q' to quit");
    println!("Updating every {:.1}s\n", stats.interval().as_secs_f64());

    enable_raw_mode()?;

    loop {
        // Check for quit key
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        // Clear screen
        print!("\x1B[2J\x1B[1;1H");

        // Get snapshot
        let snapshot = stats.snapshot()?;

        // Print summary
        // Not NVIDIA-specific: this path reports CPU, memory, thermals and any
        // detected GPU regardless of vendor.
        println!("=== IronMonitor - System Monitoring ===");
        println!("Uptime: {:?}\n", snapshot.uptime);

        // GPU
        print_gpu_info(&snapshot.gpus);
        println!();

        // CPU
        print_cpu_info(&snapshot.cpu);
        println!();

        // Memory
        print_memory_info(&snapshot.memory);
        println!();

        // Temperature
        if let Some(max_temp) = snapshot.temperature.max_temp() {
            println!("Max Temperature: {:.1}°C", max_temp);
        }

        // Power. Only report a total when power rails were actually read: with no
        // rails the total is a fixed zero, and "Total Power: 0.00W" reads as a
        // measurement of an idle machine rather than the absence of a sensor.
        if snapshot.power.rails.is_empty() {
            println!("Total Power: not measured (no power rails on this platform)");
        } else {
            println!("Total Power: {:.2}W", snapshot.power.total_watts());
        }

        println!("\nPress 'q' to quit");

        std::thread::sleep(stats.interval());
    }

    disable_raw_mode()?;
    Ok(())
}

// ============================================================================
// BACKEND-BASED PRINT FUNCTIONS (using shared MonitoringBackend)
// ============================================================================

/// Print GPU info using MonitoringBackend
#[cfg(feature = "cli")]
fn print_gpu_info_backend(backend: &ironmonlib::backend::MonitoringBackend) {
    println!("{}", "═══ GPU Information ═══".cyan().bold());

    let static_info = backend.gpu_static_info();
    let dynamic_info = backend.gpu_dynamic_info();

    if static_info.is_empty() {
        println!("  {}", "No GPUs detected".yellow());
        return;
    }

    for (si, di) in static_info.iter().zip(dynamic_info.iter()) {
        println!(
            "\n  {} {} ({})",
            "▶".green(),
            si.name.white().bold(),
            si.vendor.to_string().magenta()
        );

        // Utilization. As with the memory below: the bar is drawn only when
        // there is a reading to draw, since an empty bar beside a green 0.0%
        // reads as an idle card rather than an unmeasured one.
        match di.utilization.map(f32::from) {
            Some(util) => {
                let util_str = format!("{util:.1}%");
                let util_colored = if util > 90.0 {
                    util_str.red().bold()
                } else if util > 70.0 {
                    util_str.yellow()
                } else {
                    util_str.green()
                };
                println!(
                    "    {} {} {}",
                    "Load:".white(),
                    create_usage_bar(util, 15),
                    util_colored
                );
            }
            None => println!("    {} not read", "Load:".white()),
        }

        // Memory. The bar is drawn only when there is a percentage to draw:
        // an empty bar beside "0 / 0 MB" reads as a card with no memory rather
        // than as a card whose memory could not be read.
        match (di.memory.used, di.memory.total, di.memory.utilization) {
            (Some(used), Some(total), Some(pct)) => println!(
                "    {} {} {} / {} MB",
                "Memory:".white(),
                create_usage_bar(pct as f32, 15),
                (used / 1024 / 1024).to_string().cyan(),
                total / 1024 / 1024
            ),
            (Some(used), None, _) => println!(
                "    {} {} MB used, capacity not reported",
                "Memory:".white(),
                (used / 1024 / 1024).to_string().cyan()
            ),
            _ => println!("    {} not reported", "Memory:".white()),
        }

        // Temperature.
        //
        // `GpuThermal::temperature` is degrees Celsius, not millidegrees. This
        // previously divided by 1000 and compared against 85000/70000, so an
        // RTX 3090 Ti at 80C rendered as "0.1°C" in green and the thermal warning
        // thresholds could never fire at any temperature.
        //
        // The neighbouring power field *is* milliwatts, which is what made the
        // mistake plausible.
        if let Some(temp) = di.thermal.temperature {
            let temp_str = format!("{:.1}°C", temp as f32);
            let temp_colored = if temp > 85 {
                temp_str.red().bold()
            } else if temp > 70 {
                temp_str.yellow()
            } else {
                temp_str.green()
            };
            println!("    {} {}", "Temperature:".white(), temp_colored);
        }

        // Power
        if let Some(power) = di.power.draw {
            let power_w = power as f32 / 1000.0;
            println!(
                "    {} {}",
                "Power:".white(),
                format!("{:.1}W", power_w).yellow()
            );
        }

        // Clocks
        if let Some(graphics) = di.clocks.graphics {
            if graphics > 0 {
                let memory = di.clocks.memory.unwrap_or(0);
                println!(
                    "    {} {} MHz (mem: {} MHz)",
                    "Clocks:".white(),
                    graphics.to_string().cyan(),
                    memory
                );
            }
        }
    }
}

/// Print CPU info using MonitoringBackend
#[cfg(feature = "cli")]
fn print_cpu_info_backend(backend: &ironmonlib::backend::MonitoringBackend) {
    println!("{}", "═══ CPU Information ═══".cyan().bold());

    if let Some(cpu) = backend.cpu_stats() {
        print_cpu_identity(cpu);
        println!(
            "  {} {} (Online: {})",
            // See the note in print_cpu_info: this is the logical count.
            "Threads:".white().bold(),
            cpu.core_count().to_string().green(),
            cpu.online_count().to_string().green()
        );

        let usage = 100.0 - cpu.total.idle;
        let usage_str = format!("{:.1}%", usage);
        let usage_colored = if usage > 90.0 {
            usage_str.red().bold()
        } else if usage > 70.0 {
            usage_str.yellow()
        } else {
            usage_str.green()
        };
        println!(
            "  {} {} ({}: {:.1}%, {}: {:.1}%, {}: {:.1}%)",
            "Usage:".white().bold(),
            usage_colored,
            "user".dimmed(),
            cpu.total.user,
            "system".dimmed(),
            cpu.total.system,
            "idle".dimmed(),
            cpu.total.idle
        );

        println!("\n  {}", "Per-Core Usage:".white().bold());

        let cores_per_row = 4;
        for chunk in cpu.cores.chunks(cores_per_row) {
            print!("    ");
            for core in chunk {
                if core.online {
                    let usage = 100.0 - core.idle.unwrap_or(0.0);
                    let bar = create_usage_bar(usage, 10);
                    let usage_str = format!("{:>5.1}%", usage);
                    let usage_colored = if usage > 90.0 {
                        usage_str.red()
                    } else if usage > 70.0 {
                        usage_str.yellow()
                    } else {
                        usage_str.green()
                    };
                    print!(
                        "{}{:>2}{} {} {} ",
                        "CPU".dimmed(),
                        core.id,
                        ":".dimmed(),
                        bar,
                        usage_colored
                    );
                } else {
                    print!(
                        "{}{:>2}{} {}     ",
                        "CPU".dimmed(),
                        core.id,
                        ":".dimmed(),
                        "OFFLINE".red()
                    );
                }
            }
            println!();
        }
    } else {
        println!("  {}", "No CPU data available".yellow());
    }
}

/// Print memory info using MonitoringBackend
#[cfg(feature = "cli")]
fn print_memory_info_backend(backend: &ironmonlib::backend::MonitoringBackend) {
    println!("{}", "═══ Memory Information ═══".cyan().bold());

    if let Some(memory) = backend.memory_stats() {
        let ram_usage = memory.ram_usage_percent();
        let ram_bar = create_usage_bar(ram_usage, 20);
        let ram_used_gb = memory.ram.used as f64 / 1024.0 / 1024.0;
        let ram_total_gb = memory.ram.total as f64 / 1024.0 / 1024.0;

        let ram_pct_str = format!("{:.1}%", ram_usage);
        let ram_pct_colored = if ram_usage > 90.0 {
            ram_pct_str.red().bold()
        } else if ram_usage > 75.0 {
            ram_pct_str.yellow()
        } else {
            ram_pct_str.green()
        };

        println!(
            "  {} {} {:.2} / {:.2} GB {}",
            "RAM:".white().bold(),
            ram_bar,
            ram_used_gb,
            ram_total_gb,
            ram_pct_colored
        );

        // Second copy of the block above, and it had the same three-into-two
        // collapse: an unreported pagefile printed nothing.
        if memory.swap.total.is_none() {
            println!("  {} not reported by this platform", "SWAP:".white().bold());
        } else if memory.swap.total_or_zero() > 0 {
            let swap_usage = memory.swap_usage_percent_or_zero();
            let swap_bar = create_usage_bar(swap_usage, 20);
            let swap_used_gb = memory.swap.used_or_zero() as f64 / 1024.0 / 1024.0;
            let swap_total_gb = memory.swap.total_or_zero() as f64 / 1024.0 / 1024.0;

            let swap_pct_str = format!("{:.1}%", swap_usage);
            let swap_pct_colored = if swap_usage > 90.0 {
                swap_pct_str.red().bold()
            } else if swap_usage > 75.0 {
                swap_pct_str.yellow()
            } else {
                swap_pct_str.green()
            };

            println!(
                "  {} {} {:.2} / {:.2} GB {}",
                "SWAP:".white().bold(),
                swap_bar,
                swap_used_gb,
                swap_total_gb,
                swap_pct_colored
            );
        }
    } else {
        println!("  {}", "No memory data available".yellow());
    }
}

/// Print process info using MonitoringBackend
#[cfg(feature = "cli")]
fn print_process_info_backend(
    backend: &ironmonlib::backend::MonitoringBackend,
) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::ProcessCategory;
    use std::collections::HashMap;

    println!("{}", "═══ Process Information ═══".cyan().bold());

    let processes = backend.processes();

    if processes.is_empty() {
        println!("  {}", "No process data available".yellow());
        return Ok(());
    }

    // Group by category
    let mut by_category: HashMap<ProcessCategory, Vec<&ironmonlib::ProcessMonitorInfo>> =
        HashMap::new();
    for proc in processes.iter() {
        by_category.entry(proc.category).or_default().push(proc);
    }

    // Print category summary
    println!("\n  {}", "Category Summary:".white().bold());
    println!(
        "  {:<18} {:>8} {:>8} {:>10} {:>12}",
        "CATEGORY".dimmed(),
        "PROCS".dimmed(),
        "GPU".dimmed(),
        "CPU%".dimmed(),
        "MEMORY".dimmed()
    );
    println!("  {}", "─".repeat(60).dimmed());

    let mut categories: Vec<_> = by_category.keys().collect();
    categories.sort_by_key(|c| c.display_name());

    for category in &categories {
        if let Some(procs) = by_category.get(*category) {
            let count = procs.len();
            let gpu_count = procs.iter().filter(|p| p.is_gpu_process()).count();
            let total_cpu: f32 = procs.iter().map(|p| p.cpu_percent).sum();
            let total_mem: u64 = procs.iter().map(|p| p.memory_bytes).sum();
            let mem_mb = total_mem as f64 / 1024.0 / 1024.0;

            let gpu_indicator = if gpu_count > 0 {
                format!("{}", gpu_count).magenta().to_string()
            } else {
                "-".dimmed().to_string()
            };

            let cpu_str = format!("{:.1}", total_cpu);
            let cpu_colored = if total_cpu > 50.0 {
                cpu_str.yellow()
            } else {
                cpu_str.green()
            };

            let mem_str = format!("{:.1}MB", mem_mb);
            let mem_colored = if mem_mb > 1000.0 {
                mem_str.yellow()
            } else {
                mem_str.white()
            };

            println!(
                "  {} {:<15} {:>8} {:>8} {:>10} {:>12}",
                category.icon(),
                category.display_name().white(),
                count.to_string().cyan(),
                gpu_indicator,
                cpu_colored,
                mem_colored,
            );
        }
    }

    // Show top processes in select categories
    println!();
    for category in &[
        ProcessCategory::AiMl,
        ProcessCategory::GpuCompute,
        ProcessCategory::Gaming,
        ProcessCategory::Browser,
        ProcessCategory::Development,
        ProcessCategory::Media,
    ] {
        if let Some(procs) = by_category.get(category) {
            if !procs.is_empty() {
                let mut sorted: Vec<_> = procs.iter().collect();
                sorted.sort_by(|a, b| {
                    b.cpu_percent
                        .partial_cmp(&a.cpu_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                println!(
                    "  {} {} {}",
                    category.icon(),
                    category.display_name().white().bold(),
                    "(top 5):".dimmed()
                );
                println!(
                    "    {:<8} {:<30} {:>8} {:>10} {:>10}",
                    "PID".dimmed(),
                    "NAME".dimmed(),
                    "CPU%".dimmed(),
                    "MEM(MB)".dimmed(),
                    "GPU(MB)".dimmed()
                );

                for proc in sorted.iter().take(5) {
                    // Three states, not two. A process off the GPU shows a
                    // dash; one on the GPU whose memory no device reported
                    // showed "0.0", which is what NVML declines to tell us
                    // under WDDM — every process on Windows.
                    let gpu_mem = match (proc.is_gpu_process(), proc.gpu_memory_mb()) {
                        (true, Some(mb)) => format!("{mb:.1}").magenta().to_string(),
                        (true, None) => "n/a".dimmed().to_string(),
                        (false, _) => "-".dimmed().to_string(),
                    };

                    let cpu_str = format!("{:.1}", proc.cpu_percent);
                    let cpu_colored = if proc.cpu_percent > 50.0 {
                        cpu_str.red()
                    } else if proc.cpu_percent > 20.0 {
                        cpu_str.yellow()
                    } else {
                        cpu_str.green()
                    };

                    let name = if proc.name.len() > 30 {
                        &proc.name[..30]
                    } else {
                        &proc.name
                    };

                    println!(
                        "    {:<8} {:<30} {:>8} {:>10.1} {:>10}",
                        proc.pid.to_string().cyan(),
                        name.white(),
                        cpu_colored,
                        proc.memory_mb(),
                        gpu_mem,
                    );
                }
                println!();
            }
        }
    }

    Ok(())
}

/// Print all system info using MonitoringBackend
#[cfg(feature = "cli")]
fn print_all_info(
    backend: &ironmonlib::backend::MonitoringBackend,
) -> Result<(), Box<dyn std::error::Error>> {
    // System info
    println!("{}", "═══ System Overview ═══".cyan().bold());
    println!(
        "  {} {}",
        "Host:".white().bold(),
        backend.hostname().green()
    );
    println!("  {} {}", "OS:".white().bold(), backend.os_info().green());
    println!("  {} {:?}", "Uptime:".white().bold(), backend.uptime());
    println!();

    // CPU
    print_cpu_info_backend(backend);
    println!();

    // Memory
    print_memory_info_backend(backend);
    println!();

    // GPU
    print_gpu_info_backend(backend);
    println!();

    // Processes summary
    let procs = backend.processes();
    let gpu_procs: Vec<_> = procs.iter().filter(|p| p.is_gpu_process()).collect();
    println!("{}", "═══ Processes ═══".cyan().bold());
    println!(
        "  {} {} ({} using GPU)",
        "Total:".white().bold(),
        procs.len().to_string().cyan(),
        gpu_procs.len().to_string().magenta()
    );

    Ok(())
}

/// Handle all Jetson-specific commands
#[cfg(feature = "cli")]
fn handle_jetson_command(cmd: &JetsonSubcommand) -> Result<(), Box<dyn std::error::Error>> {
    // Platform check - Jetson commands only work on Linux
    #[cfg(not(target_os = "linux"))]
    {
        let _ = cmd; // Suppress unused variable warning
        eprintln!("Error: Jetson commands are only available on Linux");
        eprintln!("       These utilities require NVIDIA Jetson hardware (Xavier, Orin, etc.)");
        std::process::exit(1);
    }

    // The handlers below are additionally gated on `jetson-utils`, which `full`
    // deliberately omits. Without this second arm, a Linux build with `cli` and
    // without `jetson-utils` reached three functions that did not exist —
    // `cargo install iron-monitor` on Linux failed with E0425 for every version
    // published up to 3.0.1, because the default feature set is exactly that
    // combination. `--all-features` enables `jetson-utils` and so never saw it.
    #[cfg(all(target_os = "linux", feature = "jetson-utils"))]
    match cmd {
        JetsonSubcommand::Clocks { action } => handle_jetson_clocks(action),
        JetsonSubcommand::Powermode { action } => handle_nvpmodel(action),
        JetsonSubcommand::Swap { action } => handle_swap(action),
    }

    #[cfg(all(target_os = "linux", not(feature = "jetson-utils")))]
    {
        let _ = cmd;
        eprintln!("Error: this build was compiled without the `jetson-utils` feature");
        eprintln!("       Jetson clock, power mode and swap controls are gated behind it");
        eprintln!("       because they write to system state; see SECURITY.md");
        eprintln!("       Rebuild with: cargo build --features cli,jetson-utils");
        std::process::exit(1);
    }
}

#[cfg(all(feature = "cli", feature = "jetson-utils", target_os = "linux"))]
fn handle_jetson_clocks(action: &JetsonClocksAction) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::utils::clocks;

    if !clocks::is_available() {
        eprintln!("jetson_clocks is not available on this system");
        std::process::exit(1);
    }

    match action {
        JetsonClocksAction::Enable => {
            println!("Enabling jetson_clocks (maximizing performance)...");
            clocks::enable()?;
            println!("jetson_clocks enabled successfully");
        }
        JetsonClocksAction::Disable => {
            println!("Disabling jetson_clocks (restoring settings)...");
            clocks::disable()?;
            println!("jetson_clocks disabled successfully");
        }
        JetsonClocksAction::Status => {
            let status = clocks::show()?;
            println!("=== Jetson Clocks Status ===");
            println!("Active: {}", if status.active { "YES" } else { "NO" });
            println!("\nConfigured Engines:");
            for engine in &status.engines {
                println!("  - {}", engine);
            }
        }
        JetsonClocksAction::Store => {
            println!("Storing current configuration...");
            clocks::store()?;
            println!("Configuration stored successfully");
        }
    }

    Ok(())
}

#[cfg(all(feature = "cli", feature = "jetson-utils", target_os = "linux"))]
fn handle_nvpmodel(action: &NvpmodelAction) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::utils::power_mode;

    if !power_mode::is_available() {
        eprintln!("nvpmodel is not available on this system");
        std::process::exit(1);
    }

    match action {
        NvpmodelAction::Show => {
            let mode = power_mode::query()?;
            println!("=== Current Power Mode ===");
            println!("ID: {}", mode.id);
            println!("Name: {}", mode.name);
        }
        NvpmodelAction::List => {
            let status = power_mode::list_modes()?;
            println!("=== Available Power Modes ===");
            println!("\nCurrent Mode:");
            println!(
                "  ID: {} - {} {}",
                status.current.id,
                status.current.name,
                if status.current.is_default {
                    "(default)"
                } else {
                    ""
                }
            );

            println!("\nAll Modes:");
            for mode in &status.modes {
                println!(
                    "  ID: {} - {} {}",
                    mode.id,
                    mode.name,
                    if mode.is_default { "(default)" } else { "" }
                );
            }

            println!("\nDefault Mode:");
            println!("  ID: {} - {}", status.default.id, status.default.name);
        }
        NvpmodelAction::Set { mode_id, force } => {
            println!("Setting power mode to ID {}...", mode_id);
            power_mode::set_mode(*mode_id, *force)?;
            println!("Power mode set successfully");

            // Show new mode
            let mode = power_mode::query()?;
            println!("New mode: {} ({})", mode.name, mode.id);
        }
        NvpmodelAction::SetName { name, force } => {
            println!("Setting power mode to '{}'...", name);
            power_mode::set_mode_by_name(name, *force)?;
            println!("Power mode set successfully");

            // Show new mode
            let mode = power_mode::query()?;
            println!("New mode: {} ({})", mode.name, mode.id);
        }
    }

    Ok(())
}

#[cfg(all(feature = "cli", feature = "jetson-utils", target_os = "linux"))]
fn handle_swap(action: &SwapAction) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::utils::swap;

    match action {
        SwapAction::Status => {
            let swaps = swap::status()?;

            if swaps.is_empty() {
                println!("No active swap");
            } else {
                println!("=== Active Swap ===");
                println!(
                    "{:<30} {:<10} {:<10} {:<10} {:<10}",
                    "NAME", "TYPE", "SIZE", "USED", "PRIO"
                );
                println!("{}", "-".repeat(80));

                for swap_info in swaps {
                    println!(
                        "{:<30} {:<10} {:<10} {:<10} {:<10}",
                        swap_info.path,
                        swap_info.swap_type,
                        format_size(swap_info.size_kb),
                        format_size(swap_info.used_kb),
                        swap_info.priority,
                    );
                }
            }
        }
        SwapAction::Create { path, size, auto } => {
            println!("This operation requires sudo privileges");
            swap::create(path, *size, *auto)?;
        }
        SwapAction::Enable { path } => {
            println!("Enabling swap: {}", path.display());
            swap::enable(path)?;
            println!("Swap enabled successfully");
        }
        SwapAction::Disable { path } => {
            println!("Disabling swap: {}", path.display());
            swap::disable(path)?;
            println!("Swap disabled successfully");
        }
        SwapAction::Remove { path } => {
            println!("Removing swap file: {}", path.display());
            println!("This operation requires sudo privileges");
            swap::remove(path)?;
        }
    }

    Ok(())
}

#[cfg(feature = "cli")]
fn format_size(kb: u64) -> String {
    if kb < 1024 {
        format!("{}K", kb)
    } else if kb < 1024 * 1024 {
        format!("{:.1}M", kb as f64 / 1024.0)
    } else {
        format!("{:.1}G", kb as f64 / 1024.0 / 1024.0)
    }
}

#[cfg(feature = "cli")]
#[allow(dead_code)]
fn print_process_info(processes: &ironmonlib::core::process::ProcessStats) {
    println!("=== Process Information ===");
    println!("Total Processes: {}", processes.process_count());
    println!(
        "Total GPU Memory: {:.2} MB\n",
        processes.total_gpu_memory_kb as f64 / 1024.0
    );

    if processes.process_count() > 0 {
        println!(
            "{:<8} {:<12} {:<8} {:<8} {:<8} {:<10} {:<10} {:<20}",
            "PID", "USER", "GPU", "TYPE", "STATE", "CPU%", "GPU MEM", "NAME"
        );
        println!("{}", "-".repeat(100));

        for proc in processes.sorted_by_gpu_memory().iter().take(10) {
            println!(
                "{:<8} {:<12} {:<8} {:<8} {:<8} {:<10.1} {:<10} {:<20}",
                proc.pid,
                proc.user,
                proc.gpu,
                proc.process_type,
                proc.state,
                proc.cpu_percent,
                format_size(proc.gpu_memory_kb),
                proc.name,
            );
        }
    }
}

#[cfg(feature = "cli")]
#[allow(dead_code)] // Alternative process display format for future use
fn print_process_info_v2(
    monitor: &mut ironmonlib::ProcessMonitor,
) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::ProcessCategory;

    println!("{}", "═══ Process Information ═══".cyan().bold());

    // Get category statistics
    let category_stats = monitor.category_stats()?;

    // Print category summary
    println!("\n  {}", "Category Summary:".white().bold());
    println!(
        "  {:<18} {:>8} {:>8} {:>10} {:>12}",
        "CATEGORY".dimmed(),
        "PROCS".dimmed(),
        "GPU".dimmed(),
        "CPU%".dimmed(),
        "MEMORY".dimmed()
    );
    println!("  {}", "─".repeat(60).dimmed());

    for stats in &category_stats {
        if stats.process_count > 0 {
            let gpu_indicator = if stats.gpu_process_count > 0 {
                format!("{}", stats.gpu_process_count).magenta().to_string()
            } else {
                "-".dimmed().to_string()
            };

            let cpu_str = format!("{:.1}", stats.total_cpu_percent);
            let cpu_colored = if stats.total_cpu_percent > 50.0 {
                cpu_str.yellow()
            } else {
                cpu_str.green()
            };

            let mem_str = format!("{:.1}MB", stats.memory_mb());
            let mem_colored = if stats.memory_mb() > 1000.0 {
                mem_str.yellow()
            } else {
                mem_str.white()
            };

            println!(
                "  {} {:<15} {:>8} {:>8} {:>10} {:>12}",
                stats.category.icon(),
                stats.category.display_name().white(),
                stats.process_count.to_string().cyan(),
                gpu_indicator,
                cpu_colored,
                mem_colored,
            );
        }
    }

    println!();

    // Print top CPU consumers by category
    let grouped = monitor.processes_grouped_by_category()?;

    // Show top processes in active categories
    for category in &[
        ProcessCategory::AiMl,
        ProcessCategory::GpuCompute,
        ProcessCategory::Gaming,
        ProcessCategory::Browser,
        ProcessCategory::Development,
        ProcessCategory::Media,
    ] {
        if let Some(procs) = grouped.get(category) {
            if !procs.is_empty() {
                let mut sorted = procs.clone();
                sorted.sort_by(|a, b| {
                    b.cpu_percent
                        .partial_cmp(&a.cpu_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                println!(
                    "  {} {} {}",
                    category.icon(),
                    category.display_name().white().bold(),
                    "(top 5):".dimmed()
                );
                println!(
                    "    {:<8} {:<30} {:>8} {:>10} {:>10}",
                    "PID".dimmed(),
                    "NAME".dimmed(),
                    "CPU%".dimmed(),
                    "MEM(MB)".dimmed(),
                    "GPU(MB)".dimmed()
                );

                for proc in sorted.iter().take(5) {
                    // Three states, not two. A process off the GPU shows a
                    // dash; one on the GPU whose memory no device reported
                    // showed "0.0", which is what NVML declines to tell us
                    // under WDDM — every process on Windows.
                    let gpu_mem = match (proc.is_gpu_process(), proc.gpu_memory_mb()) {
                        (true, Some(mb)) => format!("{mb:.1}").magenta().to_string(),
                        (true, None) => "n/a".dimmed().to_string(),
                        (false, _) => "-".dimmed().to_string(),
                    };

                    let cpu_str = format!("{:.1}", proc.cpu_percent);
                    let cpu_colored = if proc.cpu_percent > 50.0 {
                        cpu_str.red()
                    } else if proc.cpu_percent > 20.0 {
                        cpu_str.yellow()
                    } else {
                        cpu_str.green()
                    };

                    let name = if proc.name.len() > 30 {
                        &proc.name[..30]
                    } else {
                        &proc.name
                    };

                    println!(
                        "    {:<8} {:<30} {:>8} {:>10.1} {:>10}",
                        proc.pid.to_string().cyan(),
                        name.white(),
                        cpu_colored,
                        proc.memory_mb(),
                        gpu_mem,
                    );
                }
                println!();
            }
        }
    }

    Ok(())
}

#[cfg(feature = "cli")]
fn print_engine_info(engines: &ironmonlib::core::engine::EngineStats) {
    println!("{}", "═══ Engine Information ═══".cyan().bold());
    println!(
        "  {} {} (Total: {})",
        "Engine Groups:".white().bold(),
        engines.group_count().to_string().cyan(),
        engines.engine_count().to_string().green()
    );

    if engines.group_count() > 0 {
        for (group_name, group_engines) in &engines.groups {
            println!("\n  {} {}", "▶".green(), group_name.white().bold());

            for (engine_name, info) in group_engines {
                let status = if info.online {
                    "ONLINE".green()
                } else {
                    "OFFLINE".red()
                };
                let freq_info = match (info.min, info.max) {
                    (Some(min), Some(max)) => {
                        format!(
                            "{} ({}-{} MHz)",
                            format!("{} MHz", info.current).cyan(),
                            min,
                            max
                        )
                    }
                    _ => format!("{} MHz", info.current).cyan().to_string(),
                };

                println!("    {:<15} {} {}", engine_name.white(), status, freq_info);
            }
        }
    } else {
        // "No engines detected" is a claim about the machine. On every platform
        // but Linux it was a claim about the reader: `read_engine_stats`
        // returns `Ok(EngineStats::default())` there, an empty success, under a
        // comment saying Windows has no equivalent concept. This command's
        // `--help` already says it reads /sys/kernel/debug/clk and is Linux
        // only; the output said something else.
        #[cfg(target_os = "linux")]
        println!(
            "  {}",
            concat!(
                "No engines detected — /sys/kernel/debug/clk was read and ",
                "listed no APE, DLA or NVENC clock. Those exist on Tegra and ",
                "Jetson parts; a desktop has none."
            )
            .yellow()
        );
        #[cfg(not(target_os = "linux"))]
        println!(
            "  {}",
            concat!(
                "Not readable on this platform. Engine clocks come from ",
                "/sys/kernel/debug/clk, which is Linux-only and present on ",
                "Tegra and Jetson parts. For desktop GPU encoder and decoder ",
                "utilisation, use `ironmon cli gpu`."
            )
            .yellow()
        );
    }
}

#[cfg(feature = "cli")]
fn handle_ai_query(query: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::agent::{Agent, AgentConfig};
    use ironmonlib::UnifiedMonitor;
    use std::io::{self, Write};

    // Create monitor for system state
    let monitor = UnifiedMonitor::new()?;

    // Auto-detect and configure best available backend
    let config = match AgentConfig::auto_detect() {
        Ok(cfg) => {
            // Successfully auto-detected a backend
            if let Some(ref backend) = cfg.backend {
                eprintln!("[*] Using backend: {}", backend.backend_type.display_name());
            }
            cfg
        }
        Err(e) => {
            // No backends available - return error instead of falling back
            eprintln!("[!] No AI backends available: {}", e);
            eprintln!("[!] To use AI features, install Ollama (https://ollama.com) or set an API key (OPENAI_API_KEY, GITHUB_TOKEN, etc.)");
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No AI backend configured",
            )));
        }
    }
    .with_caching(true)
    .with_cache_size(50)
    .with_timeout(Duration::from_secs(30)); // Longer timeout for remote backends

    let mut agent = Agent::new(config)?;

    if let Some(question) = query {
        // Single query mode
        println!("[AI Monitor]");
        println!("Question: {}\n", question);

        let response = agent.ask(question, &monitor)?;
        println!("{}", response.response);

        if response.from_cache {
            println!("\n[CACHE] (from cache, <1ms)");
        } else {
            println!("\n[TIME] ({}ms)", response.inference_time_ms);
        }
    } else {
        // Interactive mode
        println!("[AI Monitor - Interactive Mode]");
        println!("Ask questions about your system state. Type 'quit' or 'exit' to leave.\n");

        loop {
            print!("You: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();

            if input.is_empty() {
                continue;
            }

            if input.eq_ignore_ascii_case("quit") || input.eq_ignore_ascii_case("exit") {
                println!("Goodbye!");
                break;
            }

            match agent.ask(input, &monitor) {
                Ok(response) => {
                    println!("\n[Agent]: {}\n", response.response);
                    if response.from_cache {
                        println!("[CACHE] (from cache, <1ms)\n");
                    } else {
                        println!("[TIME] ({}ms)\n", response.inference_time_ms);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}\n", e);
                }
            }
        }
    }

    Ok(())
}

/// Run the monitoring daemon from a TOML configuration file.
#[cfg(feature = "cli")]
fn handle_daemon_command(
    config_path: Option<&str>,
    sample_config: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::daemon::{DaemonConfig, MonitoringDaemon};

    if sample_config {
        print!("{}", DaemonConfig::sample_toml());
        return Ok(());
    }

    let daemon = match config_path {
        Some(path) => MonitoringDaemon::from_config_file(path)?,
        None => MonitoringDaemon::new(DaemonConfig::default()),
    };

    // Fail before announcing settings we are about to reject.
    daemon.validate()?;

    let config = daemon.config();

    println!("{}", "═══ IronMonitor Monitoring Daemon ═══".cyan().bold());
    println!(
        "  {} {}",
        "Listening:".white().bold(),
        daemon.listen_address().cyan()
    );
    println!(
        "  {} {}s",
        "Poll interval:".white().bold(),
        config.poll_interval_secs
    );
    println!(
        "  {} {}",
        "Authentication:".white().bold(),
        if config.api_key.is_some() {
            "api key required".green()
        } else {
            "anonymous (loopback only)".yellow()
        }
    );
    if let Some(ref pid) = config.pid_file {
        println!("  {} {}", "PID file:".white().bold(), pid);
    }
    println!();
    println!("{}", "Press Ctrl+C to stop.".yellow().italic());

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async { daemon.run().await })?;

    Ok(())
}

/// Start the HTTP API and Prometheus metrics server.
///
/// `src/http_server.rs`, `src/observability/server.rs` and `src/prometheus.rs` —
/// about 1300 lines — shipped with no entry point in any binary or example, so the
/// REST API and Prometheus exporter could not be started at all. `grafana/README.md`
/// documented `ironmon --serve --port 9100` and `ironmon daemon --config ironmon.toml`;
/// neither command existed, so anyone following it got "unrecognized subcommand".
#[cfg(feature = "cli")]
fn handle_serve_command(
    port: u16,
    bind: &str,
    log_requests: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::http_server::{HttpServer, HttpServerConfig};

    let config = HttpServerConfig {
        bind_address: bind.to_string(),
        port,
        request_logging: log_requests,
        ..Default::default()
    };

    let server = HttpServer::new(config)?;

    println!("{}", "═══ IronMonitor HTTP Server ═══".cyan().bold());
    println!("  {} http://{}:{}", "Listening:".white().bold(), bind, port);
    println!(
        "  {} http://{}:{}/api/v1/metrics/prometheus",
        "Prometheus:".white().bold(),
        bind,
        port
    );
    println!(
        "  {} http://{}:{}/api/v1/context",
        "Context:".white().bold(),
        bind,
        port
    );
    println!(
        "  {} http://{}:{}/health",
        "Health:".white().bold(),
        bind,
        port
    );

    if bind == "0.0.0.0" {
        println!(
            "  {} bound to all interfaces — full hardware telemetry is reachable \
             from the network",
            "Warning:".yellow().bold()
        );
    }

    println!();
    println!("{}", "Press Ctrl+C to stop.".yellow().italic());

    // `HttpServer::run` is async; main is not, so drive it on a runtime here rather
    // than making every command pay for one.
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async { server.run().await })?;

    Ok(())
}

/// Handle record subcommands for time-series database operations
#[cfg(feature = "cli")]
fn handle_record_command(action: &RecordSubcommand) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::tsdb::{
        format_size, parse_size, MetricsRecorder, ProcessSnapshot, SystemSnapshot, TimeSeriesDb,
    };
    use std::io::{self, Write};

    match action {
        RecordSubcommand::Start {
            database,
            max_size,
            interval,
            max_processes,
            format,
        } => {
            let max_bytes = parse_size(max_size)?;
            let interval_duration = Duration::from_secs_f64(*interval);

            println!("{}", "═══ Starting Metrics Recording ═══".cyan().bold());
            println!(
                "  {} {}",
                "Database:".white().bold(),
                database.display().to_string().green()
            );
            println!(
                "  {} {}",
                "Max Size:".white().bold(),
                format_size(max_bytes).yellow()
            );
            println!(
                "  {} {}s",
                "Interval:".white().bold(),
                interval.to_string().cyan()
            );
            println!(
                "  {} {}",
                "Max Processes:".white().bold(),
                max_processes.to_string().cyan()
            );
            println!();
            println!("{}", "Press Ctrl+C to stop recording...".yellow().italic());
            println!();

            // Create recorder
            let mut recorder =
                MetricsRecorder::new(database, max_bytes, interval_duration, *max_processes)?;

            // Collect on a background thread rather than inline.
            //
            // Inline serial collection cost roughly 1.2s, and the loop slept the
            // requested interval *on top* of that — so `record --interval 1` actually
            // sampled every ~2.2s. Recorded timestamps therefore never matched the
            // requested cadence, and any rate derived from the database was wrong by
            // the ratio between them.
            //
            // The collector now runs at its own cadence and the loop only reads the
            // newest published snapshot, which is an atomic load.
            let collector =
                ironmonlib::pipeline::Collector::spawn(ironmonlib::pipeline::CollectorConfig {
                    interval: interval_duration,
                    collect_processes: *max_processes > 0,
                    ..Default::default()
                });
            let snapshots = collector.handle();

            // Wait for the first fully-populated snapshot so the database does not
            // open with a row whose GPU and process columns are empty purely because
            // driver enumeration had not finished yet.
            let warmup_deadline = std::time::Instant::now() + Duration::from_secs(30);
            while snapshots.latest().gpu_static.is_empty()
                && snapshots.latest().processes.is_empty()
                && std::time::Instant::now() < warmup_deadline
            {
                std::thread::sleep(Duration::from_millis(20));
            }

            // Install Ctrl+C handler
            let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let r = running.clone();
            ctrlc::set_handler(move || {
                r.store(false, std::sync::atomic::Ordering::SeqCst);
            })
            .expect("Error setting Ctrl-C handler");

            let mut record_count = 0u64;
            let start_time = std::time::Instant::now();

            while running.load(std::sync::atomic::Ordering::SeqCst) {
                let iteration_start = std::time::Instant::now();

                // Read the newest published snapshot. Lock-free; never blocks on a
                // driver, so a slow GPU query cannot stretch the sampling interval.
                let state = snapshots.latest();

                // Create system snapshot
                let timestamp = ironmonlib::tsdb::TimeSeriesDb::now_millis();

                // A core whose idle time was not read is `None`, not 0% busy.
                // `unwrap_or(100.0)` here meant "assume fully idle", which
                // came back out of the database as a measured, idle core.
                let cpu_per_core: Vec<Option<f32>> = state
                    .cpu
                    .as_ref()
                    .map(|c| {
                        c.cores
                            .iter()
                            .map(|core| core.idle.map(|idle| 100.0 - idle))
                            .collect()
                    })
                    .unwrap_or_default();

                let cpu_percent = state.cpu_utilization();

                // Platform collectors report memory in KB; the database stores bytes.
                // `(0, 0, 0, 0)` for an unread tick used to go into the file
                // as a machine with no memory installed -- and the readback
                // divides by `memory_total`. Swap keeps its own absence
                // separately: a pagefile that was not read is not an unused one,
                // which is why `SwapInfo` carries `Option` at all.
                let (memory_used, memory_total, swap_used, swap_total) = state
                    .memory
                    .as_ref()
                    .map(|m| {
                        (
                            Some(m.ram.used * 1024),
                            Some(m.ram.total * 1024),
                            m.swap.used.map(|v| v * 1024),
                            m.swap.total.map(|v| v * 1024),
                        )
                    })
                    .unwrap_or((None, None, None, None));

                // Collect GPU stats. `gpu_dynamic` is index-aligned with the static
                // descriptors and carries None for a device whose query failed this
                // tick; those keep their column rather than shifting their neighbours.
                let mut gpu_percent = Vec::new();
                let mut gpu_memory_used = Vec::new();
                let mut gpu_temperature = Vec::new();
                let mut gpu_power_mw = Vec::new();

                for gpu in &state.gpu_dynamic {
                    match gpu {
                        Some(g) => {
                            gpu_percent.push(g.utilization.map(f32::from));
                            // Already `Option`: a device that answered its
                            // query but reported no memory records the same
                            // absence as one that could not be queried at all.
                            gpu_memory_used.push(g.memory.used);
                            gpu_temperature.push(g.thermal.temperature.map(|t| t as f32));
                            gpu_power_mw.push(g.power.draw);
                        }
                        // The device could not be queried on this tick. It is
                        // not idle, cold and drawing no power; nothing is known
                        // about it, and the column says so.
                        None => {
                            gpu_percent.push(None);
                            gpu_memory_used.push(None);
                            gpu_temperature.push(None);
                            gpu_power_mw.push(None);
                        }
                    }
                }

                // Record the heaviest processes by CPU.
                let mut ranked: Vec<_> = state.processes.iter().collect();
                ranked.sort_by(|a, b| {
                    b.cpu_percent
                        .partial_cmp(&a.cpu_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                let processes: Vec<ProcessSnapshot> = ranked
                    .iter()
                    .take(*max_processes)
                    .map(|p| ProcessSnapshot {
                        pid: p.pid,
                        name: p.name.clone(),
                        cpu_percent: p.cpu_percent,
                        memory_bytes: p.memory_bytes,
                        gpu_memory_bytes: p.total_gpu_memory_bytes,
                        gpu_percent: p.gpu_usage_percent,
                        // These four are not tracked. They were previously
                        // written as zero, which reads back as an idle process
                        // rather than as an unmeasured one. Per-process I/O
                        // rates need delta tracking across snapshots (the
                        // absolute byte counts are in ProcessMonitorInfo), and
                        // no OS attributes network bytes per process without a
                        // packet filter. System-wide net rates are on
                        // SystemSnapshot and are real.
                        disk_read_bps: None,
                        disk_write_bps: None,
                        net_rx_bps: None,
                        net_tx_bps: None,
                    })
                    .collect();

                // `None` until a rate exists, rather than `0`: a recording
                // that starts with a zero teaches whoever reads it back that the
                // network was idle at that moment.
                let net_rx_bps = state.total_rx_rate().map(|r| r as u64);
                let net_tx_bps = state.total_tx_rate().map(|r| r as u64);

                let snapshot = SystemSnapshot {
                    timestamp,
                    cpu_percent,
                    cpu_per_core,
                    memory_used,
                    memory_total,
                    swap_used,
                    swap_total,
                    gpu_percent,
                    gpu_memory_used,
                    gpu_temperature,
                    gpu_power_mw,
                    net_rx_bps,
                    net_tx_bps,
                    processes,
                };

                // Record snapshot
                recorder.record_snapshot(snapshot)?;
                record_count += 1;

                // Print status
                if *format == "text" {
                    let elapsed = start_time.elapsed().as_secs();
                    let stats = recorder.stats();
                    print!(
                        "\r{} {} records | {} | {:.1}% full | {} elapsed",
                        "Recording:".green(),
                        record_count.to_string().cyan(),
                        format_size(stats.current_size).yellow(),
                        stats.usage_percent(),
                        format_duration(elapsed)
                    );
                    io::stdout().flush()?;
                } else {
                    // JSON format - print each record
                    println!(
                        "{{\"record\":{},\"size\":{},\"timestamp\":{}}}",
                        record_count,
                        recorder.stats().current_size,
                        timestamp
                    );
                }

                // Sleep only the remainder of the interval. Sleeping the full
                // interval regardless of how long the iteration took is what made the
                // effective sample period drift past the requested one.
                let spent = iteration_start.elapsed();
                std::thread::sleep(interval_duration.saturating_sub(spent));
            }

            println!();
            println!();
            println!("{}", "Recording stopped.".yellow());
            recorder.close()?;

            let stats = recorder.stats();
            println!("{}", "═══ Recording Summary ═══".cyan().bold());
            println!(
                "  {} {}",
                "Total Records:".white().bold(),
                record_count.to_string().green()
            );
            println!(
                "  {} {}",
                "Database Size:".white().bold(),
                format_size(stats.current_size).yellow()
            );
            if let Some(span) = stats.time_span() {
                println!("  {} {}", "Time Span:".white().bold(), span.cyan());
            }
        }

        RecordSubcommand::Info { database } => {
            if !database.exists() {
                println!(
                    "{} Database file not found: {}",
                    "Error:".red().bold(),
                    database.display()
                );
                return Ok(());
            }

            let db = TimeSeriesDb::new(database, 0)?;
            let stats = db.stats();

            println!("{}", "═══ Database Information ═══".cyan().bold());
            println!(
                "  {} {}",
                "Path:".white().bold(),
                stats.path.display().to_string().green()
            );
            println!(
                "  {} {}",
                "Max Size:".white().bold(),
                format_size(stats.max_size).yellow()
            );
            println!(
                "  {} {} ({:.1}%)",
                "Current Size:".white().bold(),
                format_size(stats.current_size).cyan(),
                stats.usage_percent()
            );
            println!(
                "  {} {}",
                "Record Count:".white().bold(),
                stats.record_count.to_string().green()
            );

            if let Some(first) = stats.first_timestamp {
                let dt = chrono::DateTime::from_timestamp_millis(first as i64)
                    .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "Unknown".to_string());
                println!("  {} {}", "First Record:".white().bold(), dt.cyan());
            }
            if let Some(last) = stats.last_timestamp {
                let dt = chrono::DateTime::from_timestamp_millis(last as i64)
                    .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "Unknown".to_string());
                println!("  {} {}", "Last Record:".white().bold(), dt.cyan());
            }
            if let Some(span) = stats.time_span() {
                println!("  {} {}", "Time Span:".white().bold(), span.magenta());
            }

            // Print usage bar
            let bar_width = 40;
            let filled = (stats.usage_percent() / 100.0 * bar_width as f32) as usize;
            let empty = bar_width - filled;
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
            let bar_colored = if stats.usage_percent() > 90.0 {
                bar.red()
            } else if stats.usage_percent() > 70.0 {
                bar.yellow()
            } else {
                bar.green()
            };
            println!("  {} [{}]", "Usage:".white().bold(), bar_colored);
        }

        RecordSubcommand::Query {
            database,
            start,
            end,
            format,
            limit,
        } => {
            if !database.exists() {
                println!(
                    "{} Database file not found: {}",
                    "Error:".red().bold(),
                    database.display()
                );
                return Ok(());
            }

            let mut db = TimeSeriesDb::new(database, 0)?;

            // Parse time range
            let now = TimeSeriesDb::now_millis();
            let start_time = if let Some(s) = start {
                parse_time_spec(s, now)?
            } else {
                0 // From beginning
            };
            let end_time = if let Some(e) = end {
                parse_time_spec(e, now)?
            } else {
                now // Until now
            };

            let mut snapshots = db.query_range(start_time, end_time)?;

            // Apply limit
            if let Some(l) = limit {
                snapshots.truncate(*l);
            }

            if *format == "json" {
                println!("{}", serde_json::to_string_pretty(&snapshots)?);
            } else {
                println!("{}", "═══ Query Results ═══".cyan().bold());
                println!(
                    "  {} {} records",
                    "Found:".white().bold(),
                    snapshots.len().to_string().green()
                );
                println!();

                for snapshot in snapshots.iter().take(20) {
                    let dt = chrono::DateTime::from_timestamp_millis(snapshot.timestamp as i64)
                        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_else(|| "Unknown".to_string());

                    println!("  {} {}", "Time:".white().bold(), dt.cyan());
                    // "not read", the same wording the GPU rows below use. The
                    // memory percentage also divides by `memory_total`, so an
                    // unread tick used to divide by zero and print `NaN%` or
                    // `inf%` next to a confident `0.0%` CPU.
                    let cpu = match snapshot.cpu_percent {
                        Some(p) => format!("{p:.1}%"),
                        None => "not read".to_string(),
                    };
                    let mem = match (snapshot.memory_used, snapshot.memory_total) {
                        (Some(used), Some(total)) if total > 0 => {
                            format!("{:.1}%", (used as f64 / total as f64) * 100.0)
                        }
                        _ => "not read".to_string(),
                    };
                    println!("    CPU: {cpu}  MEM: {mem}");
                    if !snapshot.gpu_percent.is_empty() {
                        for (i, gpu) in snapshot.gpu_percent.iter().enumerate() {
                            let util = match gpu {
                                Some(u) => format!("{:.1}%", u),
                                None => "not read".to_string(),
                            };
                            let temp = match snapshot.gpu_temperature.get(i).copied().flatten() {
                                Some(t) => format!("{:.0}°C", t),
                                // The adapter exposes no sensor, or was not
                                // queried. Printing 0°C here made an integrated
                                // GPU look like the coolest device present.
                                None => "no sensor".to_string(),
                            };
                            println!("    GPU{}: {}  Temp: {}", i, util, temp);
                        }
                    }
                    println!();
                }

                if snapshots.len() > 20 {
                    println!("  {} (showing 20 of {})", "...".dimmed(), snapshots.len());
                }
            }
        }

        RecordSubcommand::Export {
            database,
            output,
            format,
        } => {
            if !database.exists() {
                println!(
                    "{} Database file not found: {}",
                    "Error:".red().bold(),
                    database.display()
                );
                return Ok(());
            }

            let mut db = TimeSeriesDb::new(database, 0)?;
            let snapshots = db.read_all_system_snapshots()?;

            println!("{}", "═══ Exporting Data ═══".cyan().bold());
            println!(
                "  {} {} records",
                "Records:".white().bold(),
                snapshots.len().to_string().green()
            );
            println!(
                "  {} {}",
                "Output:".white().bold(),
                output.display().to_string().yellow()
            );

            if *format == "csv" {
                // Export as CSV
                let mut file = std::fs::File::create(output)?;
                writeln!(
                    file,
                    "timestamp,cpu_percent,memory_used,memory_total,swap_used,swap_total,net_rx_bps,net_tx_bps"
                )?;
                for s in &snapshots {
                    writeln!(
                        file,
                        "{},{},{},{},{},{},{},{}",
                        s.timestamp,
                        // An empty CSV field, which every reader treats as
                        // missing. `0` would be read as a measurement -- the
                        // convention the two network columns already used, now
                        // applied to the six that were writing zeros.
                        s.cpu_percent.map(|v| format!("{v:.2}")).unwrap_or_default(),
                        s.memory_used.map(|v| v.to_string()).unwrap_or_default(),
                        s.memory_total.map(|v| v.to_string()).unwrap_or_default(),
                        s.swap_used.map(|v| v.to_string()).unwrap_or_default(),
                        s.swap_total.map(|v| v.to_string()).unwrap_or_default(),
                        s.net_rx_bps.map(|v| v.to_string()).unwrap_or_default(),
                        s.net_tx_bps.map(|v| v.to_string()).unwrap_or_default()
                    )?;
                }
            } else {
                // Export as JSON
                let file = std::fs::File::create(output)?;
                serde_json::to_writer_pretty(file, &snapshots)?;
            }

            println!(
                "  {} {}",
                "Status:".white().bold(),
                "Export complete!".green()
            );
        }

        RecordSubcommand::Clear { database, force } => {
            if !database.exists() {
                println!(
                    "{} Database file not found: {}",
                    "Info:".yellow().bold(),
                    database.display()
                );
                return Ok(());
            }

            if !force {
                print!(
                    "{} Delete database {}? [y/N]: ",
                    "Confirm:".yellow().bold(),
                    database.display()
                );
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("{}", "Cancelled.".yellow());
                    return Ok(());
                }
            }

            std::fs::remove_file(database)?;
            println!(
                "{} Database deleted: {}",
                "Success:".green().bold(),
                database.display()
            );
        }
    }

    Ok(())
}

/// Handle privacy subcommands for managing data collection consent
#[cfg(feature = "cli")]
/// Print the one fact every `ironmon privacy` screen needs to lead with.
///
/// The five consent categories, their descriptions and their itemised data
/// points were written for a collector that was never built. `ConsentScope::
/// is_collected` is `false` for all of them, `should_collect_data` has no
/// callers outside its own module, and the crate has no telemetry endpoint —
/// every outbound request in it goes to an LLM backend the user configured.
///
/// Displaying "DENIED" against a category that is not running tells the user
/// they switched something off that was never on, and displaying "enabled"
/// after `opt-in` is worse: it thanks them for data nobody receives.
fn print_collects_nothing_notice() {
    println!(
        "{} {}",
        "Note:".yellow().bold(),
        "IronMonitor collects and transmits nothing.".white().bold()
    );
    println!(
        "  {}",
        concat!(
            "There is no telemetry endpoint in this program and no code that ",
            "gathers any of these categories. The settings here record a ",
            "preference that a future collector would have to honour; they do ",
            "not switch anything on or off today."
        )
        .dimmed()
    );
    println!(
        "  {}",
        concat!(
            "The only network requests IronMonitor makes are to AI backends you ",
            "configure and point at yourself."
        )
        .dimmed()
    );
    println!();
}

fn handle_privacy_command(action: &PrivacySubcommand) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::consent::{ConsentManager, ConsentScope};

    match action {
        PrivacySubcommand::Status => {
            let manager = ConsentManager::load()?;

            println!(
                "{}",
                "╔════════════════════════════════════════════════════════════════╗".cyan()
            );
            println!(
                "{}",
                "║           IronMonitor - Privacy Status                    ║".cyan()
            );
            println!(
                "{}",
                "╚════════════════════════════════════════════════════════════════╝".cyan()
            );
            println!();

            // Check runtime flags
            let no_telemetry = std::env::var("IRONMON_NO_TELEMETRY").is_ok();
            let offline = std::env::var("IRONMON_OFFLINE").is_ok();

            if no_telemetry || offline {
                println!("{}", "Session Flags:".white().bold());
                if offline {
                    println!("  {} Offline mode active (--offline)", "⦿".yellow());
                } else if no_telemetry {
                    println!(
                        "  {} Telemetry disabled for this session (--no-telemetry)",
                        "⦿".yellow()
                    );
                }
                println!();
            }

            print_collects_nothing_notice();

            println!("{}", "Recorded preferences:".white().bold());
            println!("─────────────────────────────────────────────────────────────────");

            // Report what the user chose, not what `has_consent` returns.
            // `has_consent` folds three unrelated causes into one bool — the
            // session flags, sandbox detection, and the stored answer — so
            // `ironmon privacy opt-in` could say "consented to" and `status`
            // reply "not granted" on the next line, with no way to tell that
            // a sandbox detection had overridden the choice.
            let records = manager.get_all_consents();
            for scope in [
                ConsentScope::BasicTelemetry,
                ConsentScope::HardwareInfo,
                ConsentScope::PerformanceMetrics,
                ConsentScope::DetailedDiagnostics,
                ConsentScope::Analytics,
            ] {
                let status = match records.get(&scope).map(|r| r.granted) {
                    Some(true) => format!("{} granted", "●".green()),
                    Some(false) => format!("{} denied", "○".red()),
                    None => format!("{} unanswered", "·".yellow()),
                };
                // Printing only "granted" would imply the category is running.
                // None of them is; the preference is recorded against a
                // collector that does not exist yet.
                println!(
                    "  {:<30} {:<20}   {}",
                    scope.name(),
                    status,
                    "(nothing implemented)".dimmed()
                );
            }

            println!();

            // Say plainly when something is overriding the recorded answers,
            // and what it is.
            let sandbox = ironmonlib::sandbox::SandboxDetector::new().detect();
            if sandbox.is_sandboxed() {
                println!(
                    concat!(
                        "{} every category is refused regardless of the answers ",
                        "above, because this looks like a {}."
                    ),
                    "Override:".yellow().bold(),
                    sandbox.summary().to_lowercase()
                );
                println!(
                    "  {}",
                    concat!(
                        "Detection is a heuristic and can be wrong; nothing is ",
                        "collected here either way."
                    )
                    .dimmed()
                );
                println!();
            } else if no_telemetry || offline {
                println!(
                    "{} every category is refused for this session by the flag above.",
                    "Override:".yellow().bold()
                );
                println!();
            }
            println!("{}", "Commands:".white().bold());
            println!("  ironmon privacy opt-out    # Record a denial for every category");
            println!("  ironmon privacy opt-in     # Record consent for every category");
            println!("  ironmon privacy review     # Review settings interactively");
            println!("  ironmon privacy info       # Show what each category would cover");
            println!();
            println!("{}", "Session Flags:".white().bold());
            println!("  ironmon --no-telemetry     # Deny every category for one session");
            println!("  ironmon --offline          # Also keep agent backends off the network");
        }

        PrivacySubcommand::OptOut => {
            let mut manager = ConsentManager::load()?;
            manager.revoke_all()?;

            println!(
                "{}",
                "╔════════════════════════════════════════════════════════════════╗".green()
            );
            println!(
                "{}",
                "║        Preferences Recorded: Everything Denied                 ║".green()
            );
            println!(
                "{}",
                "╚════════════════════════════════════════════════════════════════╝".green()
            );
            println!();
            println!("A denial has been recorded for every category and saved.");
            println!();
            print_collects_nothing_notice();
            println!("You can change these preferences at any time with:");
            println!("  ironmon privacy opt-in");
            println!("  ironmon privacy review");
        }

        PrivacySubcommand::OptIn => {
            let mut manager = ConsentManager::load()?;

            println!(
                "{}",
                "╔════════════════════════════════════════════════════════════════╗".cyan()
            );
            println!(
                "{}",
                "║        Preferences Recorded: Everything Consented              ║".cyan()
            );
            println!(
                "{}",
                "╚════════════════════════════════════════════════════════════════╝".cyan()
            );
            println!();
            println!("Recording consent for every category...");
            println!();

            for scope in [
                ConsentScope::BasicTelemetry,
                ConsentScope::HardwareInfo,
                ConsentScope::PerformanceMetrics,
                ConsentScope::DetailedDiagnostics,
                ConsentScope::Analytics,
            ] {
                manager.record_consent(scope, true)?;
                println!("  {} {} consented to", "✓".green(), scope.name());
            }

            println!();
            println!("{}", "Consent recorded for every category.".green());
            println!();
            // Without this the user walks away believing they have just turned
            // something on and are now sending data. They are not.
            print_collects_nothing_notice();
            println!("You can review or change these preferences at any time with:");
            println!("  ironmon privacy status");
            println!("  ironmon privacy review");
        }

        PrivacySubcommand::Review => {
            let mut manager = ConsentManager::load()?;
            manager.request_all_consents()?;
        }

        PrivacySubcommand::Info => {
            println!(
                "{}",
                "╔════════════════════════════════════════════════════════════════╗".cyan()
            );
            println!(
                "{}",
                "║           Data Collection Categories                          ║".cyan()
            );
            println!(
                "{}",
                "╚════════════════════════════════════════════════════════════════╝".cyan()
            );
            println!();

            print_collects_nothing_notice();

            for scope in [
                ConsentScope::BasicTelemetry,
                ConsentScope::HardwareInfo,
                ConsentScope::PerformanceMetrics,
                ConsentScope::DetailedDiagnostics,
                ConsentScope::Analytics,
            ] {
                println!("{}", format!("═══ {} ═══", scope.name()).yellow().bold());
                println!();
                println!("{}", "Description:".white().bold());
                println!("  {}", scope.description());
                println!();
                println!("{}", "Would cover:".white().bold());
                for point in scope.data_points() {
                    println!("  • {}", point);
                }
                println!();
            }
        }
    }

    Ok(())
}

/// Parse a time specification (e.g., "30m", "2h", Unix timestamp)
#[cfg(feature = "cli")]
fn parse_time_spec(spec: &str, now: u64) -> Result<u64, Box<dyn std::error::Error>> {
    let spec = spec.trim();

    // Check if it's a relative time (e.g., "30m", "2h", "1d")
    if spec.ends_with('m') || spec.ends_with('M') {
        let minutes: u64 = spec[..spec.len() - 1].parse()?;
        return Ok(now.saturating_sub(minutes * 60 * 1000));
    }
    if spec.ends_with('h') || spec.ends_with('H') {
        let hours: u64 = spec[..spec.len() - 1].parse()?;
        return Ok(now.saturating_sub(hours * 60 * 60 * 1000));
    }
    if spec.ends_with('d') || spec.ends_with('D') {
        let days: u64 = spec[..spec.len() - 1].parse()?;
        return Ok(now.saturating_sub(days * 24 * 60 * 60 * 1000));
    }

    // Otherwise, treat as Unix timestamp (seconds or milliseconds)
    let ts: u64 = spec.parse()?;
    if ts < 10_000_000_000 {
        // Seconds
        Ok(ts * 1000)
    } else {
        // Milliseconds
        Ok(ts)
    }
}

/// Format duration in human-readable format
#[cfg(feature = "cli")]
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Handle `ironmon profile ...` — hardware profile inspector
#[cfg(feature = "cli")]
/// Print the entity ontology.
///
/// Deliberately hardware-free: this is the schema, not a reading. An agent fetches
/// it once to learn the id space, the units, and — the part that matters — which
/// values are measurements and which are constants wearing a measurement's clothes.
/// Read one entity by id.
///
/// The exit codes carry information a caller needs: 1 means IronMonitor has never heard
/// of this id, 2 means it knows the id and could not read it here. Collapsing those
/// into a single failure would lose the distinction the ontology exists to preserve.
fn handle_get_command(id: &str, format: &str) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::ontology::resolve;

    let Some(reading) = resolve::get(id) else {
        eprintln!("Unknown entity id: {id}");
        eprintln!(
            "Try `ironmon describe --search {}`.",
            id.split('.').next().unwrap_or(id)
        );
        std::process::exit(1);
    };

    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&reading)?);
    } else {
        match &reading.value {
            Some(v) => {
                let rendered = v
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string());
                let unit = reading
                    .unit
                    .map(|u| format!(" {}", u.as_str()))
                    .unwrap_or_default();
                println!("{rendered}{unit}  [{}]", reading.provenance.as_str());
            }
            None => {
                println!("unavailable");
                if let Some(note) = &reading.note {
                    println!("  {note}");
                }
            }
        }
    }

    if reading.value.is_none() {
        std::process::exit(2);
    }
    Ok(())
}

/// Read every resolvable entity.
fn handle_snapshot_command(
    domain: Option<&str>,
    resolved_only: bool,
    validate: bool,
    format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::ontology::{resolve, Domain};

    let mut readings = resolve::snapshot();

    if let Some(d) = domain {
        let Some(parsed) = Domain::parse(d) else {
            eprintln!("Unknown domain {d:?}. Known domains:");
            for known in Domain::ALL {
                eprintln!("  {}", known.as_str());
            }
            std::process::exit(1);
        };
        let prefix = format!("{}.", parsed.as_str());
        readings.retain(|r| r.id.starts_with(&prefix));
    }
    if resolved_only {
        readings.retain(|r| r.value.is_some());
    }

    let problems = if validate {
        resolve::validate(&readings)
    } else {
        Vec::new()
    };

    if format.eq_ignore_ascii_case("json-ld") || format.eq_ignore_ascii_case("jsonld") {
        // Linked data: every term resolves through the `@context`, so a consumer
        // that has never seen IronMonitor can find out what it is looking at. Absence
        // survives the encoding — an unavailable reading is a node with no value
        // and a stated reason, never a zero.
        let stamp = chrono::Utc::now().to_rfc3339();
        let doc = ironmonlib::ontology::jsonld::document(&readings, &stamp);
        println!("{}", serde_json::to_string_pretty(&doc)?);
    } else if format.eq_ignore_ascii_case("json") {
        let resolved = readings.iter().filter(|r| r.value.is_some()).count();
        let doc = serde_json::json!({
            "ontology_version": ironmonlib::ontology::ONTOLOGY_VERSION,
            "resolved": resolved,
            "unavailable": readings.len() - resolved,
            "impossible": problems,
            "readings": readings,
        });
        println!("{}", serde_json::to_string_pretty(&doc)?);
    } else {
        for r in &readings {
            match &r.value {
                Some(v) => {
                    let rendered = v
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string());
                    let unit = r
                        .unit
                        .map(|u| format!(" {}", u.as_str()))
                        .unwrap_or_default();
                    println!(
                        "{:<44} {:<14} {rendered}{unit}",
                        r.id,
                        r.provenance.as_str()
                    );
                }
                None => println!(
                    "{:<44} {:<14} — {}",
                    r.id,
                    "unavailable",
                    r.note.as_deref().unwrap_or("no reason given")
                ),
            }
        }
        let resolved = readings.iter().filter(|r| r.value.is_some()).count();
        println!();
        println!(
            "{resolved} resolved, {} unavailable",
            readings.len() - resolved
        );
    }

    if !problems.is_empty() {
        eprintln!("\n{} impossible reading(s):", problems.len());
        for p in &problems {
            eprintln!("  {p}");
        }
        std::process::exit(3);
    }
    Ok(())
}

/// Emit the command surface as data.
///
/// Generated by walking clap's own parse tree rather than by maintaining a list, so
/// it describes what the binary actually accepts. A hand-written catalogue would be
/// wrong the first time a flag was added and nobody remembered this function.
///
/// This is the operation half of the ontology: `ironmon describe` names what can be
/// read, this names what can be run. An agent needs both to drive ironmon without
/// guessing at argv.
fn emit_command_catalog(format: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root = Cli::command();

    fn describe(cmd: &clap::Command) -> serde_json::Value {
        let args: Vec<serde_json::Value> = cmd
            .get_arguments()
            .filter(|a| a.get_id() != "help" && a.get_id() != "version")
            .map(|a| {
                serde_json::json!({
                    "name": a.get_id().as_str(),
                    "long": a.get_long(),
                    "short": a.get_short().map(|c| c.to_string()),
                    "help": a.get_help().map(|h| h.to_string()),
                    "required": a.is_required_set(),
                    // A flag takes no value; anything else consumes one.
                    "takes_value": a.get_num_args().map(|n| n.takes_values()).unwrap_or(false),
                    "default": a.get_default_values()
                        .first()
                        .map(|v| v.to_string_lossy().to_string()),
                })
            })
            .collect();

        let subcommands: Vec<serde_json::Value> = cmd
            .get_subcommands()
            .filter(|s| s.get_name() != "help")
            .map(describe)
            .collect();

        serde_json::json!({
            "name": cmd.get_name(),
            "about": cmd.get_about().map(|a| a.to_string()),
            "args": args,
            "subcommands": subcommands,
        })
    }

    let catalog = describe(&root);

    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&catalog)?);
        return Ok(());
    }

    fn print_tree(node: &serde_json::Value, depth: usize) {
        let indent = "  ".repeat(depth);
        let name = node["name"].as_str().unwrap_or("?");
        let about = node["about"].as_str().unwrap_or("");
        println!("{indent}{name}  {about}");
        for arg in node["args"].as_array().into_iter().flatten() {
            let long = arg["long"]
                .as_str()
                .map(|l| format!("--{l}"))
                .unwrap_or_default();
            let takes = if arg["takes_value"].as_bool().unwrap_or(false) {
                " <value>"
            } else {
                ""
            };
            let help = arg["help"].as_str().unwrap_or("");
            if !long.is_empty() {
                println!("{indent}    {long}{takes}  {help}");
            }
        }
        for sub in node["subcommands"].as_array().into_iter().flatten() {
            print_tree(sub, depth + 1);
        }
    }

    print_tree(&catalog, 0);
    Ok(())
}

/// Render one TUI frame to stdout and exit.
///
/// The TUI normally takes over the terminal: alternate screen, raw mode, a redraw
/// loop driven by key events. None of that is reachable by an agent without a TTY,
/// which left the TUI the one surface that could be neither read nor asserted on.
/// This renders the identical frame into an in-memory buffer and prints it.
/// Inspect the GUI from a script and report what happened.
///
/// Mirrors `tui --script` minus the `key` step, which the GUI has no use for: its
/// tabs are addressable by name, so `goto` covers navigation and there is no
/// keystroke state to drive. Exit codes match the TUI's so a caller can treat both
/// surfaces the same way — 1 for a failed assertion, 2 for a script that does not
/// parse, which keeps "the GUI is wrong" separable from "my script is wrong".
#[cfg(feature = "gui")]
fn handle_gui_script_command(source: &str) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::gui::headless;

    let text = if source == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        std::fs::read_to_string(source)
            .map_err(|e| format!("could not read script {source:?}: {e}"))?
    };

    let steps = match headless::parse_script(&text) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if steps.is_empty() {
        eprintln!("the script contains no steps");
        std::process::exit(2);
    }

    let ctx = headless::themed_context();
    let mut app = ironmonlib::gui::app::IronMonitorApp::with_context(&ctx);
    let result = headless::run_script(&mut app, &ctx, &steps);

    for capture in &result.captures {
        println!("{capture}");
        println!();
    }

    if result.failures.is_empty() {
        eprintln!("{} step(s) ok", steps.len());
        return Ok(());
    }
    eprintln!("{} assertion(s) failed:", result.failures.len());
    for failure in &result.failures {
        eprintln!("  {failure}");
    }
    std::process::exit(1);
}

/// Render one GUI tab headlessly and print the text it painted.
///
/// The GUI was the last surface with no non-visual representation. Reading the
/// painted galleys rather than capturing pixels also preserves the distinction a
/// screenshot destroys: text that was never emitted looks different here from text
/// that was emitted in the panel colour, which is the bug that made the Profiles
/// tab appear dead while it rendered all nineteen of its groups.
#[cfg(feature = "gui")]
/// `ironmon ai models` — what the local IronVault vault holds.
///
/// Read-only by construction: IronMonitor never unlocks a vault and never asks for a
/// passphrase. IronVault exposes model metadata without a key, so a locked vault
/// reports fully — see `ironmonlib::model_vault`.
#[cfg(feature = "vault")]
fn handle_vault_models(format: &str) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::model_vault::{read_vault, VaultStatus};

    let status = read_vault();

    if format.eq_ignore_ascii_case("json") {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    match status {
        // Not an error and not an empty table: there is simply no vault here,
        // and saying so beats printing headers over nothing.
        VaultStatus::NotInstalled => {
            println!("IronVault is not installed on this machine.");
            println!("  IronMonitor reports vaults; it does not create them.");
        }
        VaultStatus::Absent { path } => {
            println!(
                "IronVault is installed, but holds no vault at {}",
                path.display()
            );
            println!("  (`iv init` would create one; IronMonitor will not)");
        }
        VaultStatus::Failed { path, reason } => {
            eprintln!("Vault at {} could not be read: {reason}", path.display());
            std::process::exit(1);
        }
        VaultStatus::Present(report) => {
            println!("Vault: {}", report.path.display());
            if report.models.is_empty() {
                println!("  no models stored");
                return Ok(());
            }
            println!(
                "{:<28} {:<14} {:>10} {:>12} {:>4}",
                "MODEL", "FORMAT", "SIZE", "COMPRESSED", "VERS"
            );
            for m in &report.models {
                println!(
                    "{:<28} {:<14} {:>10} {:>12} {:>4}",
                    truncate(&m.name, 28),
                    truncate(&m.format, 14),
                    format_bytes_short(m.size_bytes),
                    format_bytes_short(m.compressed_size_bytes),
                    m.version_count
                );
            }
            println!();
            println!(
                "{} model(s) · {} stored as {} · metadata read without unlocking",
                report.models.len(),
                format_bytes_short(report.total_size_bytes),
                format_bytes_short(report.total_compressed_bytes),
            );
            // A field nothing prints is a field that was dropped, just with more
            // steps. These models exist in the vault and are in neither total.
            if !report.versionless.is_empty() {
                println!(
                    "
{} model(s) listed with no versions, so nothing about their size was read: {}",
                    report.versionless.len(),
                    report.versionless.join(", ")
                );
            }
        }
    }
    Ok(())
}

#[cfg(feature = "vault")]
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}

#[cfg(feature = "vault")]
fn format_bytes_short(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

/// `ironmon tune` — classify the workload, recommend a profile, optionally apply.
fn handle_tune_command(
    watch: Option<u64>,
    apply: bool,
    confirm: bool,
    max_risk: &str,
    force_case: Option<&str>,
    format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::profile::SettingRisk;
    use ironmonlib::tuning::serve::{self, Mode};
    use ironmonlib::tuning::{Classification, Method, UseCase};

    // `dangerous` is rejected rather than silently clamped: a caller who asked
    // for it has a different model of what this command does, and clamping would
    // let them keep it.
    let ceiling = match max_risk.trim().to_ascii_lowercase().as_str() {
        "safe" => SettingRisk::Safe,
        "moderate" => SettingRisk::Moderate,
        "dangerous" => {
            eprintln!(
                "--max-risk dangerous is not accepted. Those settings touch power, thermal, \
                 voltage or MSRs and can destabilize hardware; IronMonitor does not write them from \
                 an unattended loop. Use `ironmon profile set` with --confirm for a single \
                 deliberate change."
            );
            std::process::exit(2);
        }
        other => {
            eprintln!("Unknown risk tier {other:?}. Known tiers: safe, moderate.");
            std::process::exit(2);
        }
    };

    if apply && !confirm {
        eprintln!(
            "--apply requires --confirm. Nothing was written. Run without --apply to see the \
             recommendations first."
        );
        std::process::exit(2);
    }

    let mode = if apply {
        Mode::Apply { ceiling, confirm }
    } else {
        Mode::Recommend
    };

    let classify = |()| -> Classification {
        match force_case {
            Some(name) => match UseCase::parse(name) {
                Some(use_case) => Classification {
                    use_case,
                    confidence: 1.0,
                    method: Method::Signals,
                    evidence: vec![format!("use case forced to {name:?} on the command line")],
                },
                None => {
                    eprintln!(
                        "Unknown use case {name:?}. Known: ai_training, ai_inference, gaming, \
                         interactive, idle."
                    );
                    std::process::exit(2);
                }
            },
            // `classify`, not `classify_from_signals`: this asks a local model
            // where one is running and falls back to the signal path otherwise,
            // recording which happened in the evidence either way.
            None => ironmonlib::tuning::classify(&serve::collect_signals()),
        }
    };

    let started = std::time::Instant::now();
    let json = format.eq_ignore_ascii_case("json");

    let emit = |cycle: &serve::Cycle| {
        if json {
            match serde_json::to_string_pretty(cycle) {
                Ok(s) => println!("{s}"),
                Err(e) => eprintln!("failed to serialize cycle: {e}"),
            }
        } else {
            print_tune_cycle(cycle);
        }
    };

    match watch {
        None => {
            let cycle = serve::cycle_with(mode, started, classify(()));
            emit(&cycle);
        }
        Some(secs) => {
            if secs == 0 {
                eprintln!("--watch needs a non-zero interval.");
                std::process::exit(2);
            }
            if !json {
                println!(
                    "Watching every {secs}s in {} mode. Ctrl-C to stop.\n",
                    if apply { "apply" } else { "recommend" }
                );
            }
            // Ctrl-C sets the flag; the loop checks it between and during sleeps.
            let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let flag = running.clone();
            let _ = ctrlc::set_handler(move || {
                flag.store(false, std::sync::atomic::Ordering::SeqCst);
            });
            serve::run(
                mode,
                std::time::Duration::from_secs(secs),
                |c| emit(c),
                || !running.load(std::sync::atomic::Ordering::SeqCst),
            );
        }
    }

    Ok(())
}

fn print_tune_cycle(cycle: &ironmonlib::tuning::serve::Cycle) {
    use ironmonlib::tuning::Method;

    let c = &cycle.plan.classification;
    let method = match &c.method {
        Method::Signals => "signals".to_string(),
        Method::Model { backend } => format!("model ({backend})"),
    };
    println!(
        "Use case: {}  (confidence {:.0}%, from {method})",
        c.use_case.as_str(),
        c.confidence * 100.0
    );
    for e in &c.evidence {
        println!("  · {e}");
    }

    if cycle.plan.recommendations.is_empty() {
        println!("\nNo changes recommended.");
    } else {
        println!("\nRecommended ({}):", cycle.plan.recommendations.len());
        for r in &cycle.plan.recommendations {
            println!(
                "  {} [{:?}]\n    {} -> {}\n    basis: {}\n    why:   {}",
                r.display_name, r.risk, r.current, r.proposed, r.basis, r.rationale
            );
        }
    }

    if !cycle.applied.is_empty() {
        println!("\nApplied:");
        for a in &cycle.applied {
            println!("  {} — {} ({})", a.setting_id, a.status, a.message);
        }
    }
    if !cycle.withheld.is_empty() {
        println!("\nWithheld:");
        for w in &cycle.withheld {
            println!("  {} — {}", w.setting_id, w.reason);
        }
    }
    if !cycle.plan.skipped.is_empty() {
        println!("\nConsidered and skipped:");
        for s in &cycle.plan.skipped {
            println!("  {} — {}", s.setting_id, s.reason);
        }
    }
    println!();
}

/// Gated for the same reason its sibling `handle_gui_script_command` is: it
/// names `ironmonlib::gui`, which does not exist without the `gui` feature. Its
/// only caller is already behind `#[cfg(feature = "gui")]`, so the omission
/// broke nothing at runtime and broke `--no-default-features --features cli`
/// completely — the exact failure the CI feature-isolation job was written for
/// after 3.0.0, recurring after the 5.0.0 GUI restore.
#[cfg(feature = "gui")]
fn handle_gui_frame_command(tab: &str) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::gui::headless;

    let ctx = headless::themed_context();
    let mut app = ironmonlib::gui::app::IronMonitorApp::with_context(&ctx);

    if let Err(available) = app.select_tab_by_name(tab) {
        eprintln!("Unknown tab {tab:?}. Available tabs:");
        for name in available {
            eprintln!("  {name}");
        }
        std::process::exit(1);
    }

    // Four tabs load their contents on a background thread. Without pumping the
    // loaders between frames they render their spinner forever — which is painted
    // text, so `every_gui_tab_paints_text` passed while an agent reading the disk
    // tab learned nothing.
    //
    // Thirty seconds because the system and peripherals loaders run several
    // PowerShell CIM queries back to back and were measured at over ten. Bounded
    // rather than unbounded so a wedged reader yields a slow, honest answer
    // instead of a command that never returns.
    const SETTLE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
    let lines = headless::painted_text_until(
        &ctx,
        SETTLE_DEADLINE,
        &mut app,
        |app, ctx| app.pump_background_loaders(ctx),
        // Both signals. The painted text catches a tab still showing its
        // placeholder; the app's own flags catch the disk tab's middle state,
        // where enumeration has finished but the rows have not arrived and the
        // frame reads "No Disks Detected" — which text alone cannot tell from a
        // machine that has none.
        |app, lines| !headless::frame_is_still_loading(lines) && !app.has_pending_load(),
        |app, ui| app.draw_current_tab(ui),
    );

    // A timeout and a genuinely quick tab both end here, and the caller cannot
    // tell them apart from the painted text alone — "Loading …" looks the same
    // either way. Say which happened, on stderr so it does not pollute the frame.
    if headless::frame_is_still_loading(&lines) {
        eprintln!(
            "note: the {tab} tab was still loading after {}s; the text below is \
             whatever it had painted by then",
            SETTLE_DEADLINE.as_secs()
        );
    }

    if lines.is_empty() {
        eprintln!(
            "the {tab} tab painted no text at all — that is a blank tab, not an \
             empty one"
        );
        std::process::exit(2);
    }
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

/// Drive the TUI from a script and report what happened.
///
/// This is the half of "operable" that `--frame` does not cover: an agent can now
/// navigate and assert, not only observe. Key steps go through the same handler the
/// interactive loop calls, so a passing script is evidence about the TUI a human
/// uses rather than about a parallel implementation of its bindings.
fn handle_tui_script_command(
    source: &str,
    width: u16,
    height: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::tui::headless;

    let text = if source == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        std::fs::read_to_string(source)
            .map_err(|e| format!("could not read script {source:?}: {e}"))?
    };

    let steps = match headless::parse_script(&text) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if steps.is_empty() {
        eprintln!("the script contains no steps");
        std::process::exit(2);
    }

    let mut app = ironmonlib::tui::App::new()?;
    app.update()?;
    // Same wait as `--frame`: assertions against a frame rendered before the
    // collector published would be testing the zeroed defaults.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if app.sync_snapshot() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let result = headless::run_script(&mut app, &steps, width, height);

    for capture in &result.captures {
        println!("{capture}");
        println!();
    }

    if result.failures.is_empty() {
        eprintln!("{} step(s) ok", steps.len());
        return Ok(());
    }
    eprintln!("{} assertion(s) failed:", result.failures.len());
    for failure in &result.failures {
        eprintln!("  {failure}");
    }
    std::process::exit(1);
}

fn handle_tui_frame_command(
    tab: Option<&str>,
    width: u16,
    height: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    // `new_fast` defers collection to a background thread, which is right for an
    // interactive session that repaints continuously — but a one-shot frame rendered
    // from it shows "CPU 0% | 0 cores", zeros that look like readings and are not.
    // `new` waits for initialisation, so the single frame an agent gets carries real
    // data or nothing.
    let mut app = ironmonlib::tui::App::new()?;
    app.update()?;

    // The collector publishes asynchronously. An interactive session repaints until
    // the first snapshot lands and nobody notices the gap; a single frame rendered
    // during it shows "CPU:0% MEM:0%" — zeros indistinguishable from an idle
    // machine. Wait for real data, bounded, and say plainly if it never arrives
    // rather than printing the defaults as though they were a reading.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut have_snapshot = false;
    while std::time::Instant::now() < deadline {
        if app.sync_snapshot() {
            have_snapshot = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if !have_snapshot {
        eprintln!(
            "warning: no snapshot arrived within 5s. The frame below shows zeroed \
             defaults, not readings — do not interpret them as an idle machine."
        );
    }

    if let Some(requested) = tab {
        // Accept an index or a name, because an agent reading the tab bar has the
        // name and an agent iterating has the index.
        let resolved = requested
            .parse::<usize>()
            .ok()
            .filter(|i| *i < app.tabs.len())
            .or_else(|| {
                app.tabs
                    .iter()
                    .position(|t| t.eq_ignore_ascii_case(requested))
            });

        match resolved {
            Some(index) => app.selected_tab = index,
            None => {
                eprintln!("Unknown tab {requested:?}. Available tabs:");
                for (i, name) in app.tabs.iter().enumerate() {
                    eprintln!("  {i}  {name}");
                }
                std::process::exit(1);
            }
        }
    }

    for line in ironmonlib::tui::headless::render_rows(&app, width, height) {
        println!("{line}");
    }
    Ok(())
}

fn handle_describe_command(
    commands: bool,
    writable: bool,
    domain: Option<&str>,
    search: Option<&str>,
    id: Option<&str>,
    format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::ontology::{Domain, Ontology};

    if commands {
        return emit_command_catalog(format);
    }

    // Narrow to the write surface. Kept separate from the read filters because
    // "what can I change" is a different question from "what can I see", and an
    // agent asking it should not have to scan every entity for a null field.

    let ontology = Ontology::build();

    // Narrow by the most specific filter given.
    let selected: Vec<&ironmonlib::ontology::Entity> = if writable {
        ontology
            .entities
            .values()
            .filter(|e| e.writable_via.is_some())
            .collect()
    } else if let Some(id) = id {
        match ontology.get(id) {
            Some(e) => vec![e],
            None => {
                eprintln!("No entity with id {id:?}.");
                eprintln!(
                    "Try `ironmon describe --search {}`.",
                    id.split('.').next().unwrap_or(id)
                );
                std::process::exit(1);
            }
        }
    } else if let Some(needle) = search {
        ontology.search(needle)
    } else if let Some(d) = domain {
        let Some(parsed) = Domain::parse(d) else {
            eprintln!("Unknown domain {d:?}. Known domains:");
            for known in Domain::ALL {
                eprintln!("  {}", known.as_str());
            }
            std::process::exit(1);
        };
        ontology.in_domain(parsed)
    } else {
        ontology.entities.values().collect()
    };

    if format.eq_ignore_ascii_case("json") {
        // Re-wrap so the JSON always carries the version, even when filtered — a
        // consumer holding a fragment still needs to know which schema it speaks.
        let filtered: std::collections::BTreeMap<&str, &ironmonlib::ontology::Entity> =
            selected.iter().map(|e| (e.id.as_str(), *e)).collect();
        let doc = serde_json::json!({
            "version": ontology.version,
            "entity_count": filtered.len(),
            "entities": filtered,
        });
        println!("{}", serde_json::to_string_pretty(&doc)?);
        return Ok(());
    }

    println!("ironmon ontology v{}", ontology.version);
    println!(
        "{} entit{}\n",
        selected.len(),
        if selected.len() == 1 { "y" } else { "ies" }
    );

    let mut current_domain = None;
    for e in &selected {
        if current_domain != Some(e.domain) {
            println!("== {} ==", e.domain.as_str());
            current_domain = Some(e.domain);
        }
        let unit = e.unit.map(|u| u.as_str()).unwrap_or("-");
        println!(
            "  {:<40} {:<12} {:<10} {:<14}{}",
            e.id,
            e.kind.as_str(),
            unit,
            e.provenance.as_str(),
            if e.nullable { " nullable" } else { "" }
        );
        println!("      {}", e.description);
        if !e.derived_from.is_empty() {
            println!("      derived from: {}", e.derived_from.join(", "));
        }
    }

    println!();
    println!("provenance: measured = sampled now | specification = published constant");
    println!("            derived  = computed     | unavailable   = not obtainable here");
    println!("Only `measured` values may be treated as live observations.");

    Ok(())
}

fn handle_profile_command(action: &ProfileSubcommand) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::profile::{ProfileInspector, Subsystem};
    let mut inspector = ProfileInspector::new();

    match action {
        ProfileSubcommand::List { json } => {
            let snapshot = inspector.snapshot_all();
            if *json {
                let summary: Vec<_> = Subsystem::ALL
                    .iter()
                    .map(|s| {
                        let groups = snapshot.providers.get(s).cloned().unwrap_or_default();
                        serde_json::json!({
                            "subsystem": s.as_str(),
                            "group_count": groups.len(),
                            "setting_count": groups.iter().map(|g| g.settings.len()).sum::<usize>(),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                println!("{}", "Hardware Profile Inspector".bold().cyan());
                println!("Subsystems with readable profile data:\n");
                for sub in Subsystem::ALL {
                    let groups = snapshot.providers.get(sub);
                    let (group_count, setting_count) = match groups {
                        Some(g) => (g.len(), g.iter().map(|x| x.settings.len()).sum::<usize>()),
                        None => (0, 0),
                    };
                    println!(
                        "  {:<10} {} groups, {} settings",
                        sub.as_str().bold(),
                        group_count,
                        setting_count
                    );
                }
                println!();
                println!(
                    "Use {} for details.",
                    "ironmon profile show <subsystem>".italic()
                );
            }
        }
        ProfileSubcommand::Show { subsystem, json } => {
            let sub = Subsystem::parse(subsystem).ok_or_else(|| {
                format!(
                    "Unknown subsystem: {} (valid: gpu, cpu, nvme, display, memory)",
                    subsystem
                )
            })?;
            let groups = inspector.snapshot(sub);
            if *json {
                println!("{}", serde_json::to_string_pretty(&groups)?);
            } else {
                print_profile_groups(sub, &groups);
            }
        }
        ProfileSubcommand::Search { query, json } => {
            let snapshot = inspector.snapshot_all();
            let hits = snapshot.search(query);
            if *json {
                let out: Vec<_> = hits
                    .iter()
                    .map(|(sub, group, setting)| {
                        serde_json::json!({
                            "subsystem": sub.as_str(),
                            "device": &group.device,
                            "profile": &group.display_name,
                            "setting": setting,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!(
                    "{} matches for {:?}:\n",
                    hits.len().to_string().bold(),
                    query
                );
                for (sub, group, setting) in hits {
                    println!(
                        "  [{}] {} / {}",
                        sub.as_str().cyan(),
                        group.device,
                        group.display_name.dimmed()
                    );
                    println!(
                        "      {} = {}{}",
                        setting.display_name.bold(),
                        setting.value,
                        setting
                            .unit
                            .as_deref()
                            .map(|u| format!(" {}", u))
                            .unwrap_or_default()
                    );
                }
            }
        }
        ProfileSubcommand::Set {
            setting_id,
            value,
            confirm,
            json,
        } => {
            let parsed = parse_setting_value(value);
            let outcome = ironmonlib::profile::apply::apply_setting(setting_id, parsed, *confirm);
            if *json {
                println!("{}", serde_json::to_string_pretty(&outcome)?);
            } else {
                let status_color = match outcome.status {
                    ironmonlib::profile::apply::ApplyStatus::Applied => "applied".green(),
                    ironmonlib::profile::apply::ApplyStatus::Refused => "refused".red(),
                    ironmonlib::profile::apply::ApplyStatus::Failed => "failed".red(),
                    ironmonlib::profile::apply::ApplyStatus::NotWritable => "not writable".yellow(),
                    ironmonlib::profile::apply::ApplyStatus::NeedsConfirm => {
                        "needs --confirm".yellow()
                    }
                };
                println!("status:    {}", status_color);
                println!("setting:   {}", outcome.setting_id.bold());
                println!("requested: {}", outcome.requested);
                println!("message:   {}", outcome.message);
                println!(
                    "audit log: {}",
                    ironmonlib::profile::apply::audit_log_path().display()
                );
            }
            // Non-zero exit on non-success so scripts can detect failure.
            if !matches!(
                outcome.status,
                ironmonlib::profile::apply::ApplyStatus::Applied
            ) {
                std::process::exit(1);
            }
        }
        ProfileSubcommand::Schemes { json } => {
            #[cfg(windows)]
            {
                let schemes = ironmonlib::profile::cpu::enumerate_power_schemes();
                if *json {
                    let arr: Vec<_> = schemes
                        .iter()
                        .map(|(g, n)| serde_json::json!({"guid": g, "name": n}))
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&arr)?);
                } else if schemes.is_empty() {
                    println!("(no power schemes enumerable)");
                } else {
                    println!(
                        "{} available Windows power scheme(s):\n",
                        schemes.len().to_string().bold()
                    );
                    for (guid, name) in schemes {
                        println!("  {}  {}", guid.cyan(), name.bold());
                    }
                    println!(
                        "\nApply with: {} {} <guid> --confirm",
                        "ironmon profile set".italic(),
                        "active_scheme_guid".italic()
                    );
                }
            }
            #[cfg(not(windows))]
            {
                let _ = json;
                eprintln!("Power scheme enumeration is only available on Windows.");
                std::process::exit(2);
            }
        }
        ProfileSubcommand::Writable { json } => {
            let ids = ironmonlib::profile::apply::writable_setting_ids();
            if *json {
                println!("{}", serde_json::to_string_pretty(&ids)?);
            } else if ids.is_empty() {
                println!("(no writable handlers registered for this build)");
            } else {
                println!("{} writable setting id(s):", ids.len().to_string().bold());
                for id in ids {
                    println!("  · {}", id);
                }
                println!(
                    "\nUse: {} {} <value> --confirm",
                    "ironmon profile set".italic(),
                    "<id>".italic()
                );
            }
        }
        ProfileSubcommand::Bench { json } => {
            let report = ironmonlib::profile::bench::run_bench();
            if *json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Provider benchmark — total {:.1} ms, {} group(s), {} setting(s)\n",
                    report.total_elapsed_ms, report.total_groups, report.total_settings
                );
                println!(
                    "  {:<10} {:>10} {:>8} {:>10} {:>14}",
                    "subsystem".bold(),
                    "elapsed".bold(),
                    "groups".bold(),
                    "settings".bold(),
                    "settings/ms".bold()
                );
                let mut providers = report.providers.clone();
                providers.sort_by(|a, b| {
                    b.elapsed_ms
                        .partial_cmp(&a.elapsed_ms)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                for p in providers {
                    println!(
                        "  {:<10} {:>8.2} ms {:>8} {:>10} {:>14.2}",
                        p.subsystem.as_str(),
                        p.elapsed_ms,
                        p.group_count,
                        p.setting_count,
                        p.settings_per_ms
                    );
                }
            }
        }
        ProfileSubcommand::Watch {
            interval,
            ticks,
            subsystem,
        } => {
            use ironmonlib::profile::diff::diff_snapshots;
            use ironmonlib::profile::{ProfileSnapshot, Subsystem};
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::Arc;

            let interval_ms = (interval * 1000.0).max(100.0) as u64;
            let running = Arc::new(AtomicBool::new(true));
            let r2 = running.clone();
            let _ = ctrlc::set_handler(move || {
                r2.store(false, Ordering::SeqCst);
            });

            let filter_sub: Option<Subsystem> = match subsystem {
                Some(s) => match Subsystem::parse(s) {
                    Some(parsed) => Some(parsed),
                    None => {
                        eprintln!(
                            "Unknown subsystem {:?}. Valid: gpu, cpu, nvme, display, memory",
                            s
                        );
                        std::process::exit(2);
                    }
                },
                None => None,
            };

            println!(
                "Watching {} subsystem(s) every {}s. Press Ctrl-C to stop.",
                filter_sub
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_else(|| "all".into()),
                interval
            );

            let snapshot =
                |inspector: &mut ironmonlib::profile::ProfileInspector| -> ProfileSnapshot {
                    match filter_sub {
                        Some(sub) => {
                            let groups = inspector.snapshot(sub);
                            let mut providers = std::collections::BTreeMap::new();
                            providers.insert(sub, groups);
                            ProfileSnapshot {
                                timestamp: 0,
                                providers,
                                errors: Default::default(),
                            }
                        }
                        None => inspector.snapshot_all(),
                    }
                };

            let mut prev = snapshot(&mut inspector);
            let mut tick = 0u32;
            while running.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(interval_ms));
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                let cur = snapshot(&mut inspector);
                let d = diff_snapshots(&prev, &cur);
                if !d.is_empty() {
                    let when = chrono::Local::now().format("%H:%M:%S");
                    println!(
                        "\n[{}] {} change(s):",
                        when.to_string().dimmed(),
                        d.total_changes().to_string().bold()
                    );
                    for c in &d.changed {
                        println!(
                            "  ~ [{}] {} :: {}: {} → {}",
                            c.subsystem.as_str().cyan(),
                            c.device,
                            c.display_name,
                            c.before.to_string().red(),
                            c.after.to_string().green()
                        );
                    }
                    for a in &d.added {
                        println!(
                            "  + [{}] {} :: {} = {}",
                            a.subsystem.as_str().cyan(),
                            a.device,
                            a.display_name,
                            a.value.to_string().green()
                        );
                    }
                    for r in &d.removed {
                        println!(
                            "  - [{}] {} :: {} (was {})",
                            r.subsystem.as_str().cyan(),
                            r.device,
                            r.display_name,
                            r.value.to_string().red()
                        );
                    }
                }
                prev = cur;
                tick = tick.saturating_add(1);
                if *ticks > 0 && tick >= *ticks {
                    break;
                }
            }
            println!("\nStopped after {} tick(s).", tick);
        }
        ProfileSubcommand::Audit { lines, json } => {
            let path = ironmonlib::profile::apply::audit_log_path();
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let all: Vec<&str> = content.lines().collect();
            let tail = if all.len() > *lines {
                &all[all.len() - lines..]
            } else {
                &all[..]
            };
            if *json {
                let parsed: Vec<serde_json::Value> = tail
                    .iter()
                    .filter_map(|l| serde_json::from_str(l).ok())
                    .collect();
                println!("{}", serde_json::to_string_pretty(&parsed)?);
            } else {
                println!("Audit log: {}", path.display().to_string().italic());
                if tail.is_empty() {
                    println!("(no entries)");
                }
                for line in tail {
                    if let Ok(v) =
                        serde_json::from_str::<ironmonlib::profile::apply::ApplyOutcome>(line)
                    {
                        let st = match v.status {
                            ironmonlib::profile::apply::ApplyStatus::Applied => "applied".green(),
                            ironmonlib::profile::apply::ApplyStatus::Refused => "refused".red(),
                            ironmonlib::profile::apply::ApplyStatus::Failed => "failed".red(),
                            ironmonlib::profile::apply::ApplyStatus::NotWritable => {
                                "not writable".yellow()
                            }
                            ironmonlib::profile::apply::ApplyStatus::NeedsConfirm => {
                                "needs confirm".yellow()
                            }
                        };
                        println!(
                            "  [{}] {:>12}  {} = {}",
                            v.timestamp.to_string().dimmed(),
                            st,
                            v.setting_id.bold(),
                            v.requested
                        );
                    } else {
                        println!("  {}", line);
                    }
                }
            }
        }
        ProfileSubcommand::Deviations { json } => {
            let snapshot = inspector.snapshot_all();
            let report = ironmonlib::profile::deviation::deviation_report(&snapshot);
            let (devs, coverage) = (report.deviations, report.coverage);
            if *json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "deviations": devs,
                        "coverage": coverage,
                    }))?
                );
            } else if devs.is_empty() {
                // "No settings deviate from their declared defaults" read as a
                // statement about the machine. It was a statement about the two
                // settings out of 23,541 that declare a default at all.
                println!(
                    "None of the {} setting(s) that declare a default differs from it.",
                    coverage.comparable.to_string().bold()
                );
                if coverage.unchecked() > 0 {
                    println!(
                        "{}",
                        format!(
                            "  {} of {} settings declare no default, so this says nothing about them.",
                            coverage.unchecked(),
                            coverage.settings
                        )
                        .dimmed()
                    );
                }
            } else {
                println!(
                    "{} of {} setting(s) that declare a default differ from it:\n",
                    devs.len().to_string().bold(),
                    coverage.comparable
                );
                for d in devs {
                    let risk = match d.risk {
                        ironmonlib::profile::SettingRisk::Dangerous => "[dangerous]".red(),
                        ironmonlib::profile::SettingRisk::Moderate => "[moderate]".yellow(),
                        ironmonlib::profile::SettingRisk::Safe => "[safe]".green(),
                        ironmonlib::profile::SettingRisk::Informational => "[info]".dimmed(),
                    };
                    println!(
                        "  {} [{}] {} :: {}",
                        risk,
                        d.subsystem.as_str().cyan(),
                        d.device,
                        d.display_name.bold()
                    );
                    println!(
                        "      default: {}   current: {}",
                        d.default.to_string().dimmed(),
                        d.current.to_string().yellow()
                    );
                }
                if coverage.unchecked() > 0 {
                    println!();
                    println!(
                        "{}",
                        format!(
                            "  {} of {} settings declare no default and were not checked.",
                            coverage.unchecked(),
                            coverage.settings
                        )
                        .dimmed()
                    );
                }
            }
        }
        ProfileSubcommand::Explain { setting_id, json } => {
            let snapshot = inspector.snapshot_all();
            match ironmonlib::profile::explain::explain(&snapshot, setting_id) {
                Some(exp) => {
                    if *json {
                        println!("{}", serde_json::to_string_pretty(&exp)?);
                    } else {
                        println!("{}", exp.setting.display_name.bold().cyan());
                        println!("  id:        {}", exp.setting.id.italic());
                        println!(
                            "  scope:     [{}] {} / {}",
                            exp.subsystem.as_str(),
                            exp.device,
                            exp.profile.dimmed()
                        );
                        println!(
                            "  value:     {}{}",
                            exp.setting.value.to_string().bold(),
                            exp.setting
                                .unit
                                .as_deref()
                                .map(|u| format!(" {}", u))
                                .unwrap_or_default()
                        );
                        if let Some(default) = &exp.setting.default {
                            println!("  default:   {}", default);
                        }
                        if let Some((min, max)) = &exp.setting.range {
                            println!("  range:     [{}, {}]", min, max);
                        }
                        if let Some(choices) = &exp.setting.choices {
                            let names: Vec<String> =
                                choices.iter().map(|(n, _)| n.clone()).collect();
                            println!("  choices:   {}", names.join(", "));
                        }
                        let risk = match exp.setting.risk {
                            ironmonlib::profile::SettingRisk::Dangerous => "dangerous".red(),
                            ironmonlib::profile::SettingRisk::Moderate => "moderate".yellow(),
                            ironmonlib::profile::SettingRisk::Safe => "safe".green(),
                            ironmonlib::profile::SettingRisk::Informational => "info".dimmed(),
                        };
                        println!("  risk:      {}", risk);
                        println!("  source:    {}", exp.setting.source.italic());
                        if let Some(desc) = &exp.setting.description {
                            println!("\n  {}", desc);
                        }
                        if !exp.related.is_empty() {
                            println!("\n  {}", "Related settings:".bold());
                            for r in &exp.related {
                                println!("    · {:<32} = {}", r.id, r.value.to_string().bold());
                            }
                        }
                    }
                }
                None => {
                    let cands = ironmonlib::profile::explain::candidates(&snapshot, setting_id);
                    if cands.is_empty() {
                        eprintln!("No setting matches id {:?}", setting_id);
                        std::process::exit(1);
                    } else {
                        eprintln!("No exact match for {:?}. Did you mean:", setting_id);
                        for c in cands {
                            eprintln!("  · {}", c);
                        }
                        std::process::exit(1);
                    }
                }
            }
        }
        ProfileSubcommand::Active { matched, json } => {
            let entries = if *matched {
                ironmonlib::profile::active::matched_active_profiles()
            } else {
                ironmonlib::profile::active::active_profiles_for_processes()
            };
            if *json {
                println!("{}", serde_json::to_string_pretty(&entries)?);
            } else {
                let total = entries.len();
                let matched_count = entries.iter().filter(|e| e.has_any_profile()).count();
                println!(
                    "{} of {} processes have a known NVIDIA driver profile\n",
                    matched_count.to_string().bold(),
                    total.to_string().bold()
                );
                for e in entries {
                    let marker = if e.has_nvidia_drs_profile {
                        "✓".green().to_string()
                    } else {
                        "·".dimmed().to_string()
                    };
                    println!(
                        "  {} {:>6}  {}",
                        marker,
                        e.pid.to_string().dimmed(),
                        if e.has_nvidia_drs_profile {
                            e.name.bold().to_string()
                        } else {
                            e.name.dimmed().to_string()
                        }
                    );
                }
            }
        }
        ProfileSubcommand::Diff {
            baseline,
            against,
            json,
        } => {
            use ironmonlib::profile::diff::diff_snapshots;
            use ironmonlib::profile::ProfileSnapshot;
            let baseline_str = std::fs::read_to_string(baseline)?;
            let baseline_snap: ProfileSnapshot = serde_json::from_str(&baseline_str)?;
            let current_snap = if let Some(path) = against {
                let s = std::fs::read_to_string(path)?;
                serde_json::from_str::<ProfileSnapshot>(&s)?
            } else {
                inspector.snapshot_all()
            };
            let d = diff_snapshots(&baseline_snap, &current_snap);
            if *json {
                println!("{}", serde_json::to_string_pretty(&d)?);
            } else if d.is_empty() {
                println!("{}", "No changes between snapshots.".green());
            } else {
                println!(
                    "{} change(s) detected\n",
                    d.total_changes().to_string().bold()
                );
                if !d.added_groups.is_empty() {
                    println!("{}", "Added groups:".green().bold());
                    for g in &d.added_groups {
                        println!(
                            "  + [{}] {} / {} ({} settings)",
                            g.subsystem.as_str().cyan(),
                            g.device,
                            g.profile.dimmed(),
                            g.setting_count
                        );
                    }
                    println!();
                }
                if !d.removed_groups.is_empty() {
                    println!("{}", "Removed groups:".red().bold());
                    for g in &d.removed_groups {
                        println!(
                            "  - [{}] {} / {} ({} settings)",
                            g.subsystem.as_str().cyan(),
                            g.device,
                            g.profile.dimmed(),
                            g.setting_count
                        );
                    }
                    println!();
                }
                if !d.changed.is_empty() {
                    println!("{}", "Changed:".yellow().bold());
                    for c in &d.changed {
                        println!(
                            "  ~ [{}] {} / {} :: {}",
                            c.subsystem.as_str().cyan(),
                            c.device,
                            c.profile.dimmed(),
                            c.display_name.bold()
                        );
                        println!(
                            "      {} → {}",
                            c.before.to_string().red(),
                            c.after.to_string().green()
                        );
                    }
                    println!();
                }
                if !d.added.is_empty() {
                    println!("{}", "Added settings:".green().bold());
                    for a in &d.added {
                        println!(
                            "  + [{}] {} / {} :: {} = {}",
                            a.subsystem.as_str().cyan(),
                            a.device,
                            a.profile.dimmed(),
                            a.display_name,
                            a.value.to_string().bold()
                        );
                    }
                    println!();
                }
                if !d.removed.is_empty() {
                    println!("{}", "Removed settings:".red().bold());
                    for r in &d.removed {
                        println!(
                            "  - [{}] {} / {} :: {} (was {})",
                            r.subsystem.as_str().cyan(),
                            r.device,
                            r.profile.dimmed(),
                            r.display_name,
                            r.value
                        );
                    }
                }
            }
        }
        ProfileSubcommand::Dump { json, output } => {
            let snapshot = inspector.snapshot_all();
            if *json {
                let serialized = serde_json::to_string_pretty(&snapshot)?;
                if let Some(path) = output {
                    std::fs::write(path, serialized)?;
                    println!("Wrote profile snapshot to {}", path.display());
                } else {
                    println!("{}", serialized);
                }
            } else {
                let mut sink = String::new();
                use std::fmt::Write;
                writeln!(
                    sink,
                    "Hardware Profile Snapshot ({} groups, {} settings)\n",
                    snapshot.total_groups(),
                    snapshot.total_settings()
                )?;
                for (sub, groups) in &snapshot.providers {
                    writeln!(sink, "=== {} ===", sub.as_str().to_uppercase())?;
                    for group in groups {
                        writeln!(sink, "\n[{}] {}", group.device, group.display_name)?;
                        writeln!(sink, "  source: {}", group.source)?;
                        for s in &group.settings {
                            writeln!(
                                sink,
                                "  {:<32} = {}{}",
                                s.id,
                                s.value,
                                s.unit
                                    .as_deref()
                                    .map(|u| format!(" {}", u))
                                    .unwrap_or_default()
                            )?;
                        }
                        for n in &group.notes {
                            writeln!(sink, "  note: {}", n)?;
                        }
                    }
                    writeln!(sink)?;
                }
                if let Some(path) = output {
                    std::fs::write(path, &sink)?;
                    println!("Wrote profile snapshot to {}", path.display());
                } else {
                    print!("{}", sink);
                }
            }
        }
    }
    Ok(())
}

/// Pretty-print profile groups for a subsystem.
#[cfg(feature = "cli")]
fn print_profile_groups(
    sub: ironmonlib::profile::Subsystem,
    groups: &[ironmonlib::profile::ProfileGroup],
) {
    if groups.is_empty() {
        println!("(no profile data for {})", sub);
        return;
    }
    println!("{}", format!("== {} ==", sub).bold().cyan());
    for group in groups {
        println!();
        println!("{} {}", "▸".cyan(), group.device.bold(),);
        println!(
            "  {} {}   {} {}",
            "profile:".dimmed(),
            group.display_name,
            "source:".dimmed(),
            group.source.italic()
        );
        for s in &group.settings {
            let unit = s
                .unit
                .as_deref()
                .map(|u| format!(" {}", u))
                .unwrap_or_default();
            let risk = match s.risk {
                ironmonlib::profile::SettingRisk::Informational => String::new(),
                ironmonlib::profile::SettingRisk::Safe => " [safe]".green().to_string(),
                ironmonlib::profile::SettingRisk::Moderate => " [moderate]".yellow().to_string(),
                ironmonlib::profile::SettingRisk::Dangerous => " [dangerous]".red().to_string(),
            };
            println!(
                "    {:<28} = {}{}{}",
                s.display_name,
                s.value.to_string().bold(),
                unit,
                risk,
            );
        }
        for n in &group.notes {
            println!("    {} {}", "•".dimmed(), n.dimmed());
        }
    }
}

/// Parse a CLI string into a [`SettingValue`]. Recognizes bool literals,
/// integers, and floats; otherwise falls back to [`SettingValue::Text`].
#[cfg(feature = "cli")]
fn parse_setting_value(raw: &str) -> ironmonlib::profile::SettingValue {
    use ironmonlib::profile::SettingValue;
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "true" | "yes" | "on" | "enabled" | "enable" => return SettingValue::Bool(true),
        "false" | "no" | "off" | "disabled" | "disable" => return SettingValue::Bool(false),
        _ => {}
    }
    if let Ok(i) = raw.parse::<i64>() {
        return SettingValue::Int(i);
    }
    if let Ok(u) = raw.parse::<u64>() {
        return SettingValue::Uint(u);
    }
    if let Ok(f) = raw.parse::<f64>() {
        return SettingValue::Float(f);
    }
    SettingValue::Text(raw.to_string())
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("CLI features not enabled. Please compile with --features cli");
    std::process::exit(1);
}

/// Run one intrusion-detection pass against the recorded baseline.
///
/// The baseline is a file the caller names, holding both halves — listeners and
/// watched files — so one path is the whole "what this machine looked like"
/// record. Recorded when absent, compared when present.
///
/// Exit code is 0 for a clean or first run and 1 when anything was found, so a
/// scheduler can act on it without parsing. A failure to scan exits 2: "could
/// not look" must not be mistaken for "nothing found", which is the distinction
/// this whole module is built on.
fn handle_ids_command(
    baseline_path: &std::path::Path,
    watch: &[std::path::PathBuf],
    format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::ids::{file, network, triage, ScanStatus};

    #[derive(serde::Serialize, serde::Deserialize, Default)]
    struct Baselines {
        #[serde(default)]
        network: network::Baseline,
        #[serde(default)]
        files: file::Baseline,
    }

    let existing: Baselines = match std::fs::read_to_string(baseline_path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            format!(
                "baseline at {} is not readable: {e}",
                baseline_path.display()
            )
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Baselines::default(),
        Err(e) => return Err(format!("baseline at {}: {e}", baseline_path.display()).into()),
    };

    let net = network::scan(&existing.network);
    let files = file::scan(watch, &existing.files);

    // Record whatever is missing, so the next run has something to compare
    // against. Writing the baseline is the one thing this command does to the
    // machine, and it writes only where the caller pointed it.
    let record_needed = matches!(net, ScanStatus::NoBaseline { .. })
        || matches!(files, ScanStatus::NoBaseline { .. });
    if record_needed {
        let fresh = Baselines {
            network: if matches!(net, ScanStatus::NoBaseline { .. }) {
                network::record().unwrap_or_default()
            } else {
                existing.network.clone()
            },
            files: if matches!(files, ScanStatus::NoBaseline { .. }) {
                file::record(watch)
            } else {
                existing.files.clone()
            },
        };
        if let Some(parent) = baseline_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(baseline_path, serde_json::to_string_pretty(&fresh)?)?;
    }

    let mut findings = Vec::new();
    findings.extend(net.findings().iter().cloned());
    findings.extend(files.findings().iter().cloned());
    let ranked = triage::rank(findings.clone());
    let counts = triage::counts(&findings);

    if format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "network": net,
                "files": files,
                "ranked": ranked,
                "counts": counts,
            }))?
        );
    } else {
        print_ids_status("network", &net);
        print_ids_status("files", &files);
        if !ranked.ranked.is_empty() {
            println!();
            for f in &ranked.ranked {
                println!(
                    "[{:?}/{:?}] {} — {}",
                    f.severity, f.confidence, f.rule, f.title
                );
                for e in &f.evidence {
                    match &e.expected {
                        Some(exp) => println!("    {} = {} (was {})", e.kind, e.observed, exp),
                        None => println!("    {} = {}", e.kind, e.observed),
                    }
                }
            }
            println!();
            println!(
                "{} finding(s); {} rest on a heuristic rather than a direct observation",
                findings.len(),
                counts.possible_only
            );
        }
    }

    if matches!(net, ScanStatus::Failed { .. }) || matches!(files, ScanStatus::Failed { .. }) {
        std::process::exit(2);
    }
    if !findings.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

fn print_ids_status(label: &str, status: &ironmonlib::ids::ScanStatus) {
    use ironmonlib::ids::ScanStatus;
    match status {
        ScanStatus::NoBaseline { recorded, reason } => {
            println!("{label}: no baseline — {recorded} recorded now. {reason}");
        }
        ScanStatus::Clean { checked } => {
            println!("{label}: clean over {checked} checked");
        }
        ScanStatus::Findings { checked, findings } => {
            println!(
                "{label}: {} finding(s) over {checked} checked",
                findings.len()
            );
        }
        ScanStatus::Failed { reason } => {
            println!("{label}: scan failed — {reason}");
        }
    }
}

/// Render the one-screen summary.
fn handle_status_command(
    ascii: bool,
    no_color: bool,
    format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let readings = ironmonlib::ontology::resolve::snapshot();

    if format.eq_ignore_ascii_case("json") {
        // The same lines, structured. `value` absent and `reason` present is the
        // same distinction the text form draws, kept for a caller that scripts it.
        let lines: Vec<serde_json::Value> = ironmonlib::fetch::summary(&readings)
            .into_iter()
            .map(|l| {
                serde_json::json!({
                    "label": l.label,
                    "value": l.value,
                    "reason": l.reason,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!(lines))?
        );
    } else if ascii {
        // Colour follows the terminal unless the caller says otherwise. Piped
        // output carries no escape codes, because this gets pasted as often as
        // it gets read.
        let colour = !no_color && std::io::IsTerminal::is_terminal(&std::io::stdout());
        print!("{}", ironmonlib::fetch::render(&readings, colour));
    } else {
        // Terse by default: the summary without the artwork.
        let colour = !no_color && std::io::IsTerminal::is_terminal(&std::io::stdout());
        print!("{}", ironmonlib::fetch::render_plain(&readings, colour));
    }
    Ok(())
}
