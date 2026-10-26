// memory_management.rs - Memory and swap management module for ironmon
//
// Provides comprehensive memory, swap, and virtual memory monitoring and control.
// Inspired by jetson_stats memory management features.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::Result;

/// Memory pressure level classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryPressure {
    /// Memory is plentiful, system is healthy
    Low,
    /// Memory usage is moderate, might need attention
    Medium,
    /// Memory is under pressure, consider freeing resources
    High,
    /// Critical memory pressure, system may become unstable
    Critical,
}

impl MemoryPressure {
    /// Get the pressure level from memory usage percentage
    pub fn from_usage(percent: f64) -> Self {
        match percent {
            p if p < 60.0 => Self::Low,
            p if p < 80.0 => Self::Medium,
            p if p < 95.0 => Self::High,
            _ => Self::Critical,
        }
    }

    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::Low => "Memory pressure is low, system is healthy",
            Self::Medium => "Memory usage is moderate",
            Self::High => "High memory pressure, consider freeing resources",
            Self::Critical => "Critical memory pressure, system may become unstable",
        }
    }

    /// Get emoji representation
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Low => "🟢",
            Self::Medium => "🟡",
            Self::High => "🟠",
            Self::Critical => "🔴",
        }
    }
}

/// Swap type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwapType {
    /// File-based swap (swapfile)
    File,
    /// Partition-based swap
    Partition,
    /// Compressed RAM (zram/zswap)
    Zram,
    /// Unknown swap type
    Unknown,
}

impl std::fmt::Display for SwapType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File => write!(f, "File"),
            Self::Partition => write!(f, "Partition"),
            Self::Zram => write!(f, "ZRAM"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Information about a swap device/file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapDevice {
    /// Path to swap device/file
    pub path: String,
    /// Type of swap
    pub swap_type: SwapType,
    /// Total size in bytes, `None` when the reader did not give it
    pub total_bytes: Option<u64>,
    /// Used size in bytes, `None` when the reader did not give it
    pub used_bytes: Option<u64>,
    /// Priority (higher = preferred), `None` when the reader did not give it
    pub priority: Option<i32>,
    /// Whether the swap is currently active
    pub active: bool,
}

impl SwapDevice {
    /// Get usage percentage; `None` unless both the total and the used size were read
    pub fn usage_percent(&self) -> Option<f64> {
        match (self.total_bytes, self.used_bytes) {
            (Some(total), Some(used)) if total > 0 => Some((used as f64 / total as f64) * 100.0),
            _ => None,
        }
    }

    /// Get available bytes; `None` unless both the total and the used size were read
    pub fn available_bytes(&self) -> Option<u64> {
        Some(self.total_bytes?.saturating_sub(self.used_bytes?))
    }

    /// Format size for display
    pub fn format_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.1} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.1} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.1} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }
}

/// ZRAM (compressed RAM) information
///
/// Built only when all four of `disksize`, `compr_data_size`, `orig_data_size` and
/// `mem_used_total` were read, so the sizes below are always figures from the device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZramInfo {
    /// Device name (e.g., zram0). Where several zram devices exist, their names
    /// joined by commas, and the sizes are the sums over all of them.
    pub device: String,
    /// Total disksize in bytes
    pub disksize: u64,
    /// Compressed data size
    pub compressed_bytes: u64,
    /// Original (uncompressed) data size
    pub original_bytes: u64,
    /// Memory used for compressed storage
    pub mem_used_bytes: u64,
    /// Compression algorithm, `None` when the device's list could not be read
    pub algorithm: Option<String>,
    /// Number of streams; not parsed yet, so `None`
    pub streams: Option<u32>,
    /// Compression ratio (original/compressed); `None` while nothing is compressed
    pub compression_ratio: Option<f64>,
}

impl ZramInfo {
    /// Calculate memory savings from compression
    pub fn memory_savings(&self) -> u64 {
        self.original_bytes.saturating_sub(self.mem_used_bytes)
    }

    /// Get memory savings percentage; `None` when there is no original data to save from
    pub fn savings_percent(&self) -> Option<f64> {
        (self.original_bytes > 0)
            .then(|| (self.memory_savings() as f64 / self.original_bytes as f64) * 100.0)
    }
}

/// Detailed memory information
///
/// `total` is read on every platform that reports memory, or the read fails. Every other
/// field is `None` where the platform does not report it, or where the source omitted it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    /// Total physical RAM in bytes
    pub total: u64,
    /// Available memory in bytes (includes cached)
    pub available: Option<u64>,
    /// Free memory in bytes (not counting cached)
    pub free: Option<u64>,
    /// Used memory in bytes
    pub used: Option<u64>,
    /// Cached memory in bytes
    pub cached: Option<u64>,
    /// Buffer memory in bytes
    pub buffers: Option<u64>,
    /// Shared memory in bytes
    pub shared: Option<u64>,
    /// Slab reclaimable memory (Linux)
    pub slab_reclaimable: Option<u64>,
    /// Active memory
    pub active: Option<u64>,
    /// Inactive memory
    pub inactive: Option<u64>,
    /// Dirty pages awaiting write
    pub dirty: Option<u64>,
    /// Memory currently being written back
    pub writeback: Option<u64>,
    /// Mapped memory (memory-mapped files)
    pub mapped: Option<u64>,
    /// Memory locked in RAM
    pub mlocked: Option<u64>,
    /// Huge pages total
    pub hugepages_total: Option<u64>,
    /// Huge pages free
    pub hugepages_free: Option<u64>,
    /// Huge page size
    pub hugepage_size: Option<u64>,
}

