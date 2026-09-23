//! Unified traits and types for disk monitoring

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Disk device trait - common interface for all storage devices
pub trait DiskDevice: Send + Sync {
    /// Get device name (e.g., "nvme0n1", "sda", "PhysicalDrive0")
    fn name(&self) -> &str;

    /// Get disk type (NVMe, SATA SSD, HDD, etc.)
    fn disk_type(&self) -> DiskType;

    /// Get static device information
    fn info(&self) -> Result<DiskInfo, Error>;

    /// Get current I/O statistics
    fn io_stats(&self) -> Result<DiskIoStats, Error>;

    /// Get SMART attributes (if supported)
    fn smart_info(&self) -> Result<SmartInfo, Error> {
        Err(Error::NotSupported)
    }

    /// Get NVMe-specific information (if applicable)
    fn nvme_info(&self) -> Result<NvmeInfo, Error> {
        Err(Error::NotSupported)
    }

    /// Get filesystem information for mounted devices
    fn filesystem_info(&self) -> Result<Vec<FilesystemInfo>, Error> {
        Ok(Vec::new())
    }

    /// Get current temperature in Celsius (if available)
    fn temperature(&self) -> Result<Option<f32>, Error> {
        Ok(None)
    }

    /// Get overall health status
    fn health(&self) -> Result<DiskHealth, Error>;

    /// Get device path (e.g., "/dev/nvme0n1", "\\\\.\\PhysicalDrive0")
    fn device_path(&self) -> PathBuf;
}

/// Disk type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiskType {
    /// NVMe SSD
    NvmeSsd,
    /// SATA SSD
    SataSsd,
    /// SATA HDD
    SataHdd,
    /// SCSI device
    Scsi,
    /// USB-attached storage
    Usb,
    /// Virtual disk (VM, cloud)
    Virtual,
    /// Unknown type
    Unknown,
}

/// Static disk information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    /// Device name
    pub name: String,
    /// Device model
    pub model: String,
    /// Serial number
    pub serial: Option<String>,
    /// Firmware version
    pub firmware: Option<String>,
    /// Total capacity in bytes, or `None` where it was not read.
    ///
    /// **Optional for the same reason `block_size` below already is.** A disk
    /// reporting 0 bytes is not a plausible reading; it is the absence of one,
    /// and `macos.rs` had `size_bytes.unwrap_or(0)` producing exactly that.
    /// `ontology/resolve.rs` was already guarding `capacity > 0` to decide
    /// between a measured reading and an unavailable one — a sentinel it no
    /// longer needs.
    pub capacity: Option<u64>,
    /// Block size in bytes
    /// Block size in bytes, where it was read.
    ///
    /// A third field carrying the logical sector size, and it had the same
    /// `512, // Most common` default on Windows and macOS.
    pub block_size: Option<u32>,
    /// Disk type
    pub disk_type: DiskType,
    /// Interface type (e.g., "NVMe", "SATA", "USB", "SCSI", "PCIe")
    pub interface_type: Option<String>,
    /// Physical sector size in bytes
    /// Physical sector size in bytes, where it was read.
    ///
    /// The `Option` was already here and every reader filled it with a
    /// sentinel wrapped in `Some`: Linux `unwrap_or(512)` then `Some(..)`,
    /// Windows a flat `Some(512)`, macOS `Some(4096)`. 512 is the common value
    /// and not this drive's — a 4Kn drive reports 4096 for both, and an
    /// Advanced Format drive 512 logical over 4096 physical, which is exactly
    /// the distinction these two fields exist to carry.
    pub physical_sector_size: Option<u32>,
    /// Logical sector size in bytes
    /// Logical sector size in bytes, where it was read. See
    /// [`Self::physical_sector_size`].
    pub logical_sector_size: Option<u32>,
    /// Rotation speed (RPM) for HDDs
    pub rotation_rate: Option<u32>,
    /// Vendor
    pub vendor: Option<String>,
}

