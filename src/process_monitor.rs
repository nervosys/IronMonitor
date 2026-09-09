//! Unified Process Monitoring with GPU Attribution
//!
//! This module provides cross-platform process monitoring with GPU usage attribution.
//! It combines system-wide process enumeration with GPU-specific process tracking from
//! NVIDIA NVML, AMD sysfs, and Intel GPU drivers.
//!
//! The [`ProcessMonitor`] correlates system processes with GPU usage by matching process IDs
//! (PIDs) from GPU driver data with information from `/proc` (Linux), task manager (Windows),
//! or similar platform-specific sources.
//!
//! # Examples
//!
//! ## Basic Process Monitoring
//!
//! ```no_run
//! use ironmonlib::{ProcessMonitor, GpuCollection};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create monitor with GPU attribution
//! let gpus = GpuCollection::auto_detect()?;
//! let mut monitor = ProcessMonitor::with_gpus(gpus)?;
//!
//! // Get all processes
//! let processes = monitor.processes()?;
//! println!("Total processes: {}", processes.len());
//!
//! // Get GPU processes only
//! let gpu_processes = monitor.gpu_processes()?;
//! println!("GPU processes: {}", gpu_processes.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Top GPU Consumers
//!
//! ```no_run
//! use ironmonlib::{ProcessMonitor, GpuCollection};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let gpus = GpuCollection::auto_detect()?;
//! let mut monitor = ProcessMonitor::with_gpus(gpus)?;
//!
//! // Get top 10 processes by GPU memory usage
//! let top_gpu = monitor.processes_by_gpu_memory()?;
//! println!("Top GPU consumers:");
//! for proc in top_gpu.iter().take(10) {
//!     println!("  {} (PID {}): {} MB on {} GPUs",
//!         proc.name,
//!         proc.pid,
//!         proc.gpu_memory_mb()
//!             .map_or_else(|| "n/a".to_string(), |mb| format!("{mb:.0}")),
//!         proc.gpu_indices.len()
//!     );
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Top CPU Consumers
//!
//! ```no_run
//! use ironmonlib::{ProcessMonitor, GpuCollection};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let gpus = GpuCollection::auto_detect()?;
//! let mut monitor = ProcessMonitor::with_gpus(gpus)?;
//!
//! // Get top 10 processes by CPU usage
//! let top_cpu = monitor.processes_by_cpu()?;
//! println!("Top CPU consumers:");
//! for proc in top_cpu.iter().take(10) {
//!     println!("  {} (PID {}): {:.1}%",
//!         proc.name,
//!         proc.pid,
//!         proc.cpu_percent
//!     );
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Monitor Specific Process
//!
//! ```no_run
//! use ironmonlib::{ProcessMonitor, GpuCollection};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let gpus = GpuCollection::auto_detect()?;
//! let mut monitor = ProcessMonitor::with_gpus(gpus)?;
//!
//! // Get specific process by PID
//! if let Some(proc) = monitor.process_by_pid(1234)? {
//!     println!("Process: {}", proc.name);
//!     println!("CPU: {:.1}%", proc.cpu_percent);
//!     println!("Memory: {} MB", proc.memory_bytes / 1024 / 1024);
//!     match proc.gpu_memory_mb() {
//!         Some(mb) => println!("GPU Memory: {mb:.0} MB"),
//!         None => println!("GPU Memory: not reported by the driver"),
//!     }
//!     println!("Using GPUs: {:?}", proc.gpu_indices);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Platform Support
//!
//! | Platform | Process Enum | GPU Attribution | CPU % | Memory | User |
//! |----------|--------------|-----------------|-------|--------|------|
//! | Linux    | ✅ /proc      | ✅ All vendors  | ✅    | ✅     | ✅   |
//! | Windows  | ✅ Win32 API  | 🚧              | ✅    | ✅     | ✅   |
//! | macOS    | ✅ libproc    | 🚧              | ✅    | ✅     | 🚧   |

use crate::error::{IronError, Result};
use crate::gpu::GpuCollection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Process category for smart classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ProcessCategory {
    /// System kernel and core OS processes
    System,
    /// System services and daemons
    Service,
    /// Desktop environment and window managers
    Desktop,
    /// Web browsers
    Browser,
    /// Development tools (IDEs, compilers, debuggers)
    Development,
    /// AI/ML frameworks and tools
    AiMl,
    /// Games and game engines
    Gaming,
    /// Media players and editors
    Media,
    /// Communication apps (chat, video calls)
    Communication,
    /// Productivity apps (office, notes)
    Productivity,
    /// Containers and virtualization
    Container,
    /// Network and security tools
    Network,
    /// Database servers
    Database,
    /// Holds a GPU context, and nothing else identifies it.
    ///
    /// Not a claim that the process is computing on the GPU: on Windows every
    /// windowed application holds one, and NVML reports `C+G` for all of them.
    GpuCompute,
    /// Shell and terminal processes
    Shell,
    /// User applications (uncategorized)
    Application,
    /// Unknown/unclassified processes
    #[default]
    Unknown,
}

impl ProcessCategory {
    /// Classify a process by its name and characteristics
    pub fn classify(name: &str, user: Option<&str>, is_gpu_process: bool) -> Self {
        let name_lower = name.to_lowercase();

        // A process holding a GPU context is not thereby a GPU workload.
        //
        // This branch used to run first and end with `return Self::GpuCompute`
        // for anything not matching an AI/ML or game name, so every windowed
        // application on Windows landed there: `ironmon cli processes` reported
        // 24 processes under "GPU Compute", the top five being
        // WindowsTerminal.exe, brave.exe, Code.exe and a Logitech settings
        // agent. `nvidia-smi` labels all of them `C+G` with `[N/A]` memory —
        // they are drawing windows.
        //
        // The AI/ML and game checks stay here, because a GPU context plus one
        // of those names is real evidence. Everything else falls through to be
        // classified by what it is, so a browser is a browser.
        if is_gpu_process {
            // AI/ML specific
            if Self::matches_any(
                &name_lower,
                &[
                    "python",
                    "python3",
                    "jupyter",
                    "conda",
                    "pytorch",
                    "tensorflow",
                    "torch",
                    "ollama",
                    "llama",
                    "whisper",
                    "stable-diffusion",
                    "comfyui",
                    "automatic1111",
                    "onnx",
                    "triton",
                    "vllm",
                    "tgi",
                ],
            ) {
                return Self::AiMl;
            }
            // Games
            if Self::matches_any(
                &name_lower,
                &[
                    "steam", "game", "unity", "unreal", "godot", "wine", "proton", "lutris",
                    "heroic", "bottles",
                ],
            ) {
                return Self::Gaming;
            }
            // No `return Self::GpuCompute` here. Fall through.
        }

        // System processes (PID 1, kernel threads, etc.)
        if Self::matches_any(
            &name_lower,
            &[
                "init",
                "systemd",
                "kernel",
                "kthread",
                "ksoftirq",
                "kworker",
                "rcu_",
                "migration",
                "watchdog",
                "cpuhp",
                "idle",
                "swapper",
                "launchd",
                "system",
                "csrss",
                "smss",
                "wininit",
                "services",
                "lsass",
                "svchost",
                "dwm",
                "ntoskrnl",
            ],
        ) {
            return Self::System;
        }

        // Services and daemons
        if Self::matches_any(
            &name_lower,
            &[
                "cron",
                "crond",
                "atd",
                "cupsd",
                "avahi",
                "dbus",
                "udev",
                "polkit",
                "udisks",
                "accounts-daemon",
                "colord",
                "fwupd",
                "gdm",
                "lightdm",
                "sddm",
                "login",
                "getty",
                "agetty",
                "su",
                "sudo",
                "ssh",
                "sshd",
                "rsyslog",
                "journald",
                "logind",
                "networkmanager",
                "wpa_supplicant",
                "dhclient",
                "thermald",
                "irqbalance",
                "snapd",
                "flatpak",
                "packagekit",
                "apt",
                "dnf",
                "yum",
                "pacman",
                "zypper",
            ],
        ) {
            return Self::Service;
        }

        // Desktop environments
        if Self::matches_any(
            &name_lower,
            &[
                "gnome",
                "kde",
                "plasma",
                "xfce",
                "mate",
                "cinnamon",
                "lxde",
                "lxqt",
                "i3",
                "sway",
                "hyprland",
                "awesome",
                "bspwm",
                "dwm",
                "openbox",
                "fluxbox",
                "xorg",
                "x11",
                "wayland",
                "mutter",
                "kwin",
                "picom",
                "compton",
                "compositor",
                "nautilus",
                "dolphin",
                "thunar",
                "nemo",
                "caja",
                "pcmanfm",
                "explorer",
                "finder",
                "gvfs",
                "tracker",
                "baloo",
                "mimeapps",
                "xdg-",
            ],
        ) {
            return Self::Desktop;
        }

        // Browsers
        if Self::matches_any(
            &name_lower,
            &[
                "firefox",
                "chrome",
                "chromium",
                "brave",
                "edge",
                "safari",
                "opera",
                "vivaldi",
                "librewolf",
                "waterfox",
                "tor-browser",
                "qutebrowser",
                "web-content",
                "webextension",
                "gpu-process",
            ],
        ) {
            return Self::Browser;
        }

        // Development tools
        if Self::matches_any(
            &name_lower,
            &[
                "code",
                "vscode",
                "codium",
                "vim",
                "nvim",
                "neovim",
                "emacs",
                "sublime",
                "atom",
                "jetbrains",
                "idea",
                "pycharm",
                "webstorm",
                "clion",
                "rider",
                "goland",
                "rust-analyzer",
                "gopls",
                "clangd",
                "pylsp",
                "tsserver",
                "node",
                "npm",
                "yarn",
                "pnpm",
                "cargo",
                "rustc",
                "gcc",
                "g++",
                "clang",
                "make",
                "cmake",
                "ninja",
                "git",
                "gh",
                "gdb",
                "lldb",
                "valgrind",
                "strace",
                "ltrace",
                "perf",
                "htop",
                "btop",
                "top",
                "docker-compose",
                "kubectl",
            ],
        ) {
            return Self::Development;
        }

        // AI/ML tools (non-GPU or pre-GPU detection)
        if Self::matches_any(
            &name_lower,
            &[
                "python", "python3", "jupyter", "ipython", "conda", "pip", "poetry", "pdm", "uv",
                "ruff", "mypy",
            ],
        ) {
            return Self::AiMl;
        }

        // Media
        if Self::matches_any(
            &name_lower,
            &[
                "vlc",
                "mpv",
                "mplayer",
                "totem",
                "celluloid",
                "parole",
                "rhythmbox",
                "spotify",
                "audacious",
                "clementine",
                "lollypop",
                "gimp",
                "inkscape",
                "krita",
                "blender",
                "kdenlive",
                "shotcut",
                "obs",
                "ffmpeg",
                "handbrake",
                "audacity",
                "ardour",
                "lmms",
                "darktable",
                "rawtherapee",
                "digikam",
                "shotwell",
                "eog",
                "gwenview",
                "feh",
                "sxiv",
                "mpd",
                "pulseaudio",
                "pipewire",
                "wireplumber",
                "alsa",
                "jack",
            ],
        ) {
            return Self::Media;
        }

        // Communication
        if Self::matches_any(
            &name_lower,
            &[
                "discord",
                "slack",
                "teams",
                "zoom",
                "skype",
                "telegram",
                "signal",
                "element",
                "matrix",
                "thunderbird",
                "evolution",
                "geary",
                "mutt",
                "neomutt",
                "weechat",
                "irssi",
                "hexchat",
            ],
        ) {
            return Self::Communication;
        }

        // Productivity
        if Self::matches_any(
            &name_lower,
            &[
                "libreoffice",
                "soffice",
                "writer",
                "calc",
                "impress",
                "obsidian",
                "notion",
                "joplin",
                "simplenote",
                "standard-notes",
                "zettlr",
                "logseq",
                "roam",
                "okular",
                "evince",
                "zathura",
                "calibre",
                "foliate",
                "gnome-calendar",
                "gnome-contacts",
            ],
        ) {
            return Self::Productivity;
        }

        // Containers & Virtualization
        if Self::matches_any(
            &name_lower,
            &[
                "docker",
                "containerd",
                "runc",
                "cri-o",
                "podman",
                "buildah",
                "skopeo",
                "kubernetes",
                "kubelet",
                "k3s",
                "k8s",
                "minikube",
                "qemu",
                "kvm",
                "libvirt",
                "virt-manager",
                "virtualbox",
                "vmware",
                "vagrant",
                "lxc",
                "lxd",
                "incus",
                "systemd-nspawn",
            ],
        ) {
            return Self::Container;
        }

        // Network & Security
        if Self::matches_any(
            &name_lower,
            &[
                "nginx",
                "apache",
                "httpd",
                "caddy",
                "traefik",
                "haproxy",
                "squid",
                "dnsmasq",
                "bind",
                "named",
                "unbound",
                "pihole",
                "openvpn",
                "wireguard",
                "iptables",
                "nftables",
                "firewalld",
                "ufw",
                "fail2ban",
                "snort",
                "suricata",
                "wireshark",
                "tcpdump",
                "nmap",
                "curl",
                "wget",
                "rsync",
                "syncthing",
                "rclone",
            ],
        ) {
            return Self::Network;
        }

        // Databases
        if Self::matches_any(
            &name_lower,
            &[
                "postgres",
                "postgresql",
                "mysql",
                "mariadb",
                "sqlite",
                "mongodb",
                "redis",
                "memcached",
                "elasticsearch",
                "opensearch",
                "cassandra",
                "couchdb",
                "influxdb",
                "clickhouse",
                "duckdb",
            ],
        ) {
            return Self::Database;
        }

        // Gaming (non-GPU or waiting for GPU)
        if Self::matches_any(
            &name_lower,
            &[
                "steam",
                "steamwebhelper",
                "game",
                "unity",
                "unreal",
                "godot",
                "wine",
                "proton",
                "lutris",
                "heroic",
                "bottles",
                "gamescope",
                "mangohud",
                "gamemode",
            ],
        ) {
            return Self::Gaming;
        }

        // Shells
        if Self::matches_any(
            &name_lower,
            &[
                "bash",
                "zsh",
                "fish",
                "sh",
                "dash",
                "ksh",
                "tcsh",
                "csh",
                "powershell",
                "pwsh",
                "cmd",
                "terminal",
                "konsole",
                "gnome-terminal",
                "alacritty",
                "kitty",
                "wezterm",
                "foot",
                "tilix",
                "terminator",
                "tmux",
                "screen",
                "byobu",
            ],
        ) {
            return Self::Shell;
        }

        // Check user context - root/system users often run services
        if let Some(u) = user {
            let u_lower = u.to_lowercase();
            if u_lower == "root" || u_lower == "system" || u_lower.starts_with("_") {
                return Self::Service;
            }
        }

        // A GPU-holding process that matches nothing by name is still worth
        // separating from the general Unknown pile — but only after every
        // name-based category has had its turn, so a browser is a browser.
        if is_gpu_process {
            return Self::GpuCompute;
        }

        Self::Unknown
    }

