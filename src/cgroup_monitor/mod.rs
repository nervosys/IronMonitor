//! Cgroup v1/v2 resource monitoring.
//!
//! Monitors Linux cgroup resource limits and usage for containers and
//! system slices: CPU quotas, memory limits, I/O bandwidth, PIDs, and
//! device access. Provides container-aware resource tracking.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/fs/cgroup/` (cgroup v2 unified), `/sys/fs/cgroup/*/` (v1 hierarchical)
//! - **Windows/macOS**: Job objects / no cgroup equivalent (stubs)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::IronError;

/// Cgroup version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CgroupVersion {
    V1,
    V2,
    Unknown,
}

impl std::fmt::Display for CgroupVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1 => write!(f, "v1"),
            Self::V2 => write!(f, "v2"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// CPU resource limits and usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupCpu {
    /// CPU quota in microseconds per period (-1 = unlimited).
    pub quota_us: i64,
    /// CPU period in microseconds (typically 100000).
    pub period_us: u64,
    /// Number of CPU shares (v1 cpu.shares). None on v2, which reports
    /// `weight` instead; no v1 reader exists here, so it is never read.
    pub shares: Option<u64>,
    /// CPU weight (1-10000, v2 replacement for shares).
    pub weight: u64,
    /// Total CPU usage in microseconds.
    pub usage_us: u64,
    /// Per-CPU usage in microseconds. None on v2: cpu.stat has no per-CPU
    /// breakdown, and no v1 reader exists here, so it is never read.
    pub per_cpu_usage_us: Option<Vec<u64>>,
    /// Number of throttled periods.
    pub throttled_periods: u64,
    /// Total throttled time in microseconds.
    pub throttled_time_us: u64,
    /// Effective number of CPUs (quota / period). None when unlimited: an
    /// unlimited quota caps nothing, so there is no count to report.
    pub effective_cpus: Option<f64>,
}

/// Memory resource limits and usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupMemory {
    /// Memory usage in bytes.
    pub usage_bytes: u64,
    /// Memory limit in bytes (u64::MAX = unlimited).
    pub limit_bytes: u64,
    /// Swap usage in bytes (None without swap accounting).
    pub swap_usage_bytes: Option<u64>,
    /// Swap limit in bytes (u64::MAX = unlimited; None without swap accounting).
    pub swap_limit_bytes: Option<u64>,
    /// Cache memory in bytes.
    pub cache_bytes: u64,
    /// RSS (resident set size) in bytes.
    pub rss_bytes: u64,
    /// Number of times memory limit was hit (None without an oom_kill key).
    pub oom_events: Option<u64>,
    /// Memory usage as percentage of limit (None when there is no limit).
    pub usage_pct: Option<f64>,
}

/// I/O resource limits and usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupIo {
    /// Read bytes total.
    pub read_bytes: u64,
    /// Write bytes total.
    pub write_bytes: u64,
    /// Read I/O operations.
    pub read_ios: u64,
    /// Write I/O operations.
    pub write_ios: u64,
    /// I/O weight (1-10000).
    pub weight: u64,
    /// Per-device I/O limits (device -> max bytes/sec).
    pub device_limits: HashMap<String, u64>,
}

/// PID limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupPids {
    /// Current number of PIDs.
    pub current: u64,
    /// Maximum PIDs allowed.
    pub limit: u64,
}

/// A cgroup and its resource usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupInfo {
    /// Cgroup path (e.g. "/system.slice/docker-abc123.scope").
    pub path: String,
    /// Friendly name (container name if detectable).
    pub name: String,
    /// Whether this is a container cgroup.
    pub is_container: bool,
    /// Container runtime (docker, podman, containerd, lxc).
    pub container_runtime: Option<String>,
    /// CPU resource info.
    pub cpu: Option<CgroupCpu>,
    /// Memory resource info.
    pub memory: Option<CgroupMemory>,
    /// I/O resource info.
    pub io: Option<CgroupIo>,
    /// PID limits.
    pub pids: Option<CgroupPids>,
    /// Controllers enabled for this cgroup. `None` when `cgroup.controllers`
    /// was not readable; an empty list means it was read and named none.
    pub controllers: Option<Vec<String>>,
}

