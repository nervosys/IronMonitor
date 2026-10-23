//! Disk firmware and geometry, NVMe controller and health detail, RAID arrays and filesystems.

#[allow(unused_imports)]
use super::super::resolve::*;
#[allow(unused_imports)]
use super::super::{Domain as D, Entity, EntityKind as K, Provenance as P, Unit as U};

use crate::disk::{DiskDevice, SmartSource};
use crate::storage_controller::{RaidArrayInfo, RaidLevel, RaidStatus, StorageControllerMonitor};

#[allow(clippy::too_many_arguments)]
fn e(
    add: &mut dyn FnMut(Entity),
    id: &str,
    domain: D,
    kind: K,
    unit: Option<U>,
    prov: P,
    nullable: bool,
    description: &str,
) {
    add(Entity::new(
        id,
        domain,
        kind,
        unit,
        prov,
        nullable,
        description,
    ));
}

pub(super) fn declare(add: &mut dyn FnMut(Entity)) {
    // ── Disk identity and geometry (DiskInfo) ───────────────────────────────
    e(
        add,
        "disk.{n}.firmware",
        D::Disk,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Firmware revision the device reports for itself, read from the drive's identify data.",
    );
    e(
        add,
        "disk.{n}.vendor",
        D::Disk,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Vendor name the device or its driver reports.",
    );
    e(
        add,
        "disk.{n}.block_size",
        D::Disk,
        K::Limit,
        Some(U::Bytes),
        P::Measured,
        true,
        "Block size the device addresses in, in bytes, as the platform reports it.",
    );
    e(add, "disk.{n}.physical_sector_size", D::Disk, K::Limit, Some(U::Bytes), P::Measured, true,
      "Physical sector size in bytes. Differs from the logical size on Advanced Format drives, which is why both are published.");
    e(
        add,
        "disk.{n}.logical_sector_size",
        D::Disk,
        K::Limit,
        Some(U::Bytes),
        P::Measured,
        true,
        "Logical sector size in bytes, the unit the operating system addresses the device in.",
    );
    e(add, "disk.{n}.rotation_rate", D::Disk, K::Measurement, Some(U::Rpm), P::Measured, true,
      "Nominal spindle speed of a rotating drive, as the drive declares it. No platform reader currently obtains it, so this is unavailable with a reason.");
    e(add, "disk.{n}.interface_type", D::Disk, K::Identity, Some(U::Identifier), P::Measured, true,
      "Transport the device is attached over, such as NVMe, SATA or USB, as the platform names it.");

    // ── NVMe controller detail (NvmeInfo) ───────────────────────────────────
    e(add, "disk.{n}.nvme.unallocated_capacity", D::Disk, K::Measurement, Some(U::Bytes), P::Measured, true,
      "Capacity of the NVM the controller has not allocated to a namespace, in bytes, from the Identify Controller structure.");
    e(add, "disk.{n}.nvme.controller_id", D::Disk, K::Identity, Some(U::Count), P::Specification, true,
      "Numeric identifier the NVMe controller reports for itself in its Identify Controller structure.");
    e(
        add,
        "disk.{n}.nvme.num_namespaces",
        D::Disk,
        K::Measurement,
        Some(U::Count),
        P::Specification,
        true,
        "Number of namespaces the controller supports, from its Identify Controller structure.",
    );
    e(add, "disk.{n}.nvme.host_read_commands", D::Disk, K::Measurement, Some(U::Count), P::Measured, true,
      "Host read commands the controller has completed since it was last reset, from the SMART/Health log page.");
    e(add, "disk.{n}.nvme.host_write_commands", D::Disk, K::Measurement, Some(U::Count), P::Measured, true,
      "Host write commands the controller has completed since it was last reset, from the SMART/Health log page.");
    e(add, "disk.{n}.nvme.available_spare", D::Disk, K::Measurement, Some(U::Percent), P::Measured, true,
      "Remaining spare capacity the controller reports, percent, from the SMART/Health log page. Null where the health log was not read.");
    e(add, "disk.{n}.nvme.available_spare_threshold", D::Disk, K::Limit, Some(U::Percent), P::Measured, true,
      "Available spare level below which the controller raises a warning, percent, from the SMART/Health log page. Null where the health log was not read.");
    e(add, "disk.{n}.nvme.controller_busy_time", D::Disk, K::Measurement, Some(U::Seconds), P::Measured, true,
      "Time the controller has spent processing commands since it was last reset, in seconds. The SMART/Health log reports minutes; this is converted. Null where the health log was not read.");
    e(add, "disk.{n}.nvme.unsafe_shutdowns", D::Disk, K::Measurement, Some(U::Count), P::Measured, true,
      "Power losses the controller saw without a shutdown notification, from the SMART/Health log page. Null where the health log was not read.");
    e(
        add,
        "disk.{n}.nvme.sensor.<none>",
        D::Disk,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when the NVMe controller exposed no temperature sensor, carrying why.",
    );
    e(add, "disk.{n}.nvme.sensor.{k}.temperature", D::Disk, K::Measurement, Some(U::Celsius), P::Measured, true,
      "Temperature reported by one of the controller's own sensors, from the SMART/Health log page. A drive can have several.");
    e(
        add,
        "disk.{n}.nvme.available_power_state.<none>",
        D::Disk,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when the controller's Identify structure listed no power states, carrying why.",
    );
    e(add, "disk.{n}.nvme.available_power_state.{s}.max_power", D::Disk, K::Limit, Some(U::Watts), P::Specification, true,
      "Maximum power the controller draws in power state s, as declared in its Identify Controller power state descriptor.");
    e(add, "disk.{n}.nvme.available_power_state.{s}.entry_latency", D::Disk, K::Limit, Some(U::Milliseconds), P::Specification, true,
      "Time the controller takes to enter power state s, from its power state descriptor. Published in milliseconds from the declared microseconds.");
    e(add, "disk.{n}.nvme.available_power_state.{s}.exit_latency", D::Disk, K::Limit, Some(U::Milliseconds), P::Specification, true,
      "Time the controller takes to leave power state s, from its power state descriptor. Published in milliseconds from the declared microseconds.");

    // ── SMART attribute table (SmartInfo.attributes) ────────────────────────
    e(
        add,
        "disk.{n}.smart.attribute.<none>",
        D::Disk,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when the drive produced no ATA SMART attribute table, carrying why.",
    );
    e(add, "disk.{n}.smart.attribute.{m}.id", D::Disk, K::Identity, Some(U::Count), P::Measured, true,
      "ATA attribute number of entry m in this drive's SMART attribute table. The index m is the table position, not this number.");
    e(add, "disk.{n}.smart.attribute.{m}.name", D::Disk, K::Identity, Some(U::Text), P::Specification, true,
      "Name of ATA SMART attribute m, looked up from its attribute number. The drive does not transmit names.");
    e(add, "disk.{n}.smart.attribute.{m}.value", D::Disk, K::Measurement, Some(U::Count), P::Measured, true,
      "Normalised current value of ATA SMART attribute m, from 0 to 255. Higher is healthier for most attributes.");
    e(
        add,
        "disk.{n}.smart.attribute.{m}.worst",
        D::Disk,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Worst normalised value the drive has recorded for ATA SMART attribute m, from 0 to 255.",
    );
    e(add, "disk.{n}.smart.attribute.{m}.threshold", D::Disk, K::Limit, Some(U::Count), P::Measured, true,
      "Failure threshold the drive declares for ATA SMART attribute m, from 0 to 255. Absent where the platform did not read the threshold table.");
    e(add, "disk.{n}.smart.attribute.{m}.raw_value", D::Disk, K::Measurement, Some(U::Count), P::Measured, true,
      "Raw counter for ATA SMART attribute m. Its meaning depends on the attribute, and the drive's vendor defines it.");

    // ── Storage controllers (StorageControllerInfo and its NVMe detail) ─────
    e(
        add,
        "disk.controller.{n}.firmware",
        D::Disk,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Firmware revision of the NVMe controller. Absent for controllers that are not NVMe.",
    );
    e(add, "disk.controller.{n}.serial", D::Disk, K::Identity, Some(U::Identifier), P::Measured, true,
      "Serial number of the NVMe controller, from its Identify Controller structure. Absent for controllers that are not NVMe.");
    e(add, "disk.controller.{n}.pcie_speed", D::Disk, K::Limit, Some(U::Text), P::Measured, true,
      "PCIe link speed the NVMe controller trained to, such as 8 GT/s. Absent for controllers that are not NVMe.");
    e(add, "disk.controller.{n}.pcie_width", D::Disk, K::Limit, Some(U::Text), P::Measured, true,
      "PCIe link width the NVMe controller trained to, such as x4. Absent for controllers that are not NVMe.");
    e(add, "disk.controller.{n}.transport", D::Disk, K::Identity, Some(U::Identifier), P::Measured, true,
      "Transport the NVMe controller uses, such as pcie or tcp. Absent for controllers that are not NVMe.");
    e(add, "disk.controller.{n}.namespace_count", D::Disk, K::Measurement, Some(U::Count), P::Measured, true,
      "Number of namespaces the NVMe controller reports. Absent for controllers that are not NVMe.");

    // ── RAID arrays (RaidArrayInfo, software RAID on Linux) ─────────────────
    e(
        add,
        "disk.raid.<none>",
        D::Disk,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no RAID array was enumerated, carrying why.",
    );
    e(
        add,
        "disk.raid.{n}.name",
        D::Disk,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Name of the software RAID array, such as md0, as the kernel lists it.",
    );
    e(
        add,
        "disk.raid.{n}.level",
        D::Disk,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "RAID level of the array, such as raid1 or raid5, as the kernel reports it.",
    );
    e(
        add,
        "disk.raid.{n}.status",
        D::Disk,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Status of the array: active, degraded, rebuilding or failed, as the kernel reports it.",
    );
    e(
        add,
        "disk.raid.{n}.active_devices",
        D::Disk,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Member devices that are neither failed nor spare, counted from the array's member list.",
    );
    e(
        add,
        "disk.raid.{n}.total_devices",
        D::Disk,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Member devices listed on the array line, including failed and spare ones.",
    );
    e(add, "disk.raid.{n}.wanted_devices", D::Disk, K::Measurement, Some(U::Count), P::Measured, true,
      "Devices the array wants, the first number of the kernel's n/m device count. Absent where no blocks line was read.");
    e(add, "disk.raid.{n}.working_devices", D::Disk, K::Measurement, Some(U::Count), P::Measured, true,
      "Devices the array reports as working, the second number of the kernel's n/m device count. Absent where no blocks line was read.");
    e(
        add,
        "disk.raid.{n}.failed_devices",
        D::Disk,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Member devices the kernel flags as failed on the array line.",
    );
    e(
        add,
        "disk.raid.{n}.spare_devices",
        D::Disk,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Member devices the kernel flags as spares on the array line.",
    );
    e(
        add,
        "disk.raid.{n}.size",
        D::Disk,
        K::Limit,
        Some(U::Bytes),
        P::Measured,
        true,
        "Usable size of the array in bytes. Absent where the kernel's blocks line was not read.",
    );
    e(
        add,
        "disk.raid.{n}.members",
        D::Disk,
        K::Identity,
        None,
        P::Measured,
        true,
        "Names of the member devices of the array, as a JSON list.",
    );
    e(add, "disk.raid.{n}.resync_progress", D::Disk, K::Measurement, Some(U::Percent), P::Measured, true,
      "Progress of the array's resync or rebuild, from 0 to 100. Absent when the array is not resyncing.");

    // ── Filesystems and partitions (FilesystemInfo) ─────────────────────────
    // `disk.{n}.partition.{m}.mount_point` is declared in the base schema.
    e(
        add,
        "disk.{n}.partition.<none>",
        D::Disk,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no mounted filesystem was listed for this device, carrying why.",
    );
    e(
        add,
        "disk.{n}.partition.{m}.fs_type",
        D::Disk,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Filesystem type on this partition, such as ntfs, ext4 or apfs, as the platform names it.",
    );
    e(
        add,
        "disk.{n}.partition.{m}.total_size",
        D::Disk,
        K::Limit,
        Some(U::Bytes),
        P::Measured,
        true,
        "Total size of the filesystem in bytes. Absent where the platform did not report it.",
    );
    e(
        add,
        "disk.{n}.partition.{m}.used_size",
        D::Disk,
        K::Measurement,
        Some(U::Bytes),
        P::Measured,
        true,
        "Space in use on the filesystem, in bytes. Absent where the platform did not report it.",
    );
    e(add, "disk.{n}.partition.{m}.available_size", D::Disk, K::Measurement, Some(U::Bytes), P::Measured, true,
      "Space available to unprivileged users on the filesystem, in bytes. Absent where the platform did not report it.");
    e(
        add,
        "disk.{n}.partition.{m}.total_inodes",
        D::Disk,
        K::Limit,
        Some(U::Count),
        P::Measured,
        true,
        "Total inodes on the filesystem. Absent on platforms that do not use inodes.",
    );
    e(
        add,
        "disk.{n}.partition.{m}.used_inodes",
        D::Disk,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Inodes in use on the filesystem. Absent on platforms that do not use inodes.",
    );
    e(add, "disk.{n}.partition.{m}.read_only", D::Disk, K::Identity, None, P::Measured, true,
      "Whether the filesystem is mounted read-only. Absent where the platform's volume query does not carry the flag.");
}

