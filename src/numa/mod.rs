//! NUMA topology and memory affinity monitoring.
//!
//! Exposes Non-Uniform Memory Access (NUMA) node layout, CPU-to-node mapping,
//! per-node memory statistics, and inter-node distance matrices.
//!
//! # Platform Support
//!
//! - **Linux**: Reads `/sys/devices/system/node/nodeN/`
//! - **Windows**: Uses `GetLogicalProcessorInformationEx` / WMI
//! - **macOS**: Uses `sysctl hw.packages`, limited NUMA visibility
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::numa::NumaMonitor;
//!
//! let monitor = NumaMonitor::new().unwrap();
//! for node in monitor.nodes() {
//!     println!("Node {}: {} CPUs, {:.1} GB total memory",
//!         node.id, node.cpus.len(), node.memory_total_bytes as f64 / 1e9);
//! }
//! if let Some(dist) = monitor.distance_matrix() {
//!     println!("NUMA distance matrix: {:?}", dist);
//! }
//! ```

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// A single NUMA node and its resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumaNode {
    /// NUMA node ID (0-based)
    pub id: u32,
    /// CPUs (logical processor IDs) belonging to this node
    pub cpus: Vec<u32>,
    /// Total memory in bytes
    pub memory_total_bytes: u64,
    /// Free memory in bytes
    pub memory_free_bytes: u64,
    /// Used memory in bytes
    pub memory_used_bytes: u64,
    /// Number of huge pages (Linux)
    pub hugepages_total: u64,
    /// Free huge pages (Linux)
    pub hugepages_free: u64,
    /// PCI devices attached to this node (BDF addresses)
    pub pci_devices: Vec<String>,
}

/// Inter-node distance matrix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumaDistanceMatrix {
    /// Size (number of nodes)
    pub size: usize,
    /// Flattened row-major distance matrix.
    /// Element [i*size + j] = distance from node i to node j.
    /// Local access = 10, remote access > 10 (typically 20-40).
    pub distances: Vec<u32>,
}

impl NumaDistanceMatrix {
    /// Get distance from node `from` to node `to`.
    pub fn distance(&self, from: usize, to: usize) -> Option<u32> {
        if from < self.size && to < self.size {
            Some(self.distances[from * self.size + to])
        } else {
            None
        }
    }

    /// Check if the system has non-uniform memory access (distances vary).
    pub fn is_numa(&self) -> bool {
        self.size > 1 && self.distances.iter().any(|&d| d != 10)
    }
}

/// Summary of NUMA topology.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumaSummary {
    /// Number of NUMA nodes
    pub node_count: usize,
    /// Total CPUs across all nodes
    pub total_cpus: usize,
    /// Total memory across all nodes (bytes)
    pub total_memory_bytes: u64,
    /// Whether the system is truly NUMA (vs UMA)
    pub is_numa: bool,
    /// Maximum inter-node distance
    pub max_distance: u32,
    /// Memory imbalance ratio (max_node_mem / min_node_mem)
    pub memory_imbalance_ratio: f64,
    /// CPU imbalance ratio
    pub cpu_imbalance_ratio: f64,
}

/// NUMA topology monitor.
pub struct NumaMonitor {
    nodes: Vec<NumaNode>,
    distance_matrix: Option<NumaDistanceMatrix>,
}