/// System-wide cgroup overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupOverview {
    /// Cgroup version in use.
    pub version: CgroupVersion,
    /// Available controllers. `None` when the hierarchy listing was not
    /// readable, which is different from a hierarchy with none.
    pub available_controllers: Option<Vec<String>>,
    /// All monitored cgroups.
    pub cgroups: Vec<CgroupInfo>,
    /// Total number of containers detected.
    pub container_count: usize,
    /// Total memory limit, in bytes, over the cgroups that have one. Unlimited
    /// cgroups are left out, so this covers only capped cgroups; `None` when no
    /// cgroup is capped, or when the sum overflows.
    pub total_memory_limit_bytes: Option<u64>,
    /// Total effective CPUs over the cgroups with a CPU quota. Uncapped cgroups
    /// are left out, so this covers only capped cgroups; `None` when no cgroup
    /// is capped.
    pub total_effective_cpus: Option<f64>,
}

/// The three counters `CgroupCpu` carries, from a cgroup v2 `cpu.stat`. Each
/// must be present and parse. The struct has no `None` for them, so a missing
/// `nr_throttled` makes the whole CPU reading absent rather than a zero.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_cpu_stat(stat: &str) -> Option<(u64, u64, u64)> {
    let mut usage_us: Option<u64> = None;
    let mut throttled_periods: Option<u64> = None;
    let mut throttled_time: Option<u64> = None;

    for line in stat.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            match parts[0] {
                "usage_usec" => usage_us = parts[1].parse().ok(),
                "nr_throttled" => throttled_periods = parts[1].parse().ok(),
                "throttled_usec" => throttled_time = parts[1].parse().ok(),
                _ => {}
            }
        }
    }

    Some((usage_us?, throttled_periods?, throttled_time?))
}

/// A cgroup v2 `cpu.max` line, `$MAX $PERIOD`, as `(quota, period)`. A literal
/// `max` quota is unlimited, which `CgroupCpu` stores as -1. Anything else that
/// does not parse, or a missing or zero period, leaves the reading absent.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn parse_cpu_max(max: &str) -> Option<(i64, u64)> {
    let mut parts = max.split_whitespace();
    let quota = match parts.next()? {
        "max" => -1,
        q => q.parse::<i64>().ok()?,
    };
    // A zero period has no quota fraction, and the kernel never writes one.
    let period = parts.next()?.parse::<u64>().ok().filter(|&p| p > 0)?;
    Some((quota, period))
}

/// A cgroup v2 limit file (`memory.max`, `memory.swap.max`, `pids.max`). A
/// literal `max` is unlimited, which is `u64::MAX`. Other unparseable text is
/// absent, not unlimited.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn parse_limit(s: &str) -> Option<u64> {
    match s.trim() {
        "max" => Some(u64::MAX),
        v => v.parse().ok(),
    }
}

/// The `file` (cache) and `anon` (rss) byte counters from `memory.stat`. Both
/// must be present and parse.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_memory_stat(stat: &str) -> Option<(u64, u64)> {
    let mut cache: Option<u64> = None;
    let mut rss: Option<u64> = None;

    for line in stat.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            match parts[0] {
                "file" => cache = parts[1].parse().ok(),
                "anon" => rss = parts[1].parse().ok(),
                _ => {}
            }
        }
    }

    Some((cache?, rss?))
}

/// The `oom_kill` count from `memory.events`. Absent if the key is missing.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_oom_kill(events: &str) -> Option<u64> {
    events
        .lines()
        .find_map(|line| line.strip_prefix("oom_kill "))
        .and_then(|v| v.trim().parse().ok())
}

/// Sums figures that were read. No figure contributing gives `None`, not zero:
/// a sum over nothing is not a total. Overflow also gives `None`, since a sum
/// that does not fit in a `u64` is not a figure this program can state.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn sum_read_u64(values: impl IntoIterator<Item = u64>) -> Option<u64> {
    let mut total: Option<u64> = None;
    for value in values {
        total = Some(total.unwrap_or(0).checked_add(value)?);
    }
    total
}

/// The `f64` counterpart of [`sum_read_u64`]: `None` when nothing was summed.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn sum_read_f64(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    values.into_iter().reduce(|a, b| a + b)
}

