//! WSL2 GPU Passthrough Detection
//!
//! Detects Windows Subsystem for Linux environments and GPU passthrough
//! configuration. Identifies whether GPUs are available via WSL2's
//! DirectX/CUDA passthrough and reports virtual device mapping.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonitor::wsl::{WslDetector, WslInfo};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let info = WslDetector::detect();
//! if info.is_wsl {
//!     println!("Running in WSL{}", info.version.unwrap_or(0));
//!     if info.gpu_passthrough {
//!         println!("GPU passthrough is available");
//!         println!("  CUDA library mapped in: {}", info.cuda_available);
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
    /// WSL version, `Some(2)` only where WSL2 is evidenced. `None` when WSL is
    /// present but the version is not evidenced, and when this is not WSL.
    pub version: Option<u8>,
    /// Windows build number. Always `None` today: `/proc/version` carries the
    /// Linux kernel release, not the Windows build, and no Windows build is
    /// read from the guest.
    pub windows_build: Option<String>,
    /// Whether GPU passthrough is available
    pub gpu_passthrough: bool,
    /// Whether /dev/dxg (DirectX Graphics) device exists
    pub dxg_available: bool,
    /// Whether D3D12 is available for GPU compute
    pub d3d12_available: bool,
    /// Whether the host's CUDA user-space library (`libcuda.so`) is mapped
    /// into `/usr/lib/wsl/lib`.
    ///
    /// There is deliberately no list of GPU devices. WSL2 does not expose its
    /// adapters through the filesystem: a list read from `/usr/lib/wsl/drivers`
    /// is the Windows driver store -- 941 packages on the machine this was
    /// found on, FireWire and ACPI drivers among them, all published here as
    /// GPUs. Adapters are enumerated through DXCore, which this module does
    /// not call.
    pub cuda_available: bool,
    /// WSL distribution name
    pub distro_name: Option<String>,
    /// Windows host filesystem mount point
    pub windows_mount: Option<String>,
    /// Virtual GPU adapters, one per `/sys/class/drm` card whose PCI IDs were read
    pub virtual_adapters: Vec<VirtualGpuAdapter>,
}

