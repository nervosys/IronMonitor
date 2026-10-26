//! DRM (Direct Rendering Manager) / KMS (Kernel Mode Setting) subsystem monitor.
//!
//! Provides kernel-side GPU information: DRM devices, connectors, encoders,
//! CRTCs, planes, framebuffers, and client usage. Complements the higher-level
//! GPU module with kernel graphics subsystem details.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/class/drm/`, `/sys/kernel/debug/dri/` (debugfs)
//! - **Windows**: WDDM driver model info via WMI
//! - **macOS**: IOKit GPU info

use serde::{Deserialize, Serialize};

use crate::error::IronError;

/// DRM connector status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectorStatus {
    Connected,
    Disconnected,
    Unknown,
}

impl std::fmt::Display for ConnectorStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connected => write!(f, "Connected"),
            Self::Disconnected => write!(f, "Disconnected"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Display connector type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectorType {
    HDMI,
    DisplayPort,
    EDP,
    DVI,
    VGA,
    LVDS,
    DSI,
    USB,
    Virtual,
    Writeback,
    Unknown,
}

impl std::fmt::Display for ConnectorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HDMI => write!(f, "HDMI"),
            Self::DisplayPort => write!(f, "DisplayPort"),
            Self::EDP => write!(f, "eDP"),
            Self::DVI => write!(f, "DVI"),
            Self::VGA => write!(f, "VGA"),
            Self::LVDS => write!(f, "LVDS"),
            Self::DSI => write!(f, "DSI"),
            Self::USB => write!(f, "USB"),
            Self::Virtual => write!(f, "Virtual"),
            Self::Writeback => write!(f, "Writeback"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// A DRM connector (display output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrmConnector {
    /// Connector name (e.g. "HDMI-A-1").
    pub name: String,
    /// Connector type.
    pub connector_type: ConnectorType,
    /// Status.
    pub status: ConnectorStatus,
    /// Connected display mode (if connected).
    pub current_mode: Option<String>,
    /// DPMS state.
    pub dpms: Option<String>,
    /// Whether enabled, from sysfs `enabled` ("enabled" / "disabled").
    /// `None` when the file is unreadable or reports anything else, such as
    /// "unknown": it used to be `false` on a failed read.
    pub enabled: Option<bool>,
}

/// A DRM device (GPU kernel object).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrmDevice {
    /// Card name (e.g. "card0").
    pub card_name: String,
    /// Driver name (e.g. "amdgpu", "i915", "nouveau"), or `None` where no
    /// driver is bound or its link could not be read. It was a `String` that
    /// was empty in both cases, which reads as a driver with no name.
    pub driver: Option<String>,
    /// Device path in sysfs.
    pub sysfs_path: String,
    /// PCI device ID.
    pub pci_id: Option<String>,
    /// Render node (e.g. "renderD128").
    pub render_node: Option<String>,
    /// Connectors.
    pub connectors: Vec<DrmConnector>,
    /// Number of CRTCs. `None`: sysfs does not expose CRTCs, and the counting
    /// ioctl is not used here. It used to be a constant `0`.
    pub crtc_count: Option<u32>,
    /// Number of planes. `None` for the same reason as `crtc_count`.
    pub plane_count: Option<u32>,
    /// Whether this is the boot GPU, from `device/boot_vga` on Linux. `None`
    /// when that file is unreadable or on platforms that do not report it. It
    /// used to be set on the first enumerated card, which is not the boot GPU.
    pub is_boot_gpu: Option<bool>,
    /// GPU memory info (if available via sysfs).
    pub vram_total_bytes: Option<u64>,
    pub vram_used_bytes: Option<u64>,
}

impl DrmDevice {
    /// Connected display count.
    pub fn connected_displays(&self) -> usize {
        self.connectors
            .iter()
            .filter(|c| c.status == ConnectorStatus::Connected)
            .count()
    }

    /// VRAM usage percentage.
    pub fn vram_usage_pct(&self) -> Option<f64> {
        match (self.vram_total_bytes, self.vram_used_bytes) {
            (Some(total), Some(used)) if total > 0 => Some(used as f64 / total as f64 * 100.0),
            _ => None,
        }
    }
}