impl MemoryInfo {
    /// Get usage percentage; `None` when the used figure or the total is unknown
    pub fn usage_percent(&self) -> Option<f64> {
        let used = self.used?;
        (self.total > 0).then(|| (used as f64 / self.total as f64) * 100.0)
    }

    /// Get memory pressure level; `None` when usage could not be determined
    pub fn pressure(&self) -> Option<MemoryPressure> {
        self.usage_percent().map(MemoryPressure::from_usage)
    }

    /// Get effective available memory (includes reclaimable); `None` unless both were read
    pub fn effective_available(&self) -> Option<u64> {
        Some(self.available? + self.slab_reclaimable?)
    }
}

/// One zram device's figures as read from sysfs. Summed across devices, which
/// is why a device is only ever present with all four sizes.
#[cfg(target_os = "linux")]
struct ZramFigures {
    disksize: u64,
    compressed: u64,
    original: u64,
    mem_used: u64,
    algorithm: Option<String>,
}

/// Total swap information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapInfo {
    /// Total swap size in bytes
    pub total: u64,
    /// Used swap in bytes, or `None` where free swap was not established. On
    /// Windows used is total less free, so it is withheld with free.
    pub used: Option<u64>,
    /// Free swap in bytes, or `None` where it was not established. On Windows
    /// it is the commit headroom less the physical headroom; where that
    /// difference would go negative the reader withholds it, not clamps it.
    pub free: Option<u64>,
    /// List of swap devices
    pub devices: Vec<SwapDevice>,
    /// ZRAM information if available
    pub zram: Option<ZramInfo>,
}

impl SwapInfo {
    /// Get usage percentage; `None` when there is no swap to be used, or when
    /// the used figure was not established
    pub fn usage_percent(&self) -> Option<f64> {
        let used = self.used?;
        (self.total > 0).then(|| (used as f64 / self.total as f64) * 100.0)
    }

    /// Check if swap is configured
    pub fn has_swap(&self) -> bool {
        self.total > 0
    }

    /// Check if ZRAM is configured
    pub fn has_zram(&self) -> bool {
        self.zram.is_some()
    }
}

/// Memory statistics over time. A counter is `None` unless its key was in the last read.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Page faults (minor)
    pub page_faults_minor: Option<u64>,
    /// Page faults (major - required disk IO)
    pub page_faults_major: Option<u64>,
    /// Pages swapped in
    pub swap_in: Option<u64>,
    /// Pages swapped out
    pub swap_out: Option<u64>,
    /// Pages allocated
    pub pages_allocated: Option<u64>,
    /// Pages freed
    pub pages_freed: Option<u64>,
    /// OOM killer invocations
    pub oom_kills: Option<u64>,
}

/// Process memory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMemory {
    /// Process ID
    pub pid: u32,
    /// Process name
    pub name: String,
    /// Resident Set Size (physical memory); `None` when not read
    pub rss: Option<u64>,
    /// Virtual memory size; `None` when not read
    pub vms: Option<u64>,
    /// Shared memory; no platform reader produces it, so it is always `None`
    pub shared: Option<u64>,
    /// Private memory; no reader produces it (this was a copy of `rss`), so it
    /// is always `None`
    pub private: Option<u64>,
    /// Swap usage; `None` when not read
    pub swap: Option<u64>,
    /// Percentage of total memory; `None` when total memory was not read
    pub memory_percent: Option<f64>,
}

/// Memory monitor for comprehensive memory management
#[derive(Debug)]
pub struct MemoryMonitor {
    /// Memory information
    pub memory: MemoryInfo,
    /// Swap information
    pub swap: SwapInfo,
    /// Memory statistics
    pub stats: MemoryStats,
    /// VM settings
    pub vm_settings: HashMap<String, String>,
    /// Last update timestamp
    pub last_update: std::time::Instant,
}

