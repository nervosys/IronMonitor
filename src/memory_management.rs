// memory_management.rs - Memory and swap management module for simon
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
    /// Total size in bytes
    pub total_bytes: u64,
    /// Used size in bytes
    pub used_bytes: u64,
    /// Priority (higher = preferred)
    pub priority: i32,
    /// Whether the swap is currently active
    pub active: bool,
}

impl SwapDevice {
    /// Get usage percentage
    pub fn usage_percent(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f64 / self.total_bytes as f64) * 100.0
        }
    }

    /// Get available bytes
    pub fn available_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.used_bytes)
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZramInfo {
    /// Device name (e.g., zram0)
    pub device: String,
    /// Total disksize in bytes
    pub disksize: u64,
    /// Compressed data size
    pub compressed_bytes: u64,
    /// Original (uncompressed) data size
    pub original_bytes: u64,
    /// Memory used for compressed storage
    pub mem_used_bytes: u64,
    /// Compression algorithm
    pub algorithm: String,
    /// Number of streams
    pub streams: u32,
    /// Compression ratio (original/compressed)
    pub compression_ratio: f64,
}

impl ZramInfo {
    /// Calculate memory savings from compression
    pub fn memory_savings(&self) -> u64 {
        self.original_bytes.saturating_sub(self.mem_used_bytes)
    }

    /// Get memory savings percentage
    pub fn savings_percent(&self) -> f64 {
        if self.original_bytes == 0 {
            0.0
        } else {
            (self.memory_savings() as f64 / self.original_bytes as f64) * 100.0
        }
    }
}

/// Detailed memory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    /// Total physical RAM in bytes
    pub total: u64,
    /// Available memory in bytes (includes cached)
    pub available: u64,
    /// Free memory in bytes (not counting cached)
    pub free: u64,
    /// Used memory in bytes
    pub used: u64,
    /// Cached memory in bytes
    pub cached: u64,
    /// Buffer memory in bytes
    pub buffers: u64,
    /// Shared memory in bytes
    pub shared: u64,
    /// Slab reclaimable memory (Linux)
    pub slab_reclaimable: u64,
    /// Active memory
    pub active: u64,
    /// Inactive memory
    pub inactive: u64,
    /// Dirty pages awaiting write
    pub dirty: u64,
    /// Memory currently being written back
    pub writeback: u64,
    /// Mapped memory (memory-mapped files)
    pub mapped: u64,
    /// Memory locked in RAM
    pub mlocked: u64,
    /// Huge pages total
    pub hugepages_total: u64,
    /// Huge pages free
    pub hugepages_free: u64,
    /// Huge page size
    pub hugepage_size: u64,
}

impl MemoryInfo {
    /// Get usage percentage
    pub fn usage_percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f64 / self.total as f64) * 100.0
        }
    }

    /// Get memory pressure level
    pub fn pressure(&self) -> MemoryPressure {
        MemoryPressure::from_usage(self.usage_percent())
    }

    /// Get effective available memory (includes reclaimable)
    pub fn effective_available(&self) -> u64 {
        self.available + self.slab_reclaimable
    }
}

/// Total swap information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapInfo {
    /// Total swap size in bytes
    pub total: u64,
    /// Used swap in bytes
    pub used: u64,
    /// Free swap in bytes
    pub free: u64,
    /// List of swap devices
    pub devices: Vec<SwapDevice>,
    /// ZRAM information if available
    pub zram: Option<ZramInfo>,
}

impl SwapInfo {
    /// Get usage percentage
    pub fn usage_percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f64 / self.total as f64) * 100.0
        }
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

/// Memory statistics over time
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Page faults (minor)
    pub page_faults_minor: u64,
    /// Page faults (major - required disk IO)
    pub page_faults_major: u64,
    /// Pages swapped in
    pub swap_in: u64,
    /// Pages swapped out
    pub swap_out: u64,
    /// Pages allocated
    pub pages_allocated: u64,
    /// Pages freed
    pub pages_freed: u64,
    /// OOM killer invocations
    pub oom_kills: u64,
}