/// A reader's optional text, with an empty string treated as absent.
///
/// `push_opt` guards names that spell an absence, but an empty string passes
/// straight through it, so the emptiness check happens here.
fn text_opt(value: Option<&String>) -> Option<serde_json::Value> {
    value
        .filter(|v| !v.trim().is_empty())
        .map(|v| serde_json::json!(v))
}

fn bool_reading(out: &mut Vec<Reading>, id: impl Into<String>, value: Option<bool>, why: &str) {
    let id = id.into();
    match value {
        Some(b) => out.push(Reading::measured(id, serde_json::json!(b), None)),
        None => out.push(Reading::unavailable(id, None, why)),
    }
}

pub(super) fn resolve(out: &mut Vec<Reading>) {
    // Enumerated exactly as `resolve_disk` enumerates, so `disk.{n}` lines up with
    // the base rows. On failure the base resolver has already reported `disk.<none>`.
    if let Ok(disks) = crate::disk::enumerate_disks() {
        for (i, disk) in disks.iter().enumerate() {
            resolve_disk(out, &format!("disk.{i}"), disk.as_ref());
        }
    }

    // One monitor for both controllers and RAID arrays, constructed once.
    match StorageControllerMonitor::new() {
        Ok(monitor) => {
            resolve_controllers(out, &monitor);
            resolve_raid(out, monitor.raid_arrays());
        }
        Err(e) => {
            out.push(Reading::unavailable(
                "disk.raid.<none>",
                None,
                format!(
                    "RAID arrays were not read because storage controller enumeration failed: {e}"
                ),
            ));
        }
    }
}