/// I/O Statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskIoStats {
    /// Total bytes read since boot, or `None` where the counter was not read.
    ///
    /// **Not the same as zero.** `macos.rs` returned this struct with all four
    /// counters hardcoded to `0` — "no bytes read since boot" for a disk that
    /// has certainly read some — because that platform exposes rates rather
    /// than cumulative totals and the type had no way to say so.
    pub read_bytes: Option<u64>,
    /// Total bytes written since boot, or `None` where unread.
    pub write_bytes: Option<u64>,
    /// Total read operations, or `None` where unread.
    pub read_ops: Option<u64>,
    /// Total write operations, or `None` where unread.
    pub write_ops: Option<u64>,
    /// Time spent reading (milliseconds)
    pub read_time_ms: Option<u64>,
    /// Time spent writing (milliseconds)
    pub write_time_ms: Option<u64>,
    /// Current queue depth
    pub queue_depth: Option<u32>,
    /// Average I/O latency (microseconds)
    pub avg_latency_us: Option<f64>,
    /// Read throughput (bytes/sec) - calculated from recent samples
    pub read_throughput: Option<u64>,
    /// Write throughput (bytes/sec) - calculated from recent samples
    pub write_throughput: Option<u64>,
}

impl DiskIoStats {
    /// Total operations, or `None` unless both counters were read.
    ///
    /// A total over a counter nobody read is not a total. Same rule as
    /// `Snapshot::total_rx_rate` and the ECC totals in `crate::edac`.
    pub fn total_ops(&self) -> Option<u64> {
        Some(self.read_ops? + self.write_ops?)
    }

    /// Total bytes transferred, or `None` unless both counters were read.
    pub fn total_bytes(&self) -> Option<u64> {
        Some(self.read_bytes? + self.write_bytes?)
    }
}
/// Where a [`SmartInfo`] came from.
///
/// Every field on `SmartInfo` is an `Option`, and a `None` used to be reported
/// as "the drive did not report a power cycle count" -- a claim about the drive,
/// made without knowing whether the drive had been asked. On this development
/// machine it was wrong for all four counters of the one USB drive:
/// unelevated, `Get-StorageReliabilityCounter` fails for **every** disk here
/// ("Access to a CIM resource was not available to the client"), so nothing had
/// asked the drive anything.
///
/// The three sources answer different questions and fail for different reasons,
/// and the resolver needs to know which one produced the absence in front of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmartSource {
    /// The drive's own NVMe SMART/Health log page, read from the controller
    /// without elevation. Carries no sector counters: reallocated and pending
    /// sectors are ATA attributes and have no NVMe equivalent.
    NvmeLogPage,
    /// The drive's own ATA attribute table, via `SMART READ DATA` through the
    /// storage driver. A counter missing here is a counter this drive's table
    /// does not contain, which *is* a fact about the drive.
    AtaAttributes,
    /// The operating system's storage stack rather than the drive -- Windows'
    /// `Get-StorageReliabilityCounter`, which needs Administrator, or the
    /// `smartctl`/`nvme` output the Linux reader parses. An absence here is
    /// usually the query having been refused, not the drive having declined.
    StorageStack,
}

/// SMART Information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartInfo {
    /// The drive's own pass/fail verdict on itself.
    ///
    /// `None` where the drive was not asked or did not answer. This was a
    /// `bool` computed as `!matches!(health, Critical | Failed)`, and
    /// `DiskHealth::Unknown` — documented "health could not be determined" —
    /// is neither of those, so **a drive that reported nothing passed**. On
    /// this machine a USB mass-storage gadget, which has no SMART at all and
    /// whose every SMART counter resolves absent, published
    /// `disk.0.smart.passed = true` as a measurement.
    ///
    /// Worse, `smart::DiskHealth` is partly a score this crate computes from
    /// the counters, and the entity for this field says in as many words:
    /// "the drive's own pass/fail verdict on itself — NVMe critical warning
    /// bits, or the ATA failure prediction. **Not a judgement computed from
    /// the counters below.**" So the value was the one thing it documents
    /// itself not to be.
    pub passed: Option<bool>,
    /// Individual SMART attributes
    pub attributes: Vec<SmartAttribute>,
    /// Temperature from SMART (Celsius)
    pub temperature: Option<f32>,
    /// Power-on hours
    pub power_on_hours: Option<u64>,
    /// Power cycle count
    pub power_cycle_count: Option<u64>,
    /// Reallocated sectors count
    pub reallocated_sectors: Option<u64>,
    /// Pending sector count
    pub pending_sectors: Option<u64>,
    /// Uncorrectable sector count
    pub uncorrectable_sectors: Option<u64>,
    /// Which of the three sources answered. See [`SmartSource`]: it is what
    /// makes an absent counter explainable, and every absence reason for the
    /// fields above is written from it.
    pub source: SmartSource,
}

