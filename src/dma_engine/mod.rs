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
    /// The controller exists but its driver could not be read from sysfs.
    /// Distinct from `Other`, which names a driver that was read.
    Unknown,
    /// A driver was read and is not one this reader classifies.
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
            Self::Unknown => write!(f, "Unknown"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

/// DMA channel capabilities.
///
/// Each field is `None` when the reader did not observe it. An unreadable
/// capability file leaves every field `None`; it is not evidence that the
/// hardware lacks the capability, and it is not evidence that it has it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmaCapabilities {
    /// Memory-to-memory copy.
    pub memcpy: Option<bool>,
    /// Memory-to-memory with XOR (RAID5).
    pub xor: Option<bool>,
    /// Memory fill.
    pub memset: Option<bool>,
    /// Scatter-gather.
    pub sg: Option<bool>,
    /// PQ (RAID6).
    pub pq: Option<bool>,
    /// Interrupt.
    pub interrupt: Option<bool>,
}

impl DmaCapabilities {
    /// Every capability unknown: the source was not read.
    #[cfg(any(target_os = "linux", test))]
    fn unread() -> Self {
        Self {
            memcpy: None,
            xor: None,
            memset: None,
            sg: None,
            pq: None,
            interrupt: None,
        }
    }
}

/// Map a driver name, as read from the channel's `device/driver` link, to an
/// engine type. `None` means the link was not readable.
#[cfg(any(target_os = "linux", test))]
fn engine_type_from_driver(driver: Option<&str>) -> DmaEngineType {
    match driver {
        Some("ioatdma") => DmaEngineType::IntelIoat,
        Some("idxd") => DmaEngineType::IntelIdxd,
        Some("dw-axi-dmac") => DmaEngineType::DwAxiDma,
        Some("pl330") => DmaEngineType::Pl330,
        Some(other) => DmaEngineType::Other(other.to_string()),
        None => DmaEngineType::Unknown,
    }
}

/// Parse the device `cap` file. The file was read, so every field is known:
/// a capability it does not name is observed to be absent.
#[cfg(any(target_os = "linux", test))]
fn capabilities_from_cap_string(caps: &str) -> DmaCapabilities {
    DmaCapabilities {
        memcpy: Some(caps.contains("copy")),
        xor: Some(caps.contains("xor")),
        memset: Some(caps.contains("fill") || caps.contains("memset")),
        sg: Some(caps.contains("sg")),
        pq: Some(caps.contains("pq")),
        interrupt: Some(caps.contains("interrupt")),
    }
}