fn resolve_disk(out: &mut Vec<Reading>, base: &str, disk: &dyn DiskDevice) {
    resolve_disk_info(out, base, disk);
    resolve_nvme(out, base, disk);
    resolve_smart_attributes(out, base, disk);
    resolve_filesystems(out, base, disk);
}

fn resolve_disk_info(out: &mut Vec<Reading>, base: &str, disk: &dyn DiskDevice) {
    match disk.info() {
        Ok(info) => {
            push_opt(
                out,
                format!("{base}.firmware"),
                text_opt(info.firmware.as_ref()),
                Some(U::Identifier),
                "the device did not report a firmware revision",
            );
            push_opt(
                out,
                format!("{base}.vendor"),
                text_opt(info.vendor.as_ref()),
                Some(U::Text),
                "the device did not report a vendor name",
            );
            push_opt(
                out,
                format!("{base}.block_size"),
                info.block_size.map(|v| serde_json::json!(v)),
                Some(U::Bytes),
                "the platform did not report a block size for this device",
            );
            push_opt(
                out,
                format!("{base}.physical_sector_size"),
                info.physical_sector_size.map(|v| serde_json::json!(v)),
                Some(U::Bytes),
                "the platform did not report a physical sector size for this device",
            );
            push_opt(
                out,
                format!("{base}.logical_sector_size"),
                info.logical_sector_size.map(|v| serde_json::json!(v)),
                Some(U::Bytes),
                "the platform did not report a logical sector size for this device",
            );
            push_opt(
                out,
                format!("{base}.rotation_rate"),
                info.rotation_rate.map(|v| serde_json::json!(v)),
                Some(U::Rpm),
                "the reader reported no rotation rate for this device",
            );
            push_opt(
                out,
                format!("{base}.interface_type"),
                text_opt(info.interface_type.as_ref()),
                Some(U::Identifier),
                "the platform did not name this device's interface",
            );
        }
        Err(e) => {
            let why = format!("device info read failed: {e}");
            for (suffix, unit) in [
                ("firmware", U::Identifier),
                ("vendor", U::Text),
                ("block_size", U::Bytes),
                ("physical_sector_size", U::Bytes),
                ("logical_sector_size", U::Bytes),
                ("rotation_rate", U::Rpm),
                ("interface_type", U::Identifier),
            ] {
                out.push(Reading::unavailable(
                    format!("{base}.{suffix}"),
                    Some(unit),
                    why.clone(),
                ));
            }
        }
    }
}

