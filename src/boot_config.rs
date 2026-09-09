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
    /// Boot type (Legacy/UEFI/SecureBoot)
    pub boot_type: BootType,
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
    /// Bootloader name
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
        let mut monitor = Self {
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
                boot_type: BootType::Unknown,
                secure_boot: None,
                boot_device: None,
                boot_partition: None,
                efi_partition: None,
                bootloader: None,
                bootloader_version: None,
            },
            startup_items: Vec::new(),
            kernel_params: KernelParams::default(),
        };

        monitor.refresh()?;
        Ok(monitor)
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
            BootType::Uefi | BootType::SecureBoot
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
            self.boot_info.boot_type = match self.boot_info.secure_boot {
                Some(true) => BootType::SecureBoot,
                _ => BootType::Uefi,
            };
        } else {
            self.boot_info.boot_type = BootType::Legacy;
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
        if let Ok(content) = fs::read_to_string("/proc/uptime") {
            if let Some(uptime_str) = content.split_whitespace().next() {
                if let Ok(uptime_secs) = uptime_str.parse::<f64>() {
                    self.boot_time.uptime = Duration::from_secs_f64(uptime_secs);
                }
            }
        }

        // Try to get detailed boot time from systemd
        if let Ok(output) = std::process::Command::new("systemd-analyze").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse "Startup finished in 3.456s (firmware) + 2.345s (loader) + 1.234s (kernel) + 5.678s (userspace) = 12.713s"
            for part in stdout.split('+') {
                let part = part.trim();
                if let Some(time_str) = extract_time_value(part) {
                    if let Ok(secs) = parse_time_string(&time_str) {
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
            }

            // Parse total time
            if let Some(eq_pos) = stdout.find('=') {
                let total_part = &stdout[eq_pos + 1..].trim();
                if let Some(time_str) = extract_time_value(total_part) {
                    if let Ok(secs) = parse_time_string(&time_str) {
                        self.boot_time.total = Duration::from_secs_f64(secs);
                    }
                }
            }
        }

        // Calculate boot timestamp from uptime
        let now = chrono::Utc::now();
        self.boot_time.boot_timestamp =
            Some(now - chrono::Duration::from_std(self.boot_time.uptime).unwrap_or_default());

        Ok(())
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
                            self.startup_items.push(StartupItem {
                                name: name.trim_end_matches(".desktop").to_string(),
                                description: None,
                                item_type: StartupItemType::Application,
                                status: StartupItemStatus::Enabled,
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
                BootType::SecureBoot
            }
            Some(kind) => kind,
            // `GetFirmwareType` can fail without telling us which firmware this is;
            // claiming Legacy on a failed query is how the old code got it wrong.
            None => BootType::Unknown,
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

        // Get bootloader info
        self.boot_info.bootloader = Some("Windows Boot Manager".to_string());

        Ok(())
    }

    #[cfg(windows)]
    fn windows_read_boot_time(&mut self) -> Result<()> {
        use std::process::Command;

        // Get uptime via PowerShell
        let output = Command::new("powershell")
            .args([
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

        // Get last boot time
        let output = Command::new("powershell")
            .args([
                "-Command",
                "(Get-CimInstance Win32_OperatingSystem).LastBootUpTime | Get-Date -Format 'yyyy-MM-ddTHH:mm:ssZ'"
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
            .args([
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
            .args([
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
                        status: StartupItemStatus::Enabled,
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
            for line in stdout.lines().skip(1) {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 9 {
                    let name = parts[1].trim_matches('"').to_string();
                    let trigger = parts.get(7).unwrap_or(&"").trim_matches('"');

                    // Only include startup/logon tasks
                    if trigger.contains("logon")
                        || trigger.contains("startup")
                        || trigger.contains("boot")
                    {
                        let status = parts.get(3).unwrap_or(&"").trim_matches('"');
                        let status = if status == "Ready" || status == "Running" {
                            StartupItemStatus::Enabled
                        } else if status == "Disabled" {
                            StartupItemStatus::Disabled
                        } else {
                            StartupItemStatus::Unknown
                        };

                        self.startup_items.push(StartupItem {
                            name,
                            description: None,
                            item_type: StartupItemType::ScheduledTask,
                            status,
                            command: parts.get(8).map(|s| s.trim_matches('"').to_string()),
                            publisher: None,
                            location: Some("Task Scheduler".to_string()),
                            boot_impact: None,
                        });
                    }
                }
            }
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

        // All modern Macs boot via EFI
        self.boot_info.boot_type = BootType::Uefi;

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

        self.boot_info.bootloader = Some("iBoot".to_string());

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
                                status: StartupItemStatus::Enabled,
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
                boot_type: BootType::Unknown,
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

/// Helper function to extract time value from systemd-analyze output
#[cfg(target_os = "linux")]
fn extract_time_value(s: &str) -> Option<String> {
    // Find time value like "3.456s" or "2min 30.123s"
    let mut result = String::new();
    let mut in_number = false;

    for c in s.chars() {
        if c.is_numeric() || c == '.' {
            in_number = true;
            result.push(c);
        } else if in_number && (c == 's' || c == 'm') {
            result.push(c);
            if c == 's' {
                break;
            }
        } else if in_number && c == 'i' {
            // "min"
            result.push_str("in");
            break;
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Helper function to parse time string
#[cfg(target_os = "linux")]
fn parse_time_string(s: &str) -> std::result::Result<f64, std::num::ParseFloatError> {
    if s.ends_with("min") {
        let num = s.trim_end_matches("min").trim();
        num.parse::<f64>().map(|v| v * 60.0)
    } else if s.ends_with('s') {
        s.trim_end_matches('s').trim().parse()
    } else {
        s.parse()
    }
}

/// Boot configuration summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootSummary {
    /// Boot type
    pub boot_type: BootType,
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
}
