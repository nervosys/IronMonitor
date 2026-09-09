//! WSL2 GPU Passthrough Detection
//!
//! Detects Windows Subsystem for Linux environments and GPU passthrough
//! configuration. Identifies whether GPUs are available via WSL2's
//! DirectX/CUDA passthrough and reports virtual device mapping.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::wsl::{WslDetector, WslInfo};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let info = WslDetector::detect();
//! if info.is_wsl {
//!     println!("Running in WSL{}", info.version.unwrap_or(0));
//!     if info.gpu_passthrough {
//!         println!("GPU passthrough is available");
//!         for gpu in &info.gpu_devices {
//!             println!("  GPU: {}", gpu);
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};

/// WSL environment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WslInfo {
    /// Whether we're running inside WSL
    pub is_wsl: bool,
    /// WSL version (1 or 2), None if not WSL
    pub version: Option<u8>,
    /// Windows build number (from /proc/version)
    pub windows_build: Option<String>,
    /// Whether GPU passthrough is available
    pub gpu_passthrough: bool,
    /// GPU devices visible via /dev/dxg or /usr/lib/wsl
    pub gpu_devices: Vec<String>,
    /// Whether /dev/dxg (DirectX Graphics) device exists
    pub dxg_available: bool,
    /// Whether D3D12 is available for GPU compute
    pub d3d12_available: bool,
    /// Whether CUDA is available via WSL2 passthrough
    pub cuda_available: bool,
    /// WSL distribution name
    pub distro_name: Option<String>,
    /// Windows host filesystem mount point
    pub windows_mount: Option<String>,
    /// Virtual GPU adapter names from dxinfo
    pub virtual_adapters: Vec<VirtualGpuAdapter>,
}

/// A virtual GPU adapter visible through WSL2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualGpuAdapter {
    /// Adapter name
    pub name: String,
    /// Driver version
    pub driver_version: Option<String>,
    /// Dedicated video memory in bytes
    pub dedicated_memory: Option<u64>,
    /// Shared system memory in bytes
    pub shared_memory: Option<u64>,
}

/// WSL detection and GPU passthrough analysis
pub struct WslDetector;

impl WslDetector {
    /// Detect WSL environment and GPU passthrough capabilities
    pub fn detect() -> WslInfo {
        let mut info = WslInfo {
            is_wsl: false,
            version: None,
            windows_build: None,
            gpu_passthrough: false,
            gpu_devices: Vec::new(),
            dxg_available: false,
            d3d12_available: false,
            cuda_available: false,
            distro_name: None,
            windows_mount: None,
            virtual_adapters: Vec::new(),
        };

        #[cfg(target_os = "linux")]
        {
            Self::detect_linux(&mut info);
        }

        // On Windows, check if WSL is installed
        #[cfg(windows)]
        {
            Self::detect_windows(&mut info);
        }

        info
    }