fn resolve_nvme(out: &mut Vec<Reading>, base: &str, disk: &dyn DiskDevice) {
    match disk.nvme_info() {
        Ok(n) => {
            push_opt(
                out,
                format!("{base}.nvme.unallocated_capacity"),
                n.unallocated_capacity.map(|v| serde_json::json!(v)),
                Some(U::Bytes),
                "the controller did not report unallocated capacity",
            );
            push_opt(
                out,
                format!("{base}.nvme.controller_id"),
                n.controller_id.map(|v| serde_json::json!(v)),
                Some(U::Count),
                "the controller did not report its identifier",
            );
            push_opt(
                out,
                format!("{base}.nvme.num_namespaces"),
                n.num_namespaces.map(|v| serde_json::json!(v)),
                Some(U::Count),
                "the controller did not report a namespace count",
            );
            push_opt(
                out,
                format!("{base}.nvme.host_read_commands"),
                n.host_read_commands.map(|v| serde_json::json!(v)),
                Some(U::Count),
                "the controller did not report a host read command count",
            );
            push_opt(
                out,
                format!("{base}.nvme.host_write_commands"),
                n.host_write_commands.map(|v| serde_json::json!(v)),
                Some(U::Count),
                "the controller did not report a host write command count",
            );
            push_opt(
                out,
                format!("{base}.nvme.available_spare"),
                n.available_spare.map(|v| serde_json::json!(v)),
                Some(U::Percent),
                "this reader did not obtain the available spare from the SMART/Health log",
            );
            push_opt(
                out,
                format!("{base}.nvme.available_spare_threshold"),
                n.available_spare_threshold.map(|v| serde_json::json!(v)),
                Some(U::Percent),
                "this reader did not obtain the spare threshold from the SMART/Health log",
            );
            // The log reports minutes; the ontology publishes seconds.
            push_opt(
                out,
                format!("{base}.nvme.controller_busy_time"),
                n.controller_busy_time_minutes
                    .map(|m| serde_json::json!(m.saturating_mul(60))),
                Some(U::Seconds),
                "this reader did not obtain the controller busy time from the SMART/Health log",
            );
            push_opt(
                out,
                format!("{base}.nvme.unsafe_shutdowns"),
                n.unsafe_shutdowns.map(|v| serde_json::json!(v)),
                Some(U::Count),
                "this reader did not obtain the unsafe shutdown count from the SMART/Health log",
            );

            if n.temperature_sensors.is_empty() {
                out.push(Reading::unavailable(
                    format!("{base}.nvme.sensor.<none>"),
                    None,
                    "the controller's SMART/Health log reported no temperature sensor",
                ));
            }
            for (k, t) in n.temperature_sensors.iter().enumerate() {
                let id = format!("{base}.nvme.sensor.{k}.temperature");
                if t.is_finite() {
                    out.push(Reading::measured(
                        id,
                        serde_json::json!(f64::from(*t)),
                        Some(U::Celsius),
                    ));
                } else {
                    out.push(Reading::unavailable(
                        id,
                        Some(U::Celsius),
                        "the sensor reported a value that is not a number",
                    ));
                }
            }

            if n.available_power_states.is_empty() {
                out.push(Reading::unavailable(
                    format!("{base}.nvme.available_power_state.<none>"),
                    None,
                    "the controller's Identify structure listed no power states, or was not read",
                ));
            }
            for p in &n.available_power_states {
                let pb = format!("{base}.nvme.available_power_state.{}", p.state);
                let watts = f64::from(p.max_power_watts);
                if watts.is_finite() {
                    out.push(Reading::spec(
                        format!("{pb}.max_power"),
                        serde_json::json!(watts),
                        Some(U::Watts),
                    ));
                } else {
                    out.push(Reading::unavailable(
                        format!("{pb}.max_power"),
                        Some(U::Watts),
                        "the power state descriptor reported a maximum power that is not a number",
                    ));
                }
                // The descriptor carries microseconds, which the unit list does not
                // have; milliseconds are the same quantity at a readable scale.
                out.push(Reading::spec(
                    format!("{pb}.entry_latency"),
                    serde_json::json!(f64::from(p.entry_latency_us) / 1000.0),
                    Some(U::Milliseconds),
                ));
                out.push(Reading::spec(
                    format!("{pb}.exit_latency"),
                    serde_json::json!(f64::from(p.exit_latency_us) / 1000.0),
                    Some(U::Milliseconds),
                ));
            }
        }
        Err(e) => {
            // A SATA or USB device declines the NVMe protocol; the reason uses the
            // same wording the base resolver uses for these rows.
            let why = format!("no NVMe controller data: {e}");
            for (suffix, unit) in [
                ("unallocated_capacity", U::Bytes),
                ("controller_id", U::Count),
                ("num_namespaces", U::Count),
                ("host_read_commands", U::Count),
                ("host_write_commands", U::Count),
                ("available_spare", U::Percent),
                ("available_spare_threshold", U::Percent),
                ("controller_busy_time", U::Seconds),
                ("unsafe_shutdowns", U::Count),
            ] {
                out.push(Reading::unavailable(
                    format!("{base}.nvme.{suffix}"),
                    Some(unit),
                    why.clone(),
                ));
            }
            out.push(Reading::unavailable(
                format!("{base}.nvme.sensor.<none>"),
                None,
                why.clone(),
            ));
            out.push(Reading::unavailable(
                format!("{base}.nvme.available_power_state.<none>"),
                None,
                why,
            ));
        }
    }
}