/// DRM client (process using GPU).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrmClient {
    /// PID.
    pub pid: u32,
    /// Process command name.
    pub command: String,
    /// Card being used.
    pub card: String,
    /// Whether authenticated (the `a` column of debugfs `clients`). `None`
    /// when the column is absent or not `y`/`n`.
    pub authenticated: Option<bool>,
}

/// DRM subsystem overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrmOverview {
    /// All DRM devices.
    pub devices: Vec<DrmDevice>,
    /// Active DRM clients, or `None` where the client lists were not read.
    ///
    /// The lists live in debugfs, which is usually readable only by root. An
    /// unreadable list used to be an empty `Vec`, which reads as "no clients
    /// are using the GPU". `Some(vec![])` now means a list was read and was
    /// empty.
    pub clients: Option<Vec<DrmClient>>,
    /// Total connected displays. `None` where connectors were not enumerated
    /// (Windows, macOS): a `0` there would claim no displays are attached.
    pub total_connected_displays: Option<u32>,
    /// Total DRM devices. `None` where the platform reader is not implemented.
    pub total_devices: Option<u32>,
    /// Primary GPU driver.
    pub primary_driver: Option<String>,
}

/// DRM monitor.
pub struct DrmMonitor {
    overview: DrmOverview,
}

impl DrmMonitor {
    /// Create a new DRM monitor.
    pub fn new() -> Result<Self, IronError> {
        let overview = Self::scan()?;
        Ok(Self { overview })
    }

