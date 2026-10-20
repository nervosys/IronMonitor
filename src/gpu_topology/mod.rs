//! Multi-GPU topology and interconnect monitoring.
//!
//! Discovers GPU-to-GPU links (NVLink, PCIe peer-to-peer, AMD xGMI),
//! GPU-CPU affinity (NUMA node), peer access capability, and topology
//! structure for multi-GPU workloads.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/bus/pci/devices/`, NUMA info from `/sys/devices/system/node/`,
//!   NVLink via `/sys/bus/pci/devices/<bdf>/nvidia/gpu/nvlink*`
//! - **Windows / macOS**: PCI topology only

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// GPU interconnect type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuInterconnectType {
    /// NVIDIA NVLink.
    NvLink,
    /// AMD xGMI / Infinity Fabric.
    XGmi,
    /// PCIe peer-to-peer.
    PciePeerToPeer,
    /// PCIe through CPU (non-direct).
    PcieThroughCpu,
    /// PCIe through Host Bridge.
    PcieThroughHostBridge,
    /// Same board / on-die.
    SameBoard,
    /// No connection.
    None,
    Unknown,
}

impl std::fmt::Display for GpuInterconnectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NvLink => write!(f, "NVLink"),
            Self::XGmi => write!(f, "xGMI"),
            Self::PciePeerToPeer => write!(f, "PCIe P2P"),
            Self::PcieThroughCpu => write!(f, "PCIe (via CPU)"),
            Self::PcieThroughHostBridge => write!(f, "PCIe (via Host Bridge)"),
            Self::SameBoard => write!(f, "Same Board"),
            Self::None => write!(f, "None"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// GPU topology node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuTopologyNode {
    /// PCI Bus:Device.Function address.
    pub bdf: String,
    /// GPU index.
    pub index: u32,
    /// Device name / product.
    pub name: String,
    /// Vendor (nvidia / amd / intel).
    pub vendor: String,
    /// NUMA node affinity.
    ///
    /// `None` when the node was not read. The kernel writes `-1` for "no
    /// affinity", which is not a node, so that is `None` too.
    pub numa_node: Option<u32>,
    /// PCIe generation, or `None` when the link speed was not read or is not
    /// one this parser knows.
    pub pcie_gen: Option<u8>,
    /// PCIe lane width. `Some(0)` is the kernel reporting the link down.
    pub pcie_width: Option<u8>,
    /// PCIe link speed in GT/s, or `None` when it was not read or not recognised.
    pub pcie_speed_gts: Option<f64>,
    /// Theoretical PCIe bandwidth in GB/s (one direction). Computed only when
    /// both the generation and the width are known.
    pub pcie_bandwidth_gbs: Option<f64>,
    /// IOMMU group (if available).
    pub iommu_group: Option<u32>,
    /// Driver in use.
    pub driver: String,
}

/// Link between two GPUs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuLink {
    /// Source GPU index.
    pub from_gpu: u32,
    /// Destination GPU index.
    pub to_gpu: u32,
    /// Interconnect type.
    pub link_type: GpuInterconnectType,
    /// Number of links, where it is known.
    ///
    /// Not known anywhere yet. `estimate_link_bandwidth` returned a flat `6`
    /// for any NVLink pair and `2` for any xGMI pair, under comments reading
    /// "Typical config: 6 or 12 links" and "typically 2-4 links". A link
    /// *count* is a property of the specific board — an A100 has 12, an RTX
    /// 3090 has 4 — so a typical value is not this pair's value. Reading it
    /// needs NVML's `nvmlDeviceGetNvLinkState` or `nvidia-smi nvlink -s`.
    pub num_links: Option<u32>,
    /// Bandwidth per link in GB/s, where it was read.
    ///
    /// `None` for NVLink and xGMI, where no per-link rate is read. For PCIe it
    /// is the slower end's bandwidth, when both GPUs' `pcie_bandwidth_gbs` are
    /// known.
    pub bandwidth_per_link_gbs: Option<f64>,
    /// Total bandwidth in GB/s, where the link count and the per-link rate are
    /// both known.
    ///
    /// `num_links * bandwidth_per_link_gbs`, so `None` whenever either is.
    /// This read 150 GB/s for any NVLink pair.
    pub total_bandwidth_gbs: Option<f64>,
}