    /// Check if name matches any pattern (substring match)
    fn matches_any(name: &str, patterns: &[&str]) -> bool {
        patterns.iter().any(|p| name.contains(p))
    }

    /// Get display name for the category
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Service => "Services",
            Self::Desktop => "Desktop",
            Self::Browser => "Browsers",
            Self::Development => "Development",
            Self::AiMl => "AI/ML",
            Self::Gaming => "Gaming",
            Self::Media => "Media",
            Self::Communication => "Communication",
            Self::Productivity => "Productivity",
            Self::Container => "Containers",
            Self::Network => "Network",
            Self::Database => "Database",
            Self::GpuCompute => "Using GPU",
            Self::Shell => "Shell",
            Self::Application => "Applications",
            Self::Unknown => "Other",
        }
    }

    /// Get an emoji/icon for the category
    pub fn icon(&self) -> &'static str {
        match self {
            Self::System => "⚙️",
            Self::Service => "🔧",
            Self::Desktop => "🖥️",
            Self::Browser => "🌐",
            Self::Development => "💻",
            Self::AiMl => "🤖",
            Self::Gaming => "🎮",
            Self::Media => "🎵",
            Self::Communication => "💬",
            Self::Productivity => "📝",
            Self::Container => "📦",
            Self::Network => "🌍",
            Self::Database => "🗄️",
            Self::GpuCompute => "🔥",
            Self::Shell => "🐚",
            Self::Application => "📱",
            Self::Unknown => "❓",
        }
    }

    /// Get all categories in display order
    pub fn all() -> &'static [ProcessCategory] {
        &[
            Self::AiMl,
            Self::GpuCompute,
            Self::Gaming,
            Self::Browser,
            Self::Development,
            Self::Media,
            Self::Communication,
            Self::Productivity,
            Self::Container,
            Self::Database,
            Self::Network,
            Self::Desktop,
            Self::Service,
            Self::System,
            Self::Shell,
            Self::Application,
            Self::Unknown,
        ]
    }
}

impl std::fmt::Display for ProcessCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// GPU process type classification for process monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessGpuType {
    /// Process uses graphics rendering (3D, OpenGL, Vulkan, DirectX)
    Graphical,
    /// Process uses compute workloads (CUDA, OpenCL, SYCL)
    Compute,
    /// Process uses both graphics and compute
    GraphicalCompute,
    /// Unknown or unable to determine
    Unknown,
}

impl ProcessGpuType {
    /// Create from engine usage pattern
    pub fn from_engine_usage(gfx: u64, compute: u64) -> Self {
        match (gfx > 0, compute > 0) {
            (true, true) => Self::GraphicalCompute,
            (true, false) => Self::Graphical,
            (false, true) => Self::Compute,
            (false, false) => Self::Unknown,
        }
    }
}

