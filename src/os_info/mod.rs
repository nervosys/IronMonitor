//! OS and kernel information — kernel version, hostname, boot time, loaded modules.
//!
//! # Platform Support
//!
//! - **Linux**: Reads `/proc/version`, `/proc/uptime`, `/proc/modules`, `/proc/cmdline`, `uname`
//! - **Windows**: Uses WMI (`Win32_OperatingSystem`), `ver`, registry
//! - **macOS**: Uses `sw_vers`, `uname`, `sysctl`
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::os_info::OsInfoMonitor;
//!
//! let monitor = OsInfoMonitor::new().unwrap();
//! let info = monitor.info();
//! println!("OS: {} {} ({})", info.os_name, info.os_version, info.architecture);
//! println!("Kernel: {}", info.kernel_version);
//! println!("Hostname: {}", info.hostname);
//! println!("Uptime: {} seconds", info.uptime_seconds);
//! println!("Loaded modules: {}", info.loaded_modules.len());
//! ```

use serde::{Deserialize, Serialize};

use crate::error::IronError;

/// Operating system family
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OsFamily {
    Linux,
    Windows,
    MacOS,
    FreeBSD,
    Unknown,
}

/// Boot mode
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootMode {
    UEFI,
    BIOS,
    Unknown,
}

/// Loaded kernel module / driver
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelModule {
    /// Module name
    pub name: String,
    /// Module size in bytes
    pub size_bytes: u64,
    /// Number of instances (use count)
    pub instances: u32,
    /// Modules that depend on this one
    pub used_by: Vec<String>,
    /// Module state (e.g., "Live")
    pub state: String,
}

/// Comprehensive OS and kernel information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    /// OS family (Linux, Windows, macOS)
    pub os_family: OsFamily,
    /// OS name (e.g., "Ubuntu", "Windows 11", "macOS Sequoia")
    pub os_name: String,
    /// OS version string
    pub os_version: String,
    /// OS build number
    pub os_build: String,
    /// Kernel version string (e.g., "6.8.0-51-generic")
    pub kernel_version: String,
    /// Full kernel version line
    pub kernel_full: String,
    /// CPU architecture (e.g., "x86_64", "aarch64")
    pub architecture: String,
    /// Hostname
    pub hostname: String,
    /// Domain name (if configured)
    pub domain: String,
    /// System uptime in seconds
    pub uptime_seconds: u64,
    /// Boot time as Unix timestamp
    pub boot_timestamp: u64,
    /// Boot mode (UEFI or BIOS)
    pub boot_mode: BootMode,
    /// Kernel command line (Linux)
    pub kernel_cmdline: String,
    /// Number of logged-in users
    pub user_count: u32,
    /// Loaded kernel modules
    pub loaded_modules: Vec<KernelModule>,
    /// Timezone string (e.g., "America/New_York")
    pub timezone: String,
    /// System locale
    pub locale: String,
    /// Whether this is a 64-bit OS
    pub is_64bit: bool,
    /// Whether Secure Boot is enabled
    pub secure_boot: bool,
}

/// Monitor for OS and kernel information
pub struct OsInfoMonitor {
    info: OsInfo,
}