/// GPU topology overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuTopologyOverview {
    /// All GPUs as topology nodes.
    pub gpus: Vec<GpuTopologyNode>,
    /// GPU-to-GPU links.
    pub links: Vec<GpuLink>,
    /// Total GPU count.
    pub gpu_count: u32,
    /// Number of NUMA nodes with GPUs.
    pub numa_node_count: u32,
    /// Whether NVLink is present.
    pub has_nvlink: bool,
    /// Whether xGMI is present.
    pub has_xgmi: bool,
    /// Whether all GPUs share one NUMA node: `Some(false)` when two GPUs were
    /// read on different nodes, `Some(true)` when every GPU's node was read and
    /// they agree, `None` otherwise. A GPU with no node read is evidence for
    /// neither answer.
    pub same_numa: Option<bool>,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// GPU topology monitor.
pub struct GpuTopologyMonitor {
    overview: GpuTopologyOverview,
}

impl GpuTopologyMonitor {
    /// Create a new GPU topology monitor.
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
    pub fn overview(&self) -> &GpuTopologyOverview {
        &self.overview
    }

    /// Get GPUs.
    pub fn gpus(&self) -> &[GpuTopologyNode] {
        &self.overview.gpus
    }

    /// Get links.
    pub fn links(&self) -> &[GpuLink] {
        &self.overview.links
    }

    /// Get links for a specific GPU.
    pub fn links_for_gpu(&self, gpu_index: u32) -> Vec<&GpuLink> {
        self.overview
            .links
            .iter()
            .filter(|l| l.from_gpu == gpu_index || l.to_gpu == gpu_index)
            .collect()
    }

    /// Get GPUs whose NUMA node was read as `node`. A GPU with no node read is
    /// on none of them.
    pub fn gpus_on_numa(&self, node: u32) -> Vec<&GpuTopologyNode> {
        self.overview
            .gpus
            .iter()
            .filter(|g| g.numa_node == Some(node))
            .collect()
    }

    #[cfg(target_os = "linux")]
    fn scan() -> Result<GpuTopologyOverview, IronError> {
        let pci_path = std::path::Path::new("/sys/bus/pci/devices");

        if !pci_path.exists() {
            return Ok(Self::empty_overview());
        }

        let entries = std::fs::read_dir(pci_path).map_err(IronError::Io)?;
        let mut gpus = Vec::new();
        let mut gpu_index = 0u32;

        // GPU PCI class: 0x030000 (VGA), 0x030200 (3D controller)
        for entry in entries.flatten() {
            let path = entry.path();
            let class = Self::read_sysfs(&path.join("class")).unwrap_or_default();

            // Check if this is a display/3D controller
            let class_val = u32::from_str_radix(class.trim_start_matches("0x"), 16).unwrap_or(0);
            let class_major = class_val >> 16;
            if class_major != 0x03 {
                continue;
            }

            let bdf = entry.file_name().to_string_lossy().to_string();
            let vendor_id = Self::read_sysfs(&path.join("vendor")).unwrap_or_default();
            let device_id = Self::read_sysfs(&path.join("device")).unwrap_or_default();

            let vendor = match vendor_id.as_str() {
                "0x10de" => "nvidia",
                "0x1002" => "amd",
                "0x8086" => "intel",
                _ => "unknown",
            };

            let name = format!("{}:{} GPU {}", vendor, device_id, gpu_index);

            // NUMA node. The kernel writes -1 for "no affinity", which is not a node.
            let numa_node =
                Self::read_sysfs_i32(&path.join("numa_node")).and_then(|n| u32::try_from(n).ok());

            // PCIe speed
            let (pcie_gen, pcie_width, pcie_speed_gts) = Self::read_pcie_info(&path);
            let pcie_bandwidth_gbs = pcie_bandwidth(pcie_gen, pcie_width);

            // IOMMU group
            let iommu_group = std::fs::read_link(path.join("iommu_group"))
                .ok()
                .and_then(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str().and_then(|s| s.parse().ok()))
                });

            // Driver
            let driver = std::fs::read_link(path.join("driver"))
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_default();

            gpus.push(GpuTopologyNode {
                bdf,
                index: gpu_index,
                name,
                vendor: vendor.to_string(),
                numa_node,
                pcie_gen,
                pcie_width,
                pcie_speed_gts,
                pcie_bandwidth_gbs,
                iommu_group,
                driver,
            });