impl MemoryMonitor {
    /// Create a new memory monitor
    pub fn new() -> Result<Self> {
        let mut monitor = Self {
            // Plain zeros here are placeholders: refresh either overwrites total and the swap
            // totals or fails, so none of them is reported unread.
            memory: MemoryInfo {
                total: 0,
                available: None,
                free: None,
                used: None,
                cached: None,
                buffers: None,
                shared: None,
                slab_reclaimable: None,
                active: None,
                inactive: None,
                dirty: None,
                writeback: None,
                mapped: None,
                mlocked: None,
                hugepages_total: None,
                hugepages_free: None,
                hugepage_size: None,
            },
            swap: SwapInfo {
                total: 0,
                used: None,
                free: None,
                devices: Vec::new(),
                zram: None,
            },
            stats: MemoryStats::default(),
            vm_settings: HashMap::new(),
            last_update: std::time::Instant::now(),
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    /// Refresh all memory information
    pub fn refresh(&mut self) -> Result<()> {
        self.last_update = std::time::Instant::now();

        #[cfg(target_os = "linux")]
        {
            self.linux_read_meminfo()?;
            self.linux_read_swap_devices()?;
            self.linux_read_zram()?;
            self.linux_read_vmstat()?;
            self.linux_read_vm_settings()?;
        }

        #[cfg(windows)]
        {
            self.windows_read_memory()?;
            self.windows_read_swap()?;
        }

        #[cfg(target_os = "macos")]
        {
            self.macos_read_memory()?;
            self.macos_read_swap()?;
        }

        #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
        {
            // No reader exists here. Say so, rather than report the zeroed state `new` starts from.
            return Err(crate::error::IronError::System(
                "memory monitoring is not implemented on this platform".to_string(),
            ));
        }

        Ok(())
    }

    /// Get top memory consumers
    pub fn top_processes(&self, limit: usize) -> Vec<ProcessMemory> {
        #[cfg(target_os = "linux")]
        return self.linux_top_processes(limit);

        #[cfg(windows)]
        return self.windows_top_processes(limit);

        #[cfg(target_os = "macos")]
        return self.macos_top_processes(limit);

        #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
        Vec::new()
    }

    /// Get memory pressure level; `None` when memory usage was not read
    pub fn pressure(&self) -> Option<MemoryPressure> {
        self.memory.pressure()
    }

    /// Check if swap is being heavily used; `None` when swap usage was not read
    pub fn swap_pressure(&self) -> Option<MemoryPressure> {
        self.swap.usage_percent().map(MemoryPressure::from_usage)
    }

    /// Get system health based on memory and swap. `None` when memory usage was not read,
    /// because a failed read must not score as healthy.
    pub fn health_score(&self) -> Option<u32> {
        let mem_score: u32 = match self.pressure()? {
            MemoryPressure::Low => 50,
            MemoryPressure::Medium => 35,
            MemoryPressure::High => 20,
            MemoryPressure::Critical => 5,
        };

        let swap_score: u32 = if !self.swap.has_swap() {
            // No swap configured - neutral. The swap total was read, and it is zero.
            25
        } else {
            match self.swap_pressure()? {
                MemoryPressure::Low => 25,
                MemoryPressure::Medium => 20,
                MemoryPressure::High => 10,
                MemoryPressure::Critical => 0,
            }
        };

        // Bonus for ZRAM, only from a compression ratio that was measured
        let zram_bonus: u32 = match self
            .swap
            .zram
            .as_ref()
            .and_then(|zram| zram.compression_ratio)
        {
            Some(ratio) if ratio > 2.0 => 10,
            Some(ratio) if ratio > 1.5 => 5,
            _ => 0,
        };

        Some((mem_score + swap_score + zram_bonus).min(100))
    }

    // ==================== Linux Implementation ====================

    #[cfg(target_os = "linux")]
    fn linux_read_meminfo(&mut self) -> Result<()> {
        use std::fs;

        let content = fs::read_to_string("/proc/meminfo")
            .map_err(|e| crate::error::IronError::System(e.to_string()))?;

        let mut mem = HashMap::new();
        for line in content.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim().replace(" kB", "");
                if let Ok(kb) = value.parse::<u64>() {
                    mem.insert(key.to_string(), kb * 1024);
                }
            }
        }

        // Keys the kernel did not print are `None`, never zero. MemTotal and the swap figures
        // are required: without them there is no reading to report.
        let Some(total) = mem.get("MemTotal").copied() else {
            return Err(crate::error::IronError::System(
                "/proc/meminfo has no MemTotal".to_string(),
            ));
        };
        let Some(swap_total) = mem.get("SwapTotal").copied() else {
            return Err(crate::error::IronError::System(
                "/proc/meminfo has no SwapTotal".to_string(),
            ));
        };
        let Some(swap_free) = mem.get("SwapFree").copied() else {
            return Err(crate::error::IronError::System(
                "/proc/meminfo has no SwapFree".to_string(),
            ));
        };

        self.memory.total = total;
        self.memory.free = mem.get("MemFree").copied();
        // No MemAvailable means no available figure. MemFree is a different quantity.
        self.memory.available = mem.get("MemAvailable").copied();
        self.memory.buffers = mem.get("Buffers").copied();
        self.memory.cached = mem.get("Cached").copied();
        self.memory.shared = mem.get("Shmem").copied();
        self.memory.slab_reclaimable = mem.get("SReclaimable").copied();
        self.memory.active = mem.get("Active").copied();
        self.memory.inactive = mem.get("Inactive").copied();
        self.memory.dirty = mem.get("Dirty").copied();
        self.memory.writeback = mem.get("Writeback").copied();
        self.memory.mapped = mem.get("Mapped").copied();
        self.memory.mlocked = mem.get("Mlocked").copied();
        self.memory.hugepages_total = mem.get("HugePages_Total").copied();
        self.memory.hugepages_free = mem.get("HugePages_Free").copied();
        self.memory.hugepage_size = mem.get("Hugepagesize").copied();

        // Used memory is what is left after free, buffers and cache. It is `None` unless all
        // three were read: a missing one is not zero.
        self.memory.used = match (self.memory.free, self.memory.buffers, self.memory.cached) {
            (Some(free), Some(buffers), Some(cached)) => Some(
                total
                    .saturating_sub(free)
                    .saturating_sub(buffers)
                    .saturating_sub(cached),
            ),
            _ => None,
        };

        // Swap from meminfo
        self.swap.total = swap_total;
        self.swap.free = Some(swap_free);
        self.swap.used = Some(swap_total.saturating_sub(swap_free));

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_read_swap_devices(&mut self) -> Result<()> {
        use std::fs;

        self.swap.devices.clear();

        let content = fs::read_to_string("/proc/swaps")
            .map_err(|e| crate::error::IronError::System(e.to_string()))?;

        for line in content.lines().skip(1) {
            // Skip header
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                let path = parts[0].to_string();
                let swap_type = match parts[1] {
                    "file" => SwapType::File,
                    "partition" => SwapType::Partition,
                    _ if path.contains("zram") => SwapType::Zram,
                    _ => SwapType::Unknown,
                };
                // A figure that does not parse is `None`, not zero.
                let total = parts[2].parse::<u64>().ok().map(|kb| kb * 1024);
                let used = parts[3].parse::<u64>().ok().map(|kb| kb * 1024);
                let priority = parts[4].parse::<i32>().ok();

                self.swap.devices.push(SwapDevice {
                    path,
                    swap_type,
                    total_bytes: total,
                    used_bytes: used,
                    priority,
                    active: true,
                });
            }
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_read_zram(&mut self) -> Result<()> {
        use std::fs;
        use std::path::Path;

        // Every zram device, not only zram0: a machine with a second compressed
        // device used to report the first as if it were the whole of it.
        let block = Path::new("/sys/block");
        let mut names: Vec<String> = fs::read_dir(block)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|name| Self::is_zram_device(name))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();

        if names.is_empty() {
            self.swap.zram = None;
            return Ok(());
        }

        // A file that is missing or does not parse is `None`, never zero. A device
        // contributes only when all four figures it is built from were read.
        let read_device = |name: &str| -> Option<ZramFigures> {
            let dir = block.join(name);
            let read_file = |file: &str| -> Option<u64> {
                fs::read_to_string(dir.join(file)).ok()?.trim().parse().ok()
            };
            let algorithm = fs::read_to_string(dir.join("comp_algorithm"))
                .ok()
                .map(|raw| {
                    let raw = raw.trim().to_string();
                    let bracketed = raw
                        .split_whitespace()
                        .find(|s| s.starts_with('['))
                        .map(|s| s.trim_matches(|c| c == '[' || c == ']').to_string());
                    bracketed.unwrap_or(raw)
                });
            Some(ZramFigures {
                disksize: read_file("disksize")?,
                compressed: read_file("compr_data_size")?,
                original: read_file("orig_data_size")?,
                mem_used: read_file("mem_used_total")?,
                algorithm,
            })
        };

        // The sizes are summed across devices, so one device whose figures
        // cannot be read would leave the total short. The report is withheld
        // rather than given a total that leaves that device out.
        let mut devices = Vec::with_capacity(names.len());
        for name in &names {
            let Some(figures) = read_device(name) else {
                self.swap.zram = None;
                return Ok(());
            };
            devices.push(figures);
        }

        let sum = |pick: fn(&ZramFigures) -> u64| -> Option<u64> {
            devices
                .iter()
                .try_fold(0u64, |acc, d| acc.checked_add(pick(d)))
        };
        let (Some(disksize), Some(compressed), Some(original), Some(mem_used)) = (
            sum(|d| d.disksize),
            sum(|d| d.compressed),
            sum(|d| d.original),
            sum(|d| d.mem_used),
        ) else {
            self.swap.zram = None;
            return Ok(());
        };

        // One algorithm name only when every device agrees; otherwise it is
        // per-device and no single name describes the total.
        let algorithm = devices.first().and_then(|first| {
            devices
                .iter()
                .all(|d| d.algorithm == first.algorithm)
                .then(|| first.algorithm.clone())
                .flatten()
        });

        // Nothing compressed yet means there is no ratio to report.
        let compression_ratio = (compressed > 0).then(|| original as f64 / compressed as f64);

        self.swap.zram = Some(ZramInfo {
            device: names.join(","),
            disksize,
            compressed_bytes: compressed,
            original_bytes: original,
            mem_used_bytes: mem_used,
            algorithm,
            // Not parsed from the device yet
            streams: None,
            compression_ratio,
        });

        Ok(())
    }

    /// Whether a `/sys/block` entry is a zram device (`zram0`, `zram12`), as
    /// opposed to a name that merely begins with the letters.
    #[cfg(target_os = "linux")]
    fn is_zram_device(name: &str) -> bool {
        name.strip_prefix("zram")
            .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
    }

    #[cfg(target_os = "linux")]
    fn linux_read_vmstat(&mut self) -> Result<()> {
        use std::fs;

        // Every counter starts absent: one missing from this read must not keep the value
        // from the last one.
        self.stats = MemoryStats::default();

        let content = fs::read_to_string("/proc/vmstat").unwrap_or_default();
        self.stats = parse_vmstat(&content);

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_read_vm_settings(&mut self) -> Result<()> {
        use std::fs;

        self.vm_settings.clear();

        let settings = [
            "swappiness",
            "vfs_cache_pressure",
            "dirty_ratio",
            "dirty_background_ratio",
            "min_free_kbytes",
            "overcommit_memory",
            "overcommit_ratio",
        ];

        for setting in &settings {
            let path = format!("/proc/sys/vm/{}", setting);
            if let Ok(value) = fs::read_to_string(&path) {
                self.vm_settings
                    .insert(setting.to_string(), value.trim().to_string());
            }
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_top_processes(&self, limit: usize) -> Vec<ProcessMemory> {
        use std::fs;

        let mut processes = Vec::new();

        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() {
                    let status_path = format!("/proc/{}/status", pid);
                    let cmdline_path = format!("/proc/{}/cmdline", pid);

                    if let Ok(status) = fs::read_to_string(&status_path) {
                        let name = fs::read_to_string(&cmdline_path)
                            .unwrap_or_default()
                            .split('\0')
                            .next()
                            .unwrap_or("")
                            .split('/')
                            .next_back()
                            .unwrap_or("unknown")
                            .to_string();

                        // `rss` stays 0 when VmRSS is absent or unparseable, and such a process
                        // is skipped below. The other figures stay `None` rather than 0.
                        let mut rss = 0u64;
                        let mut vms = None;
                        let mut swap = None;

                        for line in status.lines() {
                            if let Some((key, value)) = line.split_once(':') {
                                let kb = value
                                    .trim()
                                    .replace(" kB", "")
                                    .parse::<u64>()
                                    .ok()
                                    .map(|v| v * 1024);
                                match key {
                                    "VmRSS" => rss = kb.unwrap_or(0),
                                    "VmSize" => vms = kb,
                                    "VmSwap" => swap = kb,
                                    _ => {}
                                }
                            }
                        }

                        if rss > 0 {
                            // `None`, not 0.0, when total is unread: 0.0 would say the process uses none of it.
                            let memory_percent = (self.memory.total > 0)
                                .then(|| (rss as f64 / self.memory.total as f64) * 100.0);

                            processes.push(ProcessMemory {
                                pid,
                                name,
                                rss: Some(rss),
                                vms,
                                shared: None,
                                private: None,
                                swap,
                                memory_percent,
                            });
                        }
                    }
                }
            }
        }

        // Sort by RSS descending
        processes.sort_by_key(|p| std::cmp::Reverse(p.rss));
        processes.truncate(limit);
        processes
    }

    // ==================== Windows Implementation ====================

    #[cfg(windows)]
    fn windows_read_memory(&mut self) -> Result<()> {
        use std::mem::size_of;
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

        unsafe {
            let mut status = MEMORYSTATUSEX {
                dwLength: size_of::<MEMORYSTATUSEX>() as u32,
                ..Default::default()
            };

            GlobalMemoryStatusEx(&mut status)
                .map_err(|e| crate::error::IronError::System(e.to_string()))?;

            self.memory.total = status.ullTotalPhys;
            self.memory.available = Some(status.ullAvailPhys);
            self.memory.used = Some(status.ullTotalPhys.saturating_sub(status.ullAvailPhys));

            // Windows does not report free (as distinct from available), cached or buffers
            // separately, so those stay `None` rather than being estimated.
        }

        Ok(())
    }

    #[cfg(windows)]
    fn windows_read_swap(&mut self) -> Result<()> {
        use std::mem::size_of;
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

        unsafe {
            let mut status = MEMORYSTATUSEX {
                dwLength: size_of::<MEMORYSTATUSEX>() as u32,
                ..Default::default()
            };

            GlobalMemoryStatusEx(&mut status)
                .map_err(|e| crate::error::IronError::System(e.to_string()))?;

            // Page file info
            self.swap.total = status.ullTotalPageFile.saturating_sub(status.ullTotalPhys);
            // Swap free is the page file's share of the commit headroom: available
            // commit (`ullAvailPageFile`) less the physical memory that is
            // available. The raw `ullAvailPageFile` used to be stored here, and it is
            // commit headroom for RAM and page file together, so it reported far more
            // swap free than the page file has. An underflow means the two figures
            // are not consistent with that reading, so free is left unknown.
            self.swap.free = status.ullAvailPageFile.checked_sub(status.ullAvailPhys);
            self.swap.used = self
                .swap
                .free
                .map(|free| self.swap.total.saturating_sub(free));

            // Try to get pagefile path from WMI
            self.windows_read_pagefile_devices();
        }

        Ok(())
    }

    /// Read configured pagefile paths from the Memory Management registry key.
    ///
    /// Returns `None` when the key or value is absent, and an empty vector when the
    /// machine is configured with no pagefile at all — a real state worth
    /// distinguishing from a failed read.
    #[cfg(windows)]
    fn read_pagefile_paths() -> Option<Vec<String>> {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;

        let key = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management")
            .ok()?;
        let entries: Vec<String> = key.get_value("PagingFiles").ok()?;

        Some(
            entries
                .iter()
                .filter_map(|entry| entry.split_whitespace().next())
                .filter(|path| !path.is_empty())
                .map(|path| path.to_string())
                .collect(),
        )
    }

    #[cfg(windows)]
    fn windows_read_pagefile_devices(&mut self) {
        use std::process::Command;

        self.swap.devices.clear();

        // Configured pagefile paths, from the Memory Management key the kernel reads
        // at boot. This was a `wmic pagefile list` subprocess until Windows 11 24H2
        // removed the tool; every call then fell through to the PowerShell path
        // below, paying a ~400 ms shell spawn for data one registry read supplies.
        //
        // Values look like "?:\pagefile.sys 0 0" (a system-managed file) or
        // "C:\pagefile.sys 1024 4096"; only the path is taken, since the sizes here
        // are the configured limits, not the current usage reported above.
        if let Some(paths) = Self::read_pagefile_paths() {
            for path in paths {
                self.swap.devices.push(SwapDevice {
                    path,
                    swap_type: SwapType::File,
                    // The registry names the path only, so no size or priority is reported
                    total_bytes: None,
                    used_bytes: None,
                    priority: None,
                    active: true,
                });
            }
        }

        // Fallback: try PowerShell
        if self.swap.devices.is_empty() {
            let output = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    "Get-CimInstance Win32_PageFile | Select-Object Name",
                ])
                .output();

            if let Ok(output) = output {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let line = line.trim();
                    if line.ends_with(".sys") || line.ends_with(".SYS") {
                        self.swap.devices.push(SwapDevice {
                            path: line.to_string(),
                            swap_type: SwapType::File,
                            total_bytes: None,
                            used_bytes: None,
                            priority: None,
                            active: true,
                        });
                    }
                }
            }
        }