fn resolve_smart_attributes(out: &mut Vec<Reading>, base: &str, disk: &dyn DiskDevice) {
    let id_of = |suffix: &str| format!("{base}.smart.attribute.{suffix}");
    match disk.smart_info() {
        Ok(s) => {
            if s.attributes.is_empty() {
                let why = match s.source {
                    SmartSource::NvmeLogPage => {
                        "this drive reports its counters in the NVMe health log, which has no ATA attribute table"
                    }
                    SmartSource::AtaAttributes => {
                        "the drive returned an ATA attribute table with no entries"
                    }
                    SmartSource::StorageStack => {
                        "no ATA attribute table was obtained from this drive, because the storage stack answered instead"
                    }
                };
                out.push(Reading::unavailable(id_of("<none>"), None, why));
            }
            for (m, a) in s.attributes.iter().enumerate() {
                out.push(Reading::measured(
                    id_of(&format!("{m}.id")),
                    serde_json::json!(a.id),
                    Some(U::Count),
                ));
                push_spec_text(out, id_of(&format!("{m}.name")), &a.name);
                out.push(Reading::measured(
                    id_of(&format!("{m}.value")),
                    serde_json::json!(a.value),
                    Some(U::Count),
                ));
                out.push(Reading::measured(
                    id_of(&format!("{m}.worst")),
                    serde_json::json!(a.worst),
                    Some(U::Count),
                ));
                push_opt(
                    out,
                    id_of(&format!("{m}.threshold")),
                    a.threshold.map(|v| serde_json::json!(v)),
                    Some(U::Count),
                    "the platform's ATA reader did not read this drive's threshold table",
                );
                out.push(Reading::measured(
                    id_of(&format!("{m}.raw_value")),
                    serde_json::json!(a.raw_value),
                    Some(U::Count),
                ));
            }
        }
        Err(e) => {
            out.push(Reading::unavailable(
                id_of("<none>"),
                None,
                format!("SMART read failed: {e}"),
            ));
        }
    }
}

