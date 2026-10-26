// boot_config.rs - System boot configuration monitoring for ironmon
//
// Provides boot configuration, startup items, and boot time analysis.
// Inspired by jetson_stats boot management features.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::error::Result;

/// Boot type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootType {
    /// Legacy BIOS boot
    Legacy,
    /// UEFI boot
    Uefi,
    /// UEFI with Secure Boot enabled
    SecureBoot,
    /// Unknown boot type
    Unknown,
}

impl std::fmt::Display for BootType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Legacy => write!(f, "Legacy BIOS"),
            Self::Uefi => write!(f, "UEFI"),
            Self::SecureBoot => write!(f, "UEFI + Secure Boot"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Startup item type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartupItemType {
    /// Service that runs at boot
    Service,
    /// Application that runs at login
    Application,
    /// Scheduled task
    ScheduledTask,
    /// Kernel module
    KernelModule,
    /// Driver
    Driver,
    /// Registry-based startup (Windows)
    Registry,
    /// Unknown type
    Unknown,
}

impl std::fmt::Display for StartupItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Service => write!(f, "Service"),
            Self::Application => write!(f, "Application"),
            Self::ScheduledTask => write!(f, "Scheduled Task"),
            Self::KernelModule => write!(f, "Kernel Module"),
            Self::Driver => write!(f, "Driver"),
            Self::Registry => write!(f, "Registry"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Startup item status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartupItemStatus {
    /// Item is enabled
    Enabled,
    /// Item is disabled
    Disabled,
    /// Status is unknown
    Unknown,
}

/// Information about a startup item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupItem {
    /// Name of the startup item
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Type of startup item
    pub item_type: StartupItemType,
    /// Current status
    pub status: StartupItemStatus,
    /// Command or path
    pub command: Option<String>,
    /// Publisher/vendor
    pub publisher: Option<String>,
    /// Location in registry/filesystem
    pub location: Option<String>,
    /// Impact on boot time (if known)
    pub boot_impact: Option<String>,
}

/// Boot time analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootTime {
    /// Total boot time from power on to login screen
    pub total: Duration,
    /// Firmware (BIOS/UEFI) initialization time
    pub firmware: Option<Duration>,
    /// Bootloader time
    pub bootloader: Option<Duration>,
    /// Kernel initialization time
    pub kernel: Option<Duration>,
    /// User-space initialization time
    pub userspace: Option<Duration>,
    /// Time when system was last booted
    pub boot_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    /// System uptime
    pub uptime: Duration,
}

impl BootTime {
    /// Format duration as human-readable string
    pub fn format_duration(d: Duration) -> String {
        let secs = d.as_secs();
        if secs >= 86400 {
            let days = secs / 86400;
            let hours = (secs % 86400) / 3600;
            format!("{}d {}h", days, hours)
        } else if secs >= 3600 {
            let hours = secs / 3600;
            let mins = (secs % 3600) / 60;
            format!("{}h {}m", hours, mins)
        } else if secs >= 60 {
            let mins = secs / 60;
            let secs = secs % 60;
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}.{}s", secs, d.subsec_millis() / 100)
        }
    }
}

/// Kernel boot parameters
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KernelParams {
    /// Raw command line
    pub cmdline: String,
    /// Parsed parameters
    pub params: Vec<(String, Option<String>)>,
    /// Root filesystem
    pub root: Option<String>,
    /// Init system path
    pub init: Option<String>,
    /// Quiet boot
    pub quiet: bool,
    /// Splash screen
    pub splash: bool,
}

/// Boot configuration information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootInfo {
    /// Boot type (Legacy/UEFI/SecureBoot), or `None` where the firmware type
    /// was not read. macOS does not read it, and the old literal `Uefi` there
    /// was an assumption about the hardware, not a reading.
    pub boot_type: Option<BootType>,
    /// Whether Secure Boot is enforcing.
    ///
    /// `None` when it could not be established, which is not `false`. A
    /// firmware whose Secure Boot state cannot be read and one that reports it
    /// off are different findings, and only the second is a reason to act.
    pub secure_boot: Option<bool>,
    /// Current boot device
    pub boot_device: Option<String>,
    /// Boot partition
    pub boot_partition: Option<String>,
    /// EFI system partition
    pub efi_partition: Option<String>,
    /// Bootloader name, or `None` where it was not detected.
    ///
    /// The Windows and macOS readers used to assign a fixed name ("Windows Boot
    /// Manager", "iBoot") whether or not anything was checked.
    pub bootloader: Option<String>,
    /// Bootloader version
    pub bootloader_version: Option<String>,
}