            gpu_index += 1;
        }

        // Build links
        let mut links = Vec::new();
        for i in 0..gpus.len() {
            for j in (i + 1)..gpus.len() {
                let link_type = Self::detect_link_type(&gpus[i], &gpus[j]);
                let (num_links, bw_per_link) = Self::link_bandwidth(&link_type, &gpus[i], &gpus[j]);

                links.push(GpuLink {
                    from_gpu: gpus[i].index,
                    to_gpu: gpus[j].index,
                    link_type,
                    num_links,
                    bandwidth_per_link_gbs: bw_per_link,
                    total_bandwidth_gbs: num_links.zip(bw_per_link).map(|(n, bw)| n as f64 * bw),
                });
            }
        }

        let gpu_count = gpus.len() as u32;
        let numa_nodes: std::collections::HashSet<u32> =
            gpus.iter().filter_map(|g| g.numa_node).collect();
        let numa_node_count = numa_nodes.len() as u32;
        let has_nvlink = links
            .iter()
            .any(|l| l.link_type == GpuInterconnectType::NvLink);
        let has_xgmi = links
            .iter()
            .any(|l| l.link_type == GpuInterconnectType::XGmi);
        // Only nodes that were read count. A GPU with no node read is not
        // evidence that the GPUs sit on different nodes.
        let same_numa = if numa_nodes.len() > 1 {
            Some(false)
        } else if gpus.iter().all(|g| g.numa_node.is_some()) {
            Some(true)
        } else {
            None
        };

        let mut recs = Vec::new();
        if gpu_count > 1 && numa_nodes.len() > 1 {
            recs.push("GPUs span multiple NUMA nodes — consider NUMA-aware scheduling".into());
        }
        if gpu_count > 1 && !has_nvlink && !has_xgmi {
            recs.push("No high-bandwidth GPU interconnect (NVLink/xGMI) — GPU communication will use PCIe".into());
        }

        Ok(GpuTopologyOverview {
            gpus,
            links,
            gpu_count,
            numa_node_count,
            has_nvlink,
            has_xgmi,
            same_numa,
            recommendations: recs,
        })
    }

    #[cfg(target_os = "linux")]
    fn read_pcie_info(path: &std::path::Path) -> (Option<u8>, Option<u8>, Option<f64>) {
        let speed_str = Self::read_sysfs(&path.join("current_link_speed")).unwrap_or_default();
        let width_str = Self::read_sysfs(&path.join("current_link_width")).unwrap_or_default();

        parse_link(&speed_str, &width_str)
    }

    #[cfg(target_os = "linux")]
    fn detect_link_type(gpu_a: &GpuTopologyNode, gpu_b: &GpuTopologyNode) -> GpuInterconnectType {
        // Check for NVLink (both NVIDIA on same NUMA node with NVLink sysfs entries)
        if gpu_a.vendor == "nvidia" && gpu_b.vendor == "nvidia" {
            let nvlink_path = format!("/sys/bus/pci/devices/{}/nvidia", gpu_a.bdf);
            if std::path::Path::new(&nvlink_path).exists() {
                return GpuInterconnectType::NvLink;
            }
        }

        // Both nodes must have been read: an unread node is not the same as
        // another unread node.
        let numa_pair = (gpu_a.numa_node, gpu_b.numa_node);
        let same_numa = matches!(numa_pair, (Some(a), Some(b)) if a == b);
        let different_numa = matches!(numa_pair, (Some(a), Some(b)) if a != b);

        // Check for xGMI (both AMD)
        if gpu_a.vendor == "amd" && gpu_b.vendor == "amd" && same_numa {
            // AMD GPUs on same NUMA node may have xGMI
            let xgmi_path = format!("/sys/bus/pci/devices/{}/xgmi_hive_info", gpu_a.bdf);
            if std::path::Path::new(&xgmi_path).exists() {
                return GpuInterconnectType::XGmi;
            }
        }

        // PCIe topology heuristics
        if same_numa {
            // Same root complex access likely
            GpuInterconnectType::PciePeerToPeer
        } else if different_numa {
            GpuInterconnectType::PcieThroughCpu
        } else {
            GpuInterconnectType::Unknown
        }
    }

    #[cfg(target_os = "linux")]
    /// Per-link bandwidth for an interconnect type, and its link count where
    /// that is known.
    ///
    /// The **count** is per-board and is not read anywhere: this returned a
    /// flat 6 for NVLink and 2 for xGMI, so any pair of NVLink GPUs reported
    /// 150 GB/s between them whatever they actually had. PCIe peer-to-peer is
    /// a single link by definition, so that one is known.
    ///
    /// The per-link rate is `None` for NVLink and xGMI. The 25 and 23 GB/s
    /// constants this used were specification-sheet figures that ignored the
    /// link generation, so they are not reported as readings. For PCIe the rate
    /// is the slower end's bandwidth, and it is `None` unless both ends are known.
    fn link_bandwidth(
        link_type: &GpuInterconnectType,
        gpu_a: &GpuTopologyNode,
        gpu_b: &GpuTopologyNode,
    ) -> (Option<u32>, Option<f64>) {
        match link_type {
            // Nothing here reads a per-link rate for these, and how many links
            // a board has needs NVML.
            GpuInterconnectType::NvLink | GpuInterconnectType::XGmi => (None, None),
            GpuInterconnectType::PciePeerToPeer | GpuInterconnectType::PcieThroughCpu => {
                let bw = gpu_a
                    .pcie_bandwidth_gbs
                    .zip(gpu_b.pcie_bandwidth_gbs)
                    .map(|(a, b)| a.min(b));
                (Some(1), bw)
            }
            _ => (None, None),
        }
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs(path: &std::path::Path) -> Option<String> {
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_i32(path: &std::path::Path) -> Option<i32> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    #[cfg(not(target_os = "linux"))]
    fn scan() -> Result<GpuTopologyOverview, IronError> {
        // Returning an empty overview here made "this platform cannot
        // answer" indistinguishable from "this machine has none", which
        // is the same defect RAPL shipped once. The reason travels with
        // the error so a caller can report it.
        Err(IronError::UnsupportedPlatform(
            "GPU interconnect topology is read from `/sys/class/drm` and `nvidia-smi topo`, which this platform does not expose"
                .into(),
        ))
    }

    fn empty_overview() -> GpuTopologyOverview {
        GpuTopologyOverview {
            gpus: Vec::new(),
            links: Vec::new(),
            gpu_count: 0,
            numa_node_count: 0,
            has_nvlink: false,
            has_xgmi: false,
            same_numa: Some(true),
            recommendations: Vec::new(),
        }
    }
}

impl Default for GpuTopologyMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            overview: Self::empty_overview(),
        })
    }
}