fn resolve_filesystems(out: &mut Vec<Reading>, base: &str, disk: &dyn DiskDevice) {
    match disk.filesystem_info() {
        Ok(filesystems) if filesystems.is_empty() => {
            out.push(Reading::unavailable(
                format!("{base}.partition.<none>"),
                None,
                "the platform listed no mounted filesystem on this device",
            ));
        }
        Ok(filesystems) => {
            for (m, fs) in filesystems.iter().enumerate() {
                let pb = format!("{base}.partition.{m}");
                push_text(
                    out,
                    format!("{pb}.mount_point"),
                    &fs.mount_point.to_string_lossy(),
                );
                push_id(out, format!("{pb}.fs_type"), &fs.fs_type);
                push_opt(
                    out,
                    format!("{pb}.total_size"),
                    fs.total_size.map(|v| serde_json::json!(v)),
                    Some(U::Bytes),
                    "the platform did not report the total size of this filesystem",
                );
                push_opt(
                    out,
                    format!("{pb}.used_size"),
                    fs.used_size.map(|v| serde_json::json!(v)),
                    Some(U::Bytes),
                    "the platform did not report how much of this filesystem is in use",
                );
                push_opt(
                    out,
                    format!("{pb}.available_size"),
                    fs.available_size.map(|v| serde_json::json!(v)),
                    Some(U::Bytes),
                    "the platform did not report the space available on this filesystem",
                );
                push_opt(
                    out,
                    format!("{pb}.total_inodes"),
                    fs.total_inodes.map(|v| serde_json::json!(v)),
                    Some(U::Count),
                    "the platform does not report inode counts for this filesystem",
                );
                push_opt(
                    out,
                    format!("{pb}.used_inodes"),
                    fs.used_inodes.map(|v| serde_json::json!(v)),
                    Some(U::Count),
                    "the platform does not report inode usage for this filesystem",
                );
                bool_reading(
                    out,
                    format!("{pb}.read_only"),
                    fs.read_only,
                    "the platform's volume query does not report whether this filesystem is read-only",
                );
            }
        }
        Err(e) => {
            out.push(Reading::unavailable(
                format!("{base}.partition.<none>"),
                None,
                format!("filesystem enumeration failed: {e}"),
            ));
        }
    }
}

