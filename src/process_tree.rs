//! Process Tree Visualization and Container/Cgroup Awareness
//!
//! Builds parent-child process trees and detects container/cgroup context.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonitor::process_tree::{ProcessTree, ProcessNode};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let tree = ProcessTree::build()?;
//!
//! // Print top-level processes with children
//! for root in tree.roots() {
//!     println!("{}", tree.print_tree(*root, 0));
//! }
//!
//! // Check if a PID is containerized
//! if let Some(cgroup) = tree.cgroup_info(1234) {
//!     println!("Container: {:?}", cgroup.container_id);
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A node in the process tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessNode {
    /// Process ID
    pub pid: u32,
    /// Parent Process ID; `None` when the parent field was not readable
    pub ppid: Option<u32>,
    /// Process name
    pub name: String,
    /// Command line (if available)
    pub cmdline: Option<String>,
    /// User running this process
    pub user: Option<String>,
    /// CPU usage percentage. `None`: no reader here computes it, because a
    /// rate needs two samples and a tree is built from one.
    pub cpu_percent: Option<f32>,
    /// Memory usage in bytes; `None` where the platform reader did not obtain it.
    pub memory_bytes: Option<u64>,
    /// Direct child PIDs
    pub children: Vec<u32>,
    /// Container cgroup information (Linux only). `Some` only when a container
    /// runtime marker is in the cgroup path. `None` means a host process, or a
    /// `/proc/<pid>/cgroup` that could not be read: the two are not told apart.
    pub cgroup: Option<CgroupInfo>,
}

/// Container and cgroup information for a process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupInfo {
    /// Full cgroup path
    pub path: String,
    /// Container runtime (Docker, Podman, containerd, LXC, etc.)
    pub runtime: Option<ContainerRuntime>,
    /// Container ID (short hash)
    pub container_id: Option<String>,
    /// Container name (if resolvable)
    pub container_name: Option<String>,
    /// Cgroup v2 memory limit in bytes. u64::MAX when memory.max is "max"
    /// (unlimited); None when it was not read.
    pub memory_limit: Option<u64>,
    /// Cgroup v2 CPU quota in microseconds per period. -1 when cpu.max is
    /// "max" (unlimited); None when it was not read.
    pub cpu_quota: Option<i64>,
    /// Cgroup v2 CPU period (microseconds)
    pub cpu_period: Option<u64>,
}

/// Detected container runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerRuntime {
    /// Docker container
    Docker,
    /// Podman container
    Podman,
    /// containerd/CRI
    Containerd,
    /// CRI-O
    Crio,
    /// LXC/LXD/Incus
    Lxc,
    /// Kubernetes pod
    Kubernetes,
    /// systemd-nspawn
    Nspawn,
    /// Unknown container runtime
    Unknown,
}

