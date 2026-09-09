//! Block I/O scheduler monitoring and analysis.
//!
//! Detects and reports active I/O schedulers per block device (mq-deadline,
//! BFQ, kyber, none), queue depth, rotational vs SSD classification,
//! I/O stats (IOPS, throughput, latency), and scheduler tuning parameters.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/block/*/queue/scheduler`, `/sys/block/*/stat`, `/proc/diskstats`
//! - **Windows**: I/O priority via performance counters
//! - **macOS**: IOKit disk stats

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// I/O scheduler type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IoSchedulerType {
    /// Multi-Queue Deadline.
    MqDeadline,
    /// Budget Fair Queueing.
    Bfq,
    /// Kyber (low-latency).
    Kyber,
    /// No scheduler (none/noop — direct dispatch).
    None,
    /// Legacy CFQ (pre-5.0 kernels).
    Cfq,
    /// Legacy Deadline.
    Deadline,
    /// Legacy Noop.
    Noop,
    Unknown,
}

impl std::fmt::Display for IoSchedulerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MqDeadline => write!(f, "mq-deadline"),
            Self::Bfq => write!(f, "bfq"),
            Self::Kyber => write!(f, "kyber"),
            Self::None => write!(f, "none"),
            Self::Cfq => write!(f, "cfq"),
            Self::Deadline => write!(f, "deadline"),
            Self::Noop => write!(f, "noop"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Block device I/O statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoStats {
    /// Reads completed.
    pub reads_completed: u64,
    /// Reads merged.
    pub reads_merged: u64,
    /// Sectors read.
    pub sectors_read: u64,
    /// Time reading (ms).
    pub read_time_ms: u64,
    /// Writes completed.
    pub writes_completed: u64,
    /// Writes merged.
    pub writes_merged: u64,
    /// Sectors written.
    pub sectors_written: u64,
    /// Time writing (ms).
    pub write_time_ms: u64,
    /// Current I/O in flight.
    pub in_flight: u64,
    /// Time doing I/O (ms).
    pub io_time_ms: u64,
    /// Weighted time doing I/O (ms).
    pub weighted_io_time_ms: u64,
    /// Discards completed (TRIM).
    pub discards_completed: u64,
}

/// Block device I/O scheduler info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDeviceIo {
    /// Device name (e.g. "sda", "nvme0n1").
    pub name: String,
    /// Active I/O scheduler.
    pub scheduler: IoSchedulerType,
    /// Available schedulers.
    pub available_schedulers: Vec<IoSchedulerType>,
    /// Whether this is a rotational (HDD) device. `None` when
    /// `queue/rotational` was not readable — which is not the same as an SSD.
    pub rotational: Option<bool>,
    /// Queue depth (nr_requests). `None` when not readable.
    pub queue_depth: Option<u32>,
    /// Logical block size. `None` when not readable — never 512.
    pub logical_block_size: Option<u32>,
    /// Physical block size. `None` when not readable — never 512. A 4Kn drive
    /// reports 4096 for both and an Advanced Format drive 512 over 4096, which
    /// is the distinction these two fields exist to carry.
    pub physical_block_size: Option<u32>,
    /// Whether device supports discard (TRIM). `None` when not readable.
    pub discard_support: Option<bool>,
    /// I/O statistics.
    pub stats: IoStats,
    /// Device size in bytes.
    pub size_bytes: u64,
    /// Model / product name.
    pub model: String,
}

impl BlockDeviceIo {
    /// Read operations per second of time the device spent reading, over the
    /// whole of its uptime. `None` when it has not spent any.
    ///
    /// **Two things this is not.** It is not a current rate: `/sys/block/*/stat`
    /// gives counters cumulative since boot, so this is a lifetime average and
    /// will barely move under a burst of load. A caller wanting current IOPS
    /// must read [`IoStats`] twice and difference it. And the denominator is
    /// time *spent* reading rather than elapsed time, so the figure is IOPS
    /// while busy, which on a mostly-idle device is far above its IOPS.
    ///
    /// `None` rather than the `0.0` this returned before, because
    /// `read_time_ms == 0` has two meanings and neither is "this device does
    /// zero reads per second": either it has genuinely never served a read, or
    /// the counters were never populated -- which is every platform but Linux,
    /// where [`IoStats`] is constructed with zeros.
    pub fn read_iops_from_stats(&self) -> Option<f64> {
        if self.stats.read_time_ms == 0 {
            return None;
        }
        Some(self.stats.reads_completed as f64 / (self.stats.read_time_ms as f64 / 1000.0))
    }