fn resolve_controllers(out: &mut Vec<Reading>, monitor: &StorageControllerMonitor) {
    for (i, c) in monitor.controllers().iter().enumerate() {
        let base = format!("disk.controller.{i}");
        match &c.nvme_info {
            Some(n) => {
                push_text(out, format!("{base}.firmware"), &n.firmware);
                push_id(out, format!("{base}.serial"), &n.serial);
                push_text(out, format!("{base}.pcie_speed"), &n.pcie_speed);
                push_text(out, format!("{base}.pcie_width"), &n.pcie_width);
                push_id(out, format!("{base}.transport"), &n.transport);
                out.push(Reading::measured(
                    format!("{base}.namespace_count"),
                    serde_json::json!(n.namespace_count),
                    Some(U::Count),
                ));
            }
            None => {
                let why = "this controller is not NVMe, so NVMe identity fields do not apply";
                for (suffix, unit) in [
                    ("firmware", U::Text),
                    ("serial", U::Identifier),
                    ("pcie_speed", U::Text),
                    ("pcie_width", U::Text),
                    ("transport", U::Identifier),
                    ("namespace_count", U::Count),
                ] {
                    out.push(Reading::unavailable(
                        format!("{base}.{suffix}"),
                        Some(unit),
                        why,
                    ));
                }
            }
        }
    }
}