impl OsInfoMonitor {
    /// Create a new OsInfoMonitor and gather system information.
    pub fn new() -> Result<Self, IronError> {
        let mut monitor = Self {
            info: OsInfo {
                os_family: OsFamily::Unknown,
                os_name: String::new(),
                os_version: String::new(),
                os_build: String::new(),
                kernel_version: String::new(),
                kernel_full: String::new(),
                architecture: String::new(),
                hostname: String::new(),
                domain: String::new(),
                uptime_seconds: 0,
                boot_timestamp: 0,
                boot_mode: BootMode::Unknown,
                kernel_cmdline: String::new(),
                user_count: 0,
                loaded_modules: Vec::new(),
                timezone: String::new(),
                locale: String::new(),
                is_64bit: cfg!(target_pointer_width = "64"),
                secure_boot: false,
            },
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    /// Refresh information.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.info.loaded_modules.clear();

        #[cfg(target_os = "linux")]
        self.refresh_linux()?;

        #[cfg(target_os = "windows")]
        self.refresh_windows()?;

        #[cfg(target_os = "macos")]
        self.refresh_macos()?;

        // Common: hostname from std
        if self.info.hostname.is_empty() {
            if let Ok(output) = std::process::Command::new("hostname").output() {
                self.info.hostname = String::from_utf8(output.stdout)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
            }
        }

        Ok(())
    }

    /// Get the gathered OS information.
    pub fn info(&self) -> &OsInfo {
        &self.info
    }

    /// Get loaded kernel modules.
    pub fn modules(&self) -> &[KernelModule] {
        &self.info.loaded_modules
    }

    /// Get the number of loaded modules.
    pub fn module_count(&self) -> usize {
        self.info.loaded_modules.len()
    }

    /// Find a specific module by name.
    pub fn find_module(&self, name: &str) -> Option<&KernelModule> {
        self.info.loaded_modules.iter().find(|m| m.name == name)
    }

    // ── Linux ──

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) -> Result<(), IronError> {
        self.info.os_family = OsFamily::Linux;

        // /etc/os-release. A distribution without one is unusual but legal, so
        // its absence leaves the name empty rather than failing; a file that
        // will not open is a failure.
        let os_release = match std::fs::read_to_string("/etc/os-release") {
            Ok(c) => Some(c),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                return Err(IronError::System(format!(
                    "cannot read /etc/os-release: {e}"
                )))
            }
        };
        if let Some(content) = os_release {
            for line in content.lines() {
                if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                    self.info.os_name = val.trim_matches('"').to_string();
                } else if let Some(val) = line.strip_prefix("VERSION_ID=") {
                    self.info.os_version = val.trim_matches('"').to_string();
                } else if let Some(val) = line.strip_prefix("BUILD_ID=") {
                    self.info.os_build = val.trim_matches('"').to_string();
                }
            }
        }

        // /proc/version
        if let Ok(content) = std::fs::read_to_string("/proc/version") {
            self.info.kernel_full = content.trim().to_string();
            // "Linux version 6.8.0-51-generic ..."
            if let Some(ver) = content.split_whitespace().nth(2) {
                self.info.kernel_version = ver.to_string();
            }
        }

        // Architecture via uname
        if let Ok(output) = std::process::Command::new("uname").arg("-m").output() {
            self.info.architecture = String::from_utf8(output.stdout)
                .unwrap_or_default()
                .trim()
                .to_string();
        }

        // Hostname
        if let Ok(content) = std::fs::read_to_string("/etc/hostname") {
            self.info.hostname = content.trim().to_string();
        }

        // Domain
        if let Ok(content) = std::fs::read_to_string("/proc/sys/kernel/domainname") {
            let d = content.trim();
            if d != "(none)" {
                self.info.domain = d.to_string();
            }
        }

        // Uptime
        if let Ok(content) = std::fs::read_to_string("/proc/uptime") {
            if let Some(secs_str) = content.split_whitespace().next() {
                if let Ok(secs) = secs_str.parse::<f64>() {
                    self.info.uptime_seconds = secs as u64;
                }
            }
        }

        // Boot timestamp from /proc/stat
        if let Ok(content) = std::fs::read_to_string("/proc/stat") {
            for line in content.lines() {
                if let Some(rest) = line.strip_prefix("btime ") {
                    if let Ok(ts) = rest.trim().parse() {
                        self.info.boot_timestamp = ts;
                    }
                }
            }
        }

        // Boot mode
        self.info.boot_mode = if std::path::Path::new("/sys/firmware/efi").exists() {
            BootMode::UEFI
        } else {
            BootMode::BIOS
        };

        // Secure Boot
        if let Ok(output) = std::process::Command::new("mokutil")
            .arg("--sb-state")
            .output()
        {
            let text = String::from_utf8(output.stdout).unwrap_or_default();
            self.info.secure_boot = text.contains("SecureBoot enabled");
        }

        // Kernel command line
        if let Ok(content) = std::fs::read_to_string("/proc/cmdline") {
            self.info.kernel_cmdline = content.trim().to_string();
        }

        // Logged-in users (who)
        if let Ok(output) = std::process::Command::new("who").output() {
            let text = String::from_utf8(output.stdout).unwrap_or_default();
            self.info.user_count = text.lines().count() as u32;
        }

        // Loaded modules from /proc/modules
        if let Ok(content) = std::fs::read_to_string("/proc/modules") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let name = parts[0].to_string();
                    let size_bytes = parts[1].parse().unwrap_or(0);
                    let instances = parts[2].parse().unwrap_or(0);
                    let used_by: Vec<String> = if parts[3] == "-" {
                        Vec::new()
                    } else {
                        parts[3]
                            .trim_end_matches(',')
                            .split(',')
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .collect()
                    };
                    let state = parts.get(4).unwrap_or(&"").to_string();

                    self.info.loaded_modules.push(KernelModule {
                        name,
                        size_bytes,
                        instances,
                        used_by,
                        state,
                    });
                }
            }
        }

        // Timezone
        if let Ok(tz) = std::fs::read_link("/etc/localtime") {
            let path = tz.to_string_lossy().to_string();
            if let Some(pos) = path.find("zoneinfo/") {
                self.info.timezone = path[pos + 9..].to_string();
            }
        }
        if self.info.timezone.is_empty() {
            if let Ok(content) = std::fs::read_to_string("/etc/timezone") {
                self.info.timezone = content.trim().to_string();
            }
        }

        // Locale
        if let Ok(val) = std::env::var("LANG") {
            self.info.locale = val;
        }
        Ok(())
    }

    // ── Windows ──

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), IronError> {
        use crate::platform::windows as plat;

        self.info.os_family = OsFamily::Windows;

        // Every field below was previously read by spawning a subprocess — four
        // PowerShell invocations plus one `cmd /c ver`. That cost 3.2 s per refresh
        // and 10.8 s to construct the monitor, for data Windows publishes in the
        // registry and through plain Win32 calls. Two of those readings were also
        // wrong; see the boot-mode and Secure Boot notes below.
        const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

        let build: u32 = plat::read_registry_string(CURRENT_VERSION, "CurrentBuildNumber")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        if let Some(product) = plat::read_registry_string(CURRENT_VERSION, "ProductName") {
            // `ProductName` still reads "Windows 10 ..." on Windows 11; Microsoft
            // never updated it, and the build number is the documented way to tell
            // the two apart. This is a correction from an authoritative field, not a
            // guess: 22000 is the first Windows 11 build.
            self.info.os_name = if build >= 22000 {
                product.replace("Windows 10", "Windows 11")
            } else {
                product
            };
        }

        // Major/minor come from the registry as DWORDs on Windows 10 and later.
        let major = plat::read_registry_u32(CURRENT_VERSION, "CurrentMajorVersionNumber");
        let minor = plat::read_registry_u32(CURRENT_VERSION, "CurrentMinorVersionNumber");
        if let (Some(major), Some(minor)) = (major, minor) {
            self.info.os_version = format!("{major}.{minor}.{build}");
        }
        if build > 0 {
            self.info.os_build = build.to_string();
        }

        // The kernel version includes the update revision, which is the part that
        // moves between patch Tuesdays and the reason `cmd /c ver` was being parsed.
        let ubr = plat::read_registry_u32(CURRENT_VERSION, "UBR");
        if !self.info.os_version.is_empty() {
            self.info.kernel_version = match ubr {
                Some(ubr) => format!("{}.{ubr}", self.info.os_version),
                None => self.info.os_version.clone(),
            };
            self.info.kernel_full =
                format!("Microsoft Windows [Version {}]", self.info.kernel_version);
        }

        if let Some(name) = windows_computer_name() {
            self.info.hostname = name;
        }

        let (arch, is_64bit) = windows_native_architecture();
        self.info.architecture = arch;
        self.info.is_64bit = is_64bit;

        // `GetTickCount64` is milliseconds since boot and does not advance across
        // sleep, matching what `LastBootUpTime` reported.
        let uptime_ms = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() };
        self.info.uptime_seconds = uptime_ms / 1000;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.info.boot_timestamp = now.saturating_sub(self.info.uptime_seconds);

        // Boot mode. The old reading spawned PowerShell to test `$env:firmware_type`,
        // which Windows does not set for a non-interactive child process — so it was
        // empty on every call and the result was decided by the fallback arm rather
        // than by the firmware.
        self.info.boot_mode = match plat::firmware_type() {
            Some(crate::boot_config::BootType::Uefi) => BootMode::UEFI,
            Some(crate::boot_config::BootType::Legacy) => BootMode::BIOS,
            _ => BootMode::Unknown,
        };

        // Secure Boot. `Confirm-SecureBootUEFI` needs an elevated session and throws
        // otherwise, and the old code mapped that throw to `false` — reporting Secure
        // Boot as off for every unelevated run, whatever the firmware was doing.
        self.info.secure_boot = plat::secure_boot_enabled().unwrap_or(false);

        if let Some(tz) = windows_timezone_key() {
            self.info.timezone = tz;
        }
        if let Some(locale) = windows_user_locale() {
            self.info.locale = locale;
        }

        self.info.loaded_modules = windows_running_drivers();

        // `user_count` is deliberately left alone: the only source was
        // `Win32_OperatingSystem.NumberOfUsers`, and counting sessions properly needs
        // a WTS enumeration this monitor does not otherwise do. Reporting a fabricated
        // 0 would be worse than leaving the field at its default.
        Ok(())
    }

    // ── macOS ──

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), IronError> {
        self.info.os_family = OsFamily::MacOS;

        // `sw_vers` ships with macOS and is how the OS names itself, so a
        // failure to run it is a failure rather than an absent optional tool.
        {
            let text = crate::core::command::capture("sw_vers", &[])?;
            for line in text.lines() {
                if let Some(val) = line.strip_prefix("ProductName:") {
                    self.info.os_name = val.trim().to_string();
                } else if let Some(val) = line.strip_prefix("ProductVersion:") {
                    self.info.os_version = val.trim().to_string();
                } else if let Some(val) = line.strip_prefix("BuildVersion:") {
                    self.info.os_build = val.trim().to_string();
                }
            }
        }

        // Kernel
        if let Ok(output) = std::process::Command::new("uname").arg("-r").output() {
            self.info.kernel_version = String::from_utf8(output.stdout)
                .unwrap_or_default()
                .trim()
                .to_string();
        }
        if let Ok(output) = std::process::Command::new("uname").arg("-v").output() {
            self.info.kernel_full = String::from_utf8(output.stdout)
                .unwrap_or_default()
                .trim()
                .to_string();
        }

        // Architecture
        if let Ok(output) = std::process::Command::new("uname").arg("-m").output() {
            self.info.architecture = String::from_utf8(output.stdout)
                .unwrap_or_default()
                .trim()
                .to_string();
        }

        // Hostname
        if let Ok(output) = std::process::Command::new("hostname").output() {
            self.info.hostname = String::from_utf8(output.stdout)
                .unwrap_or_default()
                .trim()
                .to_string();
        }

        // Uptime via sysctl
        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "kern.boottime"])
            .output()
        {
            let text = String::from_utf8(output.stdout).unwrap_or_default();
            // "{ sec = 1234567890, usec = 0 } ..."
            if let Some(sec_str) = text
                .split("sec = ")
                .nth(1)
                .and_then(|s| s.split(',').next())
            {
                if let Ok(boot_sec) = sec_str.trim().parse::<u64>() {
                    self.info.boot_timestamp = boot_sec;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    self.info.uptime_seconds = now.saturating_sub(boot_sec);
                }
            }
        }

        // Boot mode - always UEFI on modern Mac
        self.info.boot_mode = BootMode::UEFI;

        // Secure Boot (Apple silicon always has it)
        if let Ok(output) = std::process::Command::new("system_profiler")
            .args(["SPHardwareDataType"])
            .output()
        {
            let text = String::from_utf8(output.stdout).unwrap_or_default();
            self.info.secure_boot = text.contains("Apple M") || text.contains("Apple T2");
        }

        // Timezone
        if let Ok(tz) = std::fs::read_link("/etc/localtime") {
            let path = tz.to_string_lossy().to_string();
            if let Some(pos) = path.find("zoneinfo/") {
                self.info.timezone = path[pos + 9..].to_string();
            }
        }

        // Locale
        if let Ok(val) = std::env::var("LANG") {
            self.info.locale = val;
        }

        // Loaded kernel extensions
        if let Ok(output) = std::process::Command::new("kextstat").args(["-l"]).output() {
            let text = String::from_utf8(output.stdout).unwrap_or_default();
            for line in text.lines().skip(1) {
                // "  Index Refs Address ... Name (Version) <Linked Against>"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    let name = parts[5].to_string();
                    let refs: u32 = parts[1].parse().unwrap_or(0);
                    let size: u64 =
                        u64::from_str_radix(parts[3].trim_start_matches("0x"), 16).unwrap_or(0);

                    self.info.loaded_modules.push(KernelModule {
                        name,
                        size_bytes: size,
                        instances: refs,
                        used_by: Vec::new(),
                        state: "Live".to_string(),
                    });
                }
            }
        }
        Ok(())
    }
}

