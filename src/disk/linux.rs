//! Linux disk monitoring via sysfs, ioctl, and procfs

use crate::disk::traits::*;
use std::fs;
use std::path::{Path, PathBuf};

/// Linux disk device implementation
pub struct LinuxDisk {
    name: String,
    device_path: PathBuf,
    disk_type: DiskType,
}

impl LinuxDisk {
    /// Create a new Linux disk device
    pub fn new(name: String) -> Result<Self, Error> {
        let device_path = PathBuf::from(format!("/dev/{}", name));

        // Confirm the device exists before describing it. This used to also parse
        // the major:minor pair into two fields that nothing ever read, and it
        // indexed `parts[1]` directly — a panic on any `dev` file without a colon,
        // reached through a `.unwrap_or(0)` that looked like it was handling the
        // error. The existence check is the part that was doing work.
        let dev_path = format!("/sys/block/{}/dev", name);
        fs::read_to_string(&dev_path)
            .map_err(|e| Error::QueryFailed(format!("Failed to read {}: {}", dev_path, e)))?;

        // Determine disk type
        let disk_type = Self::detect_disk_type(&name)?;

        Ok(Self {
            name,
            device_path,
            disk_type,
        })
    }

    fn detect_disk_type(name: &str) -> Result<DiskType, Error> {
        // NVMe devices: nvme*
        if name.starts_with("nvme") {
            return Ok(DiskType::NvmeSsd);
        }

        // Check if it's a rotational device (HDD)
        let rotational_path = format!("/sys/block/{}/queue/rotational", name);
        if let Ok(content) = fs::read_to_string(&rotational_path) {
            if content.trim() == "1" {
                return Ok(DiskType::SataHdd);
            } else {
                return Ok(DiskType::SataSsd);
            }
        }

        // SCSI devices
        if name.starts_with("sd") {
            // Could be SSD or HDD, default to SSD if we can't determine
            return Ok(DiskType::SataSsd);
        }

        Ok(DiskType::Unknown)
    }

    fn read_sysfs_string(&self, attribute: &str) -> Result<String, Error> {
        let path = format!("/sys/block/{}/{}", self.name, attribute);
        fs::read_to_string(&path)
            .map(|s| s.trim().to_string())
            .map_err(|e| Error::QueryFailed(format!("Failed to read {}: {}", path, e)))
    }

    fn read_sysfs_u64(&self, attribute: &str) -> Result<u64, Error> {
        self.read_sysfs_string(attribute)?
            .parse()
            .map_err(|e| Error::ParseError(format!("Failed to parse {}: {}", attribute, e)))
    }
}

impl LinuxDisk {
    /// This drive's entry from the SMART collector, matched on device path.
    ///
    /// One collector run spawns `smartctl` once per drive, so picking one entry
    /// out of it used to cost a full sweep per call — quadratic over a machine,
    /// and the worst case of the two platforms. It now reads a run shared across
    /// every drive and every accessor,
    /// [`crate::smart::SmartMonitor::cached_disks`].
    fn smart_disk(&self) -> Option<crate::smart::SmartDiskInfo> {
        let wanted = format!("/dev/{}", self.name);
        crate::smart::SmartMonitor::cached_disks()
            .iter()
            .find(|d| d.device == wanted)
            .cloned()
    }
}

impl DiskDevice for LinuxDisk {
    fn name(&self) -> &str {
        &self.name
    }

    fn disk_type(&self) -> DiskType {
        self.disk_type
    }

    fn info(&self) -> Result<DiskInfo, Error> {
        // Read model
        let model = self
            .read_sysfs_string("device/model")
            .unwrap_or_else(|_| "Unknown".to_string());

        // Read vendor
        let vendor = self.read_sysfs_string("device/vendor").ok();

        // Read firmware
        let firmware = self.read_sysfs_string("device/rev").ok();

        // Read capacity (in 512-byte sectors)
        let sectors = self.read_sysfs_u64("size")?;
        let capacity = sectors * 512;

        // Read queue info
        // sysfs carries both; defaulting to 512 asserted a non-4Kn drive.
        let logical_block_size = self
            .read_sysfs_u64("queue/logical_block_size")
            .ok()
            .map(|v| v as u32);
        let physical_block_size = self
            .read_sysfs_u64("queue/physical_block_size")
            .ok()
            .map(|v| v as u32);

        // Read rotation rate (0 = SSD, >0 = HDD RPM)
        let rotation_rate = self.read_sysfs_u64("queue/rotational").ok().and_then(|r| {
            if r > 0 {
                Some(r as u32)
            } else {
                None
            }
        });

        Ok(DiskInfo {
            name: self.name.clone(),
            model,
            serial: None, // Would need ioctl or smartctl
            firmware,
            capacity,
            block_size: logical_block_size,
            disk_type: self.disk_type,
            interface_type: Some(match self.disk_type {
                DiskType::NvmeSsd => "NVMe (PCIe)".to_string(),
                DiskType::SataSsd | DiskType::SataHdd => "SATA".to_string(),
                DiskType::Usb => "USB".to_string(),
                DiskType::Scsi => "SCSI".to_string(),
                DiskType::Virtual => "Virtual".to_string(),
                DiskType::Unknown => "Unknown".to_string(),
            }),
            physical_sector_size: physical_block_size,
            logical_sector_size: logical_block_size,
            rotation_rate,
            vendor,
        })
    }