    #[cfg(target_os = "linux")]
    fn detect_linux(info: &mut WslInfo) {
        use std::fs;
        use std::path::Path;

        // Check /proc/version for Microsoft/WSL signature
        if let Ok(version) = fs::read_to_string("/proc/version") {
            let lower = version.to_lowercase();
            if lower.contains("microsoft") || lower.contains("wsl") {
                info.is_wsl = true;

                // Determine WSL version
                // The kernel string and /run/WSL are independent evidence of the
                // same thing; either one is sufficient.
                if lower.contains("wsl2")
                    || lower.contains("microsoft-standard-wsl2")
                    || Path::new("/run/WSL").exists()
                {
                    info.version = Some(2);
                } else {
                    // WSL1 doesn't have full Linux kernel
                    info.version = Some(1);
                }

                // Extract Windows build number
                // Format: "... Microsoft ... #1 SMP ... 5.15.133.1-microsoft-standard-WSL2"
                for part in version.split_whitespace() {
                    if part.contains("microsoft") && part.contains('.') {
                        info.windows_build = Some(part.to_string());
                    }
                }
            }
        }

        if !info.is_wsl {
            // Also check /proc/sys/fs/binfmt_misc/WSLInterop
            if Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists() {
                info.is_wsl = true;
                info.version = Some(2); // WSLInterop implies WSL2
            }
        }

        if !info.is_wsl {
            return;
        }

        // Check /dev/dxg — DirectX Graphics device (GPU passthrough)
        info.dxg_available = Path::new("/dev/dxg").exists();

        // Check for CUDA passthrough libraries
        let cuda_paths = [
            "/usr/lib/wsl/lib/libcuda.so",
            "/usr/lib/wsl/lib/libcuda.so.1",
            "/usr/lib/wsl/drivers",
        ];
        info.cuda_available = cuda_paths.iter().any(|p| Path::new(p).exists());

        // Check D3D12 availability
        let d3d12_paths = [
            "/usr/lib/wsl/lib/libd3d12.so",
            "/usr/lib/wsl/lib/libdxcore.so",
        ];
        info.d3d12_available = d3d12_paths.iter().any(|p| Path::new(p).exists());

        info.gpu_passthrough = info.dxg_available || info.cuda_available || info.d3d12_available;

        // Enumerate GPU devices from /usr/lib/wsl/drivers/
        if let Ok(entries) = fs::read_dir("/usr/lib/wsl/drivers") {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            info.gpu_devices.push(name.to_string());
                        }
                    }
                }
            }
        }

        // Also check for GPU libraries in /usr/lib/wsl/lib/
        if let Ok(entries) = fs::read_dir("/usr/lib/wsl/lib") {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("libnvidia") || name.starts_with("libcuda") {
                        if !info
                            .gpu_devices
                            .iter()
                            .any(|d| d.to_lowercase().contains("nvidia"))
                        {
                            info.gpu_devices.push("NVIDIA (WSL2 CUDA)".to_string());
                        }
                        break;
                    }
                }
            }
        }

        // Get distro name from WSL_DISTRO_NAME env var
        info.distro_name = std::env::var("WSL_DISTRO_NAME").ok();

        // Check Windows mount point
        for mount in ["/mnt/c", "/mnt/d"] {
            if Path::new(mount).exists() {
                info.windows_mount = Some(mount.to_string());
                break;
            }
        }

        // Try to parse virtual GPU adapters from dxinfo or DirectX
        Self::parse_virtual_adapters(info);
    }

    #[cfg(target_os = "linux")]
    fn parse_virtual_adapters(info: &mut WslInfo) {
        use std::fs;

        // Read adapter info from /sys/class/drm if available via WSL2
        if let Ok(entries) = fs::read_dir("/sys/class/drm") {
            for entry in entries.flatten() {
                let path = entry.path();
                let name_str = entry.file_name();
                let name = name_str.to_string_lossy();
                if name.starts_with("card") && !name.contains('-') {
                    // Try to read device info
                    let vendor_path = path.join("device/vendor");
                    let device_path = path.join("device/device");
                    if vendor_path.exists() {
                        let vendor = fs::read_to_string(&vendor_path)
                            .unwrap_or_default()
                            .trim()
                            .to_string();
                        let device = fs::read_to_string(&device_path)
                            .unwrap_or_default()
                            .trim()
                            .to_string();
                        let vendor_name = match vendor.as_str() {
                            "0x10de" => "NVIDIA",
                            "0x1002" => "AMD",
                            "0x8086" => "Intel",
                            _ => "Unknown",
                        };
                        info.virtual_adapters.push(VirtualGpuAdapter {
                            name: format!("{} GPU ({}:{})", vendor_name, vendor, device),
                            driver_version: None,
                            dedicated_memory: None,
                            shared_memory: None,
                        });
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    fn detect_windows(_info: &mut WslInfo) {
        // On Windows, check if WSL is installed by looking for the wsl.exe command
        // We don't run inside WSL on Windows, so is_wsl = false
        // But we can report whether WSL is available on this host
        use std::path::Path;

        let wsl_path = r"C:\Windows\System32\wsl.exe";
        if Path::new(wsl_path).exists() {
            // WSL is installed on this Windows host
            // The info struct stays is_wsl=false since we're the host, not the guest
            // But we could add host-side info in the future
        }
    }

    /// Check if the current environment is WSL
    pub fn is_wsl() -> bool {
        #[cfg(target_os = "linux")]
        {
            if let Ok(version) = std::fs::read_to_string("/proc/version") {
                let lower = version.to_lowercase();
                return lower.contains("microsoft") || lower.contains("wsl");
            }
            std::path::Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Check if GPU passthrough is available in WSL2
    pub fn has_gpu_passthrough() -> bool {
        #[cfg(target_os = "linux")]
        {
            std::path::Path::new("/dev/dxg").exists()
                || std::path::Path::new("/usr/lib/wsl/lib/libcuda.so.1").exists()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wsl_detection() {
        let info = WslDetector::detect();
        // On a non-WSL system, this should be false
        // On WSL, it should detect properly
        println!("WSL detected: {}", info.is_wsl);
        if info.is_wsl {
            println!("  Version: {:?}", info.version);
            println!("  GPU passthrough: {}", info.gpu_passthrough);
            println!("  DXG available: {}", info.dxg_available);
            println!("  CUDA available: {}", info.cuda_available);
            println!("  D3D12 available: {}", info.d3d12_available);
            println!("  Distro: {:?}", info.distro_name);
            for gpu in &info.gpu_devices {
                println!("  GPU: {}", gpu);
            }
        }
    }

    #[test]
    fn test_is_wsl() {
        // Should not panic on any platform
        let _ = WslDetector::is_wsl();
    }

    #[test]
    fn test_has_gpu_passthrough() {
        let _ = WslDetector::has_gpu_passthrough();
    }
}