impl std::fmt::Display for ContainerRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Docker => write!(f, "Docker"),
            Self::Podman => write!(f, "Podman"),
            Self::Containerd => write!(f, "containerd"),
            Self::Crio => write!(f, "CRI-O"),
            Self::Lxc => write!(f, "LXC"),
            Self::Kubernetes => write!(f, "Kubernetes"),
            Self::Nspawn => write!(f, "systemd-nspawn"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Complete process tree with container awareness
pub struct ProcessTree {
    /// All nodes indexed by PID
    nodes: HashMap<u32, ProcessNode>,
    /// Root process PIDs (those with no parent in the tree)
    root_pids: Vec<u32>,
}

impl ProcessTree {
    /// Build the process tree from current system state
    pub fn build() -> Result<Self> {
        let mut nodes = HashMap::new();
        let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();

        #[cfg(target_os = "linux")]
        {
            Self::build_linux(&mut nodes, &mut children_map)?;
        }

        #[cfg(windows)]
        {
            Self::build_windows(&mut nodes, &mut children_map)?;
        }

        #[cfg(target_os = "macos")]
        {
            Self::build_macos(&mut nodes, &mut children_map)?;
        }

        // Wire up children (filter self-referencing to prevent cycles, e.g. Windows PID 0)
        for (ppid, kids) in &children_map {
            if let Some(parent) = nodes.get_mut(ppid) {
                parent.children = kids.iter().copied().filter(|&kid| kid != *ppid).collect();
            }
        }

        // Find root processes (ppid == 0, ppid not in tree, or ppid unknown).
        // An unknown parent is listed as a root rather than dropped; it is not
        // presented as a child of pid 0.
        let all_pids: std::collections::HashSet<u32> = nodes.keys().copied().collect();
        let mut root_pids: Vec<u32> = nodes
            .values()
            .filter(|n| match n.ppid {
                Some(ppid) => ppid == 0 || !all_pids.contains(&ppid),
                None => true,
            })
            .map(|n| n.pid)
            .collect();
        root_pids.sort();

        Ok(Self { nodes, root_pids })
    }

    #[cfg(target_os = "linux")]
    fn build_linux(
        nodes: &mut HashMap<u32, ProcessNode>,
        children_map: &mut HashMap<u32, Vec<u32>>,
    ) -> Result<()> {
        use std::fs;

        let proc_dir = std::path::Path::new("/proc");
        if !proc_dir.exists() {
            return Err(IronError::NotImplemented(
                "Process tree requires /proc filesystem".into(),
            ));
        }

        for entry in fs::read_dir(proc_dir)
            .map_err(|e| IronError::Other(format!("Failed to read /proc: {}", e)))?
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let pid: u32 = match entry.file_name().to_string_lossy().parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let pid_path = entry.path();

            // Read stat for ppid and name
            let stat = match fs::read_to_string(pid_path.join("stat")) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Parse: pid (name) state ppid ...
            let name_start = stat.find('(').unwrap_or(0);
            let name_end = stat.rfind(')').unwrap_or(stat.len());
            let name = stat[name_start + 1..name_end].to_string();
            let after_name = &stat[name_end + 2..];
            let fields: Vec<&str> = after_name.split_whitespace().collect();
            // An unparseable ppid stays None: a 0 would file the process under pid 0.
            let ppid: Option<u32> = fields.get(1).and_then(|s| s.parse().ok());

            // Read cmdline
            let cmdline = fs::read_to_string(pid_path.join("cmdline"))
                .ok()
                .map(|s| s.replace('\0', " ").trim().to_string())
                .filter(|s| !s.is_empty());

            // Read user from status
            let user = fs::read_to_string(pid_path.join("status"))
                .ok()
                .and_then(|status| {
                    for line in status.lines() {
                        if line.starts_with("Uid:") {
                            // An unreadable uid is no user, not root.
                            let uid: u32 = line
                                .split_whitespace()
                                .nth(1)
                                .and_then(|s| s.parse().ok())?;
                            // Map common UIDs
                            return Some(match uid {
                                0 => "root".to_string(),
                                _ => format!("uid:{}", uid),
                            });
                        }
                    }
                    None
                });

            // Read cgroup info
            let cgroup = Self::read_cgroup_linux(pid);

            // Read memory from statm (pages)
            let memory_bytes = fs::read_to_string(pid_path.join("statm"))
                .ok()
                .and_then(|s| {
                    s.split_whitespace()
                        .nth(1) // RSS field
                        .and_then(|rss| rss.parse::<u64>().ok())
                        .zip(crate::platform::linux::page_size())
                        .map(|(pages, page_size)| pages * page_size)
                });

            let node = ProcessNode {
                pid,
                ppid,
                name,
                cmdline,
                user,
                // Needs two samples to difference; nothing in this reader does that.
                cpu_percent: None,
                // None when statm is unreadable: not an empty process.
                memory_bytes,
                children: Vec::new(),
                cgroup,
            };

            if let Some(ppid) = ppid {
                children_map.entry(ppid).or_default().push(pid);
            }
            nodes.insert(pid, node);
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn read_cgroup_linux(pid: u32) -> Option<CgroupInfo> {
        use std::fs;

        let cgroup_path = format!("/proc/{}/cgroup", pid);
        let content = fs::read_to_string(&cgroup_path).ok()?;

        // Parse cgroup v2 (unified): "0::/path"
        // Parse cgroup v1: "id:controller:path"
        let mut path = String::new();
        for line in content.lines() {
            let parts: Vec<&str> = line.splitn(3, ':').collect();
            if parts.len() == 3 {
                if parts[0] == "0" && parts[1].is_empty() {
                    // cgroup v2
                    path = parts[2].to_string();
                    break;
                }
                if path.is_empty() {
                    path = parts[2].to_string();
                }
            }
        }

        if path.is_empty() || path == "/" {
            return None; // Host process, not containerized
        }

        // A path with no runtime marker is a host service, session or scope
        // (`/system.slice/sshd.service`, `/user.slice/...`), not a container. It
        // used to be reported as an Unknown-runtime container, which made every
        // systemd unit count as containerized.
        let (runtime, container_id) = Self::detect_container_runtime(&path);
        let runtime = runtime?;

        // Try to read cgroup limits
        let cgroup_base = format!("/sys/fs/cgroup{}", path);
        // "max" is a value read (unlimited), so it maps to u64::MAX or -1; only
        // an unreadable or unparseable file is None, which means unread.
        let memory_limit = fs::read_to_string(format!("{}/memory.max", cgroup_base))
            .ok()
            .and_then(|s| crate::cgroup_monitor::parse_limit(&s));

        // One file gives both fields; a zero or unparseable period is unread.
        let (cpu_quota, cpu_period) = fs::read_to_string(format!("{}/cpu.max", cgroup_base))
            .ok()
            .and_then(|s| crate::cgroup_monitor::parse_cpu_max(&s))
            .map_or((None, None), |(q, p)| (Some(q), Some(p)));

        Some(CgroupInfo {
            path,
            runtime: Some(runtime),
            container_id,
            container_name: None, // Would need docker/podman API
            memory_limit,
            cpu_quota,
            cpu_period,
        })
    }

    #[cfg(target_os = "linux")]
    fn detect_container_runtime(cgroup_path: &str) -> (Option<ContainerRuntime>, Option<String>) {
        // Docker: /docker/<container_id> or /system.slice/docker-<id>.scope
        if cgroup_path.contains("/docker/") || cgroup_path.contains("/docker-") {
            let id = Self::extract_container_id(cgroup_path, "docker");
            return (Some(ContainerRuntime::Docker), id);
        }

        // Podman: /libpod-<container_id> or /machine.slice/libpod-<id>.scope
        if cgroup_path.contains("/libpod-") || cgroup_path.contains("libpod_") {
            let id = Self::extract_container_id(cgroup_path, "libpod");
            return (Some(ContainerRuntime::Podman), id);
        }

        // containerd/CRI: /cri-containerd-<id>
        if cgroup_path.contains("/cri-containerd-") {
            let id = Self::extract_container_id(cgroup_path, "cri-containerd");
            return (Some(ContainerRuntime::Containerd), id);
        }

        // CRI-O: /crio-<id>.scope, usually under a kubepods slice
        if cgroup_path.contains("/crio-") {
            let id = Self::extract_container_id(cgroup_path, "crio");
            return (Some(ContainerRuntime::Crio), id);
        }

        // Kubernetes pods: /kubepods/ or /kubepods.slice/
        if cgroup_path.contains("/kubepods") {
            let id = Self::extract_container_id(cgroup_path, "kubepods");
            return (Some(ContainerRuntime::Kubernetes), id);
        }

        // LXC: /lxc/<name>
        if cgroup_path.contains("/lxc/") {
            let name = cgroup_path
                .split("/lxc/")
                .nth(1)
                .map(|s| s.split('/').next().unwrap_or(s).to_string());
            return (Some(ContainerRuntime::Lxc), name);
        }

        // systemd-nspawn: /machine-<name>.scope
        if cgroup_path.contains("/machine-") && cgroup_path.contains(".scope") {
            let name = cgroup_path
                .split("/machine-")
                .nth(1)
                .and_then(|s| s.strip_suffix(".scope"))
                .map(|s| s.to_string());
            return (Some(ContainerRuntime::Nspawn), name);
        }

        // No runtime marker: not a container. The caller drops the cgroup record.
        (None, None)
    }

    #[cfg(target_os = "linux")]
    fn extract_container_id(path: &str, prefix: &str) -> Option<String> {
        // Try to find 64-char hex container ID
        for segment in path.split('/') {
            let stripped = segment
                .strip_prefix(&format!("{}-", prefix))
                .or_else(|| segment.strip_suffix(".scope"))
                .unwrap_or(segment);
            // Container IDs are 64 hex chars, show first 12
            if stripped.len() >= 64 && stripped[..64].chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(stripped[..12].to_string());
            }
            // Short IDs (12 chars)
            if stripped.len() >= 12 && stripped[..12].chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(stripped[..12].to_string());
            }
        }
        None
    }

    #[cfg(windows)]
    fn build_windows(
        nodes: &mut HashMap<u32, ProcessNode>,
        children_map: &mut HashMap<u32, Vec<u32>>,
    ) -> Result<()> {
        use std::mem;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        };

        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
                .map_err(|e| IronError::Other(format!("Failed to create snapshot: {}", e)))?;

            let mut entry: PROCESSENTRY32W = mem::zeroed();
            entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;

            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let pid = entry.th32ProcessID;
                    let ppid = entry.th32ParentProcessID;
                    let name_len = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name = String::from_utf16_lossy(&entry.szExeFile[..name_len]);

                    let node = ProcessNode {
                        pid,
                        ppid: Some(ppid),
                        name,
                        cmdline: None,
                        user: None,
                        // The toolhelp snapshot carries neither, and this reader does not query them.
                        cpu_percent: None,
                        memory_bytes: None,
                        children: Vec::new(),
                        cgroup: None, // No cgroups on Windows
                    };

                    children_map.entry(ppid).or_default().push(pid);
                    nodes.insert(pid, node);

                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }

            let _ = CloseHandle(snapshot);
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn build_macos(
        nodes: &mut HashMap<u32, ProcessNode>,
        children_map: &mut HashMap<u32, Vec<u32>>,
    ) -> Result<()> {
        // Use ps command for macOS process enumeration
        let output = std::process::Command::new("ps")
            .args(["-axo", "pid,ppid,rss,comm"])
            .output()
            .map_err(|e| IronError::Other(format!("Failed to run ps: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 4 {
                let pid: u32 = match fields[0].parse() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                // An unparseable ppid stays None rather than becoming pid 0.
                let ppid: Option<u32> = fields[1].parse().ok();
                // None when RSS is unreadable: not an empty process.
                let memory_bytes: Option<u64> = fields[2].parse::<u64>().ok().map(|kb| kb * 1024);
                let name = fields[3..].join(" ");

                let node = ProcessNode {
                    pid,
                    ppid,
                    name,
                    cmdline: None,
                    user: None,
                    // ps is asked for no CPU column; nothing here computes one.
                    cpu_percent: None,
                    memory_bytes,
                    children: Vec::new(),
                    cgroup: None,
                };

                if let Some(ppid) = ppid {
                    children_map.entry(ppid).or_default().push(pid);
                }
                nodes.insert(pid, node);
            }
        }

        Ok(())
    }

    /// Get all root process PIDs (init, systemd, etc.)
    pub fn roots(&self) -> &[u32] {
        &self.root_pids
    }

    /// Get a process node by PID
    pub fn get(&self, pid: u32) -> Option<&ProcessNode> {
        self.nodes.get(&pid)
    }

    /// Get all process nodes
    pub fn all_processes(&self) -> impl Iterator<Item = &ProcessNode> {
        self.nodes.values()
    }

    /// Total number of processes
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get cgroup info for a process
    pub fn cgroup_info(&self, pid: u32) -> Option<&CgroupInfo> {
        self.nodes.get(&pid).and_then(|n| n.cgroup.as_ref())
    }

    /// Get all containerized processes
    pub fn containerized_processes(&self) -> Vec<&ProcessNode> {
        self.nodes.values().filter(|n| n.cgroup.is_some()).collect()
    }

    /// Get processes grouped by container
    pub fn by_container(&self) -> HashMap<String, Vec<&ProcessNode>> {
        let mut groups: HashMap<String, Vec<&ProcessNode>> = HashMap::new();
        for node in self.nodes.values() {
            if let Some(ref cgroup) = node.cgroup {
                let key = cgroup
                    .container_id
                    .clone()
                    .unwrap_or_else(|| cgroup.path.clone());
                groups.entry(key).or_default().push(node);
            }
        }
        groups
    }

    /// Get all descendants of a process (iterative, cycle-safe)
    pub fn descendants(&self, pid: u32) -> Vec<u32> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![pid];
        visited.insert(pid);
        while let Some(current) = stack.pop() {
            if let Some(node) = self.nodes.get(&current) {
                for &child in &node.children {
                    if visited.insert(child) {
                        result.push(child);
                        stack.push(child);
                    }
                }
            }
        }
        result
    }

    /// Get ancestor chain from a process up to root
    pub fn ancestors(&self, pid: u32) -> Vec<u32> {
        let mut result = Vec::new();
        let mut current = pid;
        while let Some(node) = self.nodes.get(&current) {
            // An unknown parent ends the chain: there is no ancestor to name.
            let Some(ppid) = node.ppid else {
                break;
            };
            if ppid == 0 || ppid == current {
                break;
            }
            result.push(ppid);
            current = ppid;
        }
        result
    }

    /// Print the process tree from a given root
    pub fn print_tree(&self, pid: u32, depth: usize) -> String {
        let mut output = String::new();
        let mut visited = std::collections::HashSet::new();
        self.print_tree_inner(pid, depth, &mut output, &mut visited);
        output
    }

    fn print_tree_inner(
        &self,
        pid: u32,
        depth: usize,
        output: &mut String,
        visited: &mut std::collections::HashSet<u32>,
    ) {
        if !visited.insert(pid) || depth > 50 {
            return;
        }
        if let Some(node) = self.nodes.get(&pid) {
            let indent = if depth == 0 {
                String::new()
            } else {
                format!("{}└─ ", "  ".repeat(depth - 1))
            };
            let container_tag = node
                .cgroup
                .as_ref()
                .and_then(|c| c.runtime.as_ref())
                .map(|r| format!(" [{}]", r))
                .unwrap_or_default();
            output.push_str(&format!(
                "{}[{}] {}{}\n",
                indent, node.pid, node.name, container_tag
            ));
            for &child in &node.children {
                self.print_tree_inner(child, depth + 1, output, visited);
            }
        }
    }

    /// Summary statistics
    pub fn summary(&self) -> ProcessTreeSummary {
        let total = self.nodes.len();
        let containerized = self.nodes.values().filter(|n| n.cgroup.is_some()).count();

        let mut containers: HashMap<String, usize> = HashMap::new();
        for node in self.nodes.values() {
            if let Some(ref cgroup) = node.cgroup {
                if let Some(ref id) = cgroup.container_id {
                    *containers.entry(id.clone()).or_default() += 1;
                }
            }
        }

        let max_depth = self
            .root_pids
            .iter()
            .map(|&pid| self.tree_depth(pid))
            .max()
            .unwrap_or(0);

        ProcessTreeSummary {
            total_processes: total,
            containerized_processes: containerized,
            container_count: containers.len(),
            max_tree_depth: max_depth,
        }
    }

    fn tree_depth(&self, pid: u32) -> usize {
        use std::collections::{HashSet, VecDeque};
        let mut max_depth = 0usize;
        let mut visited = HashSet::new();
        let mut queue: VecDeque<(u32, usize)> = VecDeque::new();
        queue.push_back((pid, 1));
        visited.insert(pid);
        while let Some((current, depth)) = queue.pop_front() {
            max_depth = max_depth.max(depth);
            if let Some(node) = self.nodes.get(&current) {
                for &child in &node.children {
                    if visited.insert(child) {
                        queue.push_back((child, depth + 1));
                    }
                }
            }
        }
        max_depth
    }
}

