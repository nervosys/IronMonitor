//! DMA engine monitoring.
//!
//! Detects DMA controllers (Intel IOAT/CBDMA, Intel DSA/IAX via IDXD),
//! enumerates channels, and reports transfer capabilities and statistics.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/class/dma/`, `/sys/bus/dsa/devices/`
//! - **Windows / macOS**: Not available

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// DMA engine type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DmaEngineType {
    /// Intel I/O Acceleration Technology (Crystal Beach DMA).
    IntelIoat,
    /// Intel Data Streaming Accelerator.
    IntelDsa,
    /// Intel In-Memory Analytics Accelerator.
    IntelIax,
    /// Intel IDXD (generic).
    IntelIdxd,
    /// DW AXI DMA (DesignWare).
    DwAxiDma,
    /// PL330 / ARM DMA.
    Pl330,
    /// Unknown / other.
    Other(String),
}

impl std::fmt::Display for DmaEngineType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IntelIoat => write!(f, "Intel IOAT"),
            Self::IntelDsa => write!(f, "Intel DSA"),
            Self::IntelIax => write!(f, "Intel IAX"),
            Self::IntelIdxd => write!(f, "Intel IDXD"),
            Self::DwAxiDma => write!(f, "DW AXI DMA"),
            Self::Pl330 => write!(f, "PL330"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

/// DMA channel capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmaCapabilities {
    /// Memory-to-memory copy.
    pub memcpy: bool,
    /// Memory-to-memory with XOR (RAID5).
    pub xor: bool,
    /// Memory fill.
    pub memset: bool,
    /// Scatter-gather.
    pub sg: bool,
    /// PQ (RAID6).
    pub pq: bool,
    /// Interrupt.
    pub interrupt: bool,
}

/// Single DMA channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmaChannel {
    /// Channel name (e.g. "dma0chan0").
    pub name: String,
    /// Whether channel is in use.
    pub in_use: bool,
    /// Transfer bytes completed (if available).
    pub bytes_transferred: Option<u64>,
    /// Number of transfers (if available).
    pub transfer_count: Option<u64>,
}

/// DMA controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmaController {
    /// Controller name (e.g. "dma0").
    pub name: String,
    /// Engine type.
    pub engine_type: DmaEngineType,
    /// Number of channels.
    pub channel_count: u32,
    /// Channels.
    pub channels: Vec<DmaChannel>,
    /// Capabilities.
    pub capabilities: DmaCapabilities,
    /// NUMA node (-1 if unknown).
    pub numa_node: i32,
}

/// DMA overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmaOverview {
    /// Controllers.
    pub controllers: Vec<DmaController>,
    /// Total controller count.
    pub total_controllers: u32,
    /// Total channels.
    pub total_channels: u32,
    /// Whether IOAT is available.
    pub has_ioat: bool,
    /// Whether DSA/IAX is available.
    pub has_dsa: bool,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// DMA engine monitor.
pub struct DmaEngineMonitor {
    overview: DmaOverview,
}

impl DmaEngineMonitor {
    /// Create a new DMA engine monitor.
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
    pub fn overview(&self) -> &DmaOverview {
        &self.overview
    }

    /// Get controllers.
    pub fn controllers(&self) -> &[DmaController] {
        &self.overview.controllers
    }

    /// Get controller by name.
    pub fn controller(&self, name: &str) -> Option<&DmaController> {
        self.overview.controllers.iter().find(|c| c.name == name)
    }

    #[cfg(target_os = "linux")]
    fn scan() -> Result<DmaOverview, IronError> {
        let dma_path = std::path::Path::new("/sys/class/dma");
        let mut controllers: std::collections::HashMap<String, DmaController> =
            std::collections::HashMap::new();

        if dma_path.exists() {
            let entries = std::fs::read_dir(dma_path).map_err(IronError::Io)?;

            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                // Channels are named like dma0chan0, dma0chan1, ...
                if let Some((ctrl_name, _chan_idx)) = Self::parse_channel_name(&name) {
                    let path = entry.path();

                    let in_use = Self::read_sysfs(&path.join("in_use"))
                        .map(|s| s != "0")
                        .unwrap_or(false);

                    let bytes_transferred = Self::read_sysfs_u64(&path.join("bytes_transferred"));
                    let transfer_count = Self::read_sysfs_u64(&path.join("memcpy_count"));

                    let channel = DmaChannel {
                        name: name.clone(),
                        in_use,
                        bytes_transferred,
                        transfer_count,
                    };

                    let controller = controllers.entry(ctrl_name.clone()).or_insert_with(|| {
                        let engine_type = Self::detect_engine_type(&ctrl_name);
                        let capabilities = Self::read_capabilities(&path);

                        // Resolve NUMA node from device link
                        let device_link = path.join("device");
                        let numa_node = if device_link.exists() {
                            std::fs::read_link(&device_link)
                                .ok()
                                .and_then(|p| {
                                    let numa_path = p.join("numa_node");
                                    Self::read_sysfs_i32(&numa_path)
                                })
                                .unwrap_or(-1)
                        } else {
                            -1
                        };

                        DmaController {
                            name: ctrl_name,
                            engine_type,
                            channel_count: 0,
                            channels: Vec::new(),
                            capabilities,
                            numa_node,
                        }
                    });

                    controller.channels.push(channel);
                    controller.channel_count = controller.channels.len() as u32;
                }
            }
        }