    /// Refresh.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.overview = Self::scan()?;
        Ok(())
    }

    /// Get overview.
    pub fn overview(&self) -> &DrmOverview {
        &self.overview
    }

    /// Get all devices.
    pub fn devices(&self) -> &[DrmDevice] {
        &self.overview.devices
    }

    /// Get device by card name.
    pub fn device(&self, card: &str) -> Option<&DrmDevice> {
        self.overview.devices.iter().find(|d| d.card_name == card)
    }

    #[cfg(target_os = "linux")]
    fn scan() -> Result<DrmOverview, IronError> {
        let drm_path = std::path::Path::new("/sys/class/drm");
        let mut devices = Vec::new();

        if !drm_path.exists() {
            // No DRM subsystem at all: the directory's absence is an observed
            // zero, not an unread count.
            return Ok(DrmOverview {
                devices: Vec::new(),
                clients: Some(Vec::new()),
                total_connected_displays: Some(0),
                total_devices: Some(0),
                primary_driver: None,
            });
        }

        let entries = std::fs::read_dir(drm_path).map_err(IronError::Io)?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // Only process card* entries (not renderD*, not card*-*)
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }

            if name.chars().skip(4).all(|c| c.is_ascii_digit()) {
                if let Ok(dev) = Self::read_drm_device(&name, &entry.path()) {
                    devices.push(dev);
                }
            }
        }

        let total_displays: u32 = devices.iter().map(|d| d.connected_displays() as u32).sum();
        let total = devices.len() as u32;
        // The primary driver is the boot GPU's, when sysfs says which that is.
        // The first enumerated card is not necessarily the boot device.
        let primary_driver = devices
            .iter()
            .find(|d| d.is_boot_gpu == Some(true))
            .and_then(|d| d.driver.clone());
        let clients = Self::read_clients();

        Ok(DrmOverview {
            devices,
            clients,
            total_connected_displays: Some(total_displays),
            total_devices: Some(total),
            primary_driver,
        })
    }

    #[cfg(target_os = "linux")]
    fn read_drm_device(card_name: &str, path: &std::path::Path) -> Result<DrmDevice, IronError> {
        let device_path = path.join("device");

        // Read driver
        let driver_link = device_path.join("driver");
        // `None` when no driver is bound or its link is unreadable. Neither is
        // a driver with an empty name.
        let driver = std::fs::read_link(&driver_link)
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));

        // Read PCI ID
        let pci_id = std::fs::read_to_string(device_path.join("device"))
            .ok()
            .map(|s| s.trim().to_string());

        // Find render node
        let render_node = Self::find_render_node(card_name);

        // Read connectors
        let connectors = Self::read_connectors(card_name, path);

        // Read VRAM info (amdgpu/i915)
        let vram_total = Self::read_sysfs_u64(&device_path.join("mem_info_vram_total"));
        let vram_used = Self::read_sysfs_u64(&device_path.join("mem_info_vram_used"));

        // Boot VGA flag: "1" for the device the firmware initialised. Unreadable
        // (no file, or a non-PCI device) stays None.
        let is_boot_gpu = std::fs::read_to_string(device_path.join("boot_vga"))
            .ok()
            .map(|s| s.trim() == "1");

        Ok(DrmDevice {
            card_name: card_name.to_string(),
            driver,
            sysfs_path: path.to_string_lossy().to_string(),
            pci_id,
            render_node,
            connectors,
            // Sysfs exposes no CRTC or plane count; counting them needs a DRM
            // ioctl, which this reader does not make. Used to be a constant 0.
            crtc_count: None,
            plane_count: None,
            is_boot_gpu,
            vram_total_bytes: vram_total,
            vram_used_bytes: vram_used,
        })
    }

    #[cfg(target_os = "linux")]
    fn find_render_node(card_name: &str) -> Option<String> {
        // card0 -> renderD128, card1 -> renderD129, etc.
        let num: u32 = card_name.strip_prefix("card")?.parse().ok()?;
        let render = format!("renderD{}", 128 + num);
        let render_path = format!("/sys/class/drm/{}", render);
        if std::path::Path::new(&render_path).exists() {
            Some(render)
        } else {
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn read_connectors(card_name: &str, _card_path: &std::path::Path) -> Vec<DrmConnector> {
        let drm_path = std::path::Path::new("/sys/class/drm");
        let mut connectors = Vec::new();

        let prefix = format!("{}-", card_name);
        let entries = match std::fs::read_dir(drm_path) {
            Ok(e) => e,
            Err(_) => return connectors,
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(&prefix) {
                continue;
            }

            let connector_name = name.strip_prefix(&prefix).unwrap_or(&name).to_string();

            let status_str = std::fs::read_to_string(entry.path().join("status"))
                .unwrap_or_default()
                .trim()
                .to_lowercase();

            let status = match status_str.as_str() {
                "connected" => ConnectorStatus::Connected,
                "disconnected" => ConnectorStatus::Disconnected,
                _ => ConnectorStatus::Unknown,
            };

            // sysfs reads "enabled", "disabled" or "unknown". Only the first two
            // are an answer; anything else is None, not false.
            let enabled = std::fs::read_to_string(entry.path().join("enabled"))
                .ok()
                .and_then(|s| match s.trim() {
                    "enabled" => Some(true),
                    "disabled" => Some(false),
                    _ => None,
                });

            let dpms = std::fs::read_to_string(entry.path().join("dpms"))
                .ok()
                .map(|s| s.trim().to_string());

            let current_mode = if status == ConnectorStatus::Connected {
                // Read from modes file (first line is current)
                std::fs::read_to_string(entry.path().join("modes"))
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
            } else {
                None
            };

            let connector_type = Self::parse_connector_type(&connector_name);

            connectors.push(DrmConnector {
                name: connector_name,
                connector_type,
                status,
                current_mode,
                dpms,
                enabled,
            });
        }

        connectors
    }

    #[cfg(target_os = "linux")]
    fn parse_connector_type(name: &str) -> ConnectorType {
        let upper = name.to_uppercase();
        if upper.starts_with("HDMI") {
            ConnectorType::HDMI
        } else if upper.starts_with("DP") || upper.starts_with("DISPLAYPORT") {
            ConnectorType::DisplayPort
        } else if upper.starts_with("EDP") {
            ConnectorType::EDP
        } else if upper.starts_with("DVI") {
            ConnectorType::DVI
        } else if upper.starts_with("VGA") {
            ConnectorType::VGA
        } else if upper.starts_with("LVDS") {
            ConnectorType::LVDS
        } else if upper.starts_with("DSI") {
            ConnectorType::DSI
        } else if upper.starts_with("VIRTUAL") {
            ConnectorType::Virtual
        } else if upper.starts_with("WRITEBACK") {
            ConnectorType::Writeback
        } else {
            ConnectorType::Unknown
        }
    }

    #[cfg(target_os = "linux")]
    fn read_clients() -> Option<Vec<DrmClient>> {
        let mut clients = Vec::new();
        let mut read_any = false;

        // Try debugfs /sys/kernel/debug/dri/*/clients
        for card_num in 0..8u32 {
            let clients_path = format!("/sys/kernel/debug/dri/{}/clients", card_num);
            if let Ok(content) = std::fs::read_to_string(&clients_path) {
                read_any = true;
                clients.extend(Self::parse_clients(&content, &format!("card{}", card_num)));
            }
        }

        // No list could be read, so the client count is unknown, not zero.
        read_any.then_some(clients)
    }

    /// Parse a debugfs `clients` file. Columns: `command tgid dev master a uid magic`.
    /// The `a` column (index 4) is the authenticated flag, `y` or `n`. It used to be
    /// read from index 2, which is the device minor.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn parse_clients(content: &str, card: &str) -> Vec<DrmClient> {
        let mut clients = Vec::new();
        for line in content.lines().skip(1) {
            // header skip
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 5 {
                continue;
            }
            if let Ok(pid) = parts[1].parse::<u32>() {
                clients.push(DrmClient {
                    pid,
                    command: parts[0].to_string(),
                    card: card.to_string(),
                    authenticated: match parts[4] {
                        "y" => Some(true),
                        "n" => Some(false),
                        _ => None,
                    },
                });
            }
        }
        clients
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_u64(path: &std::path::Path) -> Option<u64> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    #[cfg(target_os = "windows")]
    fn scan() -> Result<DrmOverview, IronError> {
        // Windows uses WDDM, not DRM. Provide basic info via WMI.
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion, AdapterRAM, VideoProcessor | ConvertTo-Json",
            ])
            .output()
            .map_err(|e| {
                IronError::System(format!("Win32_VideoController query did not run: {e}"))
            })?;
        // A failed query is an error, not a machine with no GPUs. It used to
        // fall through to `total_devices: Some(0)`, which claims there are none.
        if !output.status.success() {
            return Err(IronError::System(format!(
                "Win32_VideoController query exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        // `ConvertTo-Json` given no adapters prints nothing, and the query
        // succeeded: that is an enumeration that found none.
        let items: Vec<serde_json::Value> = if text.trim().is_empty() {
            Vec::new()
        } else {
            // Parse JSON array or single object
            let val = serde_json::from_str::<serde_json::Value>(text.trim()).map_err(|e| {
                IronError::System(format!("Win32_VideoController output did not parse: {e}"))
            })?;
            if val.is_array() {
                val.as_array().cloned().unwrap_or_default()
            } else {
                vec![val]
            }
        };

        // `Win32_VideoController.AdapterRAM` is a 32-bit field: an RTX
        // 3090 Ti with 24GB reports 4293918720, one byte under 4GiB.
        // That was fixed in `hardware_ai` and `platform::windows` at
        // 7607401 and left here, in a module with no internal callers
        // and a public re-export — so a library user got the capped
        // figure. Prefer the registry's REG_QWORD, by adapter name.
        let by_name = crate::platform::windows::adapter_vram_bytes();

        let mut devices = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let name = item["Name"].as_str().unwrap_or("Unknown GPU");
            let driver_version = item["DriverVersion"].as_str().unwrap_or("");
            let vram = by_name
                .iter()
                .find(|(desc, _)| desc == name)
                .map(|(_, bytes)| *bytes)
                .or_else(|| {
                    // No registry entry for this adapter. AdapterRAM is
                    // still capped, so anything at or above the cap is
                    // withheld rather than published as ~4GB.
                    item["AdapterRAM"].as_u64().filter(|b| *b < 4_293_918_720)
                });

            // The WDDM driver version is the only driver fact read here. An
            // empty DriverVersion used to produce "WDDM ()".
            let driver = if driver_version.is_empty() {
                "WDDM".to_string()
            } else {
                format!("WDDM ({})", driver_version)
            };

            devices.push(DrmDevice {
                card_name: format!("card{}", i),
                driver: Some(driver),
                sysfs_path: String::new(),
                pci_id: None,
                render_node: None,
                connectors: Vec::new(),
                // No CRTC or plane count on WDDM through this query.
                crtc_count: None,
                plane_count: None,
                // Win32_VideoController carries no boot flag. Index 0 is
                // only enumeration order, so it is not reported as boot.
                is_boot_gpu: None,
                vram_total_bytes: vram,
                vram_used_bytes: None,
            });
        }

        let total = devices.len() as u32;

        Ok(DrmOverview {
            devices,
            // No client list is read on Windows.
            clients: None,
            // Connectors are not enumerated on Windows, so no display count
            // is read. A 0 here would claim that no displays are attached.
            total_connected_displays: None,
            total_devices: Some(total),
            // Enumeration order is not the boot adapter, and this query carries
            // no boot flag, so no adapter is named the primary one. The first
            // card used to stand here.
            primary_driver: None,
        })
    }

    #[cfg(target_os = "macos")]
    fn scan() -> Result<DrmOverview, IronError> {
        // IOKit enumeration is not implemented, so no count is read.
        Ok(DrmOverview {
            devices: Vec::new(),
            clients: None,
            total_connected_displays: None,
            total_devices: None,
            primary_driver: None,
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn scan() -> Result<DrmOverview, IronError> {
        Ok(DrmOverview {
            devices: Vec::new(),
            clients: None,
            total_connected_displays: None,
            total_devices: None,
            primary_driver: None,
        })
    }
}

impl Default for DrmMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            overview: DrmOverview {
                devices: Vec::new(),
                clients: None,
                // The scan failed, so nothing was counted.
                total_connected_displays: None,
                total_devices: None,
                primary_driver: None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connector_status_display() {
        assert_eq!(ConnectorStatus::Connected.to_string(), "Connected");
        assert_eq!(ConnectorType::HDMI.to_string(), "HDMI");
        assert_eq!(ConnectorType::EDP.to_string(), "eDP");
    }

    #[test]
    fn test_vram_usage() {
        let dev = DrmDevice {
            card_name: "card0".into(),
            driver: Some("amdgpu".into()),
            sysfs_path: String::new(),
            pci_id: None,
            render_node: Some("renderD128".into()),
            connectors: vec![DrmConnector {
                name: "HDMI-A-1".into(),
                connector_type: ConnectorType::HDMI,
                status: ConnectorStatus::Connected,
                current_mode: Some("1920x1080".into()),
                dpms: None,
                enabled: Some(true),
            }],
            crtc_count: Some(4),
            plane_count: Some(8),
            is_boot_gpu: Some(true),
            vram_total_bytes: Some(8 * 1024 * 1024 * 1024),
            vram_used_bytes: Some(2 * 1024 * 1024 * 1024),
        };
        assert_eq!(dev.connected_displays(), 1);
        let usage = dev.vram_usage_pct().unwrap();
        assert!((usage - 25.0).abs() < 0.01);
    }

    #[test]
    fn test_monitor_default() {
        let monitor = DrmMonitor::default();
        let _overview = monitor.overview();
    }

    #[test]
    fn test_serialization() {
        let connector = DrmConnector {
            name: "DP-1".into(),
            connector_type: ConnectorType::DisplayPort,
            status: ConnectorStatus::Connected,
            current_mode: Some("2560x1440".into()),
            dpms: Some("On".into()),
            enabled: Some(true),
        };
        let json = serde_json::to_string(&connector).unwrap();
        assert!(json.contains("DP-1"));
        let _: DrmConnector = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_parse_clients_authenticated_column() {
        // Captured layout of debugfs `dri/N/clients`: the `a` column is index 4.
        let content = concat!(
            "             command   tgid dev master a   uid  magic\n",
            "                Xorg   1234   0   y    y     0          0\n",
            "             firefox   5678 128   n    n  1000          0\n",
            "           weirdapp   9999   0   n    x  1000          0\n",
        );
        let clients = DrmMonitor::parse_clients(content, "card0");
        assert_eq!(clients.len(), 3);

        assert_eq!(clients[0].pid, 1234);
        assert_eq!(clients[0].command, "Xorg");
        assert_eq!(clients[0].card, "card0");
        // Device minor 0 and a dev column of 128 must not decide the flag.
        assert_eq!(clients[0].authenticated, Some(true));

        assert_eq!(clients[1].pid, 5678);
        assert_eq!(clients[1].authenticated, Some(false));

        // An unrecognised flag is unknown, not false.
        assert_eq!(clients[2].authenticated, None);
    }
}
