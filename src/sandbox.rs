//! Sandbox Detection Module
//!
//! Detects if the application is running in a sandboxed or virtualized environment.
//! This is important for ethical data collection - we should never collect or transmit
//! data when running in analysis/testing environments.
//!
//! # Detection Methods
//!
//! - Virtual machine detection (VMware, VirtualBox, QEMU, Hyper-V, KVM)
//! - Container detection (Docker, LXC, systemd-nspawn)
//! - Windows Sandbox detection
//! - macOS sandbox detection
//! - Wine/compatibility layer detection
//! - Debugger detection
//! - Known analysis tool paths
//!
//! # Example
//!
//! ```no_run
//! use ironmonlib::sandbox::SandboxDetector;
//!
//! let detector = SandboxDetector::new();
//! if detector.is_sandboxed() {
//!     println!("Running in sandbox - no data collection");
//!     return;
//! }
//! ```

#[cfg(target_os = "linux")]
use std::fs;
use std::path::Path;

/// Sandbox detection results
#[derive(Debug, Clone, Default)]
pub struct SandboxInfo {
    /// Is running in a virtual machine
    pub is_vm: bool,

    /// Is running in a container
    pub is_container: bool,

    /// Is running under Windows Sandbox
    pub is_windows_sandbox: bool,

    /// Is running under macOS sandbox
    pub is_macos_sandbox: bool,

    /// Is running under Wine/compatibility layer
    pub is_wine: bool,

    /// Is being debugged
    pub is_debugged: bool,

    /// Detected environment name
    pub environment: Option<String>,

    /// All detected indicators
    pub indicators: Vec<String>,
}

impl SandboxInfo {
    /// Check if any sandbox indicators are present
    pub fn is_sandboxed(&self) -> bool {
        self.is_vm
            || self.is_container
            || self.is_windows_sandbox
            || self.is_macos_sandbox
            || self.is_wine
            || self.is_debugged
    }

    /// Get a human-readable summary
    pub fn summary(&self) -> String {
        if !self.is_sandboxed() {
            return "Not sandboxed".to_string();
        }

        let mut parts = Vec::new();
        if self.is_vm {
            parts.push("Virtual Machine");
        }
        if self.is_container {
            parts.push("Container");
        }
        if self.is_windows_sandbox {
            parts.push("Windows Sandbox");
        }
        if self.is_macos_sandbox {
            parts.push("macOS Sandbox");
        }
        if self.is_wine {
            parts.push("Wine/Compatibility Layer");
        }
        if self.is_debugged {
            parts.push("Debugger Attached");
        }

        if let Some(env) = &self.environment {
            format!("Sandboxed: {} ({})", parts.join(", "), env)
        } else {
            format!("Sandboxed: {}", parts.join(", "))
        }
    }
}

/// Sandbox detector
pub struct SandboxDetector;

impl SandboxDetector {
    /// Create a new sandbox detector
    pub fn new() -> Self {
        Self
    }

    /// Perform comprehensive sandbox detection
    pub fn detect(&self) -> SandboxInfo {
        let mut info = SandboxInfo::default();

        // VM detection
        self.detect_vm(&mut info);

        // Container detection
        self.detect_container(&mut info);

        // Windows Sandbox detection
        #[cfg(windows)]
        self.detect_windows_sandbox(&mut info);

        // macOS sandbox detection
        #[cfg(target_os = "macos")]
        self.detect_macos_sandbox(&mut info);

        // Wine detection
        self.detect_wine(&mut info);

        // Debugger detection
        self.detect_debugger(&mut info);

        info
    }

    /// Quick check if sandboxed (convenience method)
    pub fn is_sandboxed(&self) -> bool {
        self.detect().is_sandboxed()
    }