/// Boot configuration monitor
#[derive(Debug)]
pub struct BootMonitor {
    /// Boot time analysis
    pub boot_time: BootTime,
    /// Boot configuration
    pub boot_info: BootInfo,
    /// Startup items
    pub startup_items: Vec<StartupItem>,
    /// Kernel parameters (Linux)
    pub kernel_params: KernelParams,
}

impl BootMonitor {
    /// Create a new boot monitor
    pub fn new() -> Result<Self> {
        let mut monitor = Self::unread();
        monitor.refresh()?;
        Ok(monitor)
    }

    /// Only the boot timing, without the boot configuration and startup items
    /// [`BootMonitor::new`] also reads.
    ///
    /// Those cost several helper processes on Windows -- four PowerShell
    /// sessions and `schtasks` -- and a caller that wants one duration, such
    /// as the ontology resolver, was paying for all of them: about ten seconds
    /// of every snapshot on the machine this was measured on.
    pub fn read_boot_time() -> Result<BootTime> {
        #[allow(unused_mut)]
        let mut monitor = Self::unread();
        #[cfg(target_os = "linux")]
        monitor.linux_read_boot_time()?;
        #[cfg(windows)]
        monitor.windows_read_boot_time()?;
        #[cfg(target_os = "macos")]
        monitor.macos_read_boot_time()?;
        Ok(monitor.boot_time)
    }

    /// A monitor with nothing read yet.
    fn unread() -> Self {
        Self {
            boot_time: BootTime {
                total: Duration::ZERO,
                firmware: None,
                bootloader: None,
                kernel: None,
                userspace: None,
                boot_timestamp: None,
                uptime: Duration::ZERO,
            },
            boot_info: BootInfo {
                boot_type: None,
                secure_boot: None,
                boot_device: None,
                boot_partition: None,
                efi_partition: None,
                bootloader: None,
                bootloader_version: None,
            },
            startup_items: Vec::new(),
            kernel_params: KernelParams::default(),
        }
    }