/// Single DMA channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmaChannel {
    /// Channel name (e.g. "dma0chan0").
    pub name: String,
    /// Whether channel is in use. `None` when the `in_use` (or work-queue
    /// `state`) file was not readable.
    pub in_use: Option<bool>,
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

                    // An unreadable `in_use` was reported as idle. It is unknown.
                    let in_use = Self::read_sysfs(&path.join("in_use")).map(|s| s != "0");

                    let bytes_transferred = Self::read_sysfs_u64(&path.join("bytes_transferred"));
                    let transfer_count = Self::read_sysfs_u64(&path.join("memcpy_count"));

                    let channel = DmaChannel {
                        name: name.clone(),
                        in_use,
                        bytes_transferred,
                        transfer_count,
                    };

                    let controller = controllers.entry(ctrl_name.clone()).or_insert_with(|| {
                        let engine_type = Self::detect_engine_type(&path);
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

    /// Engine type from the driver bound to a channel's device.
    ///
    /// `channel_path` is the channel's own `/sys/class/dma/dmaNchanM` entry.
    /// This used to build `/sys/class/dma/dmaNchan0` from the controller name
    /// with `replace("dma", "dma0")`, which turned `dma0` into `dma00`. The
    /// path never existed, so every controller fell through to
    /// `Other("dma0")`: a controller name sitting in the type field.
    #[cfg(target_os = "linux")]
    fn detect_engine_type(channel_path: &std::path::Path) -> DmaEngineType {
        let driver = std::fs::read_link(channel_path.join("device/driver"))
            .ok()
            .and_then(|link| link.file_name().map(|n| n.to_string_lossy().into_owned()));
        engine_type_from_driver(driver.as_deref())
    }

    #[cfg(target_os = "linux")]
    fn read_capabilities(path: &std::path::Path) -> DmaCapabilities {
        match std::fs::read_to_string(path.join("device/cap")) {
            Ok(caps) => capabilities_from_cap_string(caps.trim()),
            // The cap file was not readable: no field is known.
            Err(_) => DmaCapabilities::unread(),
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
                        let state = Self::read_sysfs(&wq_path.join("state"));
                        channels.push(DmaChannel {
                            name: format!("{}_wq{}", name, i),
                            in_use: state.map(|s| s == "enabled"),
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
                        // This used to be a fixed set (memcpy, memset, sg,
                        // interrupt true; xor, pq false) applied to every
                        // DSA/IAX device without reading any of it. None of
                        // these were observed, so none is reported.
                        capabilities: DmaCapabilities::unread(),
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
            memcpy: Some(true),
            xor: Some(true),
            memset: Some(true),
            sg: Some(false),
            pq: Some(true),
            interrupt: Some(true),
        };
        assert_eq!(caps.memcpy, Some(true));
        assert_eq!(caps.pq, Some(true));
        assert_eq!(caps.sg, Some(false));
    }

    #[test]
    fn driver_names_map_to_engine_types() {
        assert_eq!(
            engine_type_from_driver(Some("ioatdma")),
            DmaEngineType::IntelIoat
        );
        assert_eq!(
            engine_type_from_driver(Some("idxd")),
            DmaEngineType::IntelIdxd
        );
        assert_eq!(engine_type_from_driver(Some("pl330")), DmaEngineType::Pl330);
    }

    #[test]
    fn a_driver_that_is_read_but_unlisted_is_named_not_unknown() {
        assert_eq!(
            engine_type_from_driver(Some("mv_xor")),
            DmaEngineType::Other("mv_xor".into())
        );
    }

    #[test]
    fn an_unreadable_driver_link_is_unknown_not_a_controller_name() {
        // The controller name used to land here as `Other("dma0")`.
        assert_eq!(engine_type_from_driver(None), DmaEngineType::Unknown);
        assert_eq!(DmaEngineType::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn cap_file_names_each_capability_it_states() {
        let caps = capabilities_from_cap_string("copy fill interrupt");
        assert_eq!(caps.memcpy, Some(true));
        assert_eq!(caps.memset, Some(true));
        assert_eq!(caps.interrupt, Some(true));
        // Read, and not naming these: observed absent, not unknown.
        assert_eq!(caps.xor, Some(false));
        assert_eq!(caps.pq, Some(false));
        assert_eq!(caps.sg, Some(false));
    }

    #[test]
    fn an_unread_cap_file_asserts_nothing() {
        let caps = DmaCapabilities::unread();
        assert_eq!(caps.memcpy, None);
        assert_eq!(caps.xor, None);
        assert_eq!(caps.memset, None);
        assert_eq!(caps.sg, None);
        assert_eq!(caps.pq, None);
        assert_eq!(caps.interrupt, None);
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
                    in_use: Some(true),
                    bytes_transferred: Some(1_000_000),
                    transfer_count: Some(100),
                },
                DmaChannel {
                    name: "dma0chan1".into(),
                    in_use: Some(false),
                    bytes_transferred: Some(0),
                    transfer_count: Some(0),
                },
                DmaChannel {
                    name: "dma0chan2".into(),
                    in_use: Some(true),
                    bytes_transferred: Some(500_000),
                    transfer_count: Some(50),
                },
                DmaChannel {
                    name: "dma0chan3".into(),
                    in_use: Some(false),
                    bytes_transferred: None,
                    transfer_count: None,
                },
            ],
            capabilities: DmaCapabilities {
                memcpy: Some(true),
                xor: Some(true),
                memset: Some(true),
                sg: Some(true),
                pq: Some(false),
                interrupt: Some(true),
            },
            numa_node: 0,
        };
        let active: Vec<_> = ctrl
            .channels
            .iter()
            .filter(|c| c.in_use == Some(true))
            .collect();
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
            in_use: Some(true),
            bytes_transferred: Some(1_000_000),
            transfer_count: Some(100),
        };
        let json = serde_json::to_string(&ch).unwrap();
        assert!(json.contains("dma0chan0"));
        let _: DmaChannel = serde_json::from_str(&json).unwrap();
    }
}