    fn io_stats(&self) -> Result<DiskIoStats, Error> {
        let stat_path = format!("/sys/block/{}/stat", self.name);
        let stat_content = fs::read_to_string(&stat_path)
            .map_err(|e| Error::QueryFailed(format!("Failed to read {}: {}", stat_path, e)))?;

        // Format: read_ios read_merges read_sectors read_ticks write_ios write_merges write_sectors write_ticks in_flight io_ticks time_in_queue
        let parts: Vec<&str> = stat_content.split_whitespace().collect();
        if parts.len() < 11 {
            return Err(Error::ParseError("Invalid stat format".to_string()));
        }

        let read_ops: u64 = parts[0].parse().unwrap_or(0);
        let read_sectors: u64 = parts[2].parse().unwrap_or(0);
        let read_time_ms: u64 = parts[3].parse().unwrap_or(0);
        let write_ops: u64 = parts[4].parse().unwrap_or(0);
        let write_sectors: u64 = parts[6].parse().unwrap_or(0);
        let write_time_ms: u64 = parts[7].parse().unwrap_or(0);
        let in_flight: u32 = parts[8].parse().unwrap_or(0);

        Ok(DiskIoStats {
            read_bytes: read_sectors * 512,
            write_bytes: write_sectors * 512,
            read_ops,
            write_ops,
            read_time_ms: Some(read_time_ms),
            write_time_ms: Some(write_time_ms),
            queue_depth: Some(in_flight),
            avg_latency_us: None,  // Would need to calculate from deltas
            read_throughput: None, // Would need historical data
            write_throughput: None,
        })
    }

    fn temperature(&self) -> Result<Option<f32>, Error> {
        // For NVMe devices, check hwmon
        if self.disk_type == DiskType::NvmeSsd {
            let hwmon_path = format!("/sys/block/{}/device/hwmon", self.name);
            if let Ok(entries) = fs::read_dir(&hwmon_path) {
                for entry in entries.flatten() {
                    let temp_path = entry.path().join("temp1_input");
                    if let Ok(temp_str) = fs::read_to_string(&temp_path) {
                        if let Ok(temp_millicelsius) = temp_str.trim().parse::<i32>() {
                            return Ok(Some(temp_millicelsius as f32 / 1000.0));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    fn health(&self) -> Result<DiskHealth, Error> {
        // The device file existing says the kernel enumerated it, not that the
        // drive is well. Ask the SMART collector, which reads the drive's own
        // health, and fall back to Unknown rather than asserting Healthy — an
        // existence check reported as a clean bill of health is a claim nothing
        // measured.
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
        let Some(disk) = self.smart_disk() else {
            return Err(Error::QueryFailed(format!(
                "no SMART data for {}; `smartctl` or `nvme` may not be installed",
                self.name
            )));
        };

        Ok(SmartInfo {
            // Not read. `smartctl -H` prints the drive's own "SMART
            // overall-health self-assessment test result", and the NVMe log
            // page has the critical warning byte, but `SmartDiskInfo` captures
            // neither -- its `health` is partly a score computed from the
            // counters, which is what this field documents itself not to be.
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
            // `smartctl`/`nvme` output, parsed by the SMART collector -- the
            // drive answered, but this reader did not ask it directly.
            source: SmartSource::StorageStack,
        })
    }

    fn nvme_info(&self) -> Result<NvmeInfo, Error> {
        // Namespaces are named `nvme0n1`; the controller they belong to is
        // `nvme0`, and that is what sysfs keys on.
        let Some(controller) = self
            .name
            .split('n')
            .next()
            .filter(|c| c.starts_with("nvme"))
        else {
            return Err(Error::NotSupported);
        };
        let base = PathBuf::from("/sys/class/nvme").join(controller);
        if !base.exists() {
            return Err(Error::NotSupported);
        }

        let attr = |name: &str| {
            fs::read_to_string(base.join(name))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };

        // sysfs exposes identity, the controller id and the namespace list without
        // root — more than Windows gives unelevated. The SMART/Health log page
        // (temperature, wear, data units) still needs the admin ioctl, so those
        // come from the SMART collector when `nvme` is installed, and stay None
        // otherwise rather than becoming zero.
        let smart = self.smart_disk();
        let namespaces = fs::read_dir(&base).ok().map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with(&format!("{controller}n"))
                })
                .count() as u32
        });

        Ok(NvmeInfo {
            model: attr("model").unwrap_or_default(),
            serial: attr("serial").unwrap_or_default(),
            firmware: attr("firmware_rev").unwrap_or_default(),
            nvme_version: attr("nvme_version"),
            total_capacity: self.read_sysfs_u64("size").map(|s| s * 512).unwrap_or(0),
            unallocated_capacity: None,
            controller_id: attr("cntlid").and_then(|v| v.parse().ok()),
            num_namespaces: namespaces,
            temperature_sensors: smart
                .as_ref()
                .and_then(|d| d.temperature_celsius)
                .map(|t| vec![t as f32])
                .unwrap_or_default(),
            power_state: None,
            available_power_states: Vec::new(),
            percentage_used: smart.as_ref().and_then(|d| d.nvme_percentage_used),
            data_units_read: smart
                .as_ref()
                .and_then(|d| d.total_bytes_read)
                .map(|b| b / 512),
            data_units_written: smart
                .as_ref()
                .and_then(|d| d.total_bytes_written)
                .map(|b| b / 512),
            host_read_commands: None,
            host_write_commands: None,
            critical_warnings: None,
        })
    }

    fn device_path(&self) -> PathBuf {
        self.device_path.clone()
    }

    fn filesystem_info(&self) -> Result<Vec<FilesystemInfo>, Error> {
        let mut filesystems = Vec::new();

        // Read /proc/mounts
        let mounts = fs::read_to_string("/proc/mounts")
            .map_err(|e| Error::QueryFailed(format!("Failed to read /proc/mounts: {}", e)))?;

        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }

            let device = parts[0];
            let mount_point = parts[1];
            let fs_type = parts[2];

            // Check if this mount is for our device or a partition
            if device.contains(&self.name) || device == self.device_path.to_str().unwrap_or("") {
                // Get filesystem stats using statvfs
                if let Ok(stat) = nix::sys::statvfs::statvfs(mount_point) {
                    let total_size = stat.blocks() * stat.block_size();
                    let available_size = stat.blocks_available() * stat.block_size();
                    let free_size = stat.blocks_free() * stat.block_size();
                    let used_size = total_size - free_size;

                    filesystems.push(FilesystemInfo {
                        mount_point: PathBuf::from(mount_point),
                        fs_type: fs_type.to_string(),
                        total_size,
                        used_size,
                        available_size,
                        total_inodes: Some(stat.files()),
                        used_inodes: Some(stat.files() - stat.files_free()),
                        read_only: stat.flags().contains(nix::sys::statvfs::FsFlags::ST_RDONLY),
                    });
                }
            }
        }

        Ok(filesystems)
    }
}