    /// Refresh all boot information
    pub fn refresh(&mut self) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_read_boot_info()?;
            self.linux_read_boot_time()?;
            self.linux_read_startup_items()?;
            self.linux_read_kernel_params()?;
        }

        #[cfg(windows)]
        {
            self.windows_read_boot_info()?;
            self.windows_read_boot_time()?;
            self.windows_read_startup_items()?;
        }

        #[cfg(target_os = "macos")]
        {
            self.macos_read_boot_info()?;
            self.macos_read_boot_time()?;
            self.macos_read_startup_items()?;
        }

        Ok(())
    }

    /// Get enabled startup items count
    pub fn enabled_count(&self) -> usize {
        self.startup_items
            .iter()
            .filter(|i| i.status == StartupItemStatus::Enabled)
            .count()
    }

    /// Get disabled startup items count
    pub fn disabled_count(&self) -> usize {
        self.startup_items
            .iter()
            .filter(|i| i.status == StartupItemStatus::Disabled)
            .count()
    }

    /// Get startup items by type
    pub fn items_by_type(&self, item_type: StartupItemType) -> Vec<&StartupItem> {
        self.startup_items
            .iter()
            .filter(|i| i.item_type == item_type)
            .collect()
    }

    /// Check if running in UEFI mode
    pub fn is_uefi(&self) -> bool {
        matches!(
            self.boot_info.boot_type,
            Some(BootType::Uefi | BootType::SecureBoot)
        )
    }

    /// Check if Secure Boot is enabled, or `None` if that was not established.
    pub fn is_secure_boot(&self) -> Option<bool> {
        self.boot_info.secure_boot
    }

    // ==================== Linux Implementation ====================

    #[cfg(target_os = "linux")]
    fn linux_read_boot_info(&mut self) -> Result<()> {
        use std::fs;
        use std::path::Path;

        // Check for UEFI
        let efi_path = Path::new("/sys/firmware/efi");
        if efi_path.exists() {
            // Check for Secure Boot
            let sb_path =
                "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
            if Path::new(sb_path).exists() {
                if let Ok(data) = fs::read(sb_path) {
                    // Secure Boot variable: last byte indicates state. An
                    // efivar that reads back empty says nothing, and
                    // `unwrap_or(0)` used to turn that into "Secure Boot off".
                    self.boot_info.secure_boot = match data.last() {
                        Some(1) => Some(true),
                        Some(0) => Some(false),
                        _ => None,
                    };
                }
            }

            // UEFI is what was established; SecureBoot is an upgrade only the
            // variable can justify, and an unread variable justifies neither.
            self.boot_info.boot_type = Some(match self.boot_info.secure_boot {
                Some(true) => BootType::SecureBoot,
                _ => BootType::Uefi,
            });
        } else {
            self.boot_info.boot_type = Some(BootType::Legacy);
        }

        // Get boot device from /proc/cmdline
        if let Ok(cmdline) = fs::read_to_string("/proc/cmdline") {
            for part in cmdline.split_whitespace() {
                if let Some(root) = part.strip_prefix("root=") {
                    self.boot_info.boot_device = Some(root.to_string());
                }
            }
        }

        // Check for bootloader
        if Path::new("/boot/grub/grub.cfg").exists() {
            self.boot_info.bootloader = Some("GRUB".to_string());
        } else if Path::new("/boot/loader/loader.conf").exists() {
            self.boot_info.bootloader = Some("systemd-boot".to_string());
        }

        // Find EFI partition
        if let Ok(mounts) = fs::read_to_string("/proc/mounts") {
            for line in mounts.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1] == "/boot/efi" {
                    self.boot_info.efi_partition = Some(parts[0].to_string());
                }
            }
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_read_boot_time(&mut self) -> Result<()> {
        use std::fs;

        // Get uptime
        let mut uptime_read = false;
        if let Ok(content) = fs::read_to_string("/proc/uptime") {
            if let Some(uptime_str) = content.split_whitespace().next() {
                if let Ok(uptime_secs) = uptime_str.parse::<f64>() {
                    self.boot_time.uptime = Duration::from_secs_f64(uptime_secs);
                    uptime_read = true;
                }
            }
        }

        // Try to get detailed boot time from systemd
        if let Ok(output) = std::process::Command::new("systemd-analyze").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse "Startup finished in 3.456s (firmware) + 2.345s (loader) + 1.234s (kernel) + 5.678s (userspace) = 12.713s"
            for part in stdout.split('+') {
                // The last part carries the total after '=', which is not a phase.
                let part = part.split('=').next().unwrap_or("").trim();
                if let Some(secs) = parse_systemd_duration(part) {
                    let duration = Duration::from_secs_f64(secs);
                    if part.contains("firmware") {
                        self.boot_time.firmware = Some(duration);
                    } else if part.contains("loader") {
                        self.boot_time.bootloader = Some(duration);
                    } else if part.contains("kernel") {
                        self.boot_time.kernel = Some(duration);
                    } else if part.contains("userspace") {
                        self.boot_time.userspace = Some(duration);
                    }
                }
            }
            // Parse total time. Only the rest of the first line: the next line,
            // "graphical.target reached after 10.1s in userspace", would add to it.
            if let Some(eq_pos) = stdout.find('=') {
                if let Some(secs) =
                    parse_systemd_duration(stdout[eq_pos + 1..].lines().next().unwrap_or(""))
                {
                    self.boot_time.total = Duration::from_secs_f64(secs);
                }
            }
        }

        // Boot timestamp is now minus uptime, so it exists only when uptime was
        // read. Subtracting an unread (zero) uptime reported boot as happening now.
        if uptime_read {
            let now = chrono::Utc::now();
            self.boot_time.boot_timestamp =
                Some(now - chrono::Duration::from_std(self.boot_time.uptime).unwrap_or_default());
        }

        Ok(())
    }

    /// Status of one freedesktop autostart entry, read from the file itself.
    ///
    /// `Hidden=true` removes the entry, and `X-GNOME-Autostart-enabled=false`
    /// disables it in GNOME sessions. Neither was read, so every `.desktop`
    /// file in an autostart directory showed as enabled. A file that cannot be
    /// read is `Unknown`, not `Enabled`.
    #[cfg(target_os = "linux")]
    fn desktop_autostart_status(path: &std::path::Path) -> StartupItemStatus {
        let Ok(content) = std::fs::read_to_string(path) else {
            return StartupItemStatus::Unknown;
        };
        let disabled = content.lines().any(|line| {
            let Some((key, value)) = line.split_once('=') else {
                return false;
            };
            matches!(
                (key.trim(), value.trim()),
                ("Hidden", "true") | ("X-GNOME-Autostart-enabled", "false")
            )
        });
        if disabled {
            StartupItemStatus::Disabled
        } else {
            StartupItemStatus::Enabled
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_read_startup_items(&mut self) -> Result<()> {
        use std::process::Command;

        self.startup_items.clear();

        // Get enabled services from systemd
        let output = Command::new("systemctl")
            .args([
                "list-unit-files",
                "--type=service",
                "--no-pager",
                "--no-legend",
            ])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[0].trim_end_matches(".service").to_string();
                    let status = match parts[1] {
                        "enabled" | "enabled-runtime" => StartupItemStatus::Enabled,
                        "disabled" => StartupItemStatus::Disabled,
                        _ => StartupItemStatus::Unknown,
                    };

                    self.startup_items.push(StartupItem {
                        name,
                        description: None,
                        item_type: StartupItemType::Service,
                        status,
                        command: None,
                        publisher: None,
                        location: Some("/etc/systemd/system".to_string()),
                        boot_impact: None,
                    });
                }
            }
        }

        // Get autostart desktop applications
        let autostart_dirs = [
            "/etc/xdg/autostart",
            &format!(
                "{}/.config/autostart",
                std::env::var("HOME").unwrap_or_default()
            ),
        ];

        for dir in &autostart_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".desktop") {
                            let status = Self::desktop_autostart_status(&entry.path());
                            self.startup_items.push(StartupItem {
                                name: name.trim_end_matches(".desktop").to_string(),
                                description: None,
                                item_type: StartupItemType::Application,
                                status,
                                command: None,
                                publisher: None,
                                location: Some(dir.to_string()),
                                boot_impact: None,
                            });
                        }
                    }
                }
            }
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_read_kernel_params(&mut self) -> Result<()> {
        use std::fs;

        if let Ok(cmdline) = fs::read_to_string("/proc/cmdline") {
            self.kernel_params.cmdline = cmdline.trim().to_string();
            self.kernel_params.params.clear();

            for part in cmdline.split_whitespace() {
                if let Some((key, value)) = part.split_once('=') {
                    self.kernel_params
                        .params
                        .push((key.to_string(), Some(value.to_string())));

                    match key {
                        "root" => self.kernel_params.root = Some(value.to_string()),
                        "init" => self.kernel_params.init = Some(value.to_string()),
                        _ => {}
                    }
                } else {
                    self.kernel_params.params.push((part.to_string(), None));
                    match part {
                        "quiet" => self.kernel_params.quiet = true,
                        "splash" => self.kernel_params.splash = true,
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    // ==================== Windows Implementation ====================

    #[cfg(windows)]
    fn windows_read_boot_info(&mut self) -> Result<()> {
        // Firmware type and Secure Boot state.
        //
        // Both were read by spawning PowerShell, and both were wrong. The firmware
        // check tested whether `HKLM\...\Control\SecureBoot\State` *exists* and
        // reported `SecureBoot` when it did — but that key is present on every UEFI
        // machine whether or not Secure Boot is enforcing, so every UEFI system was
        // reported as Secure Boot. The correction below it only ever upgraded
        // `boot_type`, never downgraded it, so `boot_type: SecureBoot` shipped
        // alongside `secure_boot: false` on the same struct.
        //
        // This host is UEFI with `UEFISecureBootEnabled = 0`, and now reports that.
        //
        // The `.unwrap_or(false)` that used to close this line put the defect
        // back one line under the paragraph describing it: `secure_boot_enabled`
        // returns `Option<bool>` precisely because the registry value can be
        // missing, and collapsing that to `false` says Secure Boot is off on a
        // machine nobody managed to ask.
        self.boot_info.secure_boot = crate::platform::windows::secure_boot_enabled();

        self.boot_info.boot_type = match crate::platform::windows::firmware_type() {
            Some(BootType::Uefi) if self.boot_info.secure_boot == Some(true) => {
                Some(BootType::SecureBoot)
            }
            Some(kind) => Some(kind),
            // `GetFirmwareType` can fail without telling us which firmware this is;
            // claiming Legacy on a failed query is how the old code got it wrong.
            None => None,
        };

        // Get boot device.
        //
        // Was `wmic os get SystemDevice`, which cannot run on Windows 11 24H2 and
        // later — Microsoft removed the tool, so this silently left `boot_device` at
        // `None` on every current build. `GetSystemDirectoryW` is a plain Win32 call
        // and needs no subprocess.
        {
            use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;

            let mut buffer = [0u16; 260];
            let len = unsafe { GetSystemWindowsDirectoryW(Some(&mut buffer)) } as usize;
            if len > 0 && len <= buffer.len() {
                let dir = String::from_utf16_lossy(&buffer[..len]);
                // "C:\Windows" -> "C:"; anything without a drive letter (a UNC boot,
                // for instance) is left unreported rather than guessed at.
                if let Some((drive, _)) = dir.split_once('\\') {
                    if drive.ends_with(':') {
                        self.boot_info.boot_device = Some(drive.to_string());
                    }
                }
            }
        }

        // Bootloader: left unset. This used to be the literal "Windows Boot
        // Manager" on every Windows machine, which was assumed, not read.

        Ok(())
    }

    #[cfg(windows)]
    fn windows_read_boot_time(&mut self) -> Result<()> {
        use std::process::Command;

        // Get uptime via PowerShell
        let output = Command::new("powershell")
            .args(["-NoProfile",
                "-Command",
                "(Get-Date) - (Get-CimInstance Win32_OperatingSystem).LastBootUpTime | Select-Object -ExpandProperty TotalSeconds"
            ])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(secs) = stdout.parse::<f64>() {
                self.boot_time.uptime = Duration::from_secs_f64(secs);
            }
        }

        // Get last boot time. The `Z` suffix claims UTC, so the value must be
        // converted to UTC first: `Get-Date -Format` wrote the local clock
        // under that suffix, which is wrong by the machine's UTC offset.
        let output = Command::new("powershell")
            .args(["-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ', [System.Globalization.CultureInfo]::InvariantCulture)"
            ])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&stdout) {
                self.boot_time.boot_timestamp = Some(dt.with_timezone(&chrono::Utc));
            }
        }

        // Boot duration, from the only Windows source that actually measures one.
        //
        // This used to subtract `Win32_OperatingSystem.LastBootUpTime` from the
        // timestamp of System event 12 (Kernel-General, "the operating system
        // started at"). Those are two records of the *same* instant, so the
        // difference is clock skew between how they are written down, not a boot.
        // On the machine this was found on it returned 0.61 seconds, and the
        // `secs > 0.0` guard waved it through as a measurement. A boot duration of
        // 0.6s is not a plausible reading of anything; it was a confident wrong
        // answer, and worse than the 45-second constant it replaced because it
        // looks derived rather than invented.
        //
        // Windows records a real boot duration in exactly one place:
        // Microsoft-Windows-Diagnostics-Performance/Operational event 100, whose
        // `BootTime` and `MainPathBootTime` fields the boot tracing subsystem
        // writes. That log is not readable without Administrator -- verified two
        // ways, `Get-WinEvent` and `wevtutil`, both denied unelevated. So an
        // ordinary run leaves `total` at `Duration::ZERO`, which the surrounding
        // code already reads as "not measured", and an elevated run gets the
        // genuine article.
        let output = Command::new("powershell")
            .args(["-NoProfile",
                "-Command",
                "try { $e = Get-WinEvent -LogName 'Microsoft-Windows-Diagnostics-Performance/Operational' -MaxEvents 1 -FilterXPath \"*[System[(EventID=100)]]\" -ErrorAction Stop; $x = [xml]$e.ToXml(); ($x.Event.EventData.Data | Where-Object { $_.Name -eq 'BootTime' }).'#text' } catch { }"
            ])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // The field is milliseconds. Empty stdout is the unelevated case, and
            // it has to stay empty rather than parsing to a zero that reads as a
            // measurement of zero.
            if let Ok(ms) = stdout.parse::<f64>() {
                if ms > 0.0 {
                    self.boot_time.total = Duration::from_secs_f64(ms / 1000.0);
                }
            }
        }

        // No fallback. This used to assign 45 seconds — a number chosen as a "typical"
        // boot time — whenever the real duration could not be read, so a failed
        // measurement was indistinguishable from a machine that genuinely took 45s.
        // `Duration::ZERO` is what the code above already tests for as "not measured".

        Ok(())
    }

    #[cfg(windows)]
    fn windows_read_startup_items(&mut self) -> Result<()> {
        use std::process::Command;

        self.startup_items.clear();

        // Get startup items from Task Manager equivalent
        let output = Command::new("powershell")
            .args(["-NoProfile",
                "-Command",
                "Get-CimInstance Win32_StartupCommand | Select-Object Name,Command,Location,User | ConvertTo-Csv -NoTypeInformation"
            ])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1) {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 3 {
                    let name = parts[0].trim_matches('"').to_string();
                    let command = parts[1].trim_matches('"').to_string();
                    let location = parts[2].trim_matches('"').to_string();

                    let item_type = if location.contains("Registry") {
                        StartupItemType::Registry
                    } else if location.contains("Startup") {
                        StartupItemType::Application
                    } else {
                        StartupItemType::Unknown
                    };

                    self.startup_items.push(StartupItem {
                        name,
                        description: None,
                        item_type,
                        // Win32_StartupCommand lists the Run-key entries whether
                        // or not Task Manager has disabled them; that state lives
                        // in the StartupApproved keys, which are not read here.
                        status: StartupItemStatus::Unknown,
                        command: Some(command),
                        publisher: None,
                        location: Some(location),
                        boot_impact: None,
                    });
                }
            }
        }

        // Get scheduled tasks that run at startup
        let output = Command::new("schtasks")
            .args(["/query", "/fo", "csv", "/v"])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            self.startup_items.extend(startup_tasks_from_csv(&stdout));
        }

        Ok(())
    }

    // ==================== macOS Implementation ====================

    #[cfg(target_os = "macos")]
    fn macos_read_boot_info(&mut self) -> Result<()> {
        use std::process::Command;

        // macOS on Apple Silicon is always UEFI-like
        // Intel Macs can be checked via nvram
        let _output = Command::new("nvram").arg("boot-args").output();

        // Boot type is not read on macOS. This used to set `Uefi` for every
        // Mac, on the assumption that all modern Macs boot via EFI.
        self.boot_info.boot_type = None;

        // Secure Boot is not read on macOS.
        //
        // This ran `csrutil status` and set `secure_boot` from whether the
        // output contained "enabled". `csrutil status` reports **System
        // Integrity Protection**, which is a runtime kernel protection and a
        // different thing from the boot policy: an Apple silicon Mac can have
        // SIP on and its boot policy set to Reduced or Permissive, and it would
        // still have been reported here as Secure Boot. The boot policy is read
        // with `bputil -d` (which needs authentication) or from the
        // `boot-policy` NVRAM variables; until one of those is called this
        // stays `None`.

        // Bootloader: not read. This used to be the literal "iBoot" on every Mac.

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn macos_read_boot_time(&mut self) -> Result<()> {
        use std::process::Command;

        // Get boot time via sysctl
        let output = Command::new("sysctl")
            .args(["-n", "kern.boottime"])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse "{ sec = 1234567890, usec = 123456 }"
            if let Some(sec_start) = stdout.find("sec = ") {
                let sec_str = &stdout[sec_start + 6..];
                if let Some(sec_end) = sec_str.find(',') {
                    if let Ok(boot_secs) = sec_str[..sec_end].trim().parse::<i64>() {
                        let boot_time =
                            chrono::DateTime::<chrono::Utc>::from_timestamp(boot_secs, 0);
                        self.boot_time.boot_timestamp = boot_time;

                        let now = chrono::Utc::now();
                        if let Some(bt) = boot_time {
                            self.boot_time.uptime = (now - bt).to_std().unwrap_or(Duration::ZERO);
                        }
                    }
                }
            }
        }

        // Get uptime as fallback
        if self.boot_time.uptime == Duration::ZERO {
            let output = Command::new("uptime").output();
            if let Ok(output) = output {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse uptime output
                if let Some(up_pos) = stdout.find("up ") {
                    let uptime_str = &stdout[up_pos + 3..];
                    // Very basic parsing
                    if uptime_str.contains("day") {
                        // Has days
                        if let Some(days_str) = uptime_str.split_whitespace().next() {
                            if let Ok(days) = days_str.parse::<u64>() {
                                self.boot_time.uptime = Duration::from_secs(days * 86400);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn macos_read_startup_items(&mut self) -> Result<()> {
        use std::process::Command;

        self.startup_items.clear();

        // Get launch agents/daemons
        let dirs = [
            "/Library/LaunchAgents",
            "/Library/LaunchDaemons",
            "/System/Library/LaunchAgents",
            "/System/Library/LaunchDaemons",
        ];

        for dir in &dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".plist") {
                            let item_type = if dir.contains("Daemon") {
                                StartupItemType::Service
                            } else {
                                StartupItemType::Application
                            };

                            self.startup_items.push(StartupItem {
                                name: name.trim_end_matches(".plist").to_string(),
                                description: None,
                                item_type,
                                // A plist in a launch directory is installed, not
                                // necessarily enabled: `launchctl disable` and the
                                // plist's own Disabled key are not read here.
                                status: StartupItemStatus::Unknown,
                                command: None,
                                publisher: None,
                                location: Some(dir.to_string()),
                                boot_impact: None,
                            });
                        }
                    }
                }
            }
        }

        // Get login items via osascript
        let output = Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to get name of login items",
            ])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for name in stdout.split(',') {
                let name = name.trim();
                if !name.is_empty() {
                    self.startup_items.push(StartupItem {
                        name: name.to_string(),
                        description: None,
                        item_type: StartupItemType::Application,
                        status: StartupItemStatus::Enabled,
                        command: None,
                        publisher: None,
                        location: Some("Login Items".to_string()),
                        boot_impact: None,
                    });
                }
            }
        }

        Ok(())
    }
}