/// Parses sysfs `current_link_speed` and `current_link_width` into the PCIe
/// generation, lane width and speed in GT/s.
///
/// An empty or unrecognised speed gives `None` for the generation and the
/// speed, not a generation of zero. The width is read on its own, and a width
/// of 0 is the kernel reporting the link down, so it stays `Some(0)`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_link(speed: &str, width: &str) -> (Option<u8>, Option<u8>, Option<f64>) {
    let width = width.trim_start_matches('x').parse::<u8>().ok();

    let (generation, speed_gts) = if speed.contains("64.0") {
        (6, 64.0)
    } else if speed.contains("32.0") {
        (5, 32.0)
    } else if speed.contains("16.0") {
        (4, 16.0)
    } else if speed.contains("8.0") {
        (3, 8.0)
    } else if speed.contains("5.0") {
        (2, 5.0)
    } else if speed.contains("2.5") {
        (1, 2.5)
    } else {
        return (None, width, None);
    };

    (Some(generation), width, Some(speed_gts))
}

/// Theoretical one-direction PCIe bandwidth in GB/s: the per-lane rate of the
/// generation times the width. `None` unless both are known, and `None` for a
/// generation this table does not cover.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn pcie_bandwidth(generation: Option<u8>, width: Option<u8>) -> Option<f64> {
    let width = width? as f64;
    let per_lane = match generation? {
        1 => 0.25,
        2 => 0.5,
        3 => 0.985, // ~1 GB/s per lane
        4 => 1.969,
        5 => 3.938,
        6 => 7.563,
        _ => return None,
    };
    Some(per_lane * width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interconnect_display() {
        assert_eq!(GpuInterconnectType::NvLink.to_string(), "NVLink");
        assert_eq!(GpuInterconnectType::XGmi.to_string(), "xGMI");
        assert_eq!(GpuInterconnectType::PciePeerToPeer.to_string(), "PCIe P2P");
    }

    #[test]
    fn test_links_for_gpu() {
        let overview = GpuTopologyOverview {
            gpus: vec![
                GpuTopologyNode {
                    bdf: "0000:01:00.0".into(),
                    index: 0,
                    name: "GPU0".into(),
                    vendor: "nvidia".into(),
                    numa_node: Some(0),
                    pcie_gen: Some(4),
                    pcie_width: Some(16),
                    pcie_speed_gts: Some(16.0),
                    pcie_bandwidth_gbs: Some(31.5),
                    iommu_group: Some(1),
                    driver: "nvidia".into(),
                },
                GpuTopologyNode {
                    bdf: "0000:41:00.0".into(),
                    index: 1,
                    name: "GPU1".into(),
                    vendor: "nvidia".into(),
                    numa_node: Some(1),
                    pcie_gen: Some(4),
                    pcie_width: Some(16),
                    pcie_speed_gts: Some(16.0),
                    pcie_bandwidth_gbs: Some(31.5),
                    iommu_group: Some(2),
                    driver: "nvidia".into(),
                },
            ],
            links: vec![GpuLink {
                from_gpu: 0,
                to_gpu: 1,
                link_type: GpuInterconnectType::PcieThroughCpu,
                num_links: Some(1),
                bandwidth_per_link_gbs: Some(31.5),
                total_bandwidth_gbs: Some(31.5),
            }],
            gpu_count: 2,
            numa_node_count: 2,
            has_nvlink: false,
            has_xgmi: false,
            same_numa: Some(false),
            recommendations: Vec::new(),
        };
        let monitor = GpuTopologyMonitor { overview };
        assert_eq!(monitor.links_for_gpu(0).len(), 1);
        assert_eq!(monitor.links_for_gpu(1).len(), 1);
    }

    #[test]
    fn test_numa_grouping() {
        let overview = GpuTopologyOverview {
            gpus: vec![
                GpuTopologyNode {
                    bdf: "0000:01:00.0".into(),
                    index: 0,
                    name: "GPU0".into(),
                    vendor: "amd".into(),
                    numa_node: Some(0),
                    pcie_gen: Some(4),
                    pcie_width: Some(16),
                    pcie_speed_gts: Some(16.0),
                    pcie_bandwidth_gbs: Some(31.5),
                    iommu_group: None,
                    driver: "amdgpu".into(),
                },
                GpuTopologyNode {
                    bdf: "0000:02:00.0".into(),
                    index: 1,
                    name: "GPU1".into(),
                    vendor: "amd".into(),
                    numa_node: Some(0),
                    pcie_gen: Some(4),
                    pcie_width: Some(16),
                    pcie_speed_gts: Some(16.0),
                    pcie_bandwidth_gbs: Some(31.5),
                    iommu_group: None,
                    driver: "amdgpu".into(),
                },
            ],
            links: Vec::new(),
            gpu_count: 2,
            numa_node_count: 1,
            has_nvlink: false,
            has_xgmi: false,
            same_numa: Some(true),
            recommendations: Vec::new(),
        };
        let monitor = GpuTopologyMonitor { overview };
        assert_eq!(monitor.gpus_on_numa(0).len(), 2);
    }

    #[test]
    fn test_monitor_default() {
        let monitor = GpuTopologyMonitor::default();
        let _overview = monitor.overview();
    }

    #[test]
    fn test_unknown_link_speed_is_not_read() {
        // An unrecognised speed is "not read", not generation 0 at 0 GB/s.
        let (generation, width, speed) = parse_link("unknown", "x16");
        assert_eq!(generation, None);
        assert_eq!(width, Some(16));
        assert_eq!(speed, None);
        assert_eq!(pcie_bandwidth(generation, width), None);

        // A width of 0 is the kernel reporting the link down, which is a reading.
        let (generation, width, _) = parse_link("8.0 GT/s PCIe", "x0");
        assert_eq!(generation, Some(3));
        assert_eq!(pcie_bandwidth(generation, width), Some(0.0));
    }

    #[test]
    fn test_serialization() {
        let node = GpuTopologyNode {
            bdf: "0000:01:00.0".into(),
            index: 0,
            name: "GPU0".into(),
            vendor: "nvidia".into(),
            numa_node: Some(0),
            pcie_gen: Some(5),
            pcie_width: Some(16),
            pcie_speed_gts: Some(32.0),
            pcie_bandwidth_gbs: Some(63.0),
            iommu_group: Some(10),
            driver: "nvidia".into(),
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("0000:01:00.0"));
        let _: GpuTopologyNode = serde_json::from_str(&json).unwrap();
    }
}