/// Individual SMART attribute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartAttribute {
    /// Attribute ID
    pub id: u8,
    /// Attribute name
    pub name: String,
    /// Current value (0-255)
    pub value: u8,
    /// Worst value seen (0-255)
    pub worst: u8,
    /// Threshold value
    pub threshold: u8,
    /// Raw value (interpretation varies by attribute)
    pub raw_value: u64,
    /// Whether this attribute is critical
    pub critical: bool,
}

/// NVMe-specific information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvmeInfo {
    /// Controller model
    pub model: String,
    /// Serial number
    pub serial: String,
    /// Firmware revision
    pub firmware: String,
    // The fields below come from NVMe Identify Controller and the SMART/Health
    // log page. Both need elevation on Windows and root for the ioctl on Linux, so
    // a caller without them can still learn the drive's identity but not its
    // controller details. They are `Option` because a `controller_id` of 0 is a
    // real controller and `num_namespaces` of 0 is a real answer — neither can
    // stand in for "not read".
    /// NVMe version (e.g., "1.4")
    pub nvme_version: Option<String>,
    /// Total NVM capacity (bytes), or `None` where it was not read.
    ///
    /// **The comment directly above this field states the rule this field was
    /// breaking.** It was `u64` filled with `.unwrap_or(0)`, and
    /// `nvme_log::IdentifyController::total_capacity` -- the thing it is built
    /// from -- has been `Option<u128>` all along, with `(total != 0)
    /// .then_some(total)` to distinguish an absent field from a zero one. The
    /// absence was established correctly at the parse layer, travelled here,
    /// and was discarded on the last line of its journey.
    pub total_capacity: Option<u64>,
    /// Unallocated capacity (bytes)
    pub unallocated_capacity: Option<u64>,
    /// Controller ID
    pub controller_id: Option<u16>,
    /// Number of namespaces
    pub num_namespaces: Option<u32>,
    /// Temperature sensors (Celsius). Empty means none were read.
    pub temperature_sensors: Vec<f32>,
    /// Current power state
    pub power_state: Option<u8>,
    /// Available power states
    pub available_power_states: Vec<NvmePowerState>,
    /// Percentage used (wear indicator, 0-100)
    pub percentage_used: Option<u8>,
    /// Data units read (512-byte units)
    pub data_units_read: Option<u64>,
    /// Data units written (512-byte units)
    pub data_units_written: Option<u64>,
    /// Host read commands
    pub host_read_commands: Option<u64>,
    /// Host write commands
    pub host_write_commands: Option<u64>,
    /// Critical warnings (bit flags). `None` when the health log was not read —
    /// distinct from `Some(0)`, which means the drive reported no warnings.
    pub critical_warnings: Option<u8>,
}

/// NVMe power state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvmePowerState {
    /// Power state number
    pub state: u8,
    /// Maximum power in watts
    pub max_power_watts: f32,
    /// Entry latency in microseconds
    pub entry_latency_us: u32,
    /// Exit latency in microseconds
    pub exit_latency_us: u32,
}

/// Filesystem information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemInfo {
    /// Mount point
    pub mount_point: PathBuf,
    /// Filesystem type (ext4, ntfs, apfs, etc.)
    pub fs_type: String,
    /// Total size in bytes, or `None` where the platform did not report it.
    ///
    /// **Linux fills these three from `statvfs` and cannot fail to** -- the
    /// struct is built inside the `if let Ok(stat)`, so on that platform they
    /// are always readings. Windows is why they are `Option`: the WMI
    /// `Win32_LogicalDisk` fields are themselves `Option` and were flattened
    /// here with `.unwrap_or(0)`, so a drive whose `Size` came back null
    /// reported a filesystem of zero bytes with zero in use.
    pub total_size: Option<u64>,
    /// Used space in bytes, or `None` where not reported.
    pub used_size: Option<u64>,
    /// Available space in bytes, or `None` where not reported.
    pub available_size: Option<u64>,
    /// Total inodes (Unix-like systems)
    pub total_inodes: Option<u64>,
    /// Used inodes
    pub used_inodes: Option<u64>,
    /// Read-only flag
    pub read_only: bool,
}

