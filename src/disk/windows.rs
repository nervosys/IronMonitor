// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Windows disk monitoring via WMI and Windows Storage APIs
//!
//! This module provides disk monitoring for Windows using:
//! - WMI Win32_DiskDrive for device information
//! - Performance Counters for I/O statistics
//! - DeviceIoControl for SMART data

use crate::disk::traits::*;
use std::collections::HashMap;
use std::path::PathBuf;
use wmi::{COMLibrary, WMIConnection};

/// Create WMI connection with robust COM initialization
/// Handles cases where COM is already initialized by the GUI framework
fn create_wmi_connection() -> Result<WMIConnection, Error> {
    // Strategy 1: Fresh COM initialization (works best in background threads)
    if let Ok(com) = COMLibrary::new() {
        if let Ok(conn) = WMIConnection::with_namespace_path("root\\CIMV2", com) {
            return Ok(conn);
        }
    }

    // Strategy 2: COM without security init
    if let Ok(com) = COMLibrary::without_security() {
        if let Ok(conn) = WMIConnection::with_namespace_path("root\\CIMV2", com) {
            return Ok(conn);
        }
    }

    // Strategy 3: Assume COM is already initialized by the runtime (e.g., GUI apps)
    let com = unsafe { COMLibrary::assume_initialized() };
    WMIConnection::with_namespace_path("root\\CIMV2", com)
        .map_err(|e| Error::InitializationFailed(e.to_string()))
}

/// Windows disk device implementation
pub struct WindowsDisk {
    name: String,
    device_path: PathBuf,
    disk_type: DiskType,
    model: Option<String>,
    serial: Option<String>,
    size: u64,
    disk_index: u32,
    interface_type: Option<String>,
}

impl WindowsDisk {
    // Mirrors the fields the Win32 disk enumeration hands back in one shot.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        device_path: PathBuf,
        disk_type: DiskType,
        model: Option<String>,
        serial: Option<String>,
        size: u64,
        disk_index: u32,
        interface_type: Option<String>,
    ) -> Self {
        Self {
            name,
            device_path,
            disk_type,
            model,
            serial,
            size,
            disk_index,
            interface_type,
        }
    }

    /// Create from device index (e.g., 0 for PhysicalDrive0)
    pub fn from_index(index: u32) -> Result<Self, Error> {
        let name = format!("PhysicalDrive{}", index);
        let device_path = PathBuf::from(format!("\\\\.\\PhysicalDrive{}", index));

        // Try to detect disk type
        let disk_type = Self::detect_disk_type(index);

        Ok(Self {
            name,
            device_path,
            disk_type,
            model: None,
            serial: None,
            size: 0,
            disk_index: index,
            interface_type: None,
        })
    }

    /// Detect disk type from Windows APIs
    fn detect_disk_type(_index: u32) -> DiskType {
        // Would use IOCTL_STORAGE_QUERY_PROPERTY with StorageDeviceProperty
        // For now, default to unknown
        DiskType::Unknown
    }

    /// Read this drive's cumulative I/O counters.
    ///
    /// Three defects were fixed here together, because each hid the next.
    ///
    /// The class was `Win32_PerfFormattedData_PerfDisk_PhysicalDisk`, whose
    /// `DiskReadBytesPerSec` is what its name says: an instantaneous **rate**.
    /// It was stored in `DiskIoStats::read_bytes`, documented "Total bytes read
    /// since boot" and rendered by the GUI as a byte total. On an idle machine
    /// the rate is 0, so a host that had read 3.87 TB since boot reported 0 B.
    /// `Win32_PerfRawData_PerfDisk_PhysicalDisk` carries the same property
    /// names as raw cumulative counters, which is the quantity the field
    /// promises and the quantity Linux's `/sys/block/*/stat` already supplied.
    ///
    /// The instance filter was `Name LIKE '%{index}'`, matching the index as a
    /// **suffix**. The instances are named `0 C:`, `1 E:`, `2`, `3 D:` — so on
    /// a machine with a lettered drive the filter matched nothing, every call
    /// fell through to the `_Total` fallback, and each disk reported the whole
    /// machine's I/O as its own. The filter is now a prefix match, and there is
    /// no `_Total` fallback: a drive with no instance has no counters, which is
    /// not the same as having the sum of every other drive's.
    ///
    /// `read_time_ms`, `write_time_ms` and `queue_depth` were hardcoded
    /// `Some(0)` while this very class published all three.
    fn read_io_counters(&self) -> Result<DiskIoStats, Error> {
        // The instance name is the physical index, optionally followed by a
        // space and the drive letters: "0 C:", "1 E:", "2".
        let index = self.name.replace("PhysicalDrive", "");
        let query =
            format!("{DISK_PERF_SELECT} WHERE Name = \'{index}\' OR Name LIKE \'{index} %\'");

        let perfs: Vec<DiskPerf> = create_wmi_connection()?
            .raw_query(&query)
            .unwrap_or_default();
        let Some(perf) = perfs.first() else {
            return Err(Error::QueryFailed(format!(
                "no PhysicalDisk performance instance named \'{index}\'"
            )));
        };
        Ok(perf.to_io_stats())
    }
}