/// The container runtime a cgroup's name belongs to, if any.
///
/// Matches the runtimes' own naming and nothing broader. A bare `.scope`
/// suffix used to count as a container, but systemd gives login sessions and
/// ordinary units that suffix too, so `session-3.scope` was reported as one.
#[cfg(any(target_os = "linux", test))]
fn container_runtime_of(name: &str) -> Option<&'static str> {
    if name.contains("docker-") {
        Some("docker")
    } else if name.contains("cri-containerd-") {
        Some("containerd")
    } else if name.contains("crio-") {
        Some("cri-o")
    } else if name.contains("libpod-") {
        Some("podman")
    } else if name.starts_with("lxc") || name.contains("lxc-") {
        Some("lxc")
    } else {
        None
    }
}

/// Total memory limit over the cgroups that have a limit. An unlimited
/// `memory.max` reads as `u64::MAX` and is left out, so this covers only capped
/// cgroups: `None` when none of them is capped, or when the total overflows.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn sum_memory_limits(cgroups: &[CgroupInfo]) -> Option<u64> {
    sum_read_u64(
        cgroups
            .iter()
            .filter_map(|c| c.memory.as_ref())
            .map(|m| m.limit_bytes)
            .filter(|&limit| limit < u64::MAX),
    )
}

/// Effective CPUs over the cgroups with a CPU quota. A cgroup with no quota has
/// `effective_cpus: None` and is left out, so this covers only capped cgroups:
/// `None` when none of them is capped.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn sum_effective_cpus(cgroups: &[CgroupInfo]) -> Option<f64> {
    sum_read_f64(
        cgroups
            .iter()
            .filter_map(|c| c.cpu.as_ref())
            .filter_map(|c| c.effective_cpus),
    )
}

/// Cgroup resource monitor.
pub struct CgroupMonitor {
    overview: CgroupOverview,
}

impl CgroupMonitor {
    /// Create a new cgroup monitor.
    pub fn new() -> Result<Self, IronError> {
        let overview = Self::scan()?;
        Ok(Self { overview })
    }