    /// Detect virtual machine environments
    fn detect_vm(&self, info: &mut SandboxInfo) {
        // Linux: Check /sys/class/dmi/id/product_name and other DMI files
        #[cfg(target_os = "linux")]
        {
            let vm_indicators = [
                (
                    "/sys/class/dmi/id/product_name",
                    vec!["VirtualBox", "VMware", "QEMU", "KVM", "Bochs"],
                ),
                // "Microsoft Corporation" is deliberately absent: it is the
                // sys_vendor of every Surface, which is bare metal. A Hyper-V
                // guest is caught by the hypervisor CPU flag below.
                (
                    "/sys/class/dmi/id/sys_vendor",
                    vec!["QEMU", "VMware", "VirtualBox"],
                ),
                (
                    "/sys/class/dmi/id/board_vendor",
                    vec!["VMware", "VirtualBox"],
                ),
                ("/sys/class/dmi/id/bios_vendor", vec!["SeaBIOS", "Bochs"]),
            ];

            for (path, indicators) in &vm_indicators {
                if let Ok(content) = fs::read_to_string(path) {
                    for indicator in indicators {
                        if content.contains(indicator) {
                            info.is_vm = true;
                            info.environment = Some(indicator.to_string());
                            info.indicators
                                .push(format!("{} detected in {}", indicator, path));
                        }
                    }
                }
            }

            // Check for hypervisor CPU flag
            if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
                if cpuinfo.contains("hypervisor") {
                    info.is_vm = true;
                    info.indicators
                        .push("Hypervisor CPU flag detected".to_string());
                }
            }

            // Check for VM-specific devices
            if Path::new("/dev/vboxguest").exists() {
                info.is_vm = true;
                info.environment = Some("VirtualBox".to_string());
                info.indicators
                    .push("VirtualBox guest device found".to_string());
            }

            if Path::new("/dev/vmci").exists() {
                info.is_vm = true;
                info.environment = Some("VMware".to_string());
                info.indicators.push("VMware device found".to_string());
            }
        }

        // Windows: Check registry and WMI (would require winreg crate)
        #[cfg(windows)]
        {
            // This read `C:\Windows\System32\drivers\vmbus.sys` and set
            // `is_vm` if the contents were non-empty. It was wrong twice over:
            // the file ships with every modern Windows install, host or guest,
            // so its presence proves nothing; and it is a binary driver, so
            // `read_to_string` fails on invalid UTF-8 and the check never ran
            // at all. Both confirmed on a bare-metal Windows 11 desktop, where
            // the file is present, 210 KB, and invalid UTF-8 at byte 2.
            //
            // 3.10.0 already built the correct check in `virtualization::detect`:
            // it reads the hypervisor bit and vendor string out of CPUID and
            // distinguishes a Hyper-V root partition from a guest via leaf
            // 0x40000003. Use it rather than keep a second, worse one.
            if crate::virtualization::detect::detect_platform()
                == crate::virtualization::detect::VirtPlatform::VirtualMachine
            {
                info.is_vm = true;
                let named = crate::virtualization::detect::detect_hypervisor()
                    .map(|h| format!("{:?}", h.hypervisor));
                info.environment = named.clone();
                info.indicators.push(match named {
                    Some(name) => format!("{name} reported by CPUID"),
                    None => "hypervisor reported by CPUID".to_string(),
                });
            }
        }