impl NumaMonitor {
    pub fn new() -> Result<Self, IronError> {
        let mut monitor = Self {
            nodes: Vec::new(),
            distance_matrix: None,
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    /// Re-read the NUMA topology.
    ///
    /// Returns `Err` when the read failed. It used to end with
    ///
    /// ```ignore
    /// if self.nodes.is_empty() { self.create_uma_fallback(); }
    /// ```
    ///
    /// which pushed a node 0 with zeroed memory and a distance matrix of
    /// `[10]` -- the SLIT convention for "local" -- whenever nothing had been
    /// read. A machine whose topology could not be determined was therefore
    /// published as a single-node UMA machine with a fabricated distance
    /// matrix, and `memory.numa.is_numa` said `false` as a measurement. Each
    /// platform now builds its own single-node case where that is a reading,
    /// and reports a failure where it is not.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.nodes.clear();
        self.distance_matrix = None;

        #[cfg(target_os = "linux")]
        self.refresh_linux()?;

        #[cfg(target_os = "windows")]
        self.refresh_windows()?;

        #[cfg(target_os = "macos")]
        self.refresh_macos()?;

        Ok(())
    }

    pub fn nodes(&self) -> &[NumaNode] {
        &self.nodes
    }

    pub fn distance_matrix(&self) -> Option<&NumaDistanceMatrix> {
        self.distance_matrix.as_ref()
    }

    /// Get a summary of the NUMA topology.
    pub fn summary(&self) -> NumaSummary {
        let total_cpus: usize = self.nodes.iter().map(|n| n.cpus.len()).sum();
        let total_memory: u64 = self.nodes.iter().map(|n| n.memory_total_bytes).sum();

        let is_numa =
            self.nodes.len() > 1 || self.distance_matrix.as_ref().is_some_and(|d| d.is_numa());

        let max_distance = self
            .distance_matrix
            .as_ref()
            .map(|d| d.distances.iter().copied().max().unwrap_or(10))
            .unwrap_or(10);

        let mem_vals: Vec<u64> = self
            .nodes
            .iter()
            .map(|n| n.memory_total_bytes)
            .filter(|&m| m > 0)
            .collect();
        let memory_imbalance = if mem_vals.len() >= 2 {
            let max = *mem_vals.iter().max().unwrap() as f64;
            let min = *mem_vals.iter().min().unwrap() as f64;
            if min > 0.0 {
                max / min
            } else {
                1.0
            }
        } else {
            1.0
        };

        let cpu_vals: Vec<usize> = self
            .nodes
            .iter()
            .map(|n| n.cpus.len())
            .filter(|&c| c > 0)
            .collect();
        let cpu_imbalance = if cpu_vals.len() >= 2 {
            let max = *cpu_vals.iter().max().unwrap() as f64;
            let min = *cpu_vals.iter().min().unwrap() as f64;
            if min > 0.0 {
                max / min
            } else {
                1.0
            }
        } else {
            1.0
        };

        NumaSummary {
            node_count: self.nodes.len(),
            total_cpus,
            total_memory_bytes: total_memory,
            is_numa,
            max_distance,
            memory_imbalance_ratio: memory_imbalance,
            cpu_imbalance_ratio: cpu_imbalance,
        }
    }

    /// Which NUMA node owns a given CPU.
    pub fn cpu_to_node(&self, cpu_id: u32) -> Option<u32> {
        self.nodes
            .iter()
            .find(|n| n.cpus.contains(&cpu_id))
            .map(|n| n.id)
    }

    /// Get node by ID.
    pub fn node(&self, id: u32) -> Option<&NumaNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) -> Result<(), IronError> {
        let node_base = std::path::Path::new("/sys/devices/system/node");
        if !node_base.exists() {
            // A kernel built without NUMA exposes no node directory, and that
            // is a reading: one node, all of it local.
            self.single_node_fallback();
            return Ok(());
        }

        let mut node_ids: Vec<u32> = Vec::new();

        if let Ok(entries) = std::fs::read_dir(node_base) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(id_str) = name.strip_prefix("node") {
                    if let Ok(id) = id_str.parse::<u32>() {
                        node_ids.push(id);
                    }
                }
            }
        }

        node_ids.sort();

        for id in &node_ids {
            let node_path = node_base.join(format!("node{}", id));

            // Parse cpulist
            let cpus = std::fs::read_to_string(node_path.join("cpulist"))
                .map(|s| Self::parse_cpu_list(s.trim()))
                .unwrap_or_default();

            // Parse meminfo
            let (total, free, used) = std::fs::read_to_string(node_path.join("meminfo"))
                .map(|s| Self::parse_node_meminfo(&s))
                .unwrap_or((0, 0, 0));

            // Hugepages
            let hp_total =
                std::fs::read_to_string(node_path.join("hugepages/hugepages-2048kB/nr_hugepages"))
                    .ok()
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0);

            let hp_free = std::fs::read_to_string(
                node_path.join("hugepages/hugepages-2048kB/free_hugepages"),
            )
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);

            // PCI devices on this node
            let mut pci_devs = Vec::new();
            if let Ok(entries) = std::fs::read_dir("/sys/bus/pci/devices") {
                for entry in entries.flatten() {
                    let numa_node_path = entry.path().join("numa_node");
                    if let Ok(node_str) = std::fs::read_to_string(&numa_node_path) {
                        if let Ok(nid) = node_str.trim().parse::<i32>() {
                            if nid == *id as i32 {
                                pci_devs.push(entry.file_name().to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }

            self.nodes.push(NumaNode {
                id: *id,
                cpus,
                memory_total_bytes: total,
                memory_free_bytes: free,
                memory_used_bytes: used,
                hugepages_total: hp_total,
                hugepages_free: hp_free,
                pci_devices: pci_devs,
            });
        }

        // Distance matrix
        if let Some(first_node) = node_ids.first() {
            let dist_path = node_base.join(format!("node{}/distance", first_node));
            if let Ok(_text) = std::fs::read_to_string(&dist_path) {
                let size = node_ids.len();
                let mut distances = Vec::with_capacity(size * size);

                // Read distance from each node
                for nid in &node_ids {
                    let p = node_base.join(format!("node{}/distance", nid));
                    if let Ok(line) = std::fs::read_to_string(&p) {
                        for val in line.split_whitespace() {
                            if let Ok(d) = val.parse::<u32>() {
                                distances.push(d);
                            }
                        }
                    }
                }

                if distances.len() == size * size {
                    self.distance_matrix = Some(NumaDistanceMatrix { size, distances });
                }
            }
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn parse_cpu_list(s: &str) -> Vec<u32> {
        let mut result = Vec::new();
        for part in s.split(',') {
            let part = part.trim();
            if let Some((start, end)) = part.split_once('-') {
                if let (Ok(s), Ok(e)) = (start.parse::<u32>(), end.parse::<u32>()) {
                    result.extend(s..=e);
                }
            } else if let Ok(n) = part.parse::<u32>() {
                result.push(n);
            }
        }
        result
    }

    #[cfg(target_os = "linux")]
    fn parse_node_meminfo(text: &str) -> (u64, u64, u64) {
        let mut total = 0u64;
        let mut free = 0u64;
        for line in text.lines() {
            if line.contains("MemTotal:") {
                total = Self::extract_kb(line) * 1024;
            } else if line.contains("MemFree:") {
                free = Self::extract_kb(line) * 1024;
            }
        }
        (total, free, total.saturating_sub(free))
    }

    #[cfg(target_os = "linux")]
    fn extract_kb(line: &str) -> u64 {
        line.split_whitespace()
            .filter_map(|w| w.parse::<u64>().ok())
            .next()
            .unwrap_or(0)
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), IronError> {
        use windows::Win32::System::Threading::{
            GetNumaAvailableMemoryNodeEx, GetNumaHighestNodeNumber,
        };

        // The node count used to be inferred from `Win32_Processor`: one node
        // per socket. Sockets are not nodes -- a single AMD socket presents one
        // to four depending on the firmware's NPS setting -- and the kernel
        // answers the actual question directly.
        let mut highest: u32 = 0;
        unsafe { GetNumaHighestNodeNumber(&mut highest) }
            .map_err(|e| IronError::System(format!("GetNumaHighestNodeNumber failed: {e}")))?;
        let node_count = highest as u64 + 1;

        // Total physical memory, for the one case where attributing it to a
        // node is sound.
        // `MemoryStats::ram.total` is in KB; this field is bytes.
        let total_bytes = crate::platform::windows::read_memory_stats()
            .ok()
            .map(|m| m.ram.total.saturating_mul(1024));

        // Logical processors, likewise.
        let logical = std::thread::available_parallelism()
            .map(|p| p.get() as u32)
            .unwrap_or(0);

        for id in 0..=highest {
            // `GetNumaAvailableMemoryNodeEx` reports *available* bytes, which is
            // a real per-node reading on every machine. There is no matching
            // call for a node's installed total, and dividing the system total
            // by the node count -- which is what this reader used to do, under
            // a comment reading "approximation: divide total evenly" -- is an
            // invention on any machine with more than one node. On a machine
            // with exactly one node it is not an approximation at all: all the
            // memory is attached to the only node there is.
            let mut available: u64 = 0;
            let free = if unsafe { GetNumaAvailableMemoryNodeEx(id as u16, &mut available) }.is_ok()
            {
                available
            } else {
                0
            };

            let (memory_total_bytes, cpus) = if node_count == 1 {
                (total_bytes.unwrap_or(0), (0..logical).collect::<Vec<u32>>())
            } else {
                // Which processors belong to which node needs
                // `GetLogicalProcessorInformationEx(RelationNumaNode)`, which
                // this reader does not call yet.
                (0, Vec::new())
            };

            self.nodes.push(NumaNode {
                id,
                cpus,
                memory_total_bytes,
                memory_free_bytes: free,
                memory_used_bytes: memory_total_bytes.saturating_sub(free),
                hugepages_total: 0,
                hugepages_free: 0,
                pci_devices: Vec::new(),
            });
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), IronError> {
        // macOS is UMA, create a single node
        let mut total_mem = 0u64;
        let mut total_cpus = 0u32;

        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
        {
            if let Ok(mem) = String::from_utf8(output.stdout)
                .unwrap_or_default()
                .trim()
                .parse::<u64>()
            {
                total_mem = mem;
            }
        }

        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "hw.logicalcpu"])
            .output()
        {
            if let Ok(cpus) = String::from_utf8(output.stdout)
                .unwrap_or_default()
                .trim()
                .parse::<u32>()
            {
                total_cpus = cpus;
            }
        }

        self.nodes.push(NumaNode {
            id: 0,
            cpus: (0..total_cpus).collect(),
            memory_total_bytes: total_mem,
            // Neither is read. `memory_used_bytes` was `total_mem`, which says
            // every byte on the machine is in use.
            memory_free_bytes: 0,
            memory_used_bytes: 0,
            hugepages_total: 0,
            hugepages_free: 0,
            pci_devices: Vec::new(),
        });
        Ok(())
    }

    /// The one-node case, for a platform that has established there is one.
    ///
    /// This was `create_uma_fallback` and ran after *any* platform reader that
    /// produced nothing, including one that failed. It is now called only from
    /// the branch that has actually determined the machine has no NUMA
    /// partitioning, so the distance matrix of `[10]` below describes a
    /// topology someone looked at.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn single_node_fallback(&mut self) {
        // Try to get basic CPU/memory info
        self.nodes.push(NumaNode {
            id: 0,
            cpus: Vec::new(),
            memory_total_bytes: 0,
            memory_free_bytes: 0,
            memory_used_bytes: 0,
            hugepages_total: 0,
            hugepages_free: 0,
            pci_devices: Vec::new(),
        });

        self.distance_matrix = Some(NumaDistanceMatrix {
            size: 1,
            distances: vec![10],
        });
    }
}

impl Default for NumaMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            nodes: Vec::new(),
            distance_matrix: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructing the monitor either enumerates or says why it could not.
    ///
    /// This asserted `is_ok()`, which was true by construction until 6.0.0
    /// because `refresh` could not fail. Now that it can, the contract is
    /// weaker and more useful: **whatever happens, the caller can tell which
    /// happened.** A failure has to carry a reason, because a reason is the
    /// whole difference between "this machine has none" and "nobody looked".
    #[test]
    fn test_numa_monitor_creation() {
        match NumaMonitor::new() {
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
    fn test_numa_monitor_default() {
        let monitor = NumaMonitor::default();
        let summary = monitor.summary();
        assert!(summary.node_count >= 1);
    }

    #[test]
    fn test_distance_matrix_local() {
        let matrix = NumaDistanceMatrix {
            size: 2,
            distances: vec![10, 21, 21, 10],
        };
        assert_eq!(matrix.distance(0, 0), Some(10));
        assert_eq!(matrix.distance(0, 1), Some(21));
        assert!(matrix.is_numa());
    }

    #[test]
    fn test_uma_not_numa() {
        let matrix = NumaDistanceMatrix {
            size: 1,
            distances: vec![10],
        };
        assert!(!matrix.is_numa());
    }

    #[test]
    fn test_serialization() {
        let node = NumaNode {
            id: 0,
            cpus: vec![0, 1, 2, 3],
            memory_total_bytes: 16_000_000_000,
            memory_free_bytes: 8_000_000_000,
            memory_used_bytes: 8_000_000_000,
            hugepages_total: 0,
            hugepages_free: 0,
            pci_devices: vec!["0000:00:02.0".into()],
        };
        let json = serde_json::to_string(&node).unwrap();
        let _: NumaNode = serde_json::from_str(&json).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_cpu_list() {
        assert_eq!(NumaMonitor::parse_cpu_list("0-3"), vec![0, 1, 2, 3]);
        assert_eq!(NumaMonitor::parse_cpu_list("0,2,4"), vec![0, 2, 4]);
        assert_eq!(NumaMonitor::parse_cpu_list("0-1,4-5"), vec![0, 1, 4, 5]);
    }
}