    /// Write operations per second of time spent writing. See
    /// [`Self::read_iops_from_stats`] for what this figure is and is not.
    pub fn write_iops_from_stats(&self) -> Option<f64> {
        if self.stats.write_time_ms == 0 {
            return None;
        }
        Some(self.stats.writes_completed as f64 / (self.stats.write_time_ms as f64 / 1000.0))
    }

    /// Read throughput in MB/s over time spent reading. See
    /// [`Self::read_iops_from_stats`] for what this figure is and is not.
    pub fn read_throughput_mbs(&self) -> Option<f64> {
        if self.stats.read_time_ms == 0 {
            return None;
        }
        Some(
            (self.stats.sectors_read as f64 * 512.0)
                / (self.stats.read_time_ms as f64 / 1000.0)
                / 1_000_000.0,
        )
    }

    /// Whether the scheduler is optimal for this device type, or `None` when
    /// `rotational` was not read — the answer depends entirely on it, so there
    /// is no verdict to give without it.
    pub fn scheduler_optimal(&self) -> Option<bool> {
        let optimal = if self.rotational? {
            // HDDs benefit from BFQ or mq-deadline
            matches!(
                self.scheduler,
                IoSchedulerType::Bfq | IoSchedulerType::MqDeadline
            )
        } else {
            // SSDs/NVMe work best with none, mq-deadline, or kyber
            matches!(
                self.scheduler,
                IoSchedulerType::None | IoSchedulerType::MqDeadline | IoSchedulerType::Kyber
            )
        };
        Some(optimal)
    }
}

/// I/O scheduler overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoSchedulerOverview {
    /// All block devices.
    pub devices: Vec<BlockDeviceIo>,
    /// Total devices.
    pub total_devices: u32,
    /// Devices with non-optimal schedulers.
    pub non_optimal_count: u32,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// I/O scheduler monitor.
pub struct IoSchedulerMonitor {
    overview: IoSchedulerOverview,
}

impl IoSchedulerMonitor {
    /// Create a new I/O scheduler monitor.
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
    pub fn overview(&self) -> &IoSchedulerOverview {
        &self.overview
    }

    /// Get devices.
    pub fn devices(&self) -> &[BlockDeviceIo] {
        &self.overview.devices
    }

    /// Get device by name.
    pub fn device(&self, name: &str) -> Option<&BlockDeviceIo> {
        self.overview.devices.iter().find(|d| d.name == name)
    }

    #[cfg(target_os = "linux")]
    fn scan() -> Result<IoSchedulerOverview, IronError> {
        let block_path = std::path::Path::new("/sys/block");
        let mut devices = Vec::new();

        if !block_path.exists() {
            return Ok(IoSchedulerOverview {
                devices: Vec::new(),
                total_devices: 0,
                non_optimal_count: 0,
                recommendations: Vec::new(),
            });
        }

        let entries = std::fs::read_dir(block_path).map_err(IronError::Io)?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip loop, ram, dm devices
            if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("dm-") {
                continue;
            }

            let dev_path = entry.path();
            let queue_path = dev_path.join("queue");

            if !queue_path.exists() {
                continue;
            }

            // Read scheduler
            let (scheduler, available) = Self::read_scheduler(&queue_path);

            // Rotational
            let rotational = Self::read_sysfs_u32(&queue_path.join("rotational")).map(|v| v == 1);

            // Queue depth
            let queue_depth = Self::read_sysfs_u32(&queue_path.join("nr_requests"));

            // Block sizes
            let logical_block_size = Self::read_sysfs_u32(&queue_path.join("logical_block_size"));
            let physical_block_size = Self::read_sysfs_u32(&queue_path.join("physical_block_size"));

            // Discard support
            let discard_support =
                Self::read_sysfs_u32(&queue_path.join("discard_max_bytes")).map(|v| v > 0);

            // Stats
            let stats = Self::read_stats(&dev_path);

            // Size (in 512-byte sectors)
            let size_sectors = Self::read_sysfs_u64(&dev_path.join("size")).unwrap_or(0);
            let size_bytes = size_sectors * 512;

            // Model
            let model = std::fs::read_to_string(dev_path.join("device/model"))
                .ok()
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            devices.push(BlockDeviceIo {
                name,
                scheduler,
                available_schedulers: available,
                rotational,
                queue_depth,
                logical_block_size,
                physical_block_size,
                discard_support,
                stats,
                size_bytes,
                model,
            });
        }