fn raid_level_name(level: &RaidLevel) -> String {
    match level {
        RaidLevel::Raid0 => "raid0".to_string(),
        RaidLevel::Raid1 => "raid1".to_string(),
        RaidLevel::Raid5 => "raid5".to_string(),
        RaidLevel::Raid6 => "raid6".to_string(),
        RaidLevel::Raid10 => "raid10".to_string(),
        RaidLevel::Raid50 => "raid50".to_string(),
        RaidLevel::Raid60 => "raid60".to_string(),
        RaidLevel::JBOD => "jbod".to_string(),
        RaidLevel::Linear => "linear".to_string(),
        RaidLevel::Unknown(name) => name.to_ascii_lowercase(),
    }
}

fn raid_status_name(status: &RaidStatus) -> &'static str {
    match status {
        RaidStatus::Active => "active",
        RaidStatus::Degraded => "degraded",
        RaidStatus::Rebuilding => "rebuilding",
        RaidStatus::Failed => "failed",
        RaidStatus::Unknown => "unknown",
    }
}

fn resolve_raid(out: &mut Vec<Reading>, arrays: &[RaidArrayInfo]) {
    if arrays.is_empty() {
        let why = if cfg!(target_os = "linux") {
            "no software RAID array is listed in /proc/mdstat"
        } else {
            "software RAID arrays are read from /proc/mdstat, which exists only on Linux, so none were read on this platform"
        };
        out.push(Reading::unavailable("disk.raid.<none>", None, why));
        return;
    }

    for (n, a) in arrays.iter().enumerate() {
        let base = format!("disk.raid.{n}");
        push_id(out, format!("{base}.name"), &a.name);
        push_id(out, format!("{base}.level"), &raid_level_name(&a.level));
        push_id(out, format!("{base}.status"), raid_status_name(&a.status));
        out.push(Reading::measured(
            format!("{base}.active_devices"),
            serde_json::json!(a.active_devices),
            Some(U::Count),
        ));
        out.push(Reading::measured(
            format!("{base}.total_devices"),
            serde_json::json!(a.total_devices),
            Some(U::Count),
        ));
        let no_blocks = "no blocks line was read for this array";
        push_opt(
            out,
            format!("{base}.wanted_devices"),
            a.wanted_devices.map(|v| serde_json::json!(v)),
            Some(U::Count),
            no_blocks,
        );
        push_opt(
            out,
            format!("{base}.working_devices"),
            a.working_devices.map(|v| serde_json::json!(v)),
            Some(U::Count),
            no_blocks,
        );
        out.push(Reading::measured(
            format!("{base}.failed_devices"),
            serde_json::json!(a.failed_devices),
            Some(U::Count),
        ));
        out.push(Reading::measured(
            format!("{base}.spare_devices"),
            serde_json::json!(a.spare_devices),
            Some(U::Count),
        ));
        push_opt(
            out,
            format!("{base}.size"),
            a.size_bytes.map(|v| serde_json::json!(v)),
            Some(U::Bytes),
            "no blocks line was read for this array, so its size is unknown",
        );

        let members: Vec<&String> = a.members.iter().filter(|m| !m.trim().is_empty()).collect();
        if members.is_empty() {
            out.push(Reading::unavailable(
                format!("{base}.members"),
                None,
                "the kernel listed no member devices for this array",
            ));
        } else {
            out.push(Reading::measured(
                format!("{base}.members"),
                serde_json::json!(members),
                None,
            ));
        }

        push_opt(
            out,
            format!("{base}.resync_progress"),
            a.resync_progress.map(|v| serde_json::json!(v)),
            Some(U::Percent),
            "the array is not resyncing, so no resync progress applies",
        );
    }
}