impl Default for BootMonitor {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            boot_time: BootTime {
                total: Duration::ZERO,
                firmware: None,
                bootloader: None,
                kernel: None,
                userspace: None,
                boot_timestamp: None,
                uptime: Duration::ZERO,
            },
            boot_info: BootInfo {
                boot_type: None,
                secure_boot: None,
                boot_device: None,
                boot_partition: None,
                efi_partition: None,
                bootloader: None,
                bootloader_version: None,
            },
            startup_items: Vec::new(),
            kernel_params: KernelParams::default(),
        })
    }
}

/// Sum every `<number><unit>` token in a systemd-analyze duration into seconds.
///
/// systemd writes durations such as `412ms`, `3.456s`, `2min 30.123s` and
/// `1h 2min 3s`. The previous extractor stopped at the `min` of `2min`, so a
/// boot longer than a minute lost its seconds: `2min 31.3s` read as 120 seconds.
/// Tokens that are not a number followed by a known unit are skipped.
#[cfg(any(target_os = "linux", test))]
fn parse_systemd_duration(text: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut found = false;
    for token in text.split_whitespace() {
        let Some(unit_at) = token.find(|c: char| c.is_ascii_alphabetic()) else {
            continue;
        };
        let (number, unit) = token.split_at(unit_at);
        let Ok(value) = number.parse::<f64>() else {
            continue;
        };
        let scale = match unit {
            "h" => 3600.0,
            "min" => 60.0,
            "s" => 1.0,
            "ms" => 0.001,
            "us" => 0.000_001,
            _ => continue,
        };
        total += value * scale;
        found = true;
    }
    found.then_some(total)
}