        // With no pagefile named by either source, the list stays empty rather than
        // guessing a path.
    }

    #[cfg(windows)]
    fn windows_top_processes(&self, limit: usize) -> Vec<ProcessMemory> {
        use std::process::Command;

        let mut processes = Vec::new();

        // Use PowerShell to get process memory info
        let output = Command::new("powershell")
            .args(["-NoProfile",
                "-Command",
                "Get-Process | Sort-Object WorkingSet64 -Descending | Select-Object -First 50 Id,Name,WorkingSet64,VirtualMemorySize64 | ConvertTo-Csv -NoTypeInformation"
            ])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1) {
                // Skip header
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 4 {
                    // A row whose id or working set did not parse is skipped, not ranked as pid 0.
                    let (Ok(pid), Ok(rss)) = (
                        parts[0].trim_matches('"').parse::<u32>(),
                        parts[2].trim_matches('"').parse::<u64>(),
                    ) else {
                        continue;
                    };
                    let name = parts[1].trim_matches('"').to_string();
                    let vms = parts[3].trim_matches('"').parse::<u64>().ok();

                    let memory_percent = (self.memory.total > 0)
                        .then(|| (rss as f64 / self.memory.total as f64) * 100.0);

                    processes.push(ProcessMemory {
                        pid,
                        name,
                        rss: Some(rss),
                        vms,
                        shared: None,
                        private: None,
                        // Not read. PagedMemorySize64 was used here. It counts the
                        // process's pagefile-backed memory, which is not the part of
                        // it currently held in the page file, and no Windows reader
                        // here establishes that part.
                        swap: None,
                        memory_percent,
                    });
                }
            }
        }

        processes.truncate(limit);
        processes
    }

    // ==================== macOS Implementation ====================

    #[cfg(target_os = "macos")]
    /// macOS memory, through `platform::macos` rather than a third `vm_stat`.
    ///
    /// This parsed `vm_stat` itself with `let page_size = 16384u64; // Default
    /// page size on Apple Silicon`. **The page size is in the output's own
    /// header** — `Mach Virtual Memory Statistics: (page size of 16384 bytes)` —
    /// and it is 4096 on an Intel Mac, so assuming it reports every figure here
    /// four times too large on that hardware.
    ///
    /// `platform::macos::parse_vm_stat` reads the header and says why in a
    /// comment. This is the third copy of that parser to be found assuming the
    /// constant instead: the handoff records the first two in the agent tool
    /// surface, one fixed per function and the other left behind. **The question
    /// that finds these is not "where else is this bug" but "where else is this
    /// reader hand-rolled".**
    fn macos_read_memory(&mut self) -> Result<()> {
        // Nothing read is an error, not a zero: filling these in would say the machine has no
        // memory.
        let vm = crate::platform::macos::vm_stat().ok_or_else(|| {
            crate::error::IronError::System("vm_stat could not be read".to_string())
        })?;
        let total = crate::platform::macos::sysctl_u64("hw.memsize").ok_or_else(|| {
            crate::error::IronError::System("hw.memsize could not be read".to_string())
        })?;

        // `VmStat`'s helpers are in bytes and multiply by the page size it read.
        let free = vm.free * vm.page_size;
        let inactive = vm.inactive * vm.page_size;
        let available = free + inactive;

        self.memory.total = total;
        self.memory.free = Some(free);
        self.memory.active = Some(vm.active * vm.page_size);
        self.memory.inactive = Some(inactive);
        self.memory.cached = Some(vm.cached_bytes());
        self.memory.available = Some(available);
        self.memory.used = Some(total.saturating_sub(available));

        Ok(())
    }

    #[cfg(target_os = "macos")]
    /// macOS swap, through `platform::macos::swap_usage`.
    ///
    /// This was a fourth parser for `vm.swapusage`, beside the platform layer's
    /// `parse_swapusage`, the ontology's use of it, and the one removed from the
    /// agent tool surface in `742427e`. All four were correct; that is the
    /// problem, since only the ones anybody edits stay that way.
    fn macos_read_swap(&mut self) -> Result<()> {
        let usage = crate::platform::macos::swap_usage().ok_or_else(|| {
            crate::error::IronError::System("vm.swapusage could not be read".to_string())
        })?;
        self.swap.total = usage.total;
        self.swap.used = Some(usage.used);
        // `SwapUsage` carries total and used; `parse_swapusage` deliberately
        // ignores the printed free, since for swap it is the difference by
        // definition rather than an independent measurement.
        self.swap.free = Some(usage.total.saturating_sub(usage.used));

        // macOS uses dynamic swap files. The sizes are the whole swap's rather than one
        // file's, so the device carries none.
        self.swap.devices.clear();
        if self.swap.total > 0 {
            self.swap.devices.push(SwapDevice {
                path: "/private/var/vm/swapfile*".to_string(),
                swap_type: SwapType::File,
                total_bytes: None,
                used_bytes: None,
                priority: None,
                active: true,
            });
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn macos_top_processes(&self, limit: usize) -> Vec<ProcessMemory> {
        use std::process::Command;

        let mut processes = Vec::new();

        let output = Command::new("ps")
            .args(["-axo", "pid,rss,vsz,comm", "-r"])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1).take(limit) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    // A row whose pid or RSS did not parse is skipped, not ranked as pid 0.
                    let (Ok(pid), Ok(rss_kb)) = (parts[0].parse::<u32>(), parts[1].parse::<u64>())
                    else {
                        continue;
                    };
                    let rss = rss_kb * 1024;
                    let vms = parts[2].parse::<u64>().ok().map(|kb| kb * 1024);
                    let name = parts[3..].join(" ");

                    let memory_percent = (self.memory.total > 0)
                        .then(|| (rss as f64 / self.memory.total as f64) * 100.0);

                    processes.push(ProcessMemory {
                        pid,
                        name,
                        rss: Some(rss),
                        vms,
                        shared: None,
                        private: None,
                        swap: None,
                        memory_percent,
                    });
                }
            }
        }

        processes
    }

    // ==================== Control Functions ====================

    /// Set swappiness value (Linux only, requires root)
    #[cfg(target_os = "linux")]
    pub fn set_swappiness(&self, value: u8) -> Result<()> {
        use std::fs;
        use std::process::Command;

        let value = value.min(100);
        let path = "/proc/sys/vm/swappiness";

        // Try direct write first
        if fs::write(path, value.to_string()).is_ok() {
            return Ok(());
        }

        // Fall back to sysctl
        let output = Command::new("sysctl")
            .args(["-w", &format!("vm.swappiness={}", value)])
            .output()
            .map_err(|e| crate::error::IronError::System(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::IronError::System(
                "Failed to set swappiness (requires root)".to_string(),
            ));
        }

        Ok(())
    }

    /// Drop caches (Linux only, requires root)
    /// level: 1 = pagecache, 2 = dentries+inodes, 3 = all
    #[cfg(target_os = "linux")]
    pub fn drop_caches(&self, level: u8) -> Result<()> {
        use std::fs;
        use std::process::Command;

        let level = level.clamp(1, 3);
        let path = "/proc/sys/vm/drop_caches";

        // Sync first
        let _ = Command::new("sync").status();

        // Try direct write
        if fs::write(path, level.to_string()).is_ok() {
            return Ok(());
        }

        // Fall back to echo with sudo
        let output = Command::new("sh")
            .args(["-c", &format!("echo {} > {}", level, path)])
            .output()
            .map_err(|e| crate::error::IronError::System(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::IronError::System(
                "Failed to drop caches (requires root)".to_string(),
            ));
        }

        Ok(())
    }

    /// Clear standby memory (Windows only)
    #[cfg(windows)]
    pub fn clear_standby_memory(&self) -> Result<()> {
        use std::process::Command;

        // Use RAMMap-like command if available, otherwise use PowerShell
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "[System.GC]::Collect(); [System.GC]::WaitForPendingFinalizers()",
            ])
            .output()
            .map_err(|e| crate::error::IronError::System(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::IronError::System(
                "Failed to clear standby memory".to_string(),
            ));
        }

        Ok(())
    }

    /// Clear memory pressure (macOS only)
    #[cfg(target_os = "macos")]
    pub fn purge_memory(&self) -> Result<()> {
        use std::process::Command;

        let output = Command::new("purge")
            .output()
            .map_err(|e| crate::error::IronError::System(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::IronError::System(
                "Failed to purge memory (requires Developer Tools)".to_string(),
            ));
        }

        Ok(())
    }
}