/// A virtual GPU adapter visible through WSL2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualGpuAdapter {
    /// Adapter name, `None` where the PCI vendor is not one this module names.
    /// It used to be `"Unknown GPU (<empty>:<empty>)"` for an adapter whose IDs
    /// were never read.
    pub name: Option<String>,
    /// PCI vendor ID as read from sysfs, e.g. `0x10de`. `None` where unread.
    pub vendor_id: Option<String>,
    /// PCI device ID as read from sysfs. `None` where unread.
    pub device_id: Option<String>,
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
        // Only Linux and Windows fill it in, so `mut` is unused elsewhere.
        #[allow(unused_mut)]
        let mut info = WslInfo {
            is_wsl: false,
            version: None,
            windows_build: None,
            gpu_passthrough: false,
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

                // Determine WSL version. Only WSL2 is evidenced: the kernel
                // string names it, and /run/WSL is the other independent sign.
                // Nothing here evidences WSL1, so a Microsoft kernel without
                // either sign reports no version rather than 1. It used to
                // assert 1 by elimination.
                if Self::wsl2_evidenced(&lower, Path::new("/run/WSL").exists()) {
                    info.version = Some(2);
                }

                // `windows_build` is not set here. The dotted token that used to
                // fill it, "5.15.133.1-microsoft-standard-WSL2", is the Linux
                // kernel release, not a Windows build number.
            }
        }

        if !info.is_wsl {
            // Also check /proc/sys/fs/binfmt_misc/WSLInterop
            if Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists() {
                info.is_wsl = true;
                // The interop binfmt marks WSL, not its version: it does not
                // by itself say WSL2, so no version is set here.
            }
        }

        if !info.is_wsl {
            return;
        }

        // Check /dev/dxg — DirectX Graphics device (GPU passthrough)
        info.dxg_available = Path::new("/dev/dxg").exists();

        // Check for CUDA passthrough libraries. `/usr/lib/wsl/drivers` is not
        // one: it is the Windows driver store, present on every WSL2 install
        // whether or not the host has a CUDA-capable GPU.
        let cuda_paths = [
            "/usr/lib/wsl/lib/libcuda.so",
            "/usr/lib/wsl/lib/libcuda.so.1",
        ];
        info.cuda_available = cuda_paths.iter().any(|p| Path::new(p).exists());

        // Check D3D12 availability
        let d3d12_paths = [
            "/usr/lib/wsl/lib/libd3d12.so",
            "/usr/lib/wsl/lib/libdxcore.so",
        ];
        info.d3d12_available = d3d12_paths.iter().any(|p| Path::new(p).exists());

        info.gpu_passthrough = info.dxg_available || info.cuda_available || info.d3d12_available;

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
                        let read_id = |p: &std::path::Path| {
                            fs::read_to_string(p)
                                .ok()
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                        };
                        if let Some(adapter) =
                            Self::adapter_from_ids(read_id(&vendor_path), read_id(&device_path))
                        {
                            info.virtual_adapters.push(adapter);
                        }
                    }
                }
            }
        }
    }

    /// Whether WSL2 is evidenced. `proc_version_lower` is `/proc/version`,
    /// lowercased; `run_wsl_exists` is whether `/run/WSL` is present. Either is
    /// sufficient; neither is evidence of WSL1.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn wsl2_evidenced(proc_version_lower: &str, run_wsl_exists: bool) -> bool {
        proc_version_lower.contains("wsl2") || run_wsl_exists
    }

    /// Builds an adapter from PCI IDs as sysfs read them. A card with neither
    /// ID read yields nothing: there is no adapter to describe. A vendor this
    /// module does not name keeps its ID and gets no name.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn adapter_from_ids(
        vendor: Option<String>,
        device: Option<String>,
    ) -> Option<VirtualGpuAdapter> {
        if vendor.is_none() && device.is_none() {
            return None;
        }
        let vendor_label = match vendor.as_deref() {
            Some("0x10de") => Some("NVIDIA"),
            Some("0x1002") => Some("AMD"),
            Some("0x8086") => Some("Intel"),
            _ => None,
        };
        let name = vendor_label.map(|label| match (&vendor, &device) {
            (Some(v), Some(d)) => format!("{label} GPU ({v}:{d})"),
            (Some(v), None) => format!("{label} GPU ({v})"),
            _ => format!("{label} GPU"),
        });
        Some(VirtualGpuAdapter {
            name,
            vendor_id: vendor,
            device_id: device,
            driver_version: None,
            dedicated_memory: None,
            shared_memory: None,
        })
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

    /// Captured sysfs IDs. An NVIDIA card is named; a vendor this module does
    /// not name keeps its ID and no name.
    #[test]
    fn adapter_names_come_from_read_ids_only() {
        let nvidia = WslDetector::adapter_from_ids(Some("0x10de".into()), Some("0x2204".into()))
            .expect("both IDs read");
        assert_eq!(nvidia.name.as_deref(), Some("NVIDIA GPU (0x10de:0x2204)"));
        assert_eq!(nvidia.vendor_id.as_deref(), Some("0x10de"));
        assert_eq!(nvidia.device_id.as_deref(), Some("0x2204"));

        let other =
            WslDetector::adapter_from_ids(Some("0x1af4".into()), None).expect("vendor read");
        assert_eq!(other.name, None);
        assert_eq!(other.vendor_id.as_deref(), Some("0x1af4"));
        assert_eq!(other.device_id, None);
    }

    /// A card whose IDs were not read is not an adapter. The old output here was
    /// "Unknown GPU ( : )".
    #[test]
    fn unread_ids_yield_no_adapter() {
        assert!(WslDetector::adapter_from_ids(None, None).is_none());
    }

    /// A Microsoft kernel without a WSL2 marker is WSL with no evidenced
    /// version: it is not asserted as WSL1. The WSL2 marker and /run/WSL are
    /// each sufficient.
    #[test]
    fn wsl2_needs_evidence() {
        let wsl2_kernel = "linux version 5.15.133.1-microsoft-standard-wsl2 (root@x) #1 smp";
        let plain_microsoft = "linux version 4.4.0-19041-microsoft (oe-user@oe-host) #1 smp";
        assert!(WslDetector::wsl2_evidenced(wsl2_kernel, false));
        assert!(WslDetector::wsl2_evidenced(plain_microsoft, true));
        assert!(!WslDetector::wsl2_evidenced(plain_microsoft, false));
    }
}