/// Split one CSV record. Double quotes group a field, and `""` inside one is a
/// literal quote, so a quoted command line may contain commas.
#[cfg(any(windows, test))]
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    fields.push(current);
    fields
}

/// Startup and logon scheduled tasks from `schtasks /query /fo csv /v`.
///
/// Columns are found by their header names, not by position. The old code
/// tested column 7, which is `Author`, for "logon", so almost no startup task
/// was listed. A header that does not match, as on a localised Windows, yields
/// no tasks rather than tasks read from the wrong column.
#[cfg(any(windows, test))]
fn startup_tasks_from_csv(stdout: &str) -> Vec<StartupItem> {
    let mut lines = stdout.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let columns = split_csv_line(header);
    let column = |name: &str| columns.iter().position(|c| c == name);
    let (Some(name_at), Some(status_at), Some(schedule_at), Some(command_at)) = (
        column("TaskName"),
        column("Status"),
        column("Schedule Type"),
        column("Task To Run"),
    ) else {
        return Vec::new();
    };

    let mut items = Vec::new();
    for line in lines {
        let fields = split_csv_line(line);
        let field = |at: usize| fields.get(at).map(String::as_str).unwrap_or("");
        // The schedule type names the trigger: "At logon time", "At system start up".
        let schedule = field(schedule_at).to_ascii_lowercase();
        let at_startup = schedule.contains("logon")
            || schedule.contains("start up")
            || schedule.contains("startup")
            || schedule.contains("boot");
        if !at_startup {
            continue;
        }
        let status = match field(status_at) {
            "Ready" | "Running" => StartupItemStatus::Enabled,
            "Disabled" => StartupItemStatus::Disabled,
            _ => StartupItemStatus::Unknown,
        };
        items.push(StartupItem {
            name: field(name_at).to_string(),
            description: None,
            item_type: StartupItemType::ScheduledTask,
            status,
            command: Some(field(command_at).to_string()).filter(|c| !c.is_empty()),
            publisher: None,
            location: Some("Task Scheduler".to_string()),
            boot_impact: None,
        });
    }
    items
}