/// Summary of memory state. A field is `None` when the reading it comes from was not made.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySummary {
    /// Memory usage percentage
    pub memory_percent: Option<f64>,
    /// Swap usage percentage
    pub swap_percent: Option<f64>,
    /// Memory pressure level
    pub pressure: Option<MemoryPressure>,
    /// Total physical memory
    pub total_memory: u64,
    /// Available memory
    pub available_memory: Option<u64>,
    /// Total swap
    pub total_swap: u64,
    /// Used swap, or `None` where it was not established (see `SwapInfo::used`)
    pub used_swap: Option<u64>,
    /// Has ZRAM
    pub has_zram: bool,
    /// ZRAM compression ratio (if available)
    pub zram_ratio: Option<f64>,
    /// Health score (0-100), `None` when memory usage was not read
    pub health_score: Option<u32>,
}

/// Get a quick memory summary
pub fn memory_summary() -> Result<MemorySummary> {
    let monitor = MemoryMonitor::new()?;

    Ok(MemorySummary {
        memory_percent: monitor.memory.usage_percent(),
        swap_percent: monitor.swap.usage_percent(),
        pressure: monitor.pressure(),
        total_memory: monitor.memory.total,
        available_memory: monitor.memory.available,
        total_swap: monitor.swap.total,
        used_swap: monitor.swap.used,
        has_zram: monitor.swap.has_zram(),
        zram_ratio: monitor.swap.zram.as_ref().and_then(|z| z.compression_ratio),
        health_score: monitor.health_score(),
    })
}