    /// Refresh data.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.overview = Self::scan()?;
        Ok(())
    }

    /// Get the overview.
    pub fn overview(&self) -> &CgroupOverview {
        &self.overview
    }

    /// Get container cgroups only.
    pub fn containers(&self) -> Vec<&CgroupInfo> {
        self.overview
            .cgroups
            .iter()
            .filter(|c| c.is_container)
            .collect()
    }

    /// Get cgroups that are near their memory limit (>80%).
    pub fn memory_pressure_cgroups(&self) -> Vec<&CgroupInfo> {
        self.overview
            .cgroups
            .iter()
            .filter(|c| {
                c.memory
                    .as_ref()
                    .and_then(|m| m.usage_pct)
                    .is_some_and(|pct| pct > 80.0)
            })
            .collect()
    }

    /// Get cgroups that are CPU-throttled.
    pub fn throttled_cgroups(&self) -> Vec<&CgroupInfo> {
        self.overview
            .cgroups
            .iter()
            .filter(|c| {
                c.cpu
                    .as_ref()
                    .map(|cpu| cpu.throttled_periods > 0)
                    .unwrap_or(false)
            })
            .collect()
    }

    #[cfg(target_os = "linux")]
    fn scan() -> Result<CgroupOverview, IronError> {
        let version = Self::detect_version();
        let available_controllers = Self::list_controllers(&version);
        let cgroups = match version {
            CgroupVersion::V2 => Self::scan_v2()?,
            CgroupVersion::V1 => Self::scan_v1()?,
            _ => Vec::new(),
        };

        let container_count = cgroups.iter().filter(|c| c.is_container).count();
        let total_memory = sum_memory_limits(&cgroups);
        let total_cpus = sum_effective_cpus(&cgroups);

        Ok(CgroupOverview {
            version,
            available_controllers,
            cgroups,
            container_count,
            total_memory_limit_bytes: total_memory,
            total_effective_cpus: total_cpus,
        })
    }

    #[cfg(target_os = "linux")]
    fn detect_version() -> CgroupVersion {
        // Check for unified cgroup v2
        let v2_path = std::path::Path::new("/sys/fs/cgroup/cgroup.controllers");
        if v2_path.exists() {
            return CgroupVersion::V2;
        }

        // Check for v1 hierarchies
        let v1_cpu = std::path::Path::new("/sys/fs/cgroup/cpu");
        if v1_cpu.exists() {
            return CgroupVersion::V1;
        }

        CgroupVersion::Unknown
    }

    #[cfg(target_os = "linux")]
    fn list_controllers(version: &CgroupVersion) -> Option<Vec<String>> {
        // An unreadable listing used to become an empty Vec, the same as a
        // hierarchy that has no controllers. It is `None` instead.
        match version {
            CgroupVersion::V2 => std::fs::read_to_string("/sys/fs/cgroup/cgroup.controllers")
                .ok()
                .map(|s| s.split_whitespace().map(String::from).collect()),
            CgroupVersion::V1 => {
                // List controller directories
                std::fs::read_dir("/sys/fs/cgroup").ok().map(|entries| {
                    entries
                        .flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect()
                })
            }
            _ => None,
        }
    }

    #[cfg(target_os = "linux")]
    fn scan_v2() -> Result<Vec<CgroupInfo>, IronError> {
        let mut cgroups = Vec::new();
        let base = std::path::Path::new("/sys/fs/cgroup");

        // Scan immediate children and system/user slices
        Self::scan_v2_dir(base, "", &mut cgroups, 0)?;

        Ok(cgroups)
    }

    #[cfg(target_os = "linux")]
    fn scan_v2_dir(
        dir: &std::path::Path,
        rel_path: &str,
        cgroups: &mut Vec<CgroupInfo>,
        depth: u32,
    ) -> Result<(), IronError> {
        if depth > 4 {
            return Ok(()); // Limit recursion
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();

                // Skip non-cgroup directories
                if name.starts_with('.') {
                    continue;
                }

                let cg_path = format!("{}/{}", rel_path, name);

                // Check if this has cgroup.procs (is a real cgroup)
                if path.join("cgroup.procs").exists() {
                    // Only a container runtime's own cgroup names count. A bare
                    // `.scope` is also how systemd names login sessions and
                    // ordinary units, which were all reported as containers.
                    let container_runtime = container_runtime_of(&name).map(String::from);
                    let is_container = container_runtime.is_some();

                    let friendly_name = if is_container {
                        // Try to extract container ID
                        name.split('-')
                            .next_back()
                            .unwrap_or(&name)
                            .replace(".scope", "")
                    } else {
                        name.clone()
                    };

                    let controllers = std::fs::read_to_string(path.join("cgroup.controllers"))
                        .ok()
                        .map(|s| s.split_whitespace().map(String::from).collect());

                    let cpu = Self::read_cpu_v2(&path);
                    let memory = Self::read_memory_v2(&path);
                    let pids = Self::read_pids_v2(&path);

                    cgroups.push(CgroupInfo {
                        path: cg_path.clone(),
                        name: friendly_name,
                        is_container,
                        container_runtime,
                        cpu,
                        memory,
                        io: None,
                        pids,
                        controllers,
                    });
                }

                // Recurse into subdirectories
                Self::scan_v2_dir(&path, &cg_path, cgroups, depth + 1)?;
            }
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn read_cpu_v2(path: &std::path::Path) -> Option<CgroupCpu> {
        // cpu.max and cpu.weight are required, as is cpu.stat below. A missing
        // file gives no reading; the kernel defaults would look like real limits.
        let max_str = std::fs::read_to_string(path.join("cpu.max")).ok()?;
        let (quota, period) = parse_cpu_max(&max_str)?;

        let weight = std::fs::read_to_string(path.join("cpu.weight"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())?;

        // CPU usage from cpu.stat. An unreadable file, or one missing a counter,
        // leaves no CgroupCpu at all: zeros here would read as an idle cgroup.
        let stat_str = std::fs::read_to_string(path.join("cpu.stat")).ok()?;
        let (usage_us, throttled_periods, throttled_time) = parse_cpu_stat(&stat_str)?;

        // Unlimited (-1) caps nothing, so there is no effective count: None, not 0.
        let effective_cpus = if quota < 0 {
            None
        } else {
            Some(quota as f64 / period as f64)
        };

        // shares and per-CPU usage are v1 files this reader never opens.
        Some(CgroupCpu {
            quota_us: quota,
            period_us: period,
            shares: None,
            weight,
            usage_us,
            per_cpu_usage_us: None,
            throttled_periods,
            throttled_time_us: throttled_time,
            effective_cpus,
        })
    }

    #[cfg(target_os = "linux")]
    fn read_memory_v2(path: &std::path::Path) -> Option<CgroupMemory> {
        // Current usage, the limit and the stat counters are required: without
        // them there is no reading. Swap accounting (no CONFIG_MEMCG_SWAP) and
        // the oom_kill key (older kernels) are optional, so their absence is
        // None and does not discard the rest of the reading.
        let usage = std::fs::read_to_string(path.join("memory.current"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())?;

        let limit = parse_limit(&std::fs::read_to_string(path.join("memory.max")).ok()?)?;

        // Memory stat for cache/rss
        let stat_str = std::fs::read_to_string(path.join("memory.stat")).ok()?;
        let (cache, rss) = parse_memory_stat(&stat_str)?;

        let swap_usage = std::fs::read_to_string(path.join("memory.swap.current"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok());

        let swap_limit = std::fs::read_to_string(path.join("memory.swap.max"))
            .ok()
            .and_then(|s| parse_limit(&s));

        // OOM events
        let oom_events = std::fs::read_to_string(path.join("memory.events"))
            .ok()
            .and_then(|s| parse_oom_kill(&s));

        // A percentage needs a limit. Unlimited (u64::MAX) and zero have none,
        // and 0.0 there would read as "not using any of it".
        let usage_pct = if limit < u64::MAX && limit > 0 {
            Some((usage as f64 / limit as f64) * 100.0)
        } else {
            None
        };

        Some(CgroupMemory {
            usage_bytes: usage,
            limit_bytes: limit,
            swap_usage_bytes: swap_usage,
            swap_limit_bytes: swap_limit,
            cache_bytes: cache,
            rss_bytes: rss,
            oom_events,
            usage_pct,
        })
    }

    #[cfg(target_os = "linux")]
    fn read_pids_v2(path: &std::path::Path) -> Option<CgroupPids> {
        let current = std::fs::read_to_string(path.join("pids.current"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())?;

        let limit = parse_limit(&std::fs::read_to_string(path.join("pids.max")).ok()?)?;

        Some(CgroupPids { current, limit })
    }

    #[cfg(target_os = "linux")]
    fn scan_v1() -> Result<Vec<CgroupInfo>, IronError> {
        // No v1 reader exists. An empty list would read as "no cgroups and no
        // containers" on a host that has both.
        Err(IronError::FeatureNotAvailable(
            "cgroup v1 hierarchies are not read".to_string(),
        ))
    }

    #[cfg(not(target_os = "linux"))]
    fn scan() -> Result<CgroupOverview, IronError> {
        // An empty overview here read as "no cgroups and no containers" on a
        // platform where the question was never asked. The reason travels with
        // the error instead.
        Err(IronError::UnsupportedPlatform(
            "cgroups are read from `/sys/fs/cgroup`, which this platform does not expose".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cgroup_version_display() {
        assert_eq!(CgroupVersion::V2.to_string(), "v2");
        assert_eq!(CgroupVersion::V1.to_string(), "v1");
    }

    #[test]
    fn test_effective_cpus() {
        let cpu = CgroupCpu {
            quota_us: 200_000,
            period_us: 100_000,
            shares: None,
            weight: 100,
            usage_us: 0,
            per_cpu_usage_us: None,
            throttled_periods: 0,
            throttled_time_us: 0,
            effective_cpus: Some(2.0),
        };
        assert!(cpu.effective_cpus.is_some_and(|e| (e - 2.0).abs() < 0.01));
    }

    #[test]
    fn test_memory_usage_pct() {
        let mem = CgroupMemory {
            usage_bytes: 500 * 1024 * 1024,  // 500 MB
            limit_bytes: 1024 * 1024 * 1024, // 1 GB
            swap_usage_bytes: Some(0),
            swap_limit_bytes: Some(u64::MAX),
            cache_bytes: 100 * 1024 * 1024,
            rss_bytes: 400 * 1024 * 1024,
            oom_events: Some(0),
            usage_pct: Some(50.0 * 500.0 / 512.0), // ~48.8%
        };
        assert!(mem.usage_pct.is_some_and(|p| p > 0.0 && p < 100.0));
    }

    #[test]
    fn test_container_detection() {
        let cg = CgroupInfo {
            path: "/system.slice/docker-abc123def456.scope".into(),
            name: "abc123def456".into(),
            is_container: true,
            container_runtime: Some("docker".into()),
            cpu: None,
            memory: None,
            io: None,
            pids: None,
            controllers: Some(vec!["cpu".into(), "memory".into()]),
        };
        assert!(cg.is_container);
        assert_eq!(cg.container_runtime, Some("docker".into()));
    }

    #[test]
    fn test_parse_cpu_stat_full() {
        let stat = "usage_usec 5000\nuser_usec 3000\nsystem_usec 2000\nnr_periods 10\nnr_throttled 2\nthrottled_usec 700\n";
        assert_eq!(parse_cpu_stat(stat), Some((5000, 2, 700)));
    }

    #[test]
    fn test_parse_cpu_stat_missing_counter_is_absent() {
        let stat = "usage_usec 5000\nuser_usec 3000\n";
        assert_eq!(parse_cpu_stat(stat), None);
    }

    #[test]
    fn test_parse_cpu_stat_unparseable_usage_is_absent() {
        let stat = "usage_usec lots\nnr_throttled 0\nthrottled_usec 0\n";
        assert_eq!(parse_cpu_stat(stat), None);
    }

    #[test]
    fn test_parse_cpu_max() {
        assert_eq!(parse_cpu_max("max 100000\n"), Some((-1, 100_000)));
        assert_eq!(parse_cpu_max("200000 100000\n"), Some((200_000, 100_000)));
        // A missing or unparseable field is absent, not the kernel default.
        assert_eq!(parse_cpu_max(""), None);
        assert_eq!(parse_cpu_max("200000"), None);
        assert_eq!(parse_cpu_max("lots 100000"), None);
        assert_eq!(parse_cpu_max("max 0"), None);
    }

    #[test]
    fn test_parse_limit() {
        assert_eq!(parse_limit("max\n"), Some(u64::MAX));
        assert_eq!(parse_limit("1073741824\n"), Some(1_073_741_824));
        assert_eq!(parse_limit(""), None);
        assert_eq!(parse_limit("lots"), None);
    }

    #[test]
    fn test_parse_memory_stat_and_oom_kill() {
        assert_eq!(
            parse_memory_stat("anon 400\nfile 100\nkernel 8\n"),
            Some((100, 400))
        );
        assert_eq!(parse_memory_stat("anon 400\n"), None);
        assert_eq!(parse_oom_kill("low 0\noom_kill 2\n"), Some(2));
        assert_eq!(parse_oom_kill("low 0\nmax 5\n"), None);
    }

    /// A throwaway cgroup directory holding `files`. Callers remove it.
    #[cfg(target_os = "linux")]
    fn cgroup_dir(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ironmon-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (file, body) in files {
            std::fs::write(dir.join(file), body).unwrap();
        }
        dir
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_read_cpu_v2_max_is_unlimited() {
        let dir = cgroup_dir(
            "cpu-max",
            &[
                ("cpu.max", "max 100000\n"),
                ("cpu.weight", "100\n"),
                (
                    "cpu.stat",
                    "usage_usec 5000\nnr_throttled 2\nthrottled_usec 700\n",
                ),
            ],
        );
        let cpu = CgroupMonitor::read_cpu_v2(&dir);
        std::fs::remove_dir_all(&dir).ok();
        let cpu = cpu.expect("all files present");
        assert_eq!(cpu.quota_us, -1);
        assert_eq!(cpu.period_us, 100_000);
        assert_eq!(cpu.weight, 100);
        assert_eq!(cpu.usage_us, 5000);
        assert_eq!(cpu.throttled_periods, 2);
        assert_eq!(cpu.throttled_time_us, 700);
        // Unlimited caps nothing, and shares and per-CPU usage are not read on v2.
        assert_eq!(cpu.effective_cpus, None);
        assert_eq!(cpu.shares, None);
        assert_eq!(cpu.per_cpu_usage_us, None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_read_cpu_v2_quota_is_effective_cpus() {
        let dir = cgroup_dir(
            "cpu-quota",
            &[
                ("cpu.max", "200000 100000\n"),
                ("cpu.weight", "100\n"),
                (
                    "cpu.stat",
                    "usage_usec 5000\nnr_throttled 0\nthrottled_usec 0\n",
                ),
            ],
        );
        let cpu = CgroupMonitor::read_cpu_v2(&dir);
        std::fs::remove_dir_all(&dir).ok();
        let cpu = cpu.expect("all files present");
        assert_eq!(cpu.quota_us, 200_000);
        assert_eq!(cpu.effective_cpus, Some(2.0));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_unread_cgroup_files_are_absent() {
        // cpu.weight and cpu.stat missing: no CgroupCpu, not one with defaults.
        let dir = cgroup_dir("cpu-partial", &[("cpu.max", "200000 100000\n")]);
        let cpu = CgroupMonitor::read_cpu_v2(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert!(cpu.is_none());

        // A directory with none of the files at all.
        let missing = std::path::Path::new("/nonexistent/ironmon-cgroup-test");
        assert!(CgroupMonitor::read_cpu_v2(missing).is_none());
        assert!(CgroupMonitor::read_memory_v2(missing).is_none());
        assert!(CgroupMonitor::read_pids_v2(missing).is_none());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_read_memory_v2_max_is_unlimited() {
        let dir = cgroup_dir(
            "mem-max",
            &[
                ("memory.current", "4096\n"),
                ("memory.max", "max\n"),
                ("memory.swap.current", "0\n"),
                ("memory.swap.max", "max\n"),
                ("memory.stat", "anon 400\nfile 100\n"),
                ("memory.events", "low 0\noom_kill 2\n"),
            ],
        );
        let mem = CgroupMonitor::read_memory_v2(&dir);
        std::fs::remove_dir_all(&dir).ok();
        let mem = mem.expect("all files present");
        assert_eq!(mem.usage_bytes, 4096);
        assert_eq!(mem.limit_bytes, u64::MAX);
        assert_eq!(mem.swap_limit_bytes, Some(u64::MAX));
        assert_eq!(mem.cache_bytes, 100);
        assert_eq!(mem.rss_bytes, 400);
        assert_eq!(mem.oom_events, Some(2));
        // No limit, so no percentage: not 0.0.
        assert_eq!(mem.usage_pct, None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_read_memory_v2_optional_files_absent_keeps_reading() {
        // No swap accounting and no oom_kill key: those are None, and the
        // current, limit and stat readings are still returned.
        let dir = cgroup_dir(
            "mem-optional",
            &[
                ("memory.current", "4096\n"),
                ("memory.max", "1073741824\n"),
                ("memory.stat", "anon 400\nfile 100\n"),
            ],
        );
        let mem = CgroupMonitor::read_memory_v2(&dir);
        std::fs::remove_dir_all(&dir).ok();
        let mem = mem.expect("required files present");
        assert_eq!(mem.usage_bytes, 4096);
        assert_eq!(mem.limit_bytes, 1_073_741_824);
        assert_eq!(mem.rss_bytes, 400);
        assert_eq!(mem.swap_usage_bytes, None);
        assert_eq!(mem.swap_limit_bytes, None);
        assert_eq!(mem.oom_events, None);
        assert!(mem.usage_pct.is_some_and(|p| p > 0.0 && p < 1.0));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_read_memory_v2_missing_stat_is_absent() {
        let dir = cgroup_dir(
            "mem-partial",
            &[
                ("memory.current", "4096\n"),
                ("memory.max", "max\n"),
                ("memory.swap.current", "0\n"),
                ("memory.swap.max", "max\n"),
                ("memory.events", "oom_kill 0\n"),
            ],
        );
        let mem = CgroupMonitor::read_memory_v2(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert!(mem.is_none());
    }

    #[test]
    fn test_serialization() {
        let overview = CgroupOverview {
            version: CgroupVersion::V2,
            available_controllers: Some(vec!["cpu".into(), "memory".into()]),
            cgroups: Vec::new(),
            container_count: 0,
            total_memory_limit_bytes: None,
            total_effective_cpus: None,
        };
        let json = serde_json::to_string(&overview).unwrap();
        assert!(json.contains("V2"));
        let _: CgroupOverview = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_sum_read_is_none_over_nothing() {
        assert_eq!(sum_read_u64(std::iter::empty()), None);
        assert_eq!(sum_read_f64(std::iter::empty()), None);
    }

    #[test]
    fn test_sum_read_adds_contributors() {
        assert_eq!(sum_read_u64([1u64, 2, 3]), Some(6));
        assert_eq!(sum_read_f64([1.5, 2.0]), Some(3.5));
    }

    #[test]
    fn test_sum_read_overflow_is_none() {
        assert_eq!(sum_read_u64([u64::MAX - 1, 2]), None);
    }

    /// `memory_limit` of `None` means no memory reading was made, and
    /// `effective_cpus` of `None` means the cgroup has no CPU quota.
    fn cgroup_with(memory_limit: Option<u64>, effective_cpus: Option<f64>) -> CgroupInfo {
        CgroupInfo {
            path: String::new(),
            name: String::new(),
            is_container: false,
            container_runtime: None,
            cpu: Some(CgroupCpu {
                quota_us: -1,
                period_us: 100_000,
                shares: None,
                weight: 100,
                usage_us: 0,
                per_cpu_usage_us: None,
                throttled_periods: 0,
                throttled_time_us: 0,
                effective_cpus,
            }),
            memory: memory_limit.map(|limit_bytes| CgroupMemory {
                usage_bytes: 0,
                limit_bytes,
                swap_usage_bytes: None,
                swap_limit_bytes: None,
                cache_bytes: 0,
                rss_bytes: 0,
                oom_events: None,
                usage_pct: None,
            }),
            io: None,
            pids: None,
            controllers: Some(Vec::new()),
        }
    }

    #[test]
    fn test_totals_over_nothing_capped_are_none() {
        assert_eq!(sum_memory_limits(&[]), None);
        assert_eq!(sum_effective_cpus(&[]), None);
        let uncapped = [cgroup_with(Some(u64::MAX), None)];
        assert_eq!(sum_memory_limits(&uncapped), None);
        assert_eq!(sum_effective_cpus(&uncapped), None);
    }

    #[test]
    fn test_totals_cover_only_capped_cgroups() {
        let cgroups = [
            cgroup_with(Some(1024), Some(2.0)),
            cgroup_with(Some(u64::MAX), None),
            cgroup_with(Some(2048), Some(0.5)),
        ];
        assert_eq!(sum_memory_limits(&cgroups), Some(3072));
        assert_eq!(sum_effective_cpus(&cgroups), Some(2.5));
    }

    #[test]
    fn runtime_names_are_containers() {
        assert_eq!(container_runtime_of("docker-abc123.scope"), Some("docker"));
        assert_eq!(
            container_runtime_of("cri-containerd-abc123.scope"),
            Some("containerd")
        );
        assert_eq!(container_runtime_of("crio-abc123.scope"), Some("cri-o"));
        assert_eq!(container_runtime_of("libpod-abc123.scope"), Some("podman"));
        assert_eq!(container_runtime_of("lxc.payload.web"), Some("lxc"));
    }

    #[test]
    fn a_scope_suffix_alone_is_not_a_container() {
        // Login sessions and ordinary units carry `.scope` too.
        assert_eq!(container_runtime_of("session-3.scope"), None);
        assert_eq!(container_runtime_of("app-gnome-terminal-1234.scope"), None);
        assert_eq!(container_runtime_of("user@1000.service"), None);
        assert_eq!(container_runtime_of("system.slice"), None);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn scan_is_unsupported_off_linux() {
        assert!(matches!(
            CgroupMonitor::new(),
            Err(IronError::UnsupportedPlatform(_))
        ));
    }
}