/// Enumerate all block devices
pub fn enumerate() -> Result<Vec<Box<dyn DiskDevice>>, Error> {
    let mut devices = Vec::new();

    // Read /sys/block for all block devices
    let sys_block = Path::new("/sys/block");
    if !sys_block.exists() {
        return Err(Error::NoDevicesFound);
    }

    for entry in fs::read_dir(sys_block)
        .map_err(|e| Error::QueryFailed(format!("Failed to read /sys/block: {}", e)))?
    {
        let entry =
            entry.map_err(|e| Error::QueryFailed(format!("Failed to read entry: {}", e)))?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip loop devices, ram disks, etc.
        if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("dm-") {
            continue;
        }

        // Create device
        match LinuxDisk::new(name.clone()) {
            Ok(disk) => devices.push(Box::new(disk) as Box<dyn DiskDevice>),
            Err(e) => eprintln!("Warning: Failed to initialize disk {}: {}", name, e),
        }
    }

    if devices.is_empty() {
        return Err(Error::NoDevicesFound);
    }

    Ok(devices)
}

/// Get per-process I/O stats from /proc/[pid]/io
pub fn get_process_io(pid: u32) -> Result<ProcessDiskIo, Error> {
    let io_path = format!("/proc/{}/io", pid);
    let content = fs::read_to_string(&io_path)
        .map_err(|e| Error::QueryFailed(format!("Failed to read {}: {}", io_path, e)))?;

    let mut read_bytes = 0;
    let mut write_bytes = 0;
    let mut read_syscalls = 0;
    let mut write_syscalls = 0;
    let mut cancelled_write_bytes = None;

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let value: u64 = parts[1].parse().unwrap_or(0);
        match parts[0] {
            "rchar:" => read_bytes = value,
            "wchar:" => write_bytes = value,
            "syscr:" => read_syscalls = value,
            "syscw:" => write_syscalls = value,
            "cancelled_write_bytes:" => cancelled_write_bytes = Some(value),
            _ => {}
        }
    }

    Ok(ProcessDiskIo {
        pid,
        read_bytes,
        write_bytes,
        read_syscalls,
        write_syscalls,
        cancelled_write_bytes,
    })
}
