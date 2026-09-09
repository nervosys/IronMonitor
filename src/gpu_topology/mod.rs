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
    /// NUMA node affinity (-1 if unknown).
    pub numa_node: i32,
    /// PCIe generation.
    pub pcie_gen: u32,
    /// PCIe lane width.
    pub pcie_width: u32,
    /// PCIe link speed in GT/s.
    pub pcie_speed_gts: f64,
    /// Theoretical PCIe bandwidth in GB/s (one direction).
    pub pcie_bandwidth_gbs: f64,
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
    /// Bandwidth per link in GB/s.
    ///
    /// This one *is* a specification of the link type — NVLink 3.0 and 4.0 are
    /// both ~25 GB/s per link, xGMI ~23 — and is the same for every board using
    /// it, so it stays.
    pub bandwidth_per_link_gbs: f64,
    /// Total bandwidth in GB/s, where the link count is known.
    ///
    /// `num_links * bandwidth_per_link_gbs`, so `None` whenever the count is.
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
    /// Whether all GPUs share the same NUMA node.
    pub same_numa: bool,
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

    /// Get GPUs on a specific NUMA node.
    pub fn gpus_on_numa(&self, node: i32) -> Vec<&GpuTopologyNode> {
        self.overview
            .gpus
            .iter()
            .filter(|g| g.numa_node == node)
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

            // NUMA node
            let numa_node = Self::read_sysfs_i32(&path.join("numa_node")).unwrap_or(-1);

            // PCIe speed
            let (pcie_gen, pcie_width, pcie_speed_gts) = Self::read_pcie_info(&path);
            let pcie_bandwidth_gbs = match pcie_gen {
                1 => 0.25 * pcie_width as f64,
                2 => 0.5 * pcie_width as f64,
                3 => 0.985 * pcie_width as f64, // ~1 GB/s per lane
                4 => 1.969 * pcie_width as f64,
                5 => 3.938 * pcie_width as f64,
                6 => 7.563 * pcie_width as f64,
                _ => 0.0,
            };

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
                let (num_links, bw_per_link) = Self::link_bandwidth(&link_type, &gpus[i]);

                links.push(GpuLink {
                    from_gpu: gpus[i].index,
                    to_gpu: gpus[j].index,
                    link_type,
                    num_links,
                    bandwidth_per_link_gbs: bw_per_link,
                    total_bandwidth_gbs: num_links.map(|n| n as f64 * bw_per_link),
                });
            }
        }

        let gpu_count = gpus.len() as u32;
        let numa_nodes: std::collections::HashSet<i32> = gpus
            .iter()
            .map(|g| g.numa_node)
            .filter(|n| *n >= 0)
            .collect();
        let numa_node_count = numa_nodes.len() as u32;
        let has_nvlink = links
            .iter()
            .any(|l| l.link_type == GpuInterconnectType::NvLink);
        let has_xgmi = links
            .iter()
            .any(|l| l.link_type == GpuInterconnectType::XGmi);
        let same_numa = numa_nodes.len() <= 1;

        let mut recs = Vec::new();
        if gpu_count > 1 && !same_numa {
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
    fn read_pcie_info(path: &std::path::Path) -> (u32, u32, f64) {
        let speed_str = Self::read_sysfs(&path.join("current_link_speed")).unwrap_or_default();
        let width_str = Self::read_sysfs(&path.join("current_link_width")).unwrap_or_default();

        let width: u32 = width_str.trim_start_matches('x').parse().unwrap_or(0);

        let (gen, speed_gts) = if speed_str.contains("64.0") {
            (6, 64.0)
        } else if speed_str.contains("32.0") {
            (5, 32.0)
        } else if speed_str.contains("16.0") {
            (4, 16.0)
        } else if speed_str.contains("8.0") {
            (3, 8.0)
        } else if speed_str.contains("5.0") {
            (2, 5.0)
        } else if speed_str.contains("2.5") {
            (1, 2.5)
        } else {
            (0, 0.0)
        };

        (gen, width, speed_gts)
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

        // Check for xGMI (both AMD)
        if gpu_a.vendor == "amd" && gpu_b.vendor == "amd" && gpu_a.numa_node == gpu_b.numa_node {
            // AMD GPUs on same NUMA node may have xGMI
            let xgmi_path = format!("/sys/bus/pci/devices/{}/xgmi_hive_info", gpu_a.bdf);
            if std::path::Path::new(&xgmi_path).exists() {
                return GpuInterconnectType::XGmi;
            }
        }

        // PCIe topology heuristics
        if gpu_a.numa_node == gpu_b.numa_node && gpu_a.numa_node >= 0 {
            // Same root complex access likely
            GpuInterconnectType::PciePeerToPeer
        } else if gpu_a.numa_node >= 0 && gpu_b.numa_node >= 0 {
            GpuInterconnectType::PcieThroughCpu
        } else {
            GpuInterconnectType::Unknown
        }
    }

    #[cfg(target_os = "linux")]
    /// Per-link bandwidth for an interconnect type, and its link count where
    /// that is known.
    ///
    /// The per-link figure is a specification of the link type and is the same
    /// on every board using it. The **count** is per-board and is not read
    /// anywhere: this returned a flat 6 for NVLink and 2 for xGMI, so any pair
    /// of NVLink GPUs reported 150 GB/s between them whatever they actually
    /// had. PCIe peer-to-peer is a single link by definition, so that one is
    /// known.
    fn link_bandwidth(
        link_type: &GpuInterconnectType,
        gpu: &GpuTopologyNode,
    ) -> (Option<u32>, f64) {
        match link_type {
            // NVLink 3.0 and 4.0 are both ~25 GB/s per link. How many this
            // board has needs NVML.
            GpuInterconnectType::NvLink => (None, 25.0),
            GpuInterconnectType::XGmi => (None, 23.0),
            GpuInterconnectType::PciePeerToPeer | GpuInterconnectType::PcieThroughCpu => {
                (Some(1), gpu.pcie_bandwidth_gbs)
            }
            _ => (None, 0.0),
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
            same_numa: true,
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
                    numa_node: 0,
                    pcie_gen: 4,
                    pcie_width: 16,
                    pcie_speed_gts: 16.0,
                    pcie_bandwidth_gbs: 31.5,
                    iommu_group: Some(1),
                    driver: "nvidia".into(),
                },
                GpuTopologyNode {
                    bdf: "0000:41:00.0".into(),
                    index: 1,
                    name: "GPU1".into(),
                    vendor: "nvidia".into(),
                    numa_node: 1,
                    pcie_gen: 4,
                    pcie_width: 16,
                    pcie_speed_gts: 16.0,
                    pcie_bandwidth_gbs: 31.5,
                    iommu_group: Some(2),
                    driver: "nvidia".into(),
                },
            ],
            links: vec![GpuLink {
                from_gpu: 0,
                to_gpu: 1,
                link_type: GpuInterconnectType::PcieThroughCpu,
                num_links: Some(1),
                bandwidth_per_link_gbs: 31.5,
                total_bandwidth_gbs: Some(31.5),
            }],
            gpu_count: 2,
            numa_node_count: 2,
            has_nvlink: false,
            has_xgmi: false,
            same_numa: false,
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
                    numa_node: 0,
                    pcie_gen: 4,
                    pcie_width: 16,
                    pcie_speed_gts: 16.0,
                    pcie_bandwidth_gbs: 31.5,
                    iommu_group: None,
                    driver: "amdgpu".into(),
                },
                GpuTopologyNode {
                    bdf: "0000:02:00.0".into(),
                    index: 1,
                    name: "GPU1".into(),
                    vendor: "amd".into(),
                    numa_node: 0,
                    pcie_gen: 4,
                    pcie_width: 16,
                    pcie_speed_gts: 16.0,
                    pcie_bandwidth_gbs: 31.5,
                    iommu_group: None,
                    driver: "amdgpu".into(),
                },
            ],
            links: Vec::new(),
            gpu_count: 2,
            numa_node_count: 1,
            has_nvlink: false,
            has_xgmi: false,
            same_numa: true,
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
    fn test_serialization() {
        let node = GpuTopologyNode {
            bdf: "0000:01:00.0".into(),
            index: 0,
            name: "GPU0".into(),
            vendor: "nvidia".into(),
            numa_node: 0,
            pcie_gen: 5,
            pcie_width: 16,
            pcie_speed_gts: 32.0,
            pcie_bandwidth_gbs: 63.0,
            iommu_group: Some(10),
            driver: "nvidia".into(),
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("0000:01:00.0"));
        let _: GpuTopologyNode = serde_json::from_str(&json).unwrap();
    }
}