        // macOS: Check for virtualization
        #[cfg(target_os = "macos")]
        {
            if let Ok(output) = std::process::Command::new("sysctl")
                .arg("-n")
                .arg("machdep.cpu.features")
                .output()
            {
                let features = String::from_utf8_lossy(&output.stdout);
                if features.contains("VMM") || features.contains("Hypervisor") {
                    info.is_vm = true;
                    info.indicators
                        .push("Hypervisor features detected".to_string());
                }
            }
        }
    }

    /// Detect container environments
    fn detect_container(&self, _info: &mut SandboxInfo) {
        #[cfg(target_os = "linux")]
        {
            // Check for Docker
            if Path::new("/.dockerenv").exists() {
                _info.is_container = true;
                _info.environment = Some("Docker".to_string());
                _info
                    .indicators
                    .push("Docker environment file found".to_string());
            }

            // Check cgroup for container indicators
            if let Ok(cgroup) = fs::read_to_string("/proc/1/cgroup") {
                if cgroup.contains("docker")
                    || cgroup.contains("lxc")
                    || cgroup.contains("kubepods")
                {
                    _info.is_container = true;
                    if cgroup.contains("docker") {
                        _info.environment = Some("Docker".to_string());
                    } else if cgroup.contains("lxc") {
                        _info.environment = Some("LXC".to_string());
                    } else if cgroup.contains("kubepods") {
                        _info.environment = Some("Kubernetes".to_string());
                    }
                    _info
                        .indicators
                        .push("Container cgroup detected".to_string());
                }
            }

            // Check for systemd-nspawn
            if let Ok(env) = std::env::var("container") {
                _info.is_container = true;
                _info.environment = Some(env.clone());
                _info.indicators.push(format!("Container env var: {}", env));
            }

            // Check for Flatpak
            if Path::new("/.flatpak-info").exists() {
                _info.is_container = true;
                _info.environment = Some("Flatpak".to_string());
                _info
                    .indicators
                    .push("Flatpak sandbox detected".to_string());
            }

            // Check for Snap
            if std::env::var("SNAP").is_ok() {
                _info.is_container = true;
                _info.environment = Some("Snap".to_string());
                _info.indicators.push("Snap sandbox detected".to_string());
            }
        }
    }

    /// Detect Windows Sandbox
    #[cfg(windows)]
    fn detect_windows_sandbox(&self, info: &mut SandboxInfo) {
        // Windows Sandbox uses specific registry keys and computer names
        if let Ok(computer_name) = std::env::var("COMPUTERNAME") {
            if computer_name.starts_with("SANDBOX-") {
                info.is_windows_sandbox = true;
                info.environment = Some("Windows Sandbox".to_string());
                info.indicators
                    .push("Windows Sandbox computer name detected".to_string());
            }
        }

        // Check for Windows Sandbox specific marker file (more reliable than Containers directory)
        // The Containers directory exists on all modern Windows systems, not just sandbox
        if Path::new("C:\\Windows\\System32\\uxtheme.dll").exists() {
            // Check for Windows Sandbox-specific indicator: Sandbox always has limited user profile
            if let Ok(user_profile) = std::env::var("USERPROFILE") {
                // Windows Sandbox typically uses WDAGUtilityAccount or similar transient accounts
                if user_profile.contains("WDAGUtilityAccount")
                    || user_profile.contains("ContainerUser")
                {
                    info.is_windows_sandbox = true;
                    info.indicators
                        .push("Windows Sandbox user profile detected".to_string());
                }
            }
        }

        // Check for AppContainer/AppJail (UWP apps)
        if std::env::var("LOCALAPPDATA")
            .map(|p| p.contains("Packages"))
            .unwrap_or(false)
        {
            // Only mark as sandboxed if we're actually running as a packaged app
            // Check for package identity
            if std::env::var("PACKAGE_FAMILY_NAME").is_ok() {
                info.is_windows_sandbox = true;
                info.indicators.push("AppContainer detected".to_string());
            }
        }
    }

    /// Detect macOS sandbox
    #[cfg(target_os = "macos")]
    fn detect_macos_sandbox(&self, info: &mut SandboxInfo) {
        // Check if running in macOS sandbox
        if std::env::var("APP_SANDBOX_CONTAINER_ID").is_ok() {
            info.is_macos_sandbox = true;
            info.environment = Some("macOS Sandbox".to_string());
            info.indicators
                .push("macOS sandbox container ID found".to_string());
        }

        // Check for sandbox profile
        if let Ok(output) = std::process::Command::new("sandbox-exec")
            .arg("-p")
            .arg("(version 1)")
            .arg("true")
            .output()
        {
            if output.status.success() {
                info.is_macos_sandbox = true;
                info.indicators
                    .push("macOS sandbox-exec available".to_string());
            }
        }
    }

    /// Detect Wine/compatibility layers
    fn detect_wine(&self, info: &mut SandboxInfo) {
        // Check for Wine environment variables
        if std::env::var("WINE").is_ok() || std::env::var("WINEPREFIX").is_ok() {
            info.is_wine = true;
            info.environment = Some("Wine".to_string());
            info.indicators
                .push("Wine environment detected".to_string());
        }

        // Check for Wine registry files (on Linux)
        #[cfg(target_os = "linux")]
        {
            if let Ok(home) = std::env::var("HOME") {
                let wine_prefix = format!("{}/.wine", home);
                if Path::new(&wine_prefix).exists() {
                    info.is_wine = true;
                    info.indicators
                        .push("Wine prefix directory found".to_string());
                }
            }
        }

        // Check for Proton (Steam compatibility layer)
        if std::env::var("STEAM_COMPAT_DATA_PATH").is_ok() {
            info.is_wine = true;
            info.environment = Some("Proton".to_string());
            info.indicators
                .push("Proton/Steam compatibility layer detected".to_string());
        }
    }

    /// Detect debugger attachment
    fn detect_debugger(&self, info: &mut SandboxInfo) {
        #[cfg(target_os = "linux")]
        {
            // Check /proc/self/status for TracerPid
            if let Ok(status) = fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if line.starts_with("TracerPid:") {
                        if let Some(pid_str) = line.split_whitespace().nth(1) {
                            if let Ok(pid) = pid_str.parse::<i32>() {
                                if pid != 0 {
                                    info.is_debugged = true;
                                    info.indicators
                                        .push(format!("Debugger attached (PID: {})", pid));
                                }
                            }
                        }
                    }
                }
            }
        }

        #[cfg(windows)]
        {
            // Check if debugger is present using Windows API
            use windows::Win32::System::Diagnostics::Debug::IsDebuggerPresent;
            unsafe {
                if IsDebuggerPresent().as_bool() {
                    info.is_debugged = true;
                    info.indicators
                        .push("Debugger detected via IsDebuggerPresent".to_string());
                }
            }
        }

        // There is deliberately no environment-variable heuristic here.
        //
        // The list used to be `["_", "LLDB_DEBUGSERVER_PATH", "GDB",
        // "PYTHONBREAKPOINT"]`, and `_` is not a debugger variable at all:
        // every POSIX shell sets it to the last argument of the previous
        // command, so every run launched from bash, zsh or sh reported
        // "Sandboxed: Debugger Attached". On this desktop that made
        // `is_sandboxed()` true, which made `has_consent` false for every
        // scope, which made `ironmon privacy status` answer "DENIED" on the line
        // after `ironmon privacy opt-in` said "enabled". The other three say
        // nothing about *this* process either: gdb does not export `GDB`, and
        // `PYTHONBREAKPOINT` configures a Python hook, in a Rust binary.
        //
        // The two checks above are the real ones. macOS has none, and the
        // honest answer there is `false` with nothing claimed, rather than a
        // guess that fires on a shell variable.
    }
}