/// One row of `Win32_PerfRawData_PerfDisk_PhysicalDisk`.
#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct DiskPerf {
    name: String,
    disk_read_bytes_per_sec: u64,
    disk_write_bytes_per_sec: u64,
    disk_reads_per_sec: u64,
    disk_writes_per_sec: u64,
    percent_disk_read_time: u64,
    percent_disk_write_time: u64,
    current_disk_queue_length: u32,
}

const DISK_PERF_SELECT: &str = "SELECT Name, DiskReadBytesPerSec, DiskWriteBytesPerSec, \
     DiskReadsPerSec, DiskWritesPerSec, PercentDiskReadTime, PercentDiskWriteTime, \
     CurrentDiskQueueLength FROM Win32_PerfRawData_PerfDisk_PhysicalDisk";

impl DiskPerf {
    fn to_io_stats(&self) -> DiskIoStats {
        DiskIoStats {
            read_bytes: self.disk_read_bytes_per_sec,
            write_bytes: self.disk_write_bytes_per_sec,
            read_ops: self.disk_reads_per_sec,
            write_ops: self.disk_writes_per_sec,
            // The two timers are PERF_PRECISION_100NS_TIMER: the raw value is
            // busy time in 100-nanosecond units.
            read_time_ms: Some(self.percent_disk_read_time / 10_000),
            write_time_ms: Some(self.percent_disk_write_time / 10_000),
            queue_depth: Some(self.current_disk_queue_length),
            // Both need two samples to difference; one call has nothing to
            // difference against.
            avg_latency_us: None,
            read_throughput: None,
            write_throughput: None,
        }
    }

    /// The physical drive index this instance belongs to, from names shaped
    /// `"0 C:"`, `"1 E:"` or `"2"`. `_Total` and anything unparseable is None.
    fn physical_index(&self) -> Option<u32> {
        self.name
            .split_whitespace()
            .next()
            .and_then(|first| first.parse().ok())
    }
}

/// Every physical drive's cumulative I/O counters, in one query.
///
/// `read_io_counters` opens a WMI connection and runs a filtered query *per
/// disk*, and the connection is what costs: four drives on this machine measured
/// 772 ms in total, ~193 ms each, against 0.5 ms for the capacity-only path.
/// Both the Prometheus exporter and the pipeline want counters for every drive
/// at once, and paying for one connection instead of N takes that to a single
/// round trip.
///
/// Keyed by `PhysicalDriveN` to match [`DiskInfo::name`], so a caller can join
/// against an enumeration without re-deriving the index.
pub fn all_io_counters() -> Result<std::collections::HashMap<String, DiskIoStats>, Error> {
    let perfs: Vec<DiskPerf> = create_wmi_connection()?
        .raw_query(DISK_PERF_SELECT)
        .unwrap_or_default();

    Ok(perfs
        .iter()
        .filter_map(|perf| {
            // No `_Total`: it is the sum over every drive, and attributing that
            // to any one of them is the defect this reader was fixed for once
            // already.
            let index = perf.physical_index()?;
            Some((format!("PhysicalDrive{index}"), perf.to_io_stats()))
        })
        .collect())
}

impl WindowsDisk {
    /// This drive's entry from the SMART collector, matched on physical index.
    ///
    /// The collector enumerates every drive in one PowerShell round trip, so
    /// picking one entry out of it used to cost a subprocess per call. It now
    /// reads a run shared across every drive and every accessor —
    /// [`crate::smart::SmartMonitor::cached_disks`] — which is what makes the
    /// per-device trait affordable over a whole machine.
    fn smart_disk(&self) -> Option<crate::smart::SmartDiskInfo> {
        let wanted = format!(r"\\.\PhysicalDrive{}", self.disk_index);
        crate::smart::SmartMonitor::cached_disks()
            .iter()
            .find(|d| d.device == wanted)
            .cloned()
    }