impl std::fmt::Display for ProcessGpuType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Graphical => write!(f, "Graphics"),
            Self::Compute => write!(f, "Compute"),
            Self::GraphicalCompute => write!(f, "Gfx+Compute"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Unified process information with GPU attribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMonitorInfo {
    /// Process ID
    pub pid: u32,
    /// Parent Process ID
    pub parent_pid: Option<u32>,
    /// Process name/command
    pub name: String,
    /// User running the process
    pub user: Option<String>,
    /// Process category (smart classification)
    pub category: ProcessCategory,
    /// CPU usage percentage (delta-based between samples; first sample uses lifetime average)
    pub cpu_percent: f32,
    /// Memory usage in bytes (Working Set / RSS)
    pub memory_bytes: u64,
    /// Virtual memory size in bytes
    pub virtual_memory_bytes: u64,
    /// Private memory in bytes (unique to this process)
    pub private_bytes: u64,
    /// Number of threads
    pub thread_count: u32,
    /// Number of open handles (Windows) or file descriptors (Unix)
    pub handle_count: u32,
    /// I/O read bytes (total since process start)
    pub io_read_bytes: u64,
    /// I/O write bytes (total since process start)
    pub io_write_bytes: u64,
    /// Process start time (Unix timestamp in seconds)
    pub start_time: Option<u64>,
    /// GPU indices this process is using
    pub gpu_indices: Vec<usize>,
    /// GPU memory usage per GPU (GPU index -> memory in bytes).
    ///
    /// A device that reports no figure for this process has no entry, rather
    /// than an entry of zero.
    pub gpu_memory_per_device: HashMap<usize, u64>,
    /// Total GPU memory across every device that reported one, or `None` when
    /// no device did.
    ///
    /// This was `u64`, and the attribution loop wrote
    /// `gpu_proc.memory_usage.unwrap_or(0)` into it. NVML does not expose
    /// per-process GPU memory under Windows' WDDM driver model — `nvidia-smi`
    /// prints `[N/A]` for every process on this desktop — so every
    /// GPU-attributed process reported 0.0 MB as though measured.
    /// `observability/api.rs` already had to guard `> 0` to undo it.
    pub total_gpu_memory_bytes: Option<u64>,
    /// Process state (R=Running, S=Sleeping, D=Disk sleep, Z=Zombie, T=Stopped,
    /// U=Unknown).
    ///
    /// `U` is not a Linux state code: it means this platform did not tell us. Windows
    /// has no per-process state — scheduling state lives on threads — and the
    /// enumeration here used to report `'R'` for every process, which made the UI
    /// claim all 420 processes on an idle desktop were running and none were asleep.
    pub state: char,
    /// Process priority/nice value
    pub priority: Option<i32>,

    // nvtop feature parity: Per-process engine utilization
    /// GPU graphics engine time used (nanoseconds)
    pub gfx_engine_used: Option<u64>,
    /// GPU compute engine time used (nanoseconds)
    pub compute_engine_used: Option<u64>,
    /// GPU encoder time used (nanoseconds)
    pub enc_engine_used: Option<u64>,
    /// GPU decoder time used (nanoseconds)
    pub dec_engine_used: Option<u64>,
    /// GPU usage percentage (0-100)
    pub gpu_usage_percent: Option<f32>,
    /// Encoder usage percentage (0-100)
    pub encoder_usage_percent: Option<f32>,
    /// Decoder usage percentage (0-100)
    pub decoder_usage_percent: Option<f32>,
    /// GPU process type (Graphics, Compute, Mixed)
    pub gpu_process_type: ProcessGpuType,
    /// GPU memory percentage of total device memory
    pub gpu_memory_percentage: Option<f32>,
    /// Cumulative CPU time in microseconds (used for delta-based CPU% calculation)
    #[serde(skip)]
    pub cpu_time_us: u64,
}

impl ProcessMonitorInfo {
    /// Get total CPU usage percentage
    pub fn cpu_usage(&self) -> f32 {
        self.cpu_percent
    }

    /// Get memory usage in megabytes
    pub fn memory_mb(&self) -> f64 {
        self.memory_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Total GPU memory in megabytes, or `None` where no device reported it.
    pub fn gpu_memory_mb(&self) -> Option<f64> {
        self.total_gpu_memory_bytes
            .map(|b| b as f64 / (1024.0 * 1024.0))
    }

    /// Check if process is using any GPU
    pub fn is_gpu_process(&self) -> bool {
        !self.gpu_indices.is_empty()
    }

    /// Get number of GPUs used by this process
    pub fn gpu_count(&self) -> usize {
        self.gpu_indices.len()
    }

    /// Get the process category
    pub fn category(&self) -> ProcessCategory {
        self.category
    }

    /// Reclassify the process (e.g., after GPU attribution is added)
    pub fn reclassify(&mut self) {
        self.category =
            ProcessCategory::classify(&self.name, self.user.as_deref(), self.is_gpu_process());
    }
}

/// Statistics for a process category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStats {
    /// The category
    pub category: ProcessCategory,
    /// Number of processes in this category
    pub process_count: usize,
    /// Number of GPU-using processes in this category
    pub gpu_process_count: usize,
    /// Total CPU usage percentage across all processes
    pub total_cpu_percent: f32,
    /// Total memory usage in bytes
    pub total_memory_bytes: u64,
    /// Total GPU memory over the processes in this category that reported
    /// one, or `None` when none of them did. See
    /// [`ProcessMonitorInfo::total_gpu_memory_bytes`].
    pub total_gpu_memory_bytes: Option<u64>,
}

impl CategoryStats {
    /// Get memory usage in MB
    pub fn memory_mb(&self) -> f64 {
        self.total_memory_bytes as f64 / (1024.0 * 1024.0)
    }

    /// GPU memory in MB over the processes that reported one, or `None`.
    pub fn gpu_memory_mb(&self) -> Option<f64> {
        self.total_gpu_memory_bytes
            .map(|b| b as f64 / (1024.0 * 1024.0))
    }
}

/// Process monitor that combines system and GPU process information
pub struct ProcessMonitor {
    /// GPU collection for GPU process tracking
    gpu_collection: Option<GpuCollection>,
    /// Cache of last update time (for CPU percentage calculation)
    last_update: std::time::Instant,
    /// Previous per-process CPU times for delta-based CPU% calculation
    /// Maps PID -> (cumulative_cpu_time_us, wall_clock_instant)
    prev_cpu_times: HashMap<u32, (u64, std::time::Instant)>,
    /// Number of logical CPUs for normalizing per-process CPU%
    num_logical_cpus: f32,
}

impl ProcessMonitor {
    /// Create a new process monitor
    ///
    /// Automatically detects available GPUs for GPU process attribution.
    pub fn new() -> Result<Self> {
        let gpu_collection = GpuCollection::auto_detect().ok();
        let num_logical_cpus = std::thread::available_parallelism()
            .map(|n| n.get() as f32)
            .unwrap_or(1.0);

        Ok(Self {
            gpu_collection,
            last_update: std::time::Instant::now(),
            prev_cpu_times: HashMap::new(),
            num_logical_cpus,
        })
    }

    /// Create a process monitor with a pre-initialized GPU collection
    ///
    /// This is useful when you already have a [`GpuCollection`] instance
    /// and want to reuse it for process monitoring.
    pub fn with_gpus(gpu_collection: GpuCollection) -> Result<Self> {
        let num_logical_cpus = std::thread::available_parallelism()
            .map(|n| n.get() as f32)
            .unwrap_or(1.0);

        Ok(Self {
            gpu_collection: Some(gpu_collection),
            last_update: std::time::Instant::now(),
            prev_cpu_times: HashMap::new(),
            num_logical_cpus,
        })
    }

    /// Create a process monitor without GPU tracking
    pub fn without_gpu() -> Result<Self> {
        let num_logical_cpus = std::thread::available_parallelism()
            .map(|n| n.get() as f32)
            .unwrap_or(1.0);

        Ok(Self {
            gpu_collection: None,
            last_update: std::time::Instant::now(),
            prev_cpu_times: HashMap::new(),
            num_logical_cpus,
        })
    }

    /// Get all running processes with GPU attribution
    pub fn processes(&mut self) -> Result<Vec<ProcessMonitorInfo>> {
        let now = std::time::Instant::now();

        // Get system processes
        let mut system_processes = self.get_system_processes()?;

        // Compute delta-based CPU% from stored previous samples
        let mut new_cpu_times = HashMap::with_capacity(system_processes.len());
        for proc in &mut system_processes {
            if proc.cpu_time_us > 0 {
                if let Some(&(prev_time_us, prev_instant)) = self.prev_cpu_times.get(&proc.pid) {
                    let elapsed = now.duration_since(prev_instant);
                    let elapsed_us = elapsed.as_micros() as f64;
                    if elapsed_us > 0.0 && proc.cpu_time_us >= prev_time_us {
                        let delta_cpu_us = (proc.cpu_time_us - prev_time_us) as f64;
                        // CPU% = (cpu_time_delta / wall_time_delta) * 100 / num_cpus
                        // Normalized to 0-100% of total system capacity (like Task Manager)
                        proc.cpu_percent = ((delta_cpu_us / elapsed_us) * 100.0
                            / self.num_logical_cpus as f64)
                            as f32;
                    }
                } else {
                    // First sample: normalize the lifetime-average from platform code
                    proc.cpu_percent /= self.num_logical_cpus;
                }
                new_cpu_times.insert(proc.pid, (proc.cpu_time_us, now));
            }
        }
        self.prev_cpu_times = new_cpu_times;

        // Add GPU information if available
        if let Some(ref gpu_collection) = self.gpu_collection {
            self.add_gpu_attribution(&mut system_processes, gpu_collection)?;

            // Reclassify processes after GPU attribution (GPU processes may change category)
            for proc in &mut system_processes {
                if proc.is_gpu_process() {
                    proc.reclassify();
                }
            }
        }

        self.last_update = now;

        Ok(system_processes)
    }

    /// Get processes sorted by CPU usage (descending)
    pub fn processes_by_cpu(&mut self) -> Result<Vec<ProcessMonitorInfo>> {
        let mut procs = self.processes()?;
        procs.sort_by(|a, b| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(procs)
    }

    /// Get processes sorted by memory usage (descending)
    pub fn processes_by_memory(&mut self) -> Result<Vec<ProcessMonitorInfo>> {
        let mut procs = self.processes()?;
        procs.sort_by_key(|p| std::cmp::Reverse(p.memory_bytes));
        Ok(procs)
    }

    /// Get processes sorted by GPU memory usage (descending)
    pub fn processes_by_gpu_memory(&mut self) -> Result<Vec<ProcessMonitorInfo>> {
        let mut procs = self.processes()?;
        procs.sort_by_key(|p| std::cmp::Reverse(p.total_gpu_memory_bytes));
        Ok(procs)
    }

    /// Get only GPU-using processes
    pub fn gpu_processes(&mut self) -> Result<Vec<ProcessMonitorInfo>> {
        let procs = self.processes()?;
        Ok(procs.into_iter().filter(|p| p.is_gpu_process()).collect())
    }

    /// Get process by PID
    pub fn process_by_pid(&mut self, pid: u32) -> Result<Option<ProcessMonitorInfo>> {
        let procs = self.processes()?;
        Ok(procs.into_iter().find(|p| p.pid == pid))
    }

    /// Get processes filtered by category
    pub fn processes_by_category(
        &mut self,
        category: ProcessCategory,
    ) -> Result<Vec<ProcessMonitorInfo>> {
        let procs = self.processes()?;
        Ok(procs
            .into_iter()
            .filter(|p| p.category == category)
            .collect())
    }

    /// Get processes grouped by category
    pub fn processes_grouped_by_category(
        &mut self,
    ) -> Result<HashMap<ProcessCategory, Vec<ProcessMonitorInfo>>> {
        let procs = self.processes()?;
        let mut grouped: HashMap<ProcessCategory, Vec<ProcessMonitorInfo>> = HashMap::new();

        for proc in procs {
            grouped.entry(proc.category).or_default().push(proc);
        }

        Ok(grouped)
    }

    /// Get category statistics (count and total CPU/memory per category)
    pub fn category_stats(&mut self) -> Result<Vec<CategoryStats>> {
        let grouped = self.processes_grouped_by_category()?;
        let mut stats: Vec<CategoryStats> = grouped
            .into_iter()
            .map(|(category, procs)| {
                let count = procs.len();
                let total_cpu: f32 = procs.iter().map(|p| p.cpu_percent).sum();
                let total_memory: u64 = procs.iter().map(|p| p.memory_bytes).sum();
                // Sum what was reported. `None` when nothing in the category
                // reported anything, which is not the same as a category whose
                // processes all measured zero.
                let reported: Vec<u64> = procs
                    .iter()
                    .filter_map(|p| p.total_gpu_memory_bytes)
                    .collect();
                let total_gpu_memory = if reported.is_empty() {
                    None
                } else {
                    Some(reported.iter().sum())
                };
                let gpu_process_count = procs.iter().filter(|p| p.is_gpu_process()).count();

                CategoryStats {
                    category,
                    process_count: count,
                    gpu_process_count,
                    total_cpu_percent: total_cpu,
                    total_memory_bytes: total_memory,
                    total_gpu_memory_bytes: total_gpu_memory,
                }
            })
            .collect();

        // Sort by total CPU usage descending
        stats.sort_by(|a, b| {
            b.total_cpu_percent
                .partial_cmp(&a.total_cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(stats)
    }

    /// Get total number of running processes
    pub fn process_count(&mut self) -> Result<usize> {
        Ok(self.processes()?.len())
    }

    /// Get number of GPU-using processes
    pub fn gpu_process_count(&mut self) -> Result<usize> {
        Ok(self.gpu_processes()?.len())
    }

    /// Get number of detected GPUs
    pub fn gpu_count(&self) -> usize {
        self.gpu_collection.as_ref().map(|gc| gc.len()).unwrap_or(0)
    }

    /// Update process list (refresh data)
    ///
    /// This method triggers a refresh of the internal process cache.
    /// The actual update happens automatically in `processes()`, so this
    /// is primarily for explicit refresh semantics in user code.
    pub fn update(&mut self) -> Result<()> {
        // Refresh by calling processes() and discarding result
        let _ = self.processes()?;
        Ok(())
    }

    // Platform-specific system process enumeration
    #[cfg(target_os = "linux")]
    fn get_system_processes(&self) -> Result<Vec<ProcessMonitorInfo>> {
        linux::enumerate_processes()
    }

    #[cfg(target_os = "windows")]
    fn get_system_processes(&self) -> Result<Vec<ProcessMonitorInfo>> {
        windows_impl::enumerate_processes()
    }

    #[cfg(target_os = "macos")]
    fn get_system_processes(&self) -> Result<Vec<ProcessMonitorInfo>> {
        macos::enumerate_processes()
    }

    /// Add GPU attribution to processes
    fn add_gpu_attribution(
        &self,
        processes: &mut [ProcessMonitorInfo],
        gpu_collection: &GpuCollection,
    ) -> Result<()> {
        // Create a map of PID -> Process for quick lookup
        let mut process_map: HashMap<u32, &mut ProcessMonitorInfo> =
            processes.iter_mut().map(|p| (p.pid, p)).collect();

        // Iterate through each GPU and its processes
        for (gpu_idx, gpu) in gpu_collection.gpus().iter().enumerate() {
            if let Ok(gpu_processes) = gpu.processes() {
                for gpu_proc in gpu_processes {
                    let pid = gpu_proc.pid;
                    // `None` here is NVML declining to report, which is every
                    // process under WDDM. It is not zero bytes.
                    let gpu_mem = gpu_proc.memory_usage;

                    if let Some(proc_info) = process_map.get_mut(&pid) {
                        // Add this GPU to the process's GPU list
                        if !proc_info.gpu_indices.contains(&gpu_idx) {
                            proc_info.gpu_indices.push(gpu_idx);
                        }

                        // Add GPU memory for this device, if the device gave
                        // one. A process attributed to a GPU that reports no
                        // figure keeps `None`, and stays distinguishable from
                        // one measured at zero.
                        if let Some(bytes) = gpu_mem {
                            proc_info.gpu_memory_per_device.insert(gpu_idx, bytes);
                            proc_info.total_gpu_memory_bytes =
                                Some(proc_info.total_gpu_memory_bytes.unwrap_or(0) + bytes);
                        }

                        // Copy user from GPU process if not already set
                        if proc_info.user.is_none() && !gpu_proc.user.is_empty() {
                            proc_info.user = Some(gpu_proc.user.clone());
                        }

                        // Copy GPU utilization data
                        if gpu_proc.gpu_usage.is_some() {
                            proc_info.gpu_usage_percent = gpu_proc.gpu_usage.map(|u| u as f32);
                        }
                        if gpu_proc.encoder_usage.is_some() {
                            proc_info.encoder_usage_percent =
                                gpu_proc.encoder_usage.map(|u| u as f32);
                        }
                        if gpu_proc.decoder_usage.is_some() {
                            proc_info.decoder_usage_percent =
                                gpu_proc.decoder_usage.map(|u| u as f32);
                        }
                        if gpu_proc.memory_usage_percent.is_some() {
                            proc_info.gpu_memory_percentage =
                                gpu_proc.memory_usage_percent.map(|u| u as f32);
                        }

                        // Update process type based on GPU process type
                        proc_info.gpu_process_type = match gpu_proc.process_type {
                            crate::gpu::GpuProcessType::Graphics => ProcessGpuType::Graphical,
                            crate::gpu::GpuProcessType::Compute => ProcessGpuType::Compute,
                            crate::gpu::GpuProcessType::GraphicsAndCompute => {
                                ProcessGpuType::GraphicalCompute
                            }
                            crate::gpu::GpuProcessType::Unknown => ProcessGpuType::Unknown,
                        };
                    }
                }
            }
        }

        Ok(())
    }

    /// Kill a process by PID
    ///
    /// This method attempts to terminate a process. On Unix systems, it sends SIGTERM
    /// by default, which allows the process to clean up. On Windows, it terminates
    /// the process forcefully.
    ///
    /// # Safety
    ///
    /// Killing processes requires appropriate permissions:
    /// - On Linux/Unix: Must have permission to send signals to the target process
    /// - On Windows: Must have PROCESS_TERMINATE access rights
    ///
    /// # Arguments
    ///
    /// * `pid` - Process ID to terminate
    /// * `force` - If true, use SIGKILL (Unix) or forceful termination (Windows)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ironmonlib::{ProcessMonitor, GpuCollection};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let gpus = GpuCollection::auto_detect()?;
    /// let mut monitor = ProcessMonitor::with_gpus(gpus)?;
    ///
    /// // Gracefully terminate process 1234
    /// monitor.kill_process(1234, false)?;
    ///
    /// // Force kill process 5678
    /// monitor.kill_process(5678, true)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn kill_process(&self, pid: u32, force: bool) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;

            let signal = if force { "KILL" } else { "TERM" };
            let status = Command::new("kill")
                .arg(format!("-{}", signal))
                .arg(pid.to_string())
                .status()
                .map_err(IronError::Io)?;

            if !status.success() {
                return Err(IronError::Other(format!(
                    "Failed to kill process {}: {}",
                    pid,
                    status.code().unwrap_or(-1)
                )));
            }

            Ok(())
        }

        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::CloseHandle;
            use windows::Win32::System::Threading::{
                OpenProcess, TerminateProcess, PROCESS_TERMINATE,
            };

            let _ = force; // Windows always uses forceful termination

            unsafe {
                let handle = OpenProcess(PROCESS_TERMINATE, false, pid).map_err(|e| {
                    IronError::Other(format!("Failed to open process {}: {}", pid, e))
                })?;

                if handle.is_invalid() {
                    return Err(IronError::Other(format!(
                        "Invalid handle for process {}",
                        pid
                    )));
                }

                let result = TerminateProcess(handle, 1);
                let _ = CloseHandle(handle);

                if result.is_err() {
                    return Err(IronError::Other(format!(
                        "Failed to terminate process {}",
                        pid
                    )));
                }

                Ok(())
            }
        }

        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            let signal = if force { "KILL" } else { "TERM" };
            let status = Command::new("kill")
                .arg(format!("-{}", signal))
                .arg(pid.to_string())
                .status()
                .map_err(IronError::Io)?;

            if !status.success() {
                return Err(IronError::Other(format!(
                    "Failed to kill process {}: {}",
                    pid,
                    status.code().unwrap_or(-1)
                )));
            }

            Ok(())
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            let _ = (pid, force);
            Err(IronError::UnsupportedPlatform(
                "Process termination not supported on this platform".to_string(),
            ))
        }
    }
}

impl Default for ProcessMonitor {
    fn default() -> Self {
        let num_logical_cpus = std::thread::available_parallelism()
            .map(|n| n.get() as f32)
            .unwrap_or(1.0);

        Self::new().unwrap_or_else(|_| Self {
            gpu_collection: None,
            last_update: std::time::Instant::now(),
            prev_cpu_times: HashMap::new(),
            num_logical_cpus,
        })
    }
}

// Linux-specific process enumeration
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::fs;
    use std::path::Path;

    pub fn enumerate_processes() -> Result<Vec<ProcessMonitorInfo>> {
        let mut processes = Vec::new();

        let proc_dir = Path::new("/proc");
        if !proc_dir.exists() {
            return Err(IronError::UnsupportedPlatform(
                "/proc filesystem not available".to_string(),
            ));
        }

        // Read uptime for CPU calculation
        let uptime = read_uptime()?;

        // Iterate through /proc entries
        for entry in fs::read_dir(proc_dir)? {
            let entry = entry?;
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();

            // Check if directory name is a number (PID)
            if let Ok(pid) = filename_str.parse::<u32>() {
                if let Ok(proc_info) = read_process_info(pid, uptime) {
                    processes.push(proc_info);
                }
            }
        }

        Ok(processes)
    }

    fn read_uptime() -> Result<f64> {
        let uptime_str = fs::read_to_string("/proc/uptime")?;
        let uptime: f64 = uptime_str
            .split_whitespace()
            .next()
            .ok_or_else(|| IronError::Parse("Invalid uptime format".to_string()))?
            .parse()
            .map_err(|e| IronError::Parse(format!("Failed to parse uptime: {}", e)))?;
        Ok(uptime)
    }

    fn read_process_info(pid: u32, uptime: f64) -> Result<ProcessMonitorInfo> {
        let proc_path = format!("/proc/{}", pid);
        let proc_dir = Path::new(&proc_path);

        if !proc_dir.exists() {
            return Err(IronError::DeviceNotFound(format!(
                "Process {} not found",
                pid
            )));
        }

        // Read /proc/[pid]/stat
        let stat_path = format!("{}/stat", proc_path);
        let stat_content = fs::read_to_string(&stat_path)?;

        // Parse stat file (fields documented in proc(5) man page)
        let (name, stat_fields) = parse_stat_line(&stat_content)?;

        if stat_fields.len() < 22 {
            return Err(IronError::Parse(
                "Insufficient fields in stat file".to_string(),
            ));
        }

        // Extract fields (0-indexed after splitting on ')')
        // Reference: proc(5) man page
        // Field 0: state, 1: ppid, 2: pgrp, 3: session, 4: tty_nr,
        // 5: tpgid, 6: flags, 7: minflt, 8: cminflt, 9: majflt,
        // 10: cmajflt, 11: utime, 12: stime, 13: cutime, 14: cstime,
        // 15: priority, 16: nice, 17: num_threads, 18: itrealvalue,
        // 19: starttime, 20: vsize, 21: rss
        // 'U' rather than '?' for an unreadable state: the rest of IronMonitor uses 'U'
        // to mean "not reported", and two different sentinels for one condition
        // means every consumer has to know both.
        let state = stat_fields[0].chars().next().unwrap_or('U');
        let parent_pid: u32 = stat_fields[1].parse().unwrap_or(0);
        let utime: u64 = stat_fields[11].parse().unwrap_or(0);
        let stime: u64 = stat_fields[12].parse().unwrap_or(0);
        let priority: i32 = stat_fields[15].parse().unwrap_or(0);
        let num_threads: u32 = stat_fields[17].parse().unwrap_or(0);
        let starttime: u64 = stat_fields[19].parse().unwrap_or(0);
        let vsize: u64 = stat_fields[20].parse().unwrap_or(0); // virtual memory size in bytes

        // Calculate CPU percentage (lifetime average as fallback for first delta sample)
        let clk_tck: u64 = 100; // SC_CLK_TCK, typically 100
        let total_ticks = utime + stime;
        let total_time = total_ticks as f64 / clk_tck as f64;
        let seconds_since_boot = uptime;
        let proc_uptime = (seconds_since_boot - (starttime as f64 / clk_tck as f64)).max(1.0);
        let cpu_percent = ((total_time / proc_uptime) * 100.0) as f32;

        // Cumulative CPU time in microseconds for delta-based calculation
        let cpu_time_us = total_ticks * 1_000_000 / clk_tck;

        // Calculate start_time as unix timestamp
        // Boot time from /proc/stat's btime field, or approximate from uptime
        let boot_time_secs = std::fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|line| line.starts_with("btime "))
                    .and_then(|line| line.split_whitespace().nth(1))
                    .and_then(|s| s.parse::<u64>().ok())
            });
        let process_start_time =
            boot_time_secs.map(|btime| btime + (starttime as f64 / clk_tck as f64) as u64);

        // Read /proc/[pid]/statm for memory
        let statm_path = format!("{}/statm", proc_path);
        let (memory_bytes, virtual_memory_from_statm) =
            if let Ok(statm_content) = fs::read_to_string(&statm_path) {
                let parts: Vec<&str> = statm_content.split_whitespace().collect();
                let rss = if parts.len() > 1 {
                    // RSS (Resident Set Size) in pages, multiply by page size (typically 4KB)
                    parts[1].parse::<u64>().unwrap_or(0) * 4096
                } else {
                    0
                };
                let vsize_statm = if !parts.is_empty() {
                    // Total program size in pages
                    parts[0].parse::<u64>().unwrap_or(0) * 4096
                } else {
                    0
                };
                (rss, vsize_statm)
            } else {
                (0, 0)
            };

        // Use vsize from stat (in bytes) if available, otherwise statm
        let virtual_memory_bytes = if vsize > 0 {
            vsize
        } else {
            virtual_memory_from_statm
        };

        // Read /proc/[pid]/io for I/O stats (may require privileges)
        let (io_read_bytes, io_write_bytes) =
            if let Ok(io_content) = fs::read_to_string(format!("{}/io", proc_path)) {
                let mut read_bytes = 0u64;
                let mut write_bytes = 0u64;
                for line in io_content.lines() {
                    if let Some(val) = line.strip_prefix("read_bytes: ") {
                        read_bytes = val.trim().parse().unwrap_or(0);
                    } else if let Some(val) = line.strip_prefix("write_bytes: ") {
                        write_bytes = val.trim().parse().unwrap_or(0);
                    }
                }
                (read_bytes, write_bytes)
            } else {
                (0, 0)
            };

        // Count file descriptors in /proc/[pid]/fd for handle_count
        let handle_count: u32 = fs::read_dir(format!("{}/fd", proc_path))
            .map(|entries| entries.count() as u32)
            .unwrap_or(0);

        // Try to read user
        let user = read_process_user(pid);

        // Classify the process
        let category = ProcessCategory::classify(&name, user.as_deref(), false);

        Ok(ProcessMonitorInfo {
            pid,
            parent_pid: if parent_pid > 0 {
                Some(parent_pid)
            } else {
                None
            },
            name,
            user,
            category,
            cpu_percent,
            memory_bytes,
            virtual_memory_bytes,
            private_bytes: 0, // Would need /proc/[pid]/smaps parsing (expensive)
            thread_count: num_threads,
            handle_count,
            io_read_bytes,
            io_write_bytes,
            start_time: process_start_time,
            gpu_indices: Vec::new(),
            gpu_memory_per_device: HashMap::new(),
            total_gpu_memory_bytes: None,
            state,
            priority: Some(priority),
            gfx_engine_used: None,
            compute_engine_used: None,
            enc_engine_used: None,
            dec_engine_used: None,
            gpu_usage_percent: None,
            encoder_usage_percent: None,
            decoder_usage_percent: None,
            gpu_process_type: ProcessGpuType::Unknown,
            gpu_memory_percentage: None,
            cpu_time_us,
        })
    }

    fn parse_stat_line(stat: &str) -> Result<(String, Vec<String>)> {
        // Format: pid (name) state ...
        // Name can contain spaces and parentheses, so we need to find the last ')'
        let start = stat
            .find('(')
            .ok_or_else(|| IronError::Parse("No opening parenthesis in stat".to_string()))?;
        let end = stat
            .rfind(')')
            .ok_or_else(|| IronError::Parse("No closing parenthesis in stat".to_string()))?;

        let name = stat[start + 1..end].to_string();
        let rest = &stat[end + 2..]; // Skip ') '

        let fields: Vec<String> = rest.split_whitespace().map(|s| s.to_string()).collect();

        Ok((name, fields))
    }

    fn read_process_user(pid: u32) -> Option<String> {
        // Read UID from /proc/[pid]/status
        let status_path = format!("/proc/{}/status", pid);
        if let Ok(content) = fs::read_to_string(&status_path) {
            for line in content.lines() {
                if line.starts_with("Uid:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() > 1 {
                        // Get real UID (first number after "Uid:")
                        if let Ok(uid) = parts[1].parse::<u32>() {
                            // Try to resolve UID to username
                            return get_username_from_uid(uid);
                        }
                    }
                }
            }
        }
        None
    }

    fn get_username_from_uid(uid: u32) -> Option<String> {
        // Simple approach: read /etc/passwd
        if let Ok(content) = fs::read_to_string("/etc/passwd") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() > 2 {
                    if let Ok(line_uid) = parts[2].parse::<u32>() {
                        if line_uid == uid {
                            return Some(parts[0].to_string());
                        }
                    }
                }
            }
        }
        None
    }
}