impl Default for SandboxDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `$_` is set by every POSIX shell to the last argument of the previous
    /// command. It was in this module's debugger-variable list, so any run
    /// started from bash, zsh or sh reported a debugger and therefore a
    /// sandbox — and `ConsentManager::has_consent` refuses every scope inside
    /// a sandbox, so `ironmon privacy status` answered "DENIED" immediately
    /// after `ironmon privacy opt-in` recorded consent.
    ///
    /// The test runner sets `_` under `cargo test` from a shell, so this
    /// assertion exercised the defect directly: it failed before the list was
    /// removed. A real debugger attached to the test process would also fail
    /// it, which is the correct behaviour and not a flake.
    #[test]
    fn a_shell_variable_is_not_a_debugger() {
        let info = SandboxDetector::new().detect();

        assert!(
            !info.is_debugged,
            "reported a debugger with none attached; indicators: {:?}",
            info.indicators
        );

        for indicator in &info.indicators {
            assert!(
                !indicator.contains("env var"),
                concat!(
                    "an environment variable is being used as evidence of a ",
                    "sandbox: {}"
                ),
                indicator
            );
        }
    }

    #[test]
    fn test_sandbox_detector_creation() {
        let detector = SandboxDetector::new();
        let info = detector.detect();

        // Test should work regardless of environment
        assert!(!info.summary().is_empty());
    }

    #[test]
    fn test_sandbox_info_not_sandboxed() {
        let info = SandboxInfo::default();
        assert!(!info.is_sandboxed());
        assert_eq!(info.summary(), "Not sandboxed");
    }

    #[test]
    fn test_sandbox_info_vm() {
        let info = SandboxInfo {
            is_vm: true,
            environment: Some("VirtualBox".to_string()),
            ..Default::default()
        };

        assert!(info.is_sandboxed());
        assert!(info.summary().contains("Virtual Machine"));
    }
}