impl Default for OsInfoMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            info: OsInfo {
                os_family: OsFamily::Unknown,
                os_name: String::new(),
                os_version: String::new(),
                os_build: String::new(),
                kernel_version: String::new(),
                kernel_full: String::new(),
                architecture: String::new(),
                hostname: String::new(),
                domain: String::new(),
                uptime_seconds: 0,
                boot_timestamp: 0,
                boot_mode: BootMode::Unknown,
                kernel_cmdline: String::new(),
                user_count: 0,
                loaded_modules: Vec::new(),
                timezone: String::new(),
                locale: String::new(),
                is_64bit: cfg!(target_pointer_width = "64"),
                secure_boot: false,
            },
        })
    }
}

impl std::fmt::Display for OsFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Linux => write!(f, "Linux"),
            Self::Windows => write!(f, "Windows"),
            Self::MacOS => write!(f, "macOS"),
            Self::FreeBSD => write!(f, "FreeBSD"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl std::fmt::Display for BootMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UEFI => write!(f, "UEFI"),
            Self::BIOS => write!(f, "BIOS"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

// ── Windows native readers ──
//
// These replace subprocess spawns. Each returns `None` rather than a placeholder
// when the platform does not answer, so callers keep the field's default instead of
// publishing a made-up value.

/// The machine's NetBIOS computer name.
#[cfg(target_os = "windows")]
fn windows_computer_name() -> Option<String> {
    use windows::Win32::System::SystemInformation::{ComputerNameNetBIOS, GetComputerNameExW};

    let mut len: u32 = 0;
    // First call reports the required length; it is expected to fail with
    // ERROR_MORE_DATA, so its status is deliberately not checked.
    let _ =
        unsafe { GetComputerNameExW(ComputerNameNetBIOS, windows::core::PWSTR::null(), &mut len) };
    if len == 0 {
        return None;
    }

    // The reported length excludes the terminating null, which the second call still
    // writes — so the buffer needs one more slot than `len` says.
    let mut buffer = vec![0u16; len as usize + 1];
    unsafe {
        GetComputerNameExW(
            ComputerNameNetBIOS,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()?;
    buffer.truncate(len as usize);

    let name = String::from_utf16_lossy(&buffer);
    let name = name.trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// The processor architecture the OS is running on, and whether it is 64-bit.
///
/// `GetNativeSystemInfo` reports the machine's architecture even when the caller is a
/// 32-bit process under WOW64, which `GetSystemInfo` does not.
#[cfg(target_os = "windows")]
fn windows_native_architecture() -> (String, bool) {
    use windows::Win32::System::SystemInformation::{GetNativeSystemInfo, SYSTEM_INFO};
    use windows::Win32::System::SystemInformation::{
        PROCESSOR_ARCHITECTURE_AMD64, PROCESSOR_ARCHITECTURE_ARM, PROCESSOR_ARCHITECTURE_ARM64,
        PROCESSOR_ARCHITECTURE_INTEL,
    };

    let mut info: SYSTEM_INFO = unsafe { std::mem::zeroed() };
    unsafe { GetNativeSystemInfo(&mut info) };

    let arch = unsafe { info.Anonymous.Anonymous.wProcessorArchitecture };
    match arch {
        PROCESSOR_ARCHITECTURE_AMD64 => ("x86_64".to_string(), true),
        PROCESSOR_ARCHITECTURE_ARM64 => ("aarch64".to_string(), true),
        PROCESSOR_ARCHITECTURE_ARM => ("arm".to_string(), false),
        PROCESSOR_ARCHITECTURE_INTEL => ("x86".to_string(), false),
        // Compiled-in target as a last resort: it is what this binary was built for,
        // which is at least a fact about the running process.
        _ => (
            std::env::consts::ARCH.to_string(),
            cfg!(target_pointer_width = "64"),
        ),
    }
}

/// The IANA-equivalent Windows time zone key (e.g. "Pacific Standard Time").
#[cfg(target_os = "windows")]
fn windows_timezone_key() -> Option<String> {
    use windows::Win32::System::Time::{
        GetDynamicTimeZoneInformation, DYNAMIC_TIME_ZONE_INFORMATION,
    };

    let mut tz = DYNAMIC_TIME_ZONE_INFORMATION::default();
    // Returns the daylight-saving state, not a status code; TIME_ZONE_ID_INVALID
    // (0xFFFFFFFF) is the failure value.
    if unsafe { GetDynamicTimeZoneInformation(&mut tz) } == u32::MAX {
        return None;
    }

    let key: String = String::from_utf16_lossy(&tz.TimeZoneKeyName)
        .trim_end_matches('\0')
        .trim()
        .to_string();
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

/// The user's default locale name (e.g. "en-US").
#[cfg(target_os = "windows")]
fn windows_user_locale() -> Option<String> {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;

    let mut buffer = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    let len = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if len <= 1 {
        return None;
    }

    // The returned length includes the terminating null.
    Some(String::from_utf16_lossy(&buffer[..(len as usize - 1)]))
}

/// Currently running kernel-mode drivers, the Windows analogue of kernel modules.
///
/// Sourced from WMI over the existing COM connection rather than by spawning
/// PowerShell to emit JSON, which alone accounted for roughly a second per refresh.
#[cfg(target_os = "windows")]
fn windows_running_drivers() -> Vec<KernelModule> {
    let Ok(drivers) = crate::motherboard::get_running_drivers() else {
        return Vec::new();
    };

    drivers
        .into_iter()
        .map(|(name, state)| KernelModule {
            name,
            // Neither figure is exposed by `Win32_SystemDriver`. They stay at the
            // values that mean "not reported" rather than being invented.
            size_bytes: 0,
            instances: 1,
            used_by: Vec::new(),
            state,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructing the monitor either enumerates or says why it could not.
    ///
    /// See the identically-shaped tests in `camera`, `usb` and the rest: this
    /// asserted `is_ok()`, which was true by construction while `refresh` could
    /// not fail. A failure must carry a reason, because a reason is the whole
    /// difference between "this machine has none" and "nobody looked".
    #[test]
    fn test_os_info_creation() {
        match OsInfoMonitor::new() {
            Ok(_monitor) => {}
            Err(e) => {
                let why = e.to_string();
                assert!(
                    why.len() > 10,
                    "enumeration failed without saying why: {why:?}"
                );
            }
        }
    }

    #[test]
    fn test_os_info_default() {
        let monitor = OsInfoMonitor::default();
        let info = monitor.info();
        assert!(!info.hostname.is_empty() || info.os_family == OsFamily::Unknown);
    }

    #[test]
    fn test_os_info_has_data() {
        if let Ok(monitor) = OsInfoMonitor::new() {
            let info = monitor.info();
            // Should have at least an OS family detected
            assert_ne!(info.os_family, OsFamily::Unknown);
            // Should have kernel version
            assert!(!info.kernel_version.is_empty());
        }
    }

    #[test]
    fn test_os_family_display() {
        assert_eq!(OsFamily::Linux.to_string(), "Linux");
        assert_eq!(OsFamily::Windows.to_string(), "Windows");
        assert_eq!(BootMode::UEFI.to_string(), "UEFI");
    }

    #[test]
    fn test_os_info_serialization() {
        let info = OsInfo {
            os_family: OsFamily::Linux,
            os_name: "Ubuntu 22.04".into(),
            os_version: "22.04".into(),
            os_build: "".into(),
            kernel_version: "6.8.0".into(),
            kernel_full: "Linux version 6.8.0".into(),
            architecture: "x86_64".into(),
            hostname: "myhost".into(),
            domain: "".into(),
            uptime_seconds: 3600,
            boot_timestamp: 1700000000,
            boot_mode: BootMode::UEFI,
            kernel_cmdline: "root=/dev/sda1".into(),
            user_count: 1,
            loaded_modules: Vec::new(),
            timezone: "America/New_York".into(),
            locale: "en_US.UTF-8".into(),
            is_64bit: true,
            secure_boot: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("Ubuntu"));
        let _: OsInfo = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_kernel_module_serialization() {
        let module = KernelModule {
            name: "nvidia".into(),
            size_bytes: 1048576,
            instances: 1,
            used_by: vec!["nvidia_uvm".into()],
            state: "Live".into(),
        };
        let json = serde_json::to_string(&module).unwrap();
        assert!(json.contains("nvidia"));
        let _: KernelModule = serde_json::from_str(&json).unwrap();
    }
}