        let total = devices.len() as u32;
        // A device whose `rotational` was not read is counted neither optimal
        // nor not: there is nothing to compare its scheduler against.
        let non_optimal = devices
            .iter()
            .filter(|d| d.scheduler_optimal() == Some(false))
            .count() as u32;

        let mut recommendations = Vec::new();
        for dev in &devices {
            if dev.scheduler_optimal() == Some(false) {
                let rotational = dev.rotational == Some(true);
                let suggested = if rotational { "bfq" } else { "none" };
                recommendations.push(format!(
                    "{}: using '{}' scheduler; '{}' may be better for {} devices",
                    dev.name,
                    dev.scheduler,
                    suggested,
                    if rotational { "rotational" } else { "SSD/NVMe" }
                ));
            }
        }

        Ok(IoSchedulerOverview {
            devices,
            total_devices: total,
            non_optimal_count: non_optimal,
            recommendations,
        })
    }

    #[cfg(target_os = "linux")]
    fn read_scheduler(queue_path: &std::path::Path) -> (IoSchedulerType, Vec<IoSchedulerType>) {
        let content = std::fs::read_to_string(queue_path.join("scheduler")).unwrap_or_default();

        let mut active = IoSchedulerType::Unknown;
        let mut available = Vec::new();

        for part in content.split_whitespace() {
            let (name, is_active) = if part.starts_with('[') && part.ends_with(']') {
                (part.trim_matches(|c| c == '[' || c == ']'), true)
            } else {
                (part, false)
            };

            let sched = match name {
                "mq-deadline" => IoSchedulerType::MqDeadline,
                "bfq" => IoSchedulerType::Bfq,
                "kyber" => IoSchedulerType::Kyber,
                "none" => IoSchedulerType::None,
                "cfq" => IoSchedulerType::Cfq,
                "deadline" => IoSchedulerType::Deadline,
                "noop" => IoSchedulerType::Noop,
                _ => IoSchedulerType::Unknown,
            };

            available.push(sched);
            if is_active {
                active = sched;
            }
        }

        (active, available)
    }

    #[cfg(target_os = "linux")]
    fn read_stats(dev_path: &std::path::Path) -> IoStats {
        let content = std::fs::read_to_string(dev_path.join("stat")).unwrap_or_default();
        let parts: Vec<u64> = content
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();

        IoStats {
            reads_completed: *parts.first().unwrap_or(&0),
            reads_merged: *parts.get(1).unwrap_or(&0),
            sectors_read: *parts.get(2).unwrap_or(&0),
            read_time_ms: *parts.get(3).unwrap_or(&0),
            writes_completed: *parts.get(4).unwrap_or(&0),
            writes_merged: *parts.get(5).unwrap_or(&0),
            sectors_written: *parts.get(6).unwrap_or(&0),
            write_time_ms: *parts.get(7).unwrap_or(&0),
            in_flight: *parts.get(8).unwrap_or(&0),
            io_time_ms: *parts.get(9).unwrap_or(&0),
            weighted_io_time_ms: *parts.get(10).unwrap_or(&0),
            discards_completed: *parts.get(11).unwrap_or(&0),
        }
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_u32(path: &std::path::Path) -> Option<u32> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_u64(path: &std::path::Path) -> Option<u64> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    #[cfg(target_os = "windows")]
    fn scan() -> Result<IoSchedulerOverview, IronError> {
        // Returning an empty overview here made "this platform cannot
        // answer" indistinguishable from "this machine has none", which
        // is the same defect RAPL shipped once. The reason travels with
        // the error so a caller can report it.
        Err(IronError::UnsupportedPlatform(
            "the block I/O scheduler is read from `/sys/block/*/queue/scheduler`, which Windows does not expose"
                .into(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn scan() -> Result<IoSchedulerOverview, IronError> {
        // Returning an empty overview here made "this platform cannot
        // answer" indistinguishable from "this machine has none", which
        // is the same defect RAPL shipped once. The reason travels with
        // the error so a caller can report it.
        Err(IronError::UnsupportedPlatform(
            "the block I/O scheduler is read from `/sys/block/*/queue/scheduler`, which macOS does not expose"
                .into(),
        ))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn scan() -> Result<IoSchedulerOverview, IronError> {
        // Returning an empty overview here made "this platform cannot
        // answer" indistinguishable from "this machine has none", which
        // is the same defect RAPL shipped once. The reason travels with
        // the error so a caller can report it.
        Err(IronError::UnsupportedPlatform(
            "the block I/O scheduler is read from `/sys/block/*/queue/scheduler`, which this platform does not expose"
                .into(),
        ))
    }
}

impl Default for IoSchedulerMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            overview: IoSchedulerOverview {
                devices: Vec::new(),
                total_devices: 0,
                non_optimal_count: 0,
                recommendations: Vec::new(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_display() {
        assert_eq!(IoSchedulerType::MqDeadline.to_string(), "mq-deadline");
        assert_eq!(IoSchedulerType::Bfq.to_string(), "bfq");
        assert_eq!(IoSchedulerType::None.to_string(), "none");
    }

    #[test]
    fn test_optimal_ssd() {
        let dev = BlockDeviceIo {
            name: "nvme0n1".into(),
            scheduler: IoSchedulerType::None,
            available_schedulers: vec![IoSchedulerType::None, IoSchedulerType::MqDeadline],
            rotational: Some(false),
            queue_depth: Some(256),
            logical_block_size: Some(512),
            physical_block_size: Some(4096),
            discard_support: Some(true),
            stats: IoStats {
                reads_completed: 0,
                reads_merged: 0,
                sectors_read: 0,
                read_time_ms: 0,
                writes_completed: 0,
                writes_merged: 0,
                sectors_written: 0,
                write_time_ms: 0,
                in_flight: 0,
                io_time_ms: 0,
                weighted_io_time_ms: 0,
                discards_completed: 0,
            },
            size_bytes: 1_000_000_000_000,
            model: "Samsung 990 Pro".into(),
        };
        assert_eq!(dev.scheduler_optimal(), Some(true));
    }

    #[test]
    fn test_non_optimal_hdd() {
        let dev = BlockDeviceIo {
            name: "sda".into(),
            scheduler: IoSchedulerType::None,
            available_schedulers: vec![IoSchedulerType::None, IoSchedulerType::Bfq],
            rotational: Some(true),
            queue_depth: Some(128),
            logical_block_size: Some(512),
            physical_block_size: Some(512),
            discard_support: Some(false),
            stats: IoStats {
                reads_completed: 0,
                reads_merged: 0,
                sectors_read: 0,
                read_time_ms: 0,
                writes_completed: 0,
                writes_merged: 0,
                sectors_written: 0,
                write_time_ms: 0,
                in_flight: 0,
                io_time_ms: 0,
                weighted_io_time_ms: 0,
                discards_completed: 0,
            },
            size_bytes: 2_000_000_000_000,
            model: "WDC WD20EARS".into(),
        };
        assert_eq!(dev.scheduler_optimal(), Some(false));
    }

    #[test]
    fn test_monitor_default() {
        let monitor = IoSchedulerMonitor::default();
        let _overview = monitor.overview();
    }

    #[test]
    fn test_serialization() {
        let stats = IoStats {
            reads_completed: 1000,
            reads_merged: 50,
            sectors_read: 80000,
            read_time_ms: 500,
            writes_completed: 2000,
            writes_merged: 100,
            sectors_written: 160000,
            write_time_ms: 800,
            in_flight: 2,
            io_time_ms: 1000,
            weighted_io_time_ms: 1200,
            discards_completed: 10,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("1000"));
        let _: IoStats = serde_json::from_str(&json).unwrap();
    }
    /// A device whose `queue/rotational` was not readable has no optimal
    /// scheduler, because which scheduler is optimal is defined entirely by
    /// that flag. Before 6.0.0 an unreadable flag read as `0` and the device
    /// was judged as an SSD.
    #[test]
    fn an_unread_rotational_flag_yields_no_verdict() {
        let dev = BlockDeviceIo {
            name: "sdz".into(),
            scheduler: IoSchedulerType::Bfq,
            available_schedulers: vec![IoSchedulerType::Bfq],
            rotational: None,
            queue_depth: None,
            logical_block_size: None,
            physical_block_size: None,
            discard_support: None,
            stats: IoStats {
                reads_completed: 0,
                reads_merged: 0,
                sectors_read: 0,
                read_time_ms: 0,
                writes_completed: 0,
                writes_merged: 0,
                sectors_written: 0,
                write_time_ms: 0,
                in_flight: 0,
                io_time_ms: 0,
                weighted_io_time_ms: 0,
                discards_completed: 0,
            },
            size_bytes: 0,
            model: String::new(),
        };
        assert_eq!(dev.scheduler_optimal(), None);
    }
}