impl FilesystemInfo {
    /// Usage percentage, or `None` where either figure is unread or the
    /// filesystem reports no size.
    ///
    /// Returned `0.0` for an unread filesystem, which is an empty disk -- the
    /// most reassuring possible answer to a question nobody could answer.
    pub fn usage_percent(&self) -> Option<f32> {
        let (used, total) = (self.used_size?, self.total_size?);
        if total == 0 {
            return None;
        }
        Some((used as f64 / total as f64 * 100.0) as f32)
    }
}

/// Overall disk health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskHealth {
    /// Healthy, no issues detected
    Healthy,
    /// Warning - some metrics are concerning
    Warning,
    /// Critical - imminent failure likely
    Critical,
    /// Failed - disk has failed
    Failed,
    /// Unknown - cannot determine health
    Unknown,
}

/// Per-process disk I/O statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessDiskIo {
    /// Process ID
    pub pid: u32,
    /// Bytes read, or `None` where `/proc/<pid>/io` did not report it.
    ///
    /// **`cancelled_write_bytes` below is the tell**, and it is in this same
    /// struct: it is `Option` because it is only set when its line is present,
    /// while these four were accumulators initialised to `0` and left there
    /// when their line was absent. A process missing `rchar:` reported having
    /// read no bytes.
    pub read_bytes: Option<u64>,
    /// Bytes written, or `None` where not reported.
    pub write_bytes: Option<u64>,
    /// Read syscalls, or `None` where not reported.
    pub read_syscalls: Option<u64>,
    /// Write syscalls, or `None` where not reported.
    pub write_syscalls: Option<u64>,
    /// Cancelled write bytes (Linux)
    pub cancelled_write_bytes: Option<u64>,
}

