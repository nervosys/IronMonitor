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
//! use ironmonitor::numa::NumaMonitor;
//!
//! let monitor = NumaMonitor::new().unwrap();
//! for node in monitor.nodes() {
//!     // `cpus` and `memory_total_bytes` are `Option`: a value that was not
//!     // read is `None`, never an empty list or a size of zero.
//!     let cpus = node
//!         .cpus
//!         .as_ref()
//!         .map_or_else(|| "unknown".to_string(), |c| c.len().to_string());
//!     match node.memory_total_bytes {
//!         Some(bytes) => println!("Node {}: {} CPUs, {:.1} GB total memory",
//!             node.id, cpus, bytes as f64 / 1e9),
//!         None => println!("Node {}: {} CPUs, memory size not readable",
//!             node.id, cpus),
//!     }
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
    /// CPUs (logical processor IDs) belonging to this node, or `None` where
    /// they were not read.
    ///
    /// This was a `Vec` that was empty both when the node had no processors
    /// and when the reader never asked which processors it had. Windows
    /// multi-node machines and the Linux single-node fallback both produced
    /// `[]`, and `memory.numa.N.cpus` published that as a measured `0`.
    /// `Some(vec![])` is the reading "this node has no processors", which a
    /// memory-only node on Linux genuinely has.
    pub cpus: Option<Vec<u32>>,
    /// Total memory in bytes, or `None` where the node's `meminfo` was not
    /// read.
    ///
    /// **These five are what the `max_distance` correction left behind.** That
    /// field's doc, a few lines down in [`NumaSummary`], explains at length why
    /// a fallback inside the valid range is undetectable -- and these sat bare
    /// beside it. The single-node fallback carried the comment *"Neither is
    /// read"* directly above `memory_free_bytes: 0, memory_used_bytes: 0`,
    /// which is the diagnosis written down with nowhere to put it.
    pub memory_total_bytes: Option<u64>,
    /// Free memory in bytes, or `None` where not read.
    pub memory_free_bytes: Option<u64>,
    /// Used memory in bytes, or `None` where not read.
    pub memory_used_bytes: Option<u64>,
    /// Number of huge pages (Linux), or `None` where not read.
    ///
    /// Absent rather than `0` on Windows, where hugepages are not the same
    /// concept: "this platform does not report it" and "this node has none
    /// configured" are different answers.
    pub hugepages_total: Option<u64>,
    /// Free huge pages (Linux), or `None` where not read.
    pub hugepages_free: Option<u64>,
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
    /// Total CPUs across all nodes, or `None` if any node's processor list was
    /// not read. A sum over the readable nodes would understate the machine.
    pub total_cpus: Option<usize>,
    /// Total memory across all nodes (bytes), or `None` if any node's total
    /// was unread.
    ///
    /// A sum that silently omits a node understates the machine while still
    /// looking like a measurement.
    pub total_memory_bytes: Option<u64>,
    /// Whether the system is truly NUMA (vs UMA)
    pub is_numa: bool,
    /// Maximum inter-node distance, or `None` when the SLIT was not readable.
    ///
    /// This was a bare `u32` falling back to `10`, which is ACPI SLIT's code for
    /// *local* — so a machine whose distance matrix could not be read asserted
    /// uniform memory access, the one conclusion the absence cannot support. The
    /// fallback was inside the valid range, so no consumer's bounds check could
    /// tell it from a reading.
    pub max_distance: Option<u32>,
    /// Memory imbalance ratio (max_node_mem / min_node_mem), or `None` where
    /// fewer than two nodes reported a size.
    ///
    /// **`1.0` was the fallback, and `1.0` means perfectly balanced** -- the
    /// same defect as `max_distance`'s `10`, a fallback sitting inside the
    /// valid range where no bounds check can catch it. A machine whose node
    /// sizes could not be read asserted even memory distribution, which is a
    /// conclusion the absence cannot support.
    pub memory_imbalance_ratio: Option<f64>,
    /// CPU imbalance ratio, or `None` where fewer than two nodes reported CPUs
    /// or any node's processor list was not read.
    pub cpu_imbalance_ratio: Option<f64>,
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
        // Same rule as memory: one unread node makes the machine total unknown.
        let total_cpus: Option<usize> = self
            .nodes
            .iter()
            .map(|n| n.cpus.as_ref().map(Vec::len))
            .sum();
        // `Option` sum: one node with no reading makes the machine total
        // unknown rather than quietly short by that node.
        let total_memory: Option<u64> = self.nodes.iter().map(|n| n.memory_total_bytes).sum();

        let is_numa =
            self.nodes.len() > 1 || self.distance_matrix.as_ref().is_some_and(|d| d.is_numa());

        let max_distance = self
            .distance_matrix
            .as_ref()
            .and_then(|d| d.distances.iter().copied().max());

        // `filter_map` over the reading rather than `filter(|&m| m > 0)`: the
        // `> 0` test was skipping unread nodes and genuinely empty ones alike,
        // the same sentinel conflation this module's `max_distance` doc warns
        // about.
        let mem_vals: Vec<u64> = self
            .nodes
            .iter()
            .filter_map(|n| n.memory_total_bytes)
            .collect();
        let memory_imbalance = match (mem_vals.iter().max(), mem_vals.iter().min()) {
            (Some(&max), Some(&min)) if mem_vals.len() >= 2 && min > 0 => {
                Some(max as f64 / min as f64)
            }
            // Fewer than two nodes reported a size, so there is no ratio
            // between them. This returned `1.0`, which asserts balance.
            _ => None,
        };

        // Any unread processor list makes the ratio unknown: the nodes that
        // were read would otherwise stand in for the ones that were not.
        let cpu_counts: Option<Vec<usize>> = self
            .nodes
            .iter()
            .map(|n| n.cpus.as_ref().map(Vec::len))
            .collect();
        let cpu_imbalance = cpu_counts.and_then(|counts| {
            let vals: Vec<usize> = counts.into_iter().filter(|&c| c > 0).collect();
            match (vals.iter().max(), vals.iter().min()) {
                (Some(&max), Some(&min)) if vals.len() >= 2 && min > 0 => {
                    Some(max as f64 / min as f64)
                }
                _ => None,
            }
        });

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
            .find(|n| n.cpus.as_ref().is_some_and(|c| c.contains(&cpu_id)))
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
            // is a reading: one node, all of it local. Its processors are the
            // online ones.
            self.single_node_fallback(Self::online_cpus());
            return Ok(());
        }

        // The directory exists, so it must be listable. A directory that exists
        // but cannot be read used to produce a monitor with zero nodes, which
        // published `memory.numa.nodes = 0` and `is_numa = false` as readings.
        let entries = std::fs::read_dir(node_base)
            .map_err(|e| IronError::System(format!("cannot list {}: {e}", node_base.display())))?;

        let mut node_ids: Vec<u32> = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(id_str) = name.strip_prefix("node") {
                if let Ok(id) = id_str.parse::<u32>() {
                    node_ids.push(id);
                }
            }
        }

        if node_ids.is_empty() {
            // The kernel always exposes node0 when this directory exists, so an
            // empty listing is a failed read, not a machine with no nodes.
            return Err(IronError::System(format!(
                "{} lists no nodeN entries",
                node_base.display()
            )));
        }

        node_ids.sort();

        for id in &node_ids {
            let node_path = node_base.join(format!("node{}", id));

            // An unreadable cpulist is `None`, not an empty processor list. An
            // empty file is a real `Some(vec![])` for a memory-only node.
            let cpus = std::fs::read_to_string(node_path.join("cpulist"))
                .ok()
                .map(|s| Self::parse_cpu_list(s.trim()));

            // Parse meminfo. `unwrap_or((0, 0, 0))` made an unreadable node
            // look like a node with no memory at all.
            let (total, free, used) = std::fs::read_to_string(node_path.join("meminfo"))
                .map(|s| Self::parse_node_meminfo(&s))
                .unwrap_or((None, None, None));

            // Hugepages
            // A kernel built without hugepage support exposes no such file.
            // That is not a node with zero hugepages configured.
            let hp_total =
                std::fs::read_to_string(node_path.join("hugepages/hugepages-2048kB/nr_hugepages"))
                    .ok()
                    .and_then(|s| s.trim().parse().ok());

            let hp_free = std::fs::read_to_string(
                node_path.join("hugepages/hugepages-2048kB/free_hugepages"),
            )
            .ok()
            .and_then(|s| s.trim().parse().ok());

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

    /// The online processors, from `/sys/devices/system/cpu/online`, or `None`
    /// where that file cannot be read.
    #[cfg(target_os = "linux")]
    fn online_cpus() -> Option<Vec<u32>> {
        std::fs::read_to_string("/sys/devices/system/cpu/online")
            .ok()
            .map(|s| Self::parse_cpu_list(s.trim()))
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
    /// `(total, free, used)`, each absent if its line was missing or
    /// unparseable.
    ///
    /// `used` is derived, so it needs both: a total with no free figure gives
    /// no usage. It was `total.saturating_sub(free)` over two zeros, which
    /// reported a node using none of the nothing it had.
    fn parse_node_meminfo(text: &str) -> (Option<u64>, Option<u64>, Option<u64>) {
        let mut total = None;
        let mut free = None;
        for line in text.lines() {
            if line.contains("MemTotal:") {
                total = Self::extract_kb(line).map(|kb| kb * 1024);
            } else if line.contains("MemFree:") {
                free = Self::extract_kb(line).map(|kb| kb * 1024);
            }
        }
        let used = match (total, free) {
            (Some(t), Some(f)) => Some(t.saturating_sub(f)),
            _ => None,
        };
        (total, free, used)
    }

    /// The kB figure after the field's colon.
    ///
    /// A node's `meminfo` lines begin with the node id -- `Node 0 MemTotal:
    /// 48111256 kB` -- and this took the *first* number on the line, which is
    /// that id. Node 0 reported 0 bytes of memory and node 1 reported 1 kB, on
    /// every Linux machine; found by checking every zero in a live snapshot.
    #[cfg(target_os = "linux")]
    fn extract_kb(line: &str) -> Option<u64> {
        let (_, value) = line.split_once(':')?;
        value.split_whitespace().next()?.parse().ok()
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

        // Logical processors, likewise. `None` when the count was not read,
        // rather than the `0` this used to fall back to, which read as a node
        // with no processors.
        let logical = std::thread::available_parallelism()
            .ok()
            .map(|p| p.get() as u32);

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
            // A failed call is not a node with no memory available.
            let mut free = None;
            if unsafe { GetNumaAvailableMemoryNodeEx(id as u16, &mut available) }.is_ok() {
                free = Some(available);
            }

            let (memory_total_bytes, cpus) = if node_count == 1 {
                (total_bytes, logical.map(|n| (0..n).collect::<Vec<u32>>()))
            } else {
                // Which processors belong to which node needs
                // `GetLogicalProcessorInformationEx(RelationNumaNode)`, which
                // this reader does not call yet. Until it does, a multi-node
                // machine's processors are not known; `[]` used to stand here,
                // reading as a node with none.
                (None, None)
            };

            self.nodes.push(NumaNode {
                id,
                cpus,
                memory_total_bytes,
                memory_free_bytes: free,
                // Derived, so it needs both sides.
                memory_used_bytes: match (memory_total_bytes, free) {
                    (Some(total), Some(free)) => Some(total.saturating_sub(free)),
                    _ => None,
                },
                // Windows does not report per-node hugepages through this API.
                hugepages_total: None,
                hugepages_free: None,
                pci_devices: Vec::new(),
            });
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), IronError> {
        // macOS is UMA, create a single node.
        //
        // `total_mem` was `0u64`, so a `sysctl hw.memsize` that failed produced
        // a machine with no memory. It is `None` until sysctl answers -- the
        // same correction this commit's sibling made to the Linux and Windows
        // readers, in the one path neither of those compiles.
        let mut total_mem = None;
        let mut total_cpus: Option<u32> = None;

        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
        {
            if let Ok(mem) = String::from_utf8(output.stdout)
                .unwrap_or_default()
                .trim()
                .parse::<u64>()
            {
                total_mem = Some(mem);
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
                total_cpus = Some(cpus);
            }
        }

        self.nodes.push(NumaNode {
            id: 0,
            // `None` until sysctl answers; it was `0..0`, a node with no CPUs.
            cpus: total_cpus.map(|n| (0..n).collect()),
            memory_total_bytes: total_mem,
            // Neither is read, and both now say so. `memory_used_bytes` was
            // `total_mem` -- every byte on the machine in use -- then `0`,
            // which is the opposite claim and equally unobserved.
            memory_free_bytes: None,
            memory_used_bytes: None,
            hugepages_total: None,
            hugepages_free: None,
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
    fn single_node_fallback(&mut self, cpus: Option<Vec<u32>>) {
        self.nodes.push(NumaNode {
            id: 0,
            // The processors the kernel reports online, or `None` when that
            // file was not read. It used to be `Vec::new()`, which read as a
            // node with no processors.
            cpus,
            // Nothing was read here; `0` claimed a node with no memory.
            memory_total_bytes: None,
            memory_free_bytes: None,
            memory_used_bytes: None,
            hugepages_total: None,
            hugepages_free: None,
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
            cpus: Some(vec![0, 1, 2, 3]),
            memory_total_bytes: Some(16_000_000_000),
            memory_free_bytes: Some(8_000_000_000),
            memory_used_bytes: Some(8_000_000_000),
            hugepages_total: Some(0),
            hugepages_free: Some(0),
            pci_devices: vec!["0000:00:02.0".into()],
        };
        let json = serde_json::to_string(&node).unwrap();
        let _: NumaNode = serde_json::from_str(&json).unwrap();
    }

    /// The real format, from WSL2's `/sys/devices/system/node/node0/meminfo`.
    #[cfg(target_os = "linux")]
    #[test]
    fn node_meminfo_reads_the_figure_not_the_node_id() {
        let text = concat!(
            "Node 0 MemTotal:       48111256 kB\n",
            "Node 0 MemFree:          410056 kB\n",
            "Node 0 MemUsed:        47701200 kB\n",
        );
        let (total, free, used) = NumaMonitor::parse_node_meminfo(text);
        assert_eq!(total, Some(48_111_256 * 1024));
        assert_eq!(free, Some(410_056 * 1024));
        assert_eq!(used, Some((48_111_256 - 410_056) * 1024));

        let node1 = "Node 1 MemTotal:       16000000 kB\n";
        assert_eq!(
            NumaMonitor::parse_node_meminfo(node1).0,
            Some(16_000_000 * 1024)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_cpu_list() {
        assert_eq!(NumaMonitor::parse_cpu_list("0-3"), vec![0, 1, 2, 3]);
        assert_eq!(NumaMonitor::parse_cpu_list("0,2,4"), vec![0, 2, 4]);
        assert_eq!(NumaMonitor::parse_cpu_list("0-1,4-5"), vec![0, 1, 4, 5]);
    }
}

#[cfg(test)]
mod absent_slit_tests {
    use super::*;

    fn node(id: u32) -> NumaNode {
        NumaNode {
            id,
            cpus: None,
            // Nothing was read here; `0` claimed a node with no memory.
            memory_total_bytes: None,
            memory_free_bytes: None,
            memory_used_bytes: None,
            hugepages_total: None,
            hugepages_free: None,
            pci_devices: vec![],
        }
    }

    fn node_with_cpus(id: u32, cpus: Vec<u32>) -> NumaNode {
        NumaNode {
            cpus: Some(cpus),
            ..node(id)
        }
    }

    #[test]
    fn an_unread_processor_list_is_not_an_empty_one() {
        // One node's processors were read and one was not. The total and the
        // ratio must both be unknown, not the sum or ratio of the readable one.
        let monitor = NumaMonitor {
            nodes: vec![node_with_cpus(0, vec![0, 1, 2, 3]), node(1)],
            distance_matrix: None,
        };
        let summary = monitor.summary();
        assert_eq!(summary.total_cpus, None);
        assert_eq!(summary.cpu_imbalance_ratio, None);
        assert_eq!(monitor.cpu_to_node(2), Some(0));
    }

    #[test]
    fn read_processor_lists_total_and_balance() {
        let monitor = NumaMonitor {
            nodes: vec![
                node_with_cpus(0, vec![0, 1]),
                node_with_cpus(1, vec![2, 3, 4, 5]),
            ],
            distance_matrix: None,
        };
        let summary = monitor.summary();
        assert_eq!(summary.total_cpus, Some(6));
        assert_eq!(summary.cpu_imbalance_ratio, Some(2.0));
    }

    #[test]
    fn a_memory_only_node_with_no_processors_is_a_reading() {
        // `Some(vec![])` is what a CPU-less node reports; it is not `None`.
        let monitor = NumaMonitor {
            nodes: vec![node_with_cpus(0, vec![0, 1]), node_with_cpus(1, vec![])],
            distance_matrix: None,
        };
        assert_eq!(monitor.summary().total_cpus, Some(2));
    }

    #[test]
    fn an_unreadable_distance_matrix_does_not_assert_local_memory() {
        // No matrix at all: the summary must decline rather than report 10,
        // which is ACPI SLIT's own code for "local" and would describe an
        // unread machine as uniform-memory.
        let monitor = NumaMonitor {
            nodes: vec![node(0), node(1)],
            distance_matrix: None,
        };
        assert_eq!(monitor.summary().max_distance, None);
    }

    #[test]
    fn an_empty_distance_matrix_does_not_assert_local_memory() {
        let monitor = NumaMonitor {
            nodes: vec![node(0)],
            distance_matrix: Some(NumaDistanceMatrix {
                size: 0,
                distances: vec![],
            }),
        };
        assert_eq!(monitor.summary().max_distance, None);
    }

    #[test]
    fn a_read_matrix_reports_its_maximum() {
        let monitor = NumaMonitor {
            nodes: vec![node(0), node(1)],
            distance_matrix: Some(NumaDistanceMatrix {
                size: 2,
                distances: vec![10, 21, 21, 10],
            }),
        };
        assert_eq!(monitor.summary().max_distance, Some(21));
    }
}