// Windows-specific process enumeration
#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use ::windows::Win32::Foundation::CloseHandle;
    use ::windows::Win32::Security::{
        GetTokenInformation, LookupAccountSidW, TokenUser, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER,
    };
    use ::windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use ::windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    use ::windows::Win32::System::Threading::{
        GetPriorityClass, GetProcessHandleCount, GetProcessIoCounters, GetProcessTimes,
        OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, IO_COUNTERS,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::sync::{Mutex, OnceLock};

    /// Cache of per-process attributes that cannot change while a process lives.
    ///
    /// [`get_process_user`] performs `OpenProcess` + `OpenProcessToken` +
    /// `GetTokenInformation` twice + `LookupAccountSidW`. The SID lookup is the
    /// expensive part: it can round-trip to LSA, and on a domain-joined machine to a
    /// domain controller. It was previously executed for every process on every
    /// refresh, which dominated enumeration cost.
    ///
    /// The key includes the process creation time, so a recycled PID is treated as a
    /// distinct process and can never inherit another process's username.
    /// Keyed on (pid, creation_time); the value is the resolved username, or `None`
    /// when the lookup failed for that process.
    type UserCache = Mutex<HashMap<(u32, u64), Option<String>>>;

    static PROCESS_USER_CACHE: OnceLock<UserCache> = OnceLock::new();

    /// Upper bound on cached entries before the cache is dropped wholesale.
    ///
    /// Processes are created and destroyed constantly, so entries go stale by
    /// accumulation rather than by mutation. Rather than tracking liveness, the cache
    /// is simply bounded: the ceiling is far above any realistic process count, so in
    /// practice a clear is rare and only costs one repopulating scan.
    const PROCESS_USER_CACHE_CAP: usize = 4096;

    /// Username for `pid`, memoized against the process creation time.
    fn cached_process_user(pid: u32, creation_100ns: u64) -> Option<String> {
        // A creation time of 0 means GetProcessTimes failed, so there is no stable key
        // to memoize against. Fall through uncached rather than risk a wrong answer.
        if creation_100ns == 0 {
            return get_process_user(pid);
        }

        let cache = PROCESS_USER_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let key = (pid, creation_100ns);

        if let Ok(map) = cache.lock() {
            if let Some(hit) = map.get(&key) {
                return hit.clone();
            }
        }

        let user = get_process_user(pid);

        if let Ok(mut map) = cache.lock() {
            if map.len() >= PROCESS_USER_CACHE_CAP {
                map.clear();
            }
            map.insert(key, user.clone());
        }

        user
    }

    /// Get the username of a process by PID
    fn get_process_user(pid: u32) -> Option<String> {
        unsafe {
            // Try with limited access first (works on more processes)
            let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);

            if let Ok(handle) = process_handle {
                if !handle.is_invalid() {
                    let mut token_handle = ::windows::Win32::Foundation::HANDLE::default();

                    if OpenProcessToken(handle, TOKEN_QUERY, &mut token_handle).is_ok() {
                        // Get token user info size
                        let mut return_length = 0u32;
                        let _ = GetTokenInformation(
                            token_handle,
                            TokenUser,
                            None,
                            0,
                            &mut return_length,
                        );

                        if return_length > 0 {
                            let mut token_info = vec![0u8; return_length as usize];
                            if GetTokenInformation(
                                token_handle,
                                TokenUser,
                                Some(token_info.as_mut_ptr() as *mut _),
                                return_length,
                                &mut return_length,
                            )
                            .is_ok()
                            {
                                let token_user = &*(token_info.as_ptr() as *const TOKEN_USER);

                                // Lookup username from SID
                                let mut name_buf = [0u16; 256];
                                let mut name_len = name_buf.len() as u32;
                                let mut domain_buf = [0u16; 256];
                                let mut domain_len = domain_buf.len() as u32;
                                let mut sid_type = SID_NAME_USE::default();

                                if LookupAccountSidW(
                                    None,
                                    token_user.User.Sid,
                                    ::windows::core::PWSTR(name_buf.as_mut_ptr()),
                                    &mut name_len,
                                    ::windows::core::PWSTR(domain_buf.as_mut_ptr()),
                                    &mut domain_len,
                                    &mut sid_type,
                                )
                                .is_ok()
                                {
                                    let _ = CloseHandle(token_handle);
                                    let _ = CloseHandle(handle);

                                    let name =
                                        String::from_utf16_lossy(&name_buf[..name_len as usize]);
                                    return Some(name);
                                }
                            }
                        }
                        let _ = CloseHandle(token_handle);
                    }
                    let _ = CloseHandle(handle);
                }
            }
        }
        None
    }

    pub fn enumerate_processes() -> Result<Vec<ProcessMonitorInfo>> {
        let mut processes = Vec::new();

        unsafe {
            // Take a snapshot of all processes
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| {
                IronError::Other(format!("Failed to create process snapshot: {}", e))
            })?;

            if snapshot.is_invalid() {
                return Err(IronError::Other("Invalid snapshot handle".to_string()));
            }

            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };

            // Get first process
            if Process32FirstW(snapshot, &mut entry).is_err() {
                let _ = CloseHandle(snapshot);
                return Ok(processes);
            }

            // Iterate through all processes
            loop {
                let pid = entry.th32ProcessID;

                // Open with the narrowest right that still satisfies every query
                // below. On Vista and later PROCESS_QUERY_LIMITED_INFORMATION is
                // sufficient for GetProcessTimes, GetProcessMemoryInfo,
                // GetProcessIoCounters, GetPriorityClass, GetProcessHandleCount and
                // QueryFullProcessImageNameW.
                //
                // The previous PROCESS_QUERY_INFORMATION | PROCESS_VM_READ mask asked
                // for strictly more than was needed. That is both slower to grant and
                // outright denied for protected and elevated processes, so those
                // processes were silently dropped from the listing entirely.
                let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);

                // Whether the full, handle-backed path produced a row. When it does
                // not, a reduced row is emitted from the snapshot entry below.
                let mut emitted = false;

                if let Ok(handle) = process_handle {
                    if !handle.is_invalid() {
                        // Get process name
                        let mut name_buffer = [0u16; 260];
                        let mut name_len = name_buffer.len() as u32;

                        use ::windows::core::PWSTR;
                        let name = if QueryFullProcessImageNameW(
                            handle,
                            ::windows::Win32::System::Threading::PROCESS_NAME_WIN32,
                            PWSTR(name_buffer.as_mut_ptr()),
                            &mut name_len,
                        )
                        .is_ok()
                        {
                            let os_string = OsString::from_wide(&name_buffer[..name_len as usize]);
                            std::path::Path::new(&os_string)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Unknown")
                                .to_string()
                        } else {
                            // Fallback to executable name from snapshot
                            let null_pos = entry
                                .szExeFile
                                .iter()
                                .position(|&c| c == 0)
                                .unwrap_or(entry.szExeFile.len());
                            String::from_utf16_lossy(&entry.szExeFile[..null_pos])
                        };

                        // Get memory info
                        let mut mem_counters = PROCESS_MEMORY_COUNTERS_EX::default();
                        let (memory_bytes, virtual_memory_bytes, private_bytes) =
                            if GetProcessMemoryInfo(
                                handle,
                                std::ptr::addr_of_mut!(mem_counters) as *mut _,
                                std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                            )
                            .is_ok()
                            {
                                (
                                    mem_counters.WorkingSetSize as u64,
                                    mem_counters.PagefileUsage as u64,
                                    mem_counters.PrivateUsage as u64,
                                )
                            } else {
                                (0, 0, 0)
                            };

                        // Get handle count
                        let mut handle_count: u32 = 0;
                        let _ = GetProcessHandleCount(handle, &mut handle_count);

                        // Get I/O counters
                        let mut io_counters = IO_COUNTERS::default();
                        let (io_read_bytes, io_write_bytes) =
                            if GetProcessIoCounters(handle, &mut io_counters).is_ok() {
                                (
                                    io_counters.ReadTransferCount,
                                    io_counters.WriteTransferCount,
                                )
                            } else {
                                (0, 0)
                            };

                        // Get priority class
                        let priority_class = GetPriorityClass(handle);

                        // Get CPU times
                        let mut creation_time = Default::default();
                        let mut exit_time = Default::default();
                        let mut kernel_time = Default::default();
                        let mut user_time = Default::default();

                        let (cpu_percent, start_time, cpu_time_us, creation_100ns) =
                            if GetProcessTimes(
                                handle,
                                &mut creation_time,
                                &mut exit_time,
                                &mut kernel_time,
                                &mut user_time,
                            )
                            .is_ok()
                            {
                                // Convert FILETIME (100ns units)
                                let kernel_100ns = (kernel_time.dwHighDateTime as u64) << 32
                                    | (kernel_time.dwLowDateTime as u64);
                                let user_100ns = (user_time.dwHighDateTime as u64) << 32
                                    | (user_time.dwLowDateTime as u64);

                                // Cumulative CPU time in microseconds for delta-based calculation
                                let cpu_us = (kernel_100ns + user_100ns) / 10;

                                // Convert creation_time FILETIME to Unix timestamp
                                // FILETIME is 100ns intervals since Jan 1, 1601
                                // Unix epoch is Jan 1, 1970 - difference is 116444736000000000 (100ns intervals)
                                let creation_100ns = (creation_time.dwHighDateTime as u64) << 32
                                    | (creation_time.dwLowDateTime as u64);
                                let unix_epoch_diff: u64 = 116444736000000000;
                                let start_unix = if creation_100ns > unix_epoch_diff {
                                    Some((creation_100ns - unix_epoch_diff) / 10_000_000)
                                } else {
                                    None
                                };

                                // Calculate CPU% as lifetime average (fallback for first delta sample)
                                let total_cpu_secs = cpu_us as f64 / 1_000_000.0;

                                // Get current time as FILETIME for uptime calculation
                                let now_ft =
                                windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime(
                                );
                                let now_100ns = (now_ft.dwHighDateTime as u64) << 32
                                    | (now_ft.dwLowDateTime as u64);
                                let uptime_secs = if now_100ns > creation_100ns {
                                    (now_100ns - creation_100ns) as f64 / 10_000_000.0
                                } else {
                                    1.0
                                };

                                let pct = if uptime_secs > 0.0 {
                                    ((total_cpu_secs / uptime_secs) * 100.0) as f32
                                } else {
                                    0.0
                                };

                                (pct, start_unix, cpu_us, creation_100ns)
                            } else {
                                (0.0, None, 0, 0)
                            };

                        // Get user (memoized against the process creation time — the
                        // SID lookup underneath is the single most expensive call in
                        // this loop).
                        let user = cached_process_user(pid, creation_100ns);

                        // Classify the process
                        let category = ProcessCategory::classify(&name, user.as_deref(), false);

                        processes.push(ProcessMonitorInfo {
                            pid,
                            name,
                            user,
                            category,
                            cpu_percent,
                            memory_bytes,
                            gpu_indices: Vec::new(),
                            gpu_memory_per_device: HashMap::new(),
                            total_gpu_memory_bytes: None,
                            // Windows exposes scheduling state per thread, not per
                            // process. Reporting 'R' here was a guess, not a reading.
                            state: 'U',
                            priority: Some(priority_class as i32),
                            gfx_engine_used: None,
                            compute_engine_used: None,
                            enc_engine_used: None,
                            dec_engine_used: None,
                            gpu_usage_percent: None,
                            encoder_usage_percent: None,
                            decoder_usage_percent: None,
                            gpu_process_type: ProcessGpuType::Unknown,
                            gpu_memory_percentage: None,
                            // New Windows-specific fields
                            parent_pid: Some(entry.th32ParentProcessID),
                            virtual_memory_bytes,
                            private_bytes,
                            thread_count: entry.cntThreads,
                            handle_count,
                            io_read_bytes,
                            io_write_bytes,
                            start_time,
                            cpu_time_us,
                        });

                        let _ = CloseHandle(handle);
                        emitted = true;
                    }
                }

                if !emitted {
                    // The process could not be opened — almost always because it runs
                    // as SYSTEM or is protected, and IronMonitor is not elevated.
                    //
                    // Previously such processes were skipped entirely, so an
                    // unelevated run silently omitted every SYSTEM process (System,
                    // smss, csrss, wininit, services, lsass, and most svchost
                    // instances). The process count looked plausible, which made the
                    // omission easy to miss.
                    //
                    // The Toolhelp snapshot entry needs no handle, so emit what is
                    // genuinely known and leave the privileged metrics at zero rather
                    // than dropping the row.
                    let null_pos = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name = String::from_utf16_lossy(&entry.szExeFile[..null_pos]);
                    let category = ProcessCategory::classify(&name, None, false);

                    processes.push(ProcessMonitorInfo {
                        pid,
                        name,
                        // Unknown rather than a guess: attributing these to the current
                        // user would be wrong, and they are usually SYSTEM.
                        user: None,
                        category,
                        cpu_percent: 0.0,
                        memory_bytes: 0,
                        gpu_indices: Vec::new(),
                        gpu_memory_per_device: HashMap::new(),
                        total_gpu_memory_bytes: None,
                        state: 'U',
                        priority: None,
                        gfx_engine_used: None,
                        compute_engine_used: None,
                        enc_engine_used: None,
                        dec_engine_used: None,
                        gpu_usage_percent: None,
                        encoder_usage_percent: None,
                        decoder_usage_percent: None,
                        gpu_process_type: ProcessGpuType::Unknown,
                        gpu_memory_percentage: None,
                        parent_pid: Some(entry.th32ParentProcessID),
                        virtual_memory_bytes: 0,
                        private_bytes: 0,
                        thread_count: entry.cntThreads,
                        handle_count: 0,
                        io_read_bytes: 0,
                        io_write_bytes: 0,
                        start_time: None,
                        cpu_time_us: 0,
                    });
                }

                // Move to next process
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }

            let _ = CloseHandle(snapshot);
        }

        Ok(processes)
    }
}