// === Error Types ===

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Operation not supported on this device")]
    NotSupported,

    #[error("No disk devices found")]
    NoDevicesFound,

    #[error("Device not found")]
    NotFound,

    #[error("Device initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Failed to query device: {0}")]
    QueryFailed(String),

    #[error("Insufficient permissions: {0}")]
    PermissionDenied(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// SMART attribute IDs (common across vendors)
pub mod smart_ids {
    /// Raw read error rate
    pub const READ_ERROR_RATE: u8 = 0x01;
    /// Throughput performance
    pub const THROUGHPUT_PERFORMANCE: u8 = 0x02;
    /// Spin-up time
    pub const SPIN_UP_TIME: u8 = 0x03;
    /// Start/Stop count
    pub const START_STOP_COUNT: u8 = 0x04;
    /// Reallocated sectors count
    pub const REALLOCATED_SECTORS: u8 = 0x05;
    /// Seek error rate
    pub const SEEK_ERROR_RATE: u8 = 0x07;
    /// Seek time performance
    pub const SEEK_TIME_PERFORMANCE: u8 = 0x08;
    /// Power-on hours
    pub const POWER_ON_HOURS: u8 = 0x09;
    /// Spin retry count
    pub const SPIN_RETRY_COUNT: u8 = 0x0A;
    /// Recalibration retries
    pub const CALIBRATION_RETRY_COUNT: u8 = 0x0B;
    /// Power cycle count
    pub const POWER_CYCLE_COUNT: u8 = 0x0C;
    /// Current pending sector count
    pub const PENDING_SECTORS: u8 = 0xC5;
    /// Offline uncorrectable sector count
    pub const UNCORRECTABLE_SECTORS: u8 = 0xC6;
    /// UltraDMA CRC error count
    pub const UDMA_CRC_ERROR: u8 = 0xC7;
    /// Temperature (Celsius)
    pub const TEMPERATURE: u8 = 0xC2;
    /// Hardware ECC recovered
    pub const HARDWARE_ECC_RECOVERED: u8 = 0xC3;
    /// Reallocation event count
    pub const REALLOCATION_EVENTS: u8 = 0xC4;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // === DiskIoStats tests ===

    #[test]
    fn test_disk_io_total_ops() {
        let stats = DiskIoStats {
            read_bytes: Some(1000),
            write_bytes: Some(2000),
            read_ops: Some(100),
            write_ops: Some(200),
            read_time_ms: None,
            write_time_ms: None,
            queue_depth: None,
            avg_latency_us: None,
            read_throughput: None,
            write_throughput: None,
        };
        assert_eq!(stats.total_ops(), Some(300));
    }

    #[test]
    fn test_disk_io_total_bytes() {
        let stats = DiskIoStats {
            read_bytes: Some(1_000_000),
            write_bytes: Some(2_000_000),
            read_ops: Some(0),
            write_ops: Some(0),
            read_time_ms: None,
            write_time_ms: None,
            queue_depth: None,
            avg_latency_us: None,
            read_throughput: None,
            write_throughput: None,
        };
        assert_eq!(stats.total_bytes(), Some(3_000_000));
    }

    /// A disk that genuinely moved no bytes still totals zero.
    ///
    /// The companion to it is [`total_over_an_unread_counter_is_absent`]: these
    /// two fail in opposite directions, so satisfying one by breaking the other
    /// does not pass.
    #[test]
    fn test_disk_io_zero() {
        let stats = DiskIoStats {
            read_bytes: Some(0),
            write_bytes: Some(0),
            read_ops: Some(0),
            write_ops: Some(0),
            read_time_ms: None,
            write_time_ms: None,
            queue_depth: None,
            avg_latency_us: None,
            read_throughput: None,
            write_throughput: None,
        };
        assert_eq!(stats.total_ops(), Some(0));
        assert_eq!(stats.total_bytes(), Some(0));
    }

    /// An unread counter yields no total, rather than being summed as zero.
    ///
    /// `macos.rs` returned this struct with every counter hardcoded to `0`, so
    /// a total over it read as "this disk has moved nothing" for a disk nobody
    /// measured.
    #[test]
    fn total_over_an_unread_counter_is_absent() {
        let stats = DiskIoStats {
            read_bytes: Some(1_000),
            write_bytes: None,
            read_ops: Some(10),
            write_ops: None,
            read_time_ms: None,
            write_time_ms: None,
            queue_depth: None,
            avg_latency_us: None,
            read_throughput: None,
            write_throughput: None,
        };
        assert_eq!(
            stats.total_bytes(),
            None,
            "a total over a counter nobody read understates it silently"
        );
        assert_eq!(stats.total_ops(), None);
    }

    // === FilesystemInfo tests ===

    #[test]
    fn test_filesystem_usage_percent() {
        let fs = FilesystemInfo {
            mount_point: PathBuf::from("/"),
            fs_type: "ext4".to_string(),
            total_size: Some(1_000_000_000),
            used_size: Some(500_000_000),
            available_size: Some(500_000_000),
            total_inodes: None,
            used_inodes: None,
            read_only: false,
        };
        assert!((fs.usage_percent().expect("both figures read") - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_filesystem_usage_percent_zero_total() {
        let fs = FilesystemInfo {
            mount_point: PathBuf::from("/"),
            fs_type: "tmpfs".to_string(),
            total_size: Some(0),
            used_size: Some(0),
            available_size: Some(0),
            total_inodes: None,
            used_inodes: None,
            read_only: false,
        };
        // A tmpfs genuinely reporting zero total has no usage percentage --
        // there is nothing to be a percentage of. This asserted `0.0`, which
        // was also what an unread filesystem returned.
        assert_eq!(fs.usage_percent(), None);
    }

    #[test]
    fn test_filesystem_usage_percent_full() {
        let fs = FilesystemInfo {
            mount_point: PathBuf::from("/data"),
            fs_type: "ntfs".to_string(),
            total_size: Some(1_000_000),
            used_size: Some(1_000_000),
            available_size: Some(0),
            total_inodes: None,
            used_inodes: None,
            read_only: true,
        };
        assert!((fs.usage_percent().expect("both figures read") - 100.0).abs() < 0.01);
    }

    // === DiskHealth tests ===

    #[test]
    fn test_disk_health_equality() {
        assert_eq!(DiskHealth::Healthy, DiskHealth::Healthy);
        assert_ne!(DiskHealth::Healthy, DiskHealth::Warning);
        assert_ne!(DiskHealth::Warning, DiskHealth::Critical);
    }

    // === DiskType tests ===

    #[test]
    fn test_disk_type_equality() {
        assert_eq!(DiskType::NvmeSsd, DiskType::NvmeSsd);
        assert_ne!(DiskType::NvmeSsd, DiskType::SataHdd);
    }

    // === smart_ids constants ===

    #[test]
    fn test_smart_ids_values() {
        assert_eq!(smart_ids::POWER_ON_HOURS, 0x09);
        assert_eq!(smart_ids::TEMPERATURE, 0xC2);
        assert_eq!(smart_ids::REALLOCATED_SECTORS, 0x05);
        assert_eq!(smart_ids::PENDING_SECTORS, 0xC5);
    }
}