/// Boot configuration summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootSummary {
    /// Boot type; `None` where it was not read.
    pub boot_type: Option<BootType>,
    /// Whether Secure Boot is enforcing; `None` if not established.
    pub secure_boot: Option<bool>,
    /// Total boot time
    pub boot_time_secs: f64,
    /// System uptime
    pub uptime_secs: f64,
    /// Number of enabled startup items
    pub enabled_startup_items: usize,
    /// Number of disabled startup items
    pub disabled_startup_items: usize,
}

/// Get a quick boot summary
pub fn boot_summary() -> Result<BootSummary> {
    let monitor = BootMonitor::new()?;

    Ok(BootSummary {
        boot_type: monitor.boot_info.boot_type,
        secure_boot: monitor.boot_info.secure_boot,
        boot_time_secs: monitor.boot_time.total.as_secs_f64(),
        uptime_secs: monitor.boot_time.uptime.as_secs_f64(),
        enabled_startup_items: monitor.enabled_count(),
        disabled_startup_items: monitor.disabled_count(),
    })
}

/// Format uptime as human-readable string
pub fn format_uptime(uptime: Duration) -> String {
    BootTime::format_duration(uptime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(BootTime::format_duration(Duration::from_secs(30)), "30.0s");
        assert_eq!(BootTime::format_duration(Duration::from_secs(90)), "1m 30s");
        assert_eq!(
            BootTime::format_duration(Duration::from_secs(3700)),
            "1h 1m"
        );
        assert_eq!(
            BootTime::format_duration(Duration::from_secs(90000)),
            "1d 1h"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn desktop_autostart_honours_hidden_and_gnome_flags() {
        let dir = std::env::temp_dir().join(format!("ironmon-autostart-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, body: &str| {
            let path = dir.join(name);
            std::fs::write(&path, body).unwrap();
            path
        };
        let plain = write(
            "plain.desktop",
            "[Desktop Entry]\nType=Application\nName=Plain\n",
        );
        let hidden = write("hidden.desktop", "[Desktop Entry]\nHidden=true\n");
        let gnome_off = write(
            "gnome.desktop",
            "[Desktop Entry]\nX-GNOME-Autostart-enabled=false\n",
        );
        let hidden_false = write("hidden_false.desktop", "[Desktop Entry]\nHidden=false\n");

        assert_eq!(
            BootMonitor::desktop_autostart_status(&plain),
            StartupItemStatus::Enabled
        );
        assert_eq!(
            BootMonitor::desktop_autostart_status(&hidden),
            StartupItemStatus::Disabled
        );
        assert_eq!(
            BootMonitor::desktop_autostart_status(&gnome_off),
            StartupItemStatus::Disabled
        );
        assert_eq!(
            BootMonitor::desktop_autostart_status(&hidden_false),
            StartupItemStatus::Enabled
        );
        assert_eq!(
            BootMonitor::desktop_autostart_status(&dir.join("missing.desktop")),
            StartupItemStatus::Unknown
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn systemd_duration_keeps_minutes_and_seconds() {
        let close = |got: Option<f64>, want: f64| got.is_some_and(|v| (v - want).abs() < 1e-9);
        // Captured shapes of `systemd-analyze` output.
        assert!(close(parse_systemd_duration("2min 31.3s"), 151.3));
        assert!(close(parse_systemd_duration("3.456s (firmware)"), 3.456));
        assert!(close(parse_systemd_duration("412ms (kernel)"), 0.412));
        assert!(close(parse_systemd_duration("1h 2min 3s"), 3723.0));
        assert_eq!(parse_systemd_duration("no time here"), None);
    }

    #[test]
    fn csv_quotes_group_commas_and_double_quotes_are_literal() {
        assert_eq!(split_csv_line(r#""x,y",z"#), vec!["x,y", "z"]);
        assert_eq!(
            split_csv_line(r#"a,"he said ""hi""",b"#),
            vec!["a", r#"he said "hi""#, "b"]
        );
    }

    #[test]
    fn startup_tasks_are_chosen_by_header_not_position() {
        // Captured `schtasks /query /fo csv /v` layout, abridged to four columns
        // used here. The old code tested the `Author` column for "logon".
        let csv = r#""TaskName","Status","Author","Task To Run","Schedule Type"
"\Logon Task","Ready","Contoso","C:\Tools\a.exe --x=1,2","At logon time"
"\Boot Task","Disabled","Contoso","C:\c.exe","At system start up"
"\Idle Task","Ready","Contoso","C:\d.exe","At idle time"
"\Daily Task","Ready","logon author","C:\b.exe","Daily"
"#;
        let items = startup_tasks_from_csv(csv);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec![r"\Logon Task", r"\Boot Task"]);
        assert_eq!(items[0].status, StartupItemStatus::Enabled);
        assert_eq!(items[0].command.as_deref(), Some(r"C:\Tools\a.exe --x=1,2"));
        assert_eq!(items[1].status, StartupItemStatus::Disabled);
        // A header without these columns, as on a localised Windows, yields none.
        assert!(startup_tasks_from_csv(r#""Nom de tâche","État""#).is_empty());
    }
}