    /// Build [`NvmeInfo`] from what the controller itself reported.
    ///
    /// Fields stay `None` when the controller did not report them. `percentage_used`
    /// of 0 and `critical_warnings` of 0 are readings — a new drive with nothing
    /// wrong — and are preserved as such.
    fn nvme_info_from_controller(&self, data: crate::disk::windows_nvme::NvmeData) -> NvmeInfo {
        let id = data.identify;
        let health = data.health;

        // Identity from Identify Controller, falling back to WMI if the controller
        // served the log page but not the identify structure.
        let wmi = if id.is_none() {
            self.smart_disk()
        } else {
            None
        };
        let identity = |from_id: Option<String>, from_wmi: Option<String>| {
            from_id.unwrap_or_else(|| from_wmi.unwrap_or_default())
        };

        NvmeInfo {
            model: identity(
                id.as_ref().map(|i| i.model.clone()),
                wmi.as_ref().map(|w| w.model.clone()),
            ),
            serial: identity(
                id.as_ref().map(|i| i.serial.clone()),
                wmi.as_ref().map(|w| w.serial.clone()),
            ),
            firmware: identity(
                id.as_ref().map(|i| i.firmware.clone()),
                wmi.as_ref().map(|w| w.firmware.clone()),
            ),
            nvme_version: id.as_ref().and_then(|i| i.version.clone()),
            total_capacity: id
                .as_ref()
                .and_then(|i| i.total_capacity)
                .map(|c| c.min(u64::MAX as u128) as u64)
                .or_else(|| wmi.as_ref().map(|w| w.capacity_bytes))
                .unwrap_or(0),
            unallocated_capacity: id
                .as_ref()
                .and_then(|i| i.unallocated_capacity)
                .map(|c| c.min(u64::MAX as u128) as u64),
            controller_id: id.as_ref().map(|i| i.controller_id),
            num_namespaces: id.as_ref().map(|i| i.num_namespaces),
            temperature_sensors: health
                .as_ref()
                .and_then(|h| h.temperature_celsius())
                .map(|t| vec![t])
                .unwrap_or_default(),
            // From Get Features (FID 0x02); the available states below come from
            // Identify, so the list can be populated while the current state is
            // not, on a controller that does not implement Get Features.
            power_state: data.power_state,
            available_power_states: id
                .as_ref()
                .map(|i| {
                    i.power_states
                        .iter()
                        .map(|p| NvmePowerState {
                            state: p.state,
                            max_power_watts: p.max_power_watts,
                            entry_latency_us: p.entry_latency_us,
                            exit_latency_us: p.exit_latency_us,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            percentage_used: health.as_ref().map(|h| h.percentage_used),
            data_units_read: health
                .as_ref()
                .map(|h| h.data_units_read.min(u64::MAX as u128) as u64),
            data_units_written: health
                .as_ref()
                .map(|h| h.data_units_written.min(u64::MAX as u128) as u64),
            host_read_commands: health
                .as_ref()
                .map(|h| h.host_read_commands.min(u64::MAX as u128) as u64),
            host_write_commands: health
                .as_ref()
                .map(|h| h.host_write_commands.min(u64::MAX as u128) as u64),
            critical_warnings: health.as_ref().map(|h| h.critical_warning),
        }
    }

    /// The pre-3.1 path: identity from WMI, with everything the controller alone
    /// can answer left `None`. Reached only when the passthrough fails on a device
    /// that is nonetheless NVMe.
    fn nvme_info_from_wmi(&self) -> Result<NvmeInfo, Error> {
        let Some(disk) = self.smart_disk() else {
            return Err(Error::QueryFailed(format!(
                "no device data for physical drive {}",
                self.disk_index
            )));
        };
        if disk.media_type != crate::smart::DriveMediaType::NVMe {
            return Err(Error::NotSupported);
        }

        Ok(NvmeInfo {
            model: disk.model.clone(),
            serial: disk.serial.clone(),
            firmware: disk.firmware.clone(),
            nvme_version: None,
            total_capacity: disk.capacity_bytes,
            unallocated_capacity: None,
            controller_id: None,
            num_namespaces: None,
            temperature_sensors: disk
                .temperature_celsius
                .map(|t| vec![t as f32])
                .unwrap_or_default(),
            power_state: None,
            available_power_states: Vec::new(),
            percentage_used: disk.nvme_percentage_used,
            data_units_read: disk.total_bytes_read.map(|b| b / 512),
            data_units_written: disk.total_bytes_written.map(|b| b / 512),
            host_read_commands: None,
            host_write_commands: None,
            critical_warnings: None,
        })
    }
}

impl DiskDevice for WindowsDisk {
    fn name(&self) -> &str {
        &self.name
    }

    fn disk_type(&self) -> DiskType {
        self.disk_type
    }

    fn info(&self) -> Result<DiskInfo, Error> {
        Ok(DiskInfo {
            name: self.name.clone(),
            model: self.model.clone().unwrap_or_else(|| "Unknown".to_string()),
            serial: self.serial.clone(),
            firmware: None,
            capacity: self.size,
            block_size: None,
            disk_type: self.disk_type,
            interface_type: self.interface_type.clone(),
            // Not read. `IOCTL_STORAGE_QUERY_PROPERTY` with
            // `StorageAccessAlignmentProperty` reports both; until that is
            // called, 512 was the common value standing in for this drive's.
            physical_sector_size: None,
            logical_sector_size: None,
            // Not read. This was `Some(7200)` under the comment "Common HDD
            // speed" for every drive not already known to be an SSD -- an
            // invented RPM, not this drive's. A real rate comes from ATA
            // IDENTIFY word 217; `disk_type` already carries SSD vs HDD.
            rotation_rate: None,
            vendor: None,
        })
    }

    fn io_stats(&self) -> Result<DiskIoStats, Error> {
        self.read_io_counters()
    }

    fn health(&self) -> Result<DiskHealth, Error> {
        // An NVMe controller grades itself. Critical warning bits are the drive
        // saying something is wrong now; spare below its own threshold and wear at
        // or past 100% are conditions it will not flag until they bite.
        if let Ok(data) = crate::disk::windows_nvme::query(self.disk_index) {
            if let Some(health) = data.health {
                // Bit 0 spare below threshold, 1 temperature past threshold,
                // 2 reliability degraded, 3 read-only, 4 volatile backup failed.
                const RELIABILITY_DEGRADED: u8 = 0b0000_0100;
                const READ_ONLY: u8 = 0b0000_1000;

                return Ok(
                    if health.critical_warning & (RELIABILITY_DEGRADED | READ_ONLY) != 0 {
                        DiskHealth::Critical
                    } else if health.critical_warning != 0
                        || health.percentage_used >= 100
                        || health.available_spare_percent < health.available_spare_threshold_percent
                    {
                        DiskHealth::Warning
                    } else {
                        DiskHealth::Healthy
                    },
                );
            }
        }

        // For an ATA device the same question goes to the storage driver, which
        // answers with the drive's own failure prediction. That prediction is a
        // reading, so grading `Healthy` from it is not the empty-scorecard problem
        // `smart::tests::a_drive_with_no_readable_counters_is_not_graded_healthy`
        // guards against: here the drive was asked and said no.
        if let Ok(ata) = crate::disk::windows_ata::query(self.disk_index) {
            if ata.predict_failure {
                return Ok(DiskHealth::Failed);
            }
            // Sectors the drive could not read and has not yet remapped are data at
            // risk now, and it will not predict failure over them. Reallocated
            // sectors are deliberately not graded: whether a count is alarming is
            // the threshold's call, and SMART READ THRESHOLDS is not reachable
            // through this ioctl.
            if let Some(smart) = &ata.smart {
                let at_risk = smart.pending_sectors().unwrap_or(0)
                    + smart.uncorrectable_sectors().unwrap_or(0);
                if at_risk > 0 {
                    return Ok(DiskHealth::Warning);
                }
            }
            return Ok(DiskHealth::Healthy);
        }

        // Windows publishes a HealthStatus per physical disk; the SMART collector
        // already reads it, so ask that rather than guessing Unknown.
        match self.smart_disk() {
            Some(disk) => Ok(match disk.health {
                crate::smart::DiskHealth::Good => DiskHealth::Healthy,
                crate::smart::DiskHealth::Warning => DiskHealth::Warning,
                crate::smart::DiskHealth::Critical => DiskHealth::Critical,
                crate::smart::DiskHealth::Failed => DiskHealth::Failed,
                crate::smart::DiskHealth::Unknown => DiskHealth::Unknown,
            }),
            None => Ok(DiskHealth::Unknown),
        }
    }

    fn smart_info(&self) -> Result<SmartInfo, Error> {
        // On NVMe the health log page carries temperature, power-on hours, power
        // cycles and error counts, and needs no elevation. The WMI path below
        // reaches the same numbers only through `Get-StorageReliabilityCounter`,
        // which does require it — so prefer the controller and keep WMI for the
        // SATA and USB devices that have no log page.
        if let Ok(data) = crate::disk::windows_nvme::query(self.disk_index) {
            if let Some(health) = data.health {
                return Ok(SmartInfo {
                    // The critical warning field is the drive's own verdict: any
                    // bit set means it is reporting a condition against itself.
                    // The NVMe log page is the controller grading itself.
                    passed: Some(health.critical_warning == 0),
                    attributes: Vec::new(),
                    temperature: health.temperature_celsius(),
                    power_on_hours: Some(health.power_on_hours.min(u64::MAX as u128) as u64),
                    power_cycle_count: Some(health.power_cycles.min(u64::MAX as u128) as u64),
                    // NVMe has no reallocated or pending sector concept; those are
                    // ATA notions. Reporting 0 would assert a healthy count that
                    // was never measured.
                    reallocated_sectors: None,
                    pending_sectors: None,
                    uncorrectable_sectors: Some(health.media_errors.min(u64::MAX as u128) as u64),
                    source: SmartSource::NvmeLogPage,
                });
            }
        }

        // SATA and USB drives have no log page, but the storage driver will issue
        // SMART READ DATA on their behalf for an unelevated caller. This is the
        // only path on Windows to reallocated and pending sector counts — the WMI
        // fallback below cannot reach them at all, and needs elevation for the
        // counters it can reach.
        if let Ok(ata) = crate::disk::windows_ata::query(self.disk_index) {
            if let Some(smart) = ata.smart {
                return Ok(SmartInfo {
                    // The drive's own prediction, not a verdict computed from the
                    // attributes below.
                    // The ATA failure prediction is the drive grading itself.
                    passed: Some(!ata.predict_failure),
                    attributes: smart
                        .attributes
                        .iter()
                        .map(|a| SmartAttribute {
                            id: a.id,
                            name: a.name(),
                            value: a.value,
                            worst: a.worst,
                            // SMART READ THRESHOLDS is a separate ATA command with
                            // no ioctl of its own; 0 is ATA's own encoding for a
                            // threshold that never trips.
                            threshold: 0,
                            raw_value: a.raw,
                            critical: a.pre_fail(),
                        })
                        .collect(),
                    temperature: smart.temperature_celsius().map(|c| c as f32),
                    power_on_hours: smart.power_on_hours(),
                    power_cycle_count: smart.power_cycle_count(),
                    reallocated_sectors: smart.reallocated_sectors(),
                    pending_sectors: smart.pending_sectors(),
                    uncorrectable_sectors: smart.uncorrectable_sectors(),
                    source: SmartSource::AtaAttributes,
                });
            }
        }

        let Some(disk) = self.smart_disk() else {
            return Err(Error::QueryFailed(format!(
                "no SMART data for physical drive {}",
                self.disk_index
            )));
        };

        // Every counter is Option because the Windows storage stack only exposes
        // them to an elevated caller: unelevated, `Get-StorageReliabilityCounter`
        // fails with PermissionDenied and there is genuinely nothing to report.
        // `passed` reflects the platform's own health verdict, which *is*
        // available without elevation.
        Ok(SmartInfo {
            // Not read on this path. Neither the NVMe log page nor the ATA
            // prediction was reachable, and what is left is Windows'
            // `HealthStatus` -- the storage stack's opinion, not the drive's
            // verdict, and `smart::DiskHealth` is partly a score computed here
            // from the counters. `disk.{n}.health` publishes that verdict
            // separately and honestly; this field is for the drive's own.
            passed: None,
            attributes: disk
                .attributes
                .iter()
                .map(|a| SmartAttribute {
                    id: a.id.min(u8::MAX as u16) as u8,
                    name: a.name.clone(),
                    value: a.value.min(u8::MAX as u64) as u8,
                    worst: a.worst.min(u8::MAX as u64) as u8,
                    threshold: a.threshold.min(u8::MAX as u64) as u8,
                    raw_value: a.raw_value,
                    critical: a.pre_fail,
                })
                .collect(),
            temperature: disk.temperature_celsius.map(|t| t as f32),
            power_on_hours: disk.power_on_hours,
            power_cycle_count: disk.power_cycle_count,
            reallocated_sectors: disk.reallocated_sectors,
            pending_sectors: disk.pending_sectors,
            uncorrectable_sectors: disk.uncorrectable_errors,
            source: SmartSource::StorageStack,
        })
    }

    fn nvme_info(&self) -> Result<NvmeInfo, Error> {
        // Ask the controller directly. This settles "is this NVMe" without
        // consulting WMI's MediaType, which reports the medium (`SSD`) rather than
        // the transport and so classified every NVMe drive here as a plain SSD
        // until 3.0.0 — making this method refuse the very drives it exists for.
        // The device itself rejecting the NVMe protocol is a better answer than any
        // string comparison.
        match crate::disk::windows_nvme::query(self.disk_index) {
            Ok(data) => Ok(self.nvme_info_from_controller(data)),
            Err(Error::NotSupported) => Err(Error::NotSupported),
            // The passthrough can fail on a drive that is genuinely NVMe — a driver
            // that does not implement the protocol query, for instance. Fall back
            // to what WMI knows rather than losing the identity fields entirely.
            Err(_) => self.nvme_info_from_wmi(),
        }
    }

    fn device_path(&self) -> PathBuf {
        self.device_path.clone()
    }

    fn filesystem_info(&self) -> Result<Vec<FilesystemInfo>, Error> {
        use serde::Deserialize;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
        };

        // Drive type constants
        const DRIVE_FIXED: u32 = 3;
        const DRIVE_REMOVABLE: u32 = 2;

        #[derive(Deserialize, Debug)]
        #[serde(rename_all = "PascalCase")]
        #[allow(dead_code)]
        struct Win32DiskPartition {
            device_i_d: String,
            disk_index: u32,
            index: u32,
            size: Option<u64>,
        }

        #[derive(Deserialize, Debug)]
        #[serde(rename_all = "PascalCase")]
        #[allow(dead_code)]
        struct Win32LogicalDiskToPartition {
            antecedent: String,
            dependent: String,
        }

        #[derive(Deserialize, Debug)]
        #[serde(rename_all = "PascalCase")]
        #[allow(dead_code)]
        struct Win32LogicalDisk {
            device_i_d: String,
            file_system: Option<String>,
            size: Option<u64>,
            free_space: Option<u64>,
            volume_name: Option<String>,
        }

        let mut filesystems = Vec::new();

        // Try WMI approach first to get partition-to-logical disk mapping
        if let Ok(wmi_con) = create_wmi_connection() {
            // Get partitions for this disk
            let partition_query = format!(
                "SELECT DeviceID, DiskIndex, Index, Size FROM Win32_DiskPartition WHERE DiskIndex = {}",
                self.disk_index
            );
            let partitions: Vec<Win32DiskPartition> =
                wmi_con.raw_query(&partition_query).unwrap_or_default();

            // Get logical disk to partition mappings
            let mappings: Vec<Win32LogicalDiskToPartition> = wmi_con
                .raw_query("SELECT Antecedent, Dependent FROM Win32_LogicalDiskToPartition")
                .unwrap_or_default();

            // Get all logical disks
            let logical_disks: Vec<Win32LogicalDisk> = wmi_con
                .raw_query("SELECT DeviceID, FileSystem, Size, FreeSpace, VolumeName FROM Win32_LogicalDisk")
                .unwrap_or_default();

            // Match partitions to logical disks
            for partition in &partitions {
                // Find mapping for this partition
                for mapping in &mappings {
                    if mapping.antecedent.contains(&partition.device_i_d) {
                        // Extract drive letter from dependent (e.g., "Win32_LogicalDisk.DeviceID=\"C:\"")
                        if let Some(start) = mapping.dependent.find("DeviceID=\"") {
                            let start = start + 10;
                            if let Some(end) = mapping.dependent[start..].find('"') {
                                let drive_letter = &mapping.dependent[start..start + end];

                                // Find the logical disk info
                                if let Some(ld) =
                                    logical_disks.iter().find(|d| d.device_i_d == drive_letter)
                                {
                                    let total_size = ld.size.unwrap_or(0);
                                    let free_space = ld.free_space.unwrap_or(0);
                                    let used_size = total_size.saturating_sub(free_space);

                                    filesystems.push(FilesystemInfo {
                                        mount_point: PathBuf::from(format!("{}\\", drive_letter)),
                                        fs_type: ld
                                            .file_system
                                            .clone()
                                            .unwrap_or_else(|| "Unknown".to_string()),
                                        total_size,
                                        used_size,
                                        available_size: free_space,
                                        total_inodes: None,
                                        used_inodes: None,
                                        read_only: false,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback: if WMI didn't work, try Windows API directly for all drives
        // (this won't correctly map to physical disks but at least shows something)
        if filesystems.is_empty() {
            unsafe {
                let drives = GetLogicalDrives();
                for i in 0..26u32 {
                    if drives & (1 << i) != 0 {
                        let drive_letter = (b'A' + i as u8) as char;
                        let root_path: Vec<u16> = format!("{}:\\", drive_letter)
                            .encode_utf16()
                            .chain(std::iter::once(0))
                            .collect();

                        let drive_type = GetDriveTypeW(PCWSTR(root_path.as_ptr()));

                        // Only include fixed and removable drives
                        if drive_type == DRIVE_FIXED || drive_type == DRIVE_REMOVABLE {
                            let mut free_bytes_available: u64 = 0;
                            let mut total_bytes: u64 = 0;
                            let mut total_free_bytes: u64 = 0;

                            if GetDiskFreeSpaceExW(
                                PCWSTR(root_path.as_ptr()),
                                Some(&mut free_bytes_available),
                                Some(&mut total_bytes),
                                Some(&mut total_free_bytes),
                            )
                            .is_ok()
                            {
                                // Get filesystem type
                                let mut fs_name: [u16; 256] = [0; 256];
                                let mut volume_name: [u16; 256] = [0; 256];

                                let _ = GetVolumeInformationW(
                                    PCWSTR(root_path.as_ptr()),
                                    Some(&mut volume_name),
                                    None,
                                    None,
                                    None,
                                    Some(&mut fs_name),
                                );

                                let fs_type = String::from_utf16_lossy(&fs_name)
                                    .trim_end_matches('\0')
                                    .to_string();

                                if !fs_type.is_empty() {
                                    filesystems.push(FilesystemInfo {
                                        mount_point: PathBuf::from(format!("{}:\\", drive_letter)),
                                        fs_type,
                                        total_size: total_bytes,
                                        used_size: total_bytes.saturating_sub(total_free_bytes),
                                        available_size: free_bytes_available,
                                        total_inodes: None,
                                        used_inodes: None,
                                        read_only: false,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(filesystems)
    }
}

/// Enumerate Windows disk devices using WMI
pub fn enumerate() -> Result<Vec<Box<dyn DiskDevice>>, Error> {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    #[allow(dead_code)]
    struct Win32DiskDrive {
        device_i_d: String,
        model: Option<String>,
        serial_number: Option<String>,
        size: Option<u64>,
        media_type: Option<String>,
        interface_type: Option<String>,
        index: u32,
    }

    let mut disks: Vec<Box<dyn DiskDevice>> = Vec::new();

    // Use robust WMI connection that handles GUI context
    if let Ok(wmi_con) = create_wmi_connection() {
        let wmi_disks: Vec<Win32DiskDrive> = wmi_con
            .raw_query(
                "SELECT DeviceID, Model, SerialNumber, Size, MediaType, InterfaceType, Index FROM Win32_DiskDrive",
            )
            .unwrap_or_default();

        for wmi_disk in wmi_disks {
            // Determine disk type - check multiple sources for best accuracy
            let disk_type = {
                let model_upper = wmi_disk.model.as_ref().map(|m| m.to_uppercase());
                let interface_upper = wmi_disk.interface_type.as_ref().map(|i| i.to_uppercase());

                // Check model name first (most reliable for SSDs)
                if let Some(ref model) = model_upper {
                    if model.contains("NVME")
                        || model.contains("990 PRO")
                        || model.contains("9100 PRO")
                        || model.contains("980 PRO")
                        || model.contains("970 EVO PLUS")
                    {
                        DiskType::NvmeSsd
                    } else if model.contains("SSD") || model.contains("970 EVO") {
                        // Could be SATA or NVMe SSD - check interface
                        if interface_upper.as_deref() == Some("SCSI") {
                            DiskType::NvmeSsd // SCSI interface on SSD = NVMe
                        } else {
                            DiskType::SataSsd
                        }
                    } else {
                        // Model doesn't clearly indicate SSD type
                        // Check if interface is SCSI (modern NVMe) vs IDE (SATA)
                        if interface_upper.as_deref() == Some("SCSI") {
                            // SCSI interface on modern systems usually means NVMe
                            // Check media type for additional hints
                            match wmi_disk.media_type.as_deref() {
                                Some(media) if media.contains("Fixed") => {
                                    // Fixed disk on SCSI - likely NVMe SSD
                                    DiskType::NvmeSsd
                                }
                                Some(media) if media.contains("Removable") => DiskType::Usb,
                                _ => DiskType::NvmeSsd, // Default SCSI to NVMe
                            }
                        } else {
                            // IDE interface or other
                            match wmi_disk.media_type.as_deref() {
                                Some(media) if media.contains("SSD") || media.contains("Solid") => {
                                    DiskType::SataSsd
                                }
                                Some(media) if media.contains("NVMe") => DiskType::NvmeSsd,
                                Some(media) if media.contains("Removable") => DiskType::Usb,
                                Some(media) if media.contains("Fixed") => DiskType::SataHdd,
                                _ => DiskType::Unknown,
                            }
                        }
                    }
                } else {
                    // No model - use interface and media type
                    if interface_upper.as_deref() == Some("SCSI") {
                        DiskType::NvmeSsd // SCSI without model = likely NVMe
                    } else {
                        match wmi_disk.media_type.as_deref() {
                            Some(media) if media.contains("SSD") => DiskType::SataSsd,
                            Some(media) if media.contains("NVMe") => DiskType::NvmeSsd,
                            Some(media) if media.contains("Removable") => DiskType::Usb,
                            Some(media) if media.contains("Fixed") => DiskType::SataHdd,
                            _ => DiskType::Unknown,
                        }
                    }
                }
            };

            // Format interface type for display
            let interface_type = wmi_disk.interface_type.map(|iface| {
                match iface.to_uppercase().as_str() {
                    "SCSI" => {
                        // SCSI often means NVMe on modern systems
                        if matches!(disk_type, DiskType::NvmeSsd) {
                            "NVMe (PCIe)".to_string()
                        } else {
                            "SCSI".to_string()
                        }
                    }
                    "IDE" => "SATA".to_string(),
                    "USB" => "USB".to_string(),
                    "1394" => "FireWire".to_string(),
                    other => other.to_string(),
                }
            });

            let disk = WindowsDisk::new(
                format!("PhysicalDrive{}", wmi_disk.index),
                PathBuf::from(format!("\\\\.\\PhysicalDrive{}", wmi_disk.index)),
                disk_type,
                wmi_disk.model,
                wmi_disk.serial_number.map(|s| s.trim().to_string()),
                wmi_disk.size.unwrap_or(0),
                wmi_disk.index,
                interface_type,
            );

            disks.push(Box::new(disk));
        }
    }

    // Fallback: try to enumerate physical drives directly
    if disks.is_empty() {
        for index in 0..8 {
            match WindowsDisk::from_index(index) {
                Ok(disk) => {
                    use std::fs::OpenOptions;
                    use std::os::windows::fs::OpenOptionsExt;

                    const FILE_FLAG_NO_BUFFERING: u32 = 0x20000000;
                    const FILE_SHARE_READ: u32 = 0x00000001;
                    const FILE_SHARE_WRITE: u32 = 0x00000002;

                    let device_path = format!("\\\\.\\PhysicalDrive{}", index);

                    if let Ok(_file) = OpenOptions::new()
                        .read(true)
                        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                        .custom_flags(FILE_FLAG_NO_BUFFERING)
                        .open(&device_path)
                    {
                        disks.push(Box::new(disk));
                    }
                }
                Err(_) => continue,
            }
        }
    }

    // If no disks found, return error
    if disks.is_empty() {
        return Err(Error::NotFound);
    }

    Ok(disks)
}

/// Enhanced disk monitor with caching
pub struct WindowsDiskMonitor {
    disks: HashMap<String, WindowsDisk>,
}

impl WindowsDiskMonitor {
    pub fn new() -> Result<Self, Error> {
        let disks_vec = enumerate()?;
        let mut disks = HashMap::new();

        for (i, disk) in disks_vec.iter().enumerate() {
            disks.insert(
                disk.name().to_string(),
                WindowsDisk {
                    name: disk.name().to_string(),
                    device_path: disk.device_path(),
                    disk_type: disk.disk_type(),
                    model: None,
                    serial: None,
                    size: 0,
                    disk_index: i as u32,
                    interface_type: None,
                },
            );
        }

        Ok(Self { disks })
    }

    pub fn disks(&self) -> Vec<&WindowsDisk> {
        self.disks.values().collect()
    }

    pub fn disk_by_name(&self, name: &str) -> Option<&WindowsDisk> {
        self.disks.get(name)
    }
}

impl Default for WindowsDiskMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            disks: HashMap::new(),
        })
    }
}