/// Format bytes as human-readable string
pub fn format_bytes(bytes: u64) -> String {
    SwapDevice::format_size(bytes)
}

/// Parse `/proc/vmstat` into the counters. A counter whose line is absent or does not
/// parse stays `None`.
///
/// `pages_allocated` sums every `pgalloc_*` zone. It used to take `pgalloc_normal`
/// alone, which is one zone; the DMA, DMA32, movable and device zones are pages too.
#[cfg(target_os = "linux")]
fn parse_vmstat(content: &str) -> MemoryStats {
    let mut stats = MemoryStats::default();
    let mut allocated: Option<u64> = None;

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let Ok(value) = parts[1].parse::<u64>() else {
                continue;
            };
            if parts[0].starts_with("pgalloc_") {
                allocated = Some(allocated.unwrap_or(0).saturating_add(value));
                continue;
            }
            match parts[0] {
                "pgfault" => stats.page_faults_minor = Some(value),
                "pgmajfault" => stats.page_faults_major = Some(value),
                "pswpin" => stats.swap_in = Some(value),
                "pswpout" => stats.swap_out = Some(value),
                "pgfree" => stats.pages_freed = Some(value),
                "oom_kill" => stats.oom_kills = Some(value),
                _ => {}
            }
        }
    }

    stats.pages_allocated = allocated;
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pressure() {
        assert_eq!(MemoryPressure::from_usage(50.0), MemoryPressure::Low);
        assert_eq!(MemoryPressure::from_usage(70.0), MemoryPressure::Medium);
        assert_eq!(MemoryPressure::from_usage(90.0), MemoryPressure::High);
        assert_eq!(MemoryPressure::from_usage(99.0), MemoryPressure::Critical);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_swap_device_usage() {
        let device = SwapDevice {
            path: "/swapfile".to_string(),
            swap_type: SwapType::File,
            total_bytes: Some(1024 * 1024 * 1024),
            used_bytes: Some(512 * 1024 * 1024),
            priority: Some(0),
            active: true,
        };

        assert_eq!(device.usage_percent(), Some(50.0));
        assert_eq!(device.available_bytes(), Some(512 * 1024 * 1024));
    }

    #[test]
    fn test_unread_usage_is_none() {
        // A device whose sizes were not read has no usage; zero is not a stand-in.
        let device = SwapDevice {
            path: "/swapfile".to_string(),
            swap_type: SwapType::File,
            total_bytes: None,
            used_bytes: None,
            priority: None,
            active: true,
        };
        assert_eq!(device.usage_percent(), None);
        assert_eq!(device.available_bytes(), None);

        // No swap at all has no usage either: there is nothing to be a percentage of.
        let swap = SwapInfo {
            total: 0,
            used: Some(0),
            free: Some(0),
            devices: Vec::new(),
            zram: None,
        };
        assert_eq!(swap.usage_percent(), None);
        assert!(!swap.has_swap());
    }

    #[test]
    fn swap_usage_is_unknown_when_used_was_not_established() {
        // Swap exists, but the used figure was not read (Windows withholds it when
        // free underflows). A percentage from that would be a guess.
        let swap = SwapInfo {
            total: 2048,
            used: None,
            free: None,
            devices: Vec::new(),
            zram: None,
        };
        assert!(swap.has_swap());
        assert_eq!(swap.usage_percent(), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn vmstat_allocations_sum_every_zone() {
        // Captured shape of /proc/vmstat; the zone counters are the allocation total.
        let stats = parse_vmstat(concat!(
            "pgfault 900\npgmajfault 3\npgalloc_dma 10\npgalloc_dma32 20\n",
            "pgalloc_normal 300\npgalloc_movable 4\npgfree 500\noom_kill 0\n",
        ));
        assert_eq!(stats.pages_allocated, Some(334));
        assert_eq!(stats.page_faults_minor, Some(900));
        assert_eq!(stats.page_faults_major, Some(3));
        assert_eq!(stats.pages_freed, Some(500));
        assert_eq!(stats.oom_kills, Some(0));
        assert_eq!(stats.swap_in, None, "absent from the text, so unread");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn vmstat_without_allocation_lines_leaves_them_unread() {
        let stats = parse_vmstat("pgfault 5\n");
        assert_eq!(stats.pages_allocated, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn every_zram_block_device_is_a_zram_device() {
        assert!(MemoryMonitor::is_zram_device("zram0"));
        assert!(MemoryMonitor::is_zram_device("zram1"));
        assert!(MemoryMonitor::is_zram_device("zram12"));
        // Names that begin with the letters but are not zram devices.
        assert!(!MemoryMonitor::is_zram_device("zram"));
        assert!(!MemoryMonitor::is_zram_device("zram0p1"));
        assert!(!MemoryMonitor::is_zram_device("zramx"));
        assert!(!MemoryMonitor::is_zram_device("sda"));
        assert!(!MemoryMonitor::is_zram_device("nvme0n1"));
    }
}