// macOS-specific process enumeration
#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::CStr;
    use std::mem;

    // macOS proc_info constants
    const PROC_PIDTASKALLINFO: i32 = 2;
    const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;

    // FFI declarations for libproc
    #[link(name = "proc", kind = "dylib")]
    extern "C" {
        fn proc_listpids(proc_type: u32, type_info: u32, buffer: *mut u8, buffer_size: i32) -> i32;

        fn proc_pidinfo(pid: i32, flavor: i32, arg: u64, buffer: *mut u8, buffer_size: i32) -> i32;

        fn proc_pidpath(pid: i32, buffer: *mut u8, buffer_size: u32) -> i32;
    }

    #[repr(C)]
    struct ProcTaskAllInfo {
        pbsd: ProcBsdInfo,
        ptinfo: ProcTaskInfo,
    }

    #[repr(C)]
    struct ProcBsdInfo {
        pbi_flags: u32,
        pbi_status: u32,
        pbi_xstatus: u32,
        pbi_pid: u32,
        pbi_ppid: u32,
        pbi_uid: u32,
        pbi_gid: u32,
        pbi_ruid: u32,
        pbi_rgid: u32,
        pbi_svuid: u32,
        pbi_svgid: u32,
        _pad1: u32,
        pbi_comm: [u8; 16],
        pbi_name: [u8; 32],
        pbi_nfiles: u32,
        pbi_pgid: u32,
        pbi_pjobc: u32,
        e_tdev: u32,
        e_tpgid: u32,
        pbi_nice: i32,
        pbi_start_tvsec: u64,
        pbi_start_tvusec: u64,
    }

    #[repr(C)]
    struct ProcTaskInfo {
        pti_virtual_size: u64,
        pti_resident_size: u64,
        pti_total_user: u64,
        pti_total_system: u64,
        pti_threads_user: u64,
        pti_threads_system: u64,
        pti_policy: i32,
        pti_faults: i32,
        pti_pageins: i32,
        pti_cow_faults: i32,
        pti_messages_sent: i32,
        pti_messages_received: i32,
        pti_syscalls_mach: i32,
        pti_syscalls_unix: i32,
        pti_csw: i32,
        pti_threadnum: i32,
        pti_numrunning: i32,
        pti_priority: i32,
    }

    pub fn enumerate_processes() -> Result<Vec<ProcessMonitorInfo>> {
        let mut processes = Vec::new();

        unsafe {
            // First, get the number of processes
            let num_pids = proc_listpids(1, 0, std::ptr::null_mut(), 0); // PROC_ALL_PIDS = 1
            if num_pids <= 0 {
                return Err(IronError::Other("Failed to get process count".to_string()));
            }

            // Allocate buffer for PIDs
            let buffer_size = num_pids * mem::size_of::<i32>() as i32;
            let mut pid_buffer = vec![0i32; (num_pids / mem::size_of::<i32>() as i32) as usize];

            // Get all PIDs
            let actual_size = proc_listpids(
                1, // PROC_ALL_PIDS
                0,
                pid_buffer.as_mut_ptr() as *mut u8,
                buffer_size,
            );

            if actual_size <= 0 {
                return Err(IronError::Other("Failed to list processes".to_string()));
            }

            let num_processes = (actual_size / mem::size_of::<i32>() as i32) as usize;

            // Iterate through each PID
            for &pid in &pid_buffer[..num_processes] {
                if pid <= 0 {
                    continue;
                }

                // Get process info
                let mut task_info: ProcTaskAllInfo = mem::zeroed();
                let info_size = proc_pidinfo(
                    pid,
                    PROC_PIDTASKALLINFO,
                    0,
                    &mut task_info as *mut _ as *mut u8,
                    mem::size_of::<ProcTaskAllInfo>() as i32,
                );

                if info_size <= 0 {
                    continue; // Process may have exited
                }

                // Get process path
                let mut path_buffer = [0u8; PROC_PIDPATHINFO_MAXSIZE];
                let path_len = proc_pidpath(
                    pid,
                    path_buffer.as_mut_ptr(),
                    PROC_PIDPATHINFO_MAXSIZE as u32,
                );

                let name = if path_len > 0 {
                    // Extract filename from path
                    let path_slice = &path_buffer[..path_len as usize];
                    if let Ok(path_str) = CStr::from_bytes_until_nul(path_slice)
                        .map(|c| c.to_string_lossy().to_string())
                    {
                        std::path::Path::new(&path_str)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Unknown")
                            .to_string()
                    } else {
                        // Fallback to comm name
                        let null_pos = task_info
                            .pbsd
                            .pbi_comm
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(task_info.pbsd.pbi_comm.len());
                        String::from_utf8_lossy(&task_info.pbsd.pbi_comm[..null_pos]).to_string()
                    }
                } else {
                    // Use process name from pbsd info
                    let null_pos = task_info
                        .pbsd
                        .pbi_comm
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(task_info.pbsd.pbi_comm.len());
                    String::from_utf8_lossy(&task_info.pbsd.pbi_comm[..null_pos]).to_string()
                };

                // Cumulative CPU time in microseconds for delta-based calculation
                let cpu_time_us =
                    task_info.ptinfo.pti_total_user + task_info.ptinfo.pti_total_system;

                // Calculate CPU% as lifetime average (fallback for first delta sample)
                let process_start_secs = task_info.pbsd.pbi_start_tvsec;
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let uptime_secs = if now_secs > process_start_secs {
                    (now_secs - process_start_secs) as f64
                } else {
                    1.0
                };
                let total_cpu_secs = cpu_time_us as f64 / 1_000_000.0;
                let cpu_percent = if uptime_secs > 0.0 {
                    ((total_cpu_secs / uptime_secs) * 100.0) as f32
                } else {
                    0.0
                };

                // Status mapping
                let status = match task_info.pbsd.pbi_status {
                    1 => "Idle",
                    2 => "Running",
                    3 => "Sleeping",
                    4 => "Stopped",
                    5 => "Zombie",
                    _ => "Unknown",
                }
                .to_string();

                let user_str = format!("{}", task_info.pbsd.pbi_uid);
                let category = ProcessCategory::classify(&name, Some(&user_str), false);

                processes.push(ProcessMonitorInfo {
                    pid: pid as u32,
                    parent_pid: Some(task_info.pbsd.pbi_ppid),
                    name,
                    user: Some(user_str),
                    category,
                    cpu_percent,
                    memory_bytes: task_info.ptinfo.pti_resident_size,
                    virtual_memory_bytes: task_info.ptinfo.pti_virtual_size,
                    private_bytes: 0, // Not easily available on macOS
                    thread_count: task_info.ptinfo.pti_threadnum as u32,
                    handle_count: 0,  // Not available on macOS
                    io_read_bytes: 0, // Would need ioreg
                    io_write_bytes: 0,
                    start_time: Some(task_info.pbsd.pbi_start_tvsec),
                    gpu_indices: Vec::new(),
                    gpu_memory_per_device: HashMap::new(),
                    total_gpu_memory_bytes: None,
                    state: status.chars().next().unwrap_or('U'), // First char of status
                    priority: Some(task_info.ptinfo.pti_priority),
                    gfx_engine_used: None,
                    compute_engine_used: None,
                    enc_engine_used: None,
                    dec_engine_used: None,
                    gpu_usage_percent: None,
                    encoder_usage_percent: None,
                    decoder_usage_percent: None,
                    gpu_process_type: ProcessGpuType::Unknown,
                    gpu_memory_percentage: None,
                    cpu_time_us,
                });
            }
        }

        Ok(processes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Protected system processes must appear in the listing.
    ///
    /// `csrss.exe` and `wininit.exe` run as protected processes: `OpenProcess` with
    /// `PROCESS_VM_READ` is denied for them even from an elevated context. The
    /// enumeration loop only emits a process when the open succeeds, so requesting
    /// that right silently dropped every protected process from the results.
    /// Narrowing the mask to `PROCESS_QUERY_LIMITED_INFORMATION` both restores them
    /// and removes the failed-open cost.
    #[cfg(target_os = "windows")]
    #[test]
    fn enumeration_includes_protected_system_processes() {
        let mut monitor = match ProcessMonitor::new() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("skipping: ProcessMonitor unavailable: {e}");
                return;
            }
        };

        let processes = monitor.processes().expect("enumeration should succeed");
        assert!(
            processes.len() > 20,
            "implausibly short process listing: {}",
            processes.len()
        );

        let names: Vec<String> = processes.iter().map(|p| p.name.to_lowercase()).collect();
        for protected in ["csrss.exe", "wininit.exe"] {
            assert!(
                names.iter().any(|n| n == protected),
                "protected process {protected} missing; OpenProcess is requesting more \
                 access than it needs. Present: {} processes",
                processes.len()
            );
        }
    }

    /// The username memoization must not leak one process's identity to another.
    ///
    /// Windows recycles PIDs aggressively, so the cache is keyed on
    /// (pid, creation_time) rather than pid alone.
    #[cfg(target_os = "windows")]
    #[test]
    fn repeated_enumeration_is_stable_and_consistent() {
        let mut monitor = match ProcessMonitor::new() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("skipping: ProcessMonitor unavailable: {e}");
                return;
            }
        };

        let first = monitor.processes().expect("first enumeration");
        let second = monitor
            .processes()
            .expect("second enumeration (cache warm)");

        // The invariant under test is that a PID never reports one *identity* and
        // then a different one — that would mean the memoization handed back another
        // process's user.
        //
        // Some -> None is not that bug: it happens when a process becomes unopenable
        // between scans (usually because it is exiting), and the reduced
        // snapshot-only row legitimately carries no user. Only compare when both
        // scans actually resolved a name.
        let by_pid: HashMap<u32, Option<String>> =
            first.iter().map(|p| (p.pid, p.user.clone())).collect();

        for proc in &second {
            let (Some(Some(previous)), Some(current)) = (by_pid.get(&proc.pid), proc.user.as_ref())
            else {
                continue;
            };
            assert_eq!(
                previous, current,
                "pid {} reported user {:?} then {:?}; the username cache returned \
                 another process's identity",
                proc.pid, previous, current
            );
        }
    }

    fn make_test_process() -> ProcessMonitorInfo {
        ProcessMonitorInfo {
            pid: 1234,
            parent_pid: Some(1),
            name: "test_process".to_string(),
            user: Some("testuser".to_string()),
            category: ProcessCategory::Unknown,
            cpu_percent: 25.5,
            memory_bytes: 1024 * 1024 * 512,
            virtual_memory_bytes: 1024 * 1024 * 1024,
            private_bytes: 1024 * 1024 * 256,
            thread_count: 4,
            handle_count: 100,
            io_read_bytes: 0,
            io_write_bytes: 0,
            start_time: None,
            gpu_indices: vec![],
            gpu_memory_per_device: HashMap::new(),
            total_gpu_memory_bytes: None,
            state: 'R',
            priority: Some(0),
            gfx_engine_used: None,
            compute_engine_used: None,
            enc_engine_used: None,
            dec_engine_used: None,
            gpu_usage_percent: None,
            encoder_usage_percent: None,
            decoder_usage_percent: None,
            gpu_process_type: ProcessGpuType::Unknown,
            cpu_time_us: 0,
            gpu_memory_percentage: None,
        }
    }

    #[test]
    fn test_classify_system() {
        assert_eq!(
            ProcessCategory::classify("systemd", None, false),
            ProcessCategory::System
        );
    }

    #[test]
    fn test_classify_browser() {
        assert_eq!(
            ProcessCategory::classify("firefox", None, false),
            ProcessCategory::Browser
        );
        assert_eq!(
            ProcessCategory::classify("chrome", None, false),
            ProcessCategory::Browser
        );
    }

    #[test]
    fn test_classify_development() {
        assert_eq!(
            ProcessCategory::classify("cargo", None, false),
            ProcessCategory::Development
        );
        assert_eq!(
            ProcessCategory::classify("rustc", None, false),
            ProcessCategory::Development
        );
    }

    #[test]
    fn test_classify_ai_ml_gpu() {
        assert_eq!(
            ProcessCategory::classify("python", None, true),
            ProcessCategory::AiMl
        );
        assert_eq!(
            ProcessCategory::classify("ollama", None, true),
            ProcessCategory::AiMl
        );
    }

    #[test]
    fn test_classify_gpu_gaming() {
        assert_eq!(
            ProcessCategory::classify("steam", None, true),
            ProcessCategory::Gaming
        );
    }

    /// A name nothing recognises, holding a GPU context, is still separated
    /// out — but only after the name categories have had their turn.
    #[test]
    fn test_classify_gpu_generic() {
        assert_eq!(
            ProcessCategory::classify("someapp", None, true),
            ProcessCategory::GpuCompute
        );
    }

    /// A browser holding a GPU context is a browser.
    ///
    /// Every windowed application on Windows holds one, so this branch used to
    /// report 24 processes as "GPU Compute" — WindowsTerminal.exe, brave.exe
    /// and Code.exe among them, all of which `nvidia-smi` labels `C+G` with
    /// `[N/A]` memory. A GPU context is not evidence of a GPU workload.
    #[test]
    fn a_gpu_context_does_not_override_what_a_process_is() {
        for (name, want) in [
            ("brave.exe", ProcessCategory::Browser),
            ("Code.exe", ProcessCategory::Development),
        ] {
            assert_eq!(
                ProcessCategory::classify(name, None, true),
                want,
                "{name} holds a GPU context and is still a {want:?}"
            );
            assert_eq!(
                ProcessCategory::classify(name, None, false),
                want,
                "{name} classifies the same either way"
            );
        }

        // The evidence-backed branches are untouched: a GPU context plus one of
        // these names is a real signal.
        assert_eq!(
            ProcessCategory::classify("python", None, true),
            ProcessCategory::AiMl
        );
        assert_eq!(
            ProcessCategory::classify("steam", None, true),
            ProcessCategory::Gaming
        );
    }

    #[test]
    fn test_classify_service() {
        assert_eq!(
            ProcessCategory::classify("sshd", None, false),
            ProcessCategory::Service
        );
    }

    #[test]
    fn test_classify_media() {
        assert_eq!(
            ProcessCategory::classify("vlc", None, false),
            ProcessCategory::Media
        );
    }

    #[test]
    fn test_classify_unknown() {
        assert_eq!(
            ProcessCategory::classify("xyzzy_unknown_proc", None, false),
            ProcessCategory::Unknown
        );
    }

    #[test]
    fn test_classify_case_insensitive() {
        assert_eq!(
            ProcessCategory::classify("Firefox", None, false),
            ProcessCategory::Browser
        );
        assert_eq!(
            ProcessCategory::classify("CHROME", None, false),
            ProcessCategory::Browser
        );
    }

    #[test]
    fn test_memory_mb() {
        let p = make_test_process();
        assert!((p.memory_mb() - 512.0).abs() < 0.01);
    }

    #[test]
    fn test_cpu_usage() {
        let p = make_test_process();
        assert!((p.cpu_usage() - 25.5).abs() < 0.01);
    }

    #[test]
    fn test_is_gpu_process() {
        let mut p = make_test_process();
        assert!(!p.is_gpu_process());
        p.gpu_indices.push(0);
        assert!(p.is_gpu_process());
    }

    /// A process no device reported memory for reads back as unknown, and a
    /// process measured at zero reads back as zero. NVML declines to report at
    /// all under WDDM, so on Windows the first case is every process.
    #[test]
    fn test_gpu_memory_mb() {
        let mut p = make_test_process();
        assert_eq!(p.gpu_memory_mb(), None, "nothing reported is not zero MB");

        p.total_gpu_memory_bytes = Some(0);
        assert_eq!(
            p.gpu_memory_mb(),
            Some(0.0),
            "a device reporting zero is a measurement"
        );

        p.total_gpu_memory_bytes = Some(1024 * 1024 * 256);
        assert!((p.gpu_memory_mb().expect("reported") - 256.0).abs() < 0.01);
    }

    #[test]
    fn test_reclassify() {
        let mut p = make_test_process();
        p.name = "firefox".to_string();
        p.reclassify();
        assert_eq!(p.category, ProcessCategory::Browser);
    }

    #[test]
    fn test_process_gpu_type_from_engine_usage() {
        assert_eq!(
            ProcessGpuType::from_engine_usage(100, 0),
            ProcessGpuType::Graphical
        );
        assert_eq!(
            ProcessGpuType::from_engine_usage(0, 100),
            ProcessGpuType::Compute
        );
        assert_eq!(
            ProcessGpuType::from_engine_usage(100, 100),
            ProcessGpuType::GraphicalCompute
        );
        assert_eq!(
            ProcessGpuType::from_engine_usage(0, 0),
            ProcessGpuType::Unknown
        );
    }

    #[test]
    fn test_process_gpu_type_display() {
        assert_eq!(format!("{}", ProcessGpuType::Graphical), "Graphics");
        assert_eq!(format!("{}", ProcessGpuType::Compute), "Compute");
        assert_eq!(
            format!("{}", ProcessGpuType::GraphicalCompute),
            "Gfx+Compute"
        );
        assert_eq!(format!("{}", ProcessGpuType::Unknown), "Unknown");
    }

    #[test]
    fn test_category_display_name() {
        assert_eq!(ProcessCategory::System.display_name(), "System");
        assert_eq!(ProcessCategory::Browser.display_name(), "Browsers");
        assert_eq!(ProcessCategory::AiMl.display_name(), "AI/ML");
        assert_eq!(ProcessCategory::GpuCompute.display_name(), "Using GPU");
    }

    #[test]
    fn test_category_icon() {
        assert!(!ProcessCategory::System.icon().is_empty());
        assert!(!ProcessCategory::Browser.icon().is_empty());
        assert!(!ProcessCategory::Unknown.icon().is_empty());
    }

    #[test]
    fn test_category_all_unique() {
        let all = ProcessCategory::all();
        let unique: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(all.len(), unique.len());
    }

    #[test]
    fn test_process_gpu_count() {
        let mut p = make_test_process();
        assert_eq!(p.gpu_count(), 0);
        p.gpu_indices = vec![0, 1];
        assert_eq!(p.gpu_count(), 2);
    }

    #[test]
    fn test_process_category_default() {
        assert_eq!(ProcessCategory::default(), ProcessCategory::Unknown);
    }
}