/// Process memory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMemory {
    /// Process ID
    pub pid: u32,
    /// Process name
    pub name: String,
    /// Resident Set Size (physical memory)
    pub rss: u64,
    /// Virtual memory size
    pub vms: u64,
    /// Shared memory
    pub shared: u64,
    /// Private memory
    pub private: u64,
    /// Swap usage
    pub swap: u64,
    /// Percentage of total memory
    pub memory_percent: f64,
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
            memory: MemoryInfo {
                total: 0,
                available: 0,
                free: 0,
                used: 0,
                cached: 0,
                buffers: 0,
                shared: 0,
                slab_reclaimable: 0,
                active: 0,
                inactive: 0,
                dirty: 0,
                writeback: 0,
                mapped: 0,
                mlocked: 0,
                hugepages_total: 0,
                hugepages_free: 0,
                hugepage_size: 0,
            },
            swap: SwapInfo {
                total: 0,
                used: 0,
                free: 0,
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

    /// Get memory pressure level
    pub fn pressure(&self) -> MemoryPressure {
        self.memory.pressure()
    }

    /// Check if swap is being heavily used
    pub fn swap_pressure(&self) -> MemoryPressure {
        MemoryPressure::from_usage(self.swap.usage_percent())
    }

    /// Get system health based on memory and swap
    pub fn health_score(&self) -> u32 {
        let mem_score = match self.pressure() {
            MemoryPressure::Low => 50,
            MemoryPressure::Medium => 35,
            MemoryPressure::High => 20,
            MemoryPressure::Critical => 5,
        };

        let swap_score = if !self.swap.has_swap() {
            // No swap configured - neutral
            25
        } else {
            match self.swap_pressure() {
                MemoryPressure::Low => 25,
                MemoryPressure::Medium => 20,
                MemoryPressure::High => 10,
                MemoryPressure::Critical => 0,
            }
        };

        // Bonus for ZRAM
        let zram_bonus = if let Some(ref zram) = self.swap.zram {
            if zram.compression_ratio > 2.0 {
                10
            } else if zram.compression_ratio > 1.5 {
                5
            } else {
                0
            }
        } else {
            0
        };

        (mem_score + swap_score + zram_bonus).min(100)
    }

    // ==================== Linux Implementation ====================

    #[cfg(target_os = "linux")]
    fn linux_read_meminfo(&mut self) -> Result<()> {
        use std::fs;

        let content = fs::read_to_string("/proc/meminfo").unwrap_or_default();

        let mut mem = HashMap::new();
        for line in content.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim().replace(" kB", "");
                if let Ok(kb) = value.parse::<u64>() {
                    mem.insert(key.to_string(), kb * 1024);
                }
            }
        }

        self.memory.total = *mem.get("MemTotal").unwrap_or(&0);
        self.memory.free = *mem.get("MemFree").unwrap_or(&0);
        self.memory.available = *mem.get("MemAvailable").unwrap_or(&self.memory.free);
        self.memory.buffers = *mem.get("Buffers").unwrap_or(&0);
        self.memory.cached = *mem.get("Cached").unwrap_or(&0);
        self.memory.shared = *mem.get("Shmem").unwrap_or(&0);
        self.memory.slab_reclaimable = *mem.get("SReclaimable").unwrap_or(&0);
        self.memory.active = *mem.get("Active").unwrap_or(&0);
        self.memory.inactive = *mem.get("Inactive").unwrap_or(&0);
        self.memory.dirty = *mem.get("Dirty").unwrap_or(&0);
        self.memory.writeback = *mem.get("Writeback").unwrap_or(&0);
        self.memory.mapped = *mem.get("Mapped").unwrap_or(&0);
        self.memory.mlocked = *mem.get("Mlocked").unwrap_or(&0);
        self.memory.hugepages_total = *mem.get("HugePages_Total").unwrap_or(&0);
        self.memory.hugepages_free = *mem.get("HugePages_Free").unwrap_or(&0);
        self.memory.hugepage_size = *mem.get("Hugepagesize").unwrap_or(&0);

        // Calculate used memory
        self.memory.used = self
            .memory
            .total
            .saturating_sub(self.memory.free)
            .saturating_sub(self.memory.buffers)
            .saturating_sub(self.memory.cached);

        // Swap from meminfo
        self.swap.total = *mem.get("SwapTotal").unwrap_or(&0);
        self.swap.free = *mem.get("SwapFree").unwrap_or(&0);
        self.swap.used = self.swap.total.saturating_sub(self.swap.free);

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_read_swap_devices(&mut self) -> Result<()> {
        use std::fs;

        self.swap.devices.clear();

        let content = fs::read_to_string("/proc/swaps").unwrap_or_default();

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
                let total = parts[2].parse::<u64>().unwrap_or(0) * 1024;
                let used = parts[3].parse::<u64>().unwrap_or(0) * 1024;
                let priority = parts[4].parse::<i32>().unwrap_or(0);

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

        // Check for zram devices
        let zram_path = Path::new("/sys/block/zram0");
        if !zram_path.exists() {
            self.swap.zram = None;
            return Ok(());
        }

        let read_file = |name: &str| -> u64 {
            fs::read_to_string(zram_path.join(name))
                .unwrap_or_default()
                .trim()
                .parse()
                .unwrap_or(0)
        };

        let read_string = |name: &str| -> String {
            fs::read_to_string(zram_path.join(name))
                .unwrap_or_default()
                .trim()
                .to_string()
        };

        let disksize = read_file("disksize");
        let compressed = read_file("compr_data_size");
        let original = read_file("orig_data_size");
        let mem_used = read_file("mem_used_total");
        let algorithm = read_string("comp_algorithm")
            .split_whitespace()
            .find(|s| s.starts_with('['))
            .map(|s| s.trim_matches(|c| c == '[' || c == ']').to_string())
            .unwrap_or_else(|| read_string("comp_algorithm"));

        let compression_ratio = if compressed > 0 {
            original as f64 / compressed as f64
        } else {
            1.0
        };

        self.swap.zram = Some(ZramInfo {
            device: "zram0".to_string(),
            disksize,
            compressed_bytes: compressed,
            original_bytes: original,
            mem_used_bytes: mem_used,
            algorithm,
            streams: 0, // Would need additional parsing
            compression_ratio,
        });

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_read_vmstat(&mut self) -> Result<()> {
        use std::fs;

        let content = fs::read_to_string("/proc/vmstat").unwrap_or_default();

        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let value = parts[1].parse::<u64>().unwrap_or(0);
                match parts[0] {
                    "pgfault" => self.stats.page_faults_minor = value,
                    "pgmajfault" => self.stats.page_faults_major = value,
                    "pswpin" => self.stats.swap_in = value,
                    "pswpout" => self.stats.swap_out = value,
                    "pgalloc_normal" => self.stats.pages_allocated = value,
                    "pgfree" => self.stats.pages_freed = value,
                    "oom_kill" => self.stats.oom_kills = value,
                    _ => {}
                }
            }
        }

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

                        let mut rss = 0u64;
                        let mut vms = 0u64;
                        let mut swap = 0u64;

                        for line in status.lines() {
                            if let Some((key, value)) = line.split_once(':') {
                                let value = value.trim().replace(" kB", "");
                                let kb = value.parse::<u64>().unwrap_or(0) * 1024;
                                match key {
                                    "VmRSS" => rss = kb,
                                    "VmSize" => vms = kb,
                                    "VmSwap" => swap = kb,
                                    _ => {}
                                }
                            }
                        }

                        if rss > 0 {
                            let memory_percent = if self.memory.total > 0 {
                                (rss as f64 / self.memory.total as f64) * 100.0
                            } else {
                                0.0
                            };

                            processes.push(ProcessMemory {
                                pid,
                                name,
                                rss,
                                vms,
                                shared: 0,
                                private: rss,
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

            if GlobalMemoryStatusEx(&mut status).is_ok() {
                self.memory.total = status.ullTotalPhys;
                self.memory.available = status.ullAvailPhys;
                self.memory.free = status.ullAvailPhys;
                self.memory.used = status.ullTotalPhys.saturating_sub(status.ullAvailPhys);

                // Windows doesn't separate cached/buffers in the same way
                // Estimate cached as available - free (usually 0 on Windows)
                self.memory.cached = 0;
                self.memory.buffers = 0;
            }
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

            if GlobalMemoryStatusEx(&mut status).is_ok() {
                // Page file info
                self.swap.total = status.ullTotalPageFile.saturating_sub(status.ullTotalPhys);
                self.swap.free = status.ullAvailPageFile;
                self.swap.used = self
                    .swap
                    .total
                    .saturating_sub(status.ullAvailPageFile.saturating_sub(status.ullAvailPhys));

                // Try to get pagefile path from WMI
                self.windows_read_pagefile_devices();
            }
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
                    total_bytes: self.swap.total,
                    used_bytes: self.swap.used,
                    priority: 0,
                    active: true,
                });
            }
        }

        // Fallback: try PowerShell
        if self.swap.devices.is_empty() {
            let output = Command::new("powershell")
                .args([
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
                            total_bytes: self.swap.total,
                            used_bytes: self.swap.used,
                            priority: 0,
                            active: true,
                        });
                    }
                }
            }
        }

        // Final fallback
        if self.swap.devices.is_empty() && self.swap.total > 0 {
            self.swap.devices.push(SwapDevice {
                path: "C:\\pagefile.sys".to_string(),
                swap_type: SwapType::File,
                total_bytes: self.swap.total,
                used_bytes: self.swap.used,
                priority: 0,
                active: true,
            });
        }
    }

    #[cfg(windows)]
    fn windows_top_processes(&self, limit: usize) -> Vec<ProcessMemory> {
        use std::process::Command;

        let mut processes = Vec::new();

        // Use PowerShell to get process memory info
        let output = Command::new("powershell")
            .args([
                "-Command",
                "Get-Process | Sort-Object WorkingSet64 -Descending | Select-Object -First 50 Id,Name,WorkingSet64,VirtualMemorySize64,PagedMemorySize64 | ConvertTo-Csv -NoTypeInformation"
            ])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1) {
                // Skip header
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 5 {
                    let pid = parts[0].trim_matches('"').parse::<u32>().unwrap_or(0);
                    let name = parts[1].trim_matches('"').to_string();
                    let rss = parts[2].trim_matches('"').parse::<u64>().unwrap_or(0);
                    let vms = parts[3].trim_matches('"').parse::<u64>().unwrap_or(0);
                    let paged = parts[4].trim_matches('"').parse::<u64>().unwrap_or(0);

                    let memory_percent = if self.memory.total > 0 {
                        (rss as f64 / self.memory.total as f64) * 100.0
                    } else {
                        0.0
                    };

                    processes.push(ProcessMemory {
                        pid,
                        name,
                        rss,
                        vms,
                        shared: 0,
                        private: rss,
                        swap: paged,
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
        let Some(vm) = crate::platform::macos::vm_stat() else {
            // Nothing was read, so nothing is reported. Filling these with
            // zeros would say the machine has no memory.
            return Ok(());
        };

        // `VmStat`'s helpers are in bytes and multiply by the page size it read.
        self.memory.free = vm.free * vm.page_size;
        self.memory.active = vm.active * vm.page_size;
        self.memory.inactive = vm.inactive * vm.page_size;
        self.memory.cached = vm.cached_bytes();

        if let Some(total) = crate::platform::macos::sysctl_u64("hw.memsize") {
            self.memory.total = total;
            self.memory.available = self.memory.free + self.memory.inactive;
            self.memory.used = self.memory.total.saturating_sub(self.memory.available);
        }

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
        if let Some(usage) = crate::platform::macos::swap_usage() {
            self.swap.total = usage.total;
            self.swap.used = usage.used;
            // `SwapUsage` carries total and used; `parse_swapusage` deliberately
            // ignores the printed free, since for swap it is the difference by
            // definition rather than an independent measurement.
            self.swap.free = usage.total.saturating_sub(usage.used);
        }

        // macOS uses dynamic swap files
        if self.swap.total > 0 {
            self.swap.devices.push(SwapDevice {
                path: "/private/var/vm/swapfile*".to_string(),
                swap_type: SwapType::File,
                total_bytes: self.swap.total,
                used_bytes: self.swap.used,
                priority: 0,
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
                    let pid = parts[0].parse::<u32>().unwrap_or(0);
                    let rss = parts[1].parse::<u64>().unwrap_or(0) * 1024;
                    let vms = parts[2].parse::<u64>().unwrap_or(0) * 1024;
                    let name = parts[3..].join(" ");

                    let memory_percent = if self.memory.total > 0 {
                        (rss as f64 / self.memory.total as f64) * 100.0
                    } else {
                        0.0
                    };

                    processes.push(ProcessMemory {
                        pid,
                        name,
                        rss,
                        vms,
                        shared: 0,
                        private: rss,
                        swap: 0,
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
            .map_err(|e| crate::error::SimonError::System(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::SimonError::System(
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
            .map_err(|e| crate::error::SimonError::System(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::SimonError::System(
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
                "-Command",
                "[System.GC]::Collect(); [System.GC]::WaitForPendingFinalizers()",
            ])
            .output()
            .map_err(|e| crate::error::SimonError::System(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::SimonError::System(
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
            .map_err(|e| crate::error::SimonError::System(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::SimonError::System(
                "Failed to purge memory (requires Developer Tools)".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for MemoryMonitor {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            memory: MemoryInfo {
                total: 0,
                available: 0,
                free: 0,
                used: 0,
                cached: 0,
                buffers: 0,
                shared: 0,
                slab_reclaimable: 0,
                active: 0,
                inactive: 0,
                dirty: 0,
                writeback: 0,
                mapped: 0,
                mlocked: 0,
                hugepages_total: 0,
                hugepages_free: 0,
                hugepage_size: 0,
            },
            swap: SwapInfo {
                total: 0,
                used: 0,
                free: 0,
                devices: Vec::new(),
                zram: None,
            },
            stats: MemoryStats::default(),
            vm_settings: HashMap::new(),
            last_update: std::time::Instant::now(),
        })
    }
}

/// Summary of memory state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySummary {
    /// Memory usage percentage
    pub memory_percent: f64,
    /// Swap usage percentage
    pub swap_percent: f64,
    /// Memory pressure level
    pub pressure: MemoryPressure,
    /// Total physical memory
    pub total_memory: u64,
    /// Available memory
    pub available_memory: u64,
    /// Total swap
    pub total_swap: u64,
    /// Used swap
    pub used_swap: u64,
    /// Has ZRAM
    pub has_zram: bool,
    /// ZRAM compression ratio (if available)
    pub zram_ratio: Option<f64>,
    /// Health score (0-100)
    pub health_score: u32,
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
        zram_ratio: monitor.swap.zram.as_ref().map(|z| z.compression_ratio),
        health_score: monitor.health_score(),
    })
}

/// Format bytes as human-readable string
pub fn format_bytes(bytes: u64) -> String {
    SwapDevice::format_size(bytes)
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
            total_bytes: 1024 * 1024 * 1024,
            used_bytes: 512 * 1024 * 1024,
            priority: 0,
            active: true,
        };

        assert_eq!(device.usage_percent(), 50.0);
        assert_eq!(device.available_bytes(), 512 * 1024 * 1024);
    }
}