        // Also check for DSA/IAX devices
        Self::scan_idxd(&mut controllers);

        let mut ctrls: Vec<DmaController> = controllers.into_values().collect();
        ctrls.sort_by(|a, b| a.name.cmp(&b.name));

        let total = ctrls.len() as u32;
        let total_channels: u32 = ctrls.iter().map(|c| c.channel_count).sum();
        let has_ioat = ctrls
            .iter()
            .any(|c| c.engine_type == DmaEngineType::IntelIoat);
        let has_dsa = ctrls.iter().any(|c| {
            matches!(
                c.engine_type,
                DmaEngineType::IntelDsa | DmaEngineType::IntelIax | DmaEngineType::IntelIdxd
            )
        });

        let mut recs = Vec::new();
        if has_ioat {
            recs.push(
                "Intel IOAT available — enable kernel DMA offload for network/storage workloads"
                    .into(),
            );
        }
        if has_dsa {
            recs.push(
                "Intel DSA/IAX available — use IDXD driver for memory/compression offload".into(),
            );
        }

        Ok(DmaOverview {
            controllers: ctrls,
            total_controllers: total,
            total_channels,
            has_ioat,
            has_dsa,
            recommendations: recs,
        })
    }

    #[cfg(target_os = "linux")]
    fn parse_channel_name(name: &str) -> Option<(String, u32)> {
        // Pattern: dma<N>chan<M>
        if let Some(pos) = name.find("chan") {
            let ctrl_name = format!("dma{}", &name[3..pos]);
            let chan_idx: u32 = name[pos + 4..].parse().unwrap_or(0);
            Some((ctrl_name, chan_idx))
        } else {
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn detect_engine_type(ctrl_name: &str) -> DmaEngineType {
        // Try to read driver name for the DMA device
        let sysfs_path = format!(
            "/sys/class/dma/{}chan0/device/driver",
            ctrl_name.replace("dma", "dma0")
        );
        if let Ok(link) = std::fs::read_link(&sysfs_path) {
            let driver = link
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            return match driver.as_str() {
                "ioatdma" => DmaEngineType::IntelIoat,
                "idxd" => DmaEngineType::IntelIdxd,
                "dw-axi-dmac" => DmaEngineType::DwAxiDma,
                "pl330" => DmaEngineType::Pl330,
                other => DmaEngineType::Other(other.to_string()),
            };
        }
        DmaEngineType::Other(ctrl_name.to_string())
    }

    #[cfg(target_os = "linux")]
    fn read_capabilities(path: &std::path::Path) -> DmaCapabilities {
        // Read from device caps or from channel-level info
        let device_path = path.join("device");
        let caps_str = if device_path.exists() {
            Self::read_sysfs(&device_path.join("cap")).unwrap_or_default()
        } else {
            String::new()
        };

        DmaCapabilities {
            // Was `|| caps_str.is_empty()`, which claimed memcpy support
            // whenever the `cap` file was missing or unreadable -- asserting a
            // hardware capability the reader never observed. An unreadable cap
            // file is not evidence of a capability.
            memcpy: caps_str.contains("copy"),
            xor: caps_str.contains("xor"),
            memset: caps_str.contains("fill") || caps_str.contains("memset"),
            sg: caps_str.contains("sg"),
            pq: caps_str.contains("pq"),
            interrupt: caps_str.contains("interrupt"),
        }
    }

    #[cfg(target_os = "linux")]
    fn scan_idxd(controllers: &mut std::collections::HashMap<String, DmaController>) {
        let dsa_path = std::path::Path::new("/sys/bus/dsa/devices");
        if !dsa_path.exists() {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(dsa_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("dsa") && !name.starts_with("iax") {
                    continue;
                }

                let engine_type = if name.starts_with("dsa") {
                    DmaEngineType::IntelDsa
                } else {
                    DmaEngineType::IntelIax
                };

                let path = entry.path();
                let numa_node = Self::read_sysfs_i32(&path.join("numa_node")).unwrap_or(-1);

                // Count work queues
                let mut channels = Vec::new();
                for i in 0..16 {
                    let wq_path = path.join(format!("wq{}.{}", name, i));
                    if wq_path.exists() {
                        let state = Self::read_sysfs(&wq_path.join("state")).unwrap_or_default();
                        channels.push(DmaChannel {
                            name: format!("{}_wq{}", name, i),
                            in_use: state == "enabled",
                            bytes_transferred: None,
                            transfer_count: None,
                        });
                    }
                }

                let ch_count = channels.len() as u32;
                controllers.insert(
                    name.clone(),
                    DmaController {
                        name,
                        engine_type,
                        channel_count: ch_count,
                        channels,
                        capabilities: DmaCapabilities {
                            memcpy: true,
                            xor: false,
                            memset: true,
                            sg: true,
                            pq: false,
                            interrupt: true,
                        },
                        numa_node,
                    },
                );
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs(path: &std::path::Path) -> Option<String> {
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_u64(path: &std::path::Path) -> Option<u64> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_i32(path: &std::path::Path) -> Option<i32> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    #[cfg(not(target_os = "linux"))]
    fn scan() -> Result<DmaOverview, IronError> {
        // Returning an empty overview here made "this platform cannot
        // answer" indistinguishable from "this machine has none", which
        // is the same defect RAPL shipped once. The reason travels with
        // the error so a caller can report it.
        Err(IronError::UnsupportedPlatform(
            "DMA engine channels are read from `/sys/class/dma`, which this platform does not expose"
                .into(),
        ))
    }

    fn empty_overview() -> DmaOverview {
        DmaOverview {
            controllers: Vec::new(),
            total_controllers: 0,
            total_channels: 0,
            has_ioat: false,
            has_dsa: false,
            recommendations: Vec::new(),
        }
    }
}

impl Default for DmaEngineMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            overview: Self::empty_overview(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_type_display() {
        assert_eq!(DmaEngineType::IntelIoat.to_string(), "Intel IOAT");
        assert_eq!(DmaEngineType::IntelDsa.to_string(), "Intel DSA");
        assert_eq!(DmaEngineType::IntelIax.to_string(), "Intel IAX");
    }

    #[test]
    fn test_capabilities() {
        let caps = DmaCapabilities {
            memcpy: true,
            xor: true,
            memset: true,
            sg: false,
            pq: true,
            interrupt: true,
        };
        assert!(caps.memcpy);
        assert!(caps.pq);
        assert!(!caps.sg);
    }

    #[test]
    fn test_controller_channels() {
        let ctrl = DmaController {
            name: "dma0".into(),
            engine_type: DmaEngineType::IntelIoat,
            channel_count: 4,
            channels: vec![
                DmaChannel {
                    name: "dma0chan0".into(),
                    in_use: true,
                    bytes_transferred: Some(1_000_000),
                    transfer_count: Some(100),
                },
                DmaChannel {
                    name: "dma0chan1".into(),
                    in_use: false,
                    bytes_transferred: Some(0),
                    transfer_count: Some(0),
                },
                DmaChannel {
                    name: "dma0chan2".into(),
                    in_use: true,
                    bytes_transferred: Some(500_000),
                    transfer_count: Some(50),
                },
                DmaChannel {
                    name: "dma0chan3".into(),
                    in_use: false,
                    bytes_transferred: None,
                    transfer_count: None,
                },
            ],
            capabilities: DmaCapabilities {
                memcpy: true,
                xor: true,
                memset: true,
                sg: true,
                pq: false,
                interrupt: true,
            },
            numa_node: 0,
        };
        let active: Vec<_> = ctrl.channels.iter().filter(|c| c.in_use).collect();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_monitor_default() {
        let monitor = DmaEngineMonitor::default();
        let _overview = monitor.overview();
    }

    #[test]
    fn test_serialization() {
        let ch = DmaChannel {
            name: "dma0chan0".into(),
            in_use: true,
            bytes_transferred: Some(1_000_000),
            transfer_count: Some(100),
        };
        let json = serde_json::to_string(&ch).unwrap();
        assert!(json.contains("dma0chan0"));
        let _: DmaChannel = serde_json::from_str(&json).unwrap();
    }
}