/// Summary of process tree statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessTreeSummary {
    /// Total number of processes
    pub total_processes: usize,
    /// Number of processes in containers
    pub containerized_processes: usize,
    /// Number of unique containers
    pub container_count: usize,
    /// Maximum tree depth
    pub max_tree_depth: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_process_tree() {
        match ProcessTree::build() {
            Ok(tree) => {
                assert!(!tree.is_empty(), "Process tree should not be empty");
                let summary = tree.summary();
                println!(
                    "Process tree: {} procs, {} containerized, depth {}",
                    summary.total_processes,
                    summary.containerized_processes,
                    summary.max_tree_depth
                );
            }
            Err(e) => println!("Process tree not available: {}", e),
        }
    }

    #[test]
    fn test_tree_summary() {
        if let Ok(tree) = ProcessTree::build() {
            let summary = tree.summary();
            assert!(summary.total_processes > 0);
            assert!(summary.max_tree_depth > 0);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_detect_docker_cgroup() {
        let (runtime, id) = ProcessTree::detect_container_runtime(
            "/system.slice/docker-abc123def456abc123def456abc123def456abc123def456abc123def456abcd.scope",
        );
        assert_eq!(runtime, Some(ContainerRuntime::Docker));
        assert!(id.is_some());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_detect_host_cgroup() {
        let (runtime, _) = ProcessTree::detect_container_runtime("/");
        assert_eq!(runtime, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_host_services_are_not_containers() {
        // Captured systemd cgroup paths: none carries a runtime marker.
        for path in [
            "/system.slice/sshd.service",
            "/init.scope",
            "/user.slice/user-1000.slice/session-2.scope",
            "/system.slice/docker.service",
        ] {
            let (runtime, id) = ProcessTree::detect_container_runtime(path);
            assert_eq!(runtime, None, "{path} is a host unit, not a container");
            assert_eq!(id, None, "{path}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_detect_runtime_markers() {
        let crio = "/kubepods.slice/kubepods-pod1.slice/crio-abc123def456abc123def456abc123def456abc123def456abc123def456abcd.scope";
        let (runtime, id) = ProcessTree::detect_container_runtime(crio);
        assert_eq!(runtime, Some(ContainerRuntime::Crio));
        assert!(id.is_some());

        let (runtime, _) = ProcessTree::detect_container_runtime("/lxc/web01/init.scope");
        assert_eq!(runtime, Some(ContainerRuntime::Lxc));
    }
}
