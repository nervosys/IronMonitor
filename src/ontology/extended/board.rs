//! Motherboard and system identity, BIOS, firmware, TPM and chassis detail,
//! motherboard voltage rails and fans, batteries and power supplies.

#[allow(unused_imports)]
use super::super::resolve::*;
#[allow(unused_imports)]
use super::super::{Domain as D, Entity, EntityKind as K, Provenance as P, Unit as U};

pub(super) fn declare(add: &mut dyn FnMut(Entity)) {
    // ── System identity, from the firmware's identity tables ─────────────────
    add(Entity::new(
        "board.serial",
        D::Board,
        K::Identity,
        Some(U::Identifier),
        P::Specification,
        true,
        concat!(
            "Serial number from the firmware's identity tables. The source differs ",
            "by platform: on Windows it is the baseboard's serial, not the chassis's. ",
            "Null where the firmware reports none or a stand-in value."
        ),
    ));
    add(Entity::new(
        "board.uuid",
        D::Board,
        K::Identity,
        Some(U::Identifier),
        P::Specification,
        true,
        "System UUID from the firmware's identity tables. Null where the firmware reports the all-zero or all-F value that means unset.",
    ));
    add(Entity::new(
        "board.version",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "Revision string the board vendor printed for the motherboard, from the firmware's baseboard table.",
    ));
    add(Entity::new(
        "board.bios.version",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "BIOS or UEFI version string, as the firmware reports it.",
    ));
    add(Entity::new(
        "board.bios.release_date",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "Date the installed BIOS or UEFI image was released, in the firmware's own format, normally YYYY-MM-DD.",
    ));
    add(Entity::new(
        "board.bios.revision",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "BIOS revision as major.minor, from the SMBIOS system BIOS version fields. Null where the firmware reports no revision.",
    ));

    // ── Firmware inventory ──────────────────────────────────────────────────
    add(Entity::new(
        "board.firmware.<none>",
        D::Board,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when the firmware inventory enumerated no entries, carrying why.",
    ));
    add(Entity::new(
        "board.firmware.serial",
        D::Board,
        K::Identity,
        Some(U::Identifier),
        P::Specification,
        true,
        "System serial number from the firmware inventory. Null where the reader cannot read it, which on Linux needs root.",
    ));
    add(Entity::new(
        "board.firmware.{n}.vendor",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "Vendor of this firmware entry, as the source declares it. Null where the source names none.",
    ));
    add(Entity::new(
        "board.firmware.{n}.date",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "Release date of this firmware entry, in the source's own format, normally YYYY-MM-DD. Null where the source gives none.",
    ));
    add(Entity::new(
        "board.firmware.{n}.device",
        D::Board,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Device path or identifier this firmware entry belongs to.",
    ));
    add(Entity::new(
        "board.firmware.{n}.updateable",
        D::Board,
        K::Identity,
        None,
        P::Measured,
        true,
        "Whether the operating system can update this firmware, where the source says so. Null means the source did not say, not that the component cannot be updated.",
    ));

    // ── TPM detail ──────────────────────────────────────────────────────────
    add(Entity::new(
        "board.tpm.device",
        D::Board,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Name of the TPM device node, such as tpm0. Null where the TPM does not report one.",
    ));
    add(Entity::new(
        "board.tpm.firmware_version",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Firmware version the TPM reports for itself. Null where it does not report one.",
    ));
    add(Entity::new(
        "board.tpm.algorithms",
        D::Board,
        K::Identity,
        None,
        P::Measured,
        true,
        "JSON array of the hash and signing algorithms the TPM supports, such as SHA-256 and RSA. An empty array means the TPM enumerated none.",
    ));
    add(Entity::new(
        "board.tpm.pcr_banks",
        D::Board,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Number of platform configuration register banks the TPM exposes. Null where it was not read.",
    ));

    // ── Chassis, read live from SMBIOS on Windows and DMI on Linux ──────────
    // Every field is read from the running system's tables (or, on macOS, from
    // `system_profiler`), so each is a measurement of this machine. A placeholder
    // the firmware left in a field is reported as absent.
    add(Entity::new(
        "board.chassis.serial",
        D::Board,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Serial number of the chassis, read from the firmware's system enclosure table. Null where it is unset, a placeholder, or unreadable.",
    ));
    add(Entity::new(
        "board.chassis.asset_tag",
        D::Board,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Asset tag the owner or vendor wrote into the chassis firmware. Null where none is set.",
    ));
    add(Entity::new(
        "board.chassis.sku",
        D::Board,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Stock-keeping unit code the vendor assigned to this system. Null where the firmware reports none.",
    ));
    add(Entity::new(
        "board.chassis.version",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Version string of the chassis, read from the firmware's enclosure table. Null where none is reported.",
    ));
    add(Entity::new(
        "board.chassis.manufacturer",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Manufacturer of the chassis, read from the firmware's system tables. Null where the firmware reports none, and on macOS, where no manufacturer is read.",
    ));
    add(Entity::new(
        "board.chassis.product_name",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Product name of the chassis, read from the firmware's system tables. Null where the firmware reports none or a placeholder.",
    ));

    // ── Motherboard voltage rails and fans ──────────────────────────────────
    add(Entity::new(
        "board.voltage.<none>",
        D::Board,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no voltage rail was reported, carrying why.",
    ));
    add(Entity::new(
        "board.voltage.<unreadable>",
        D::Board,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when a motherboard sensor device was enumerated but its voltage rails could not be read, naming the device and the error.",
    ));
    add(Entity::new(
        "board.voltage.{n}.label",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Measured,
        false,
        "Label the sensor chip gives this voltage rail, such as VCore or +12V.",
    ));
    add(Entity::new(
        "board.voltage.{n}.voltage",
        D::Board,
        K::Measurement,
        Some(U::Volts),
        P::Measured,
        true,
        "Current voltage on this rail. Null where the rail is listed but could not be sampled.",
    ));
    add(Entity::new(
        "board.voltage.{n}.min",
        D::Board,
        K::Limit,
        Some(U::Volts),
        P::Specification,
        true,
        "Lowest voltage this rail is specified to accept, as declared by the sensor source. Null where no limit was declared.",
    ));
    add(Entity::new(
        "board.voltage.{n}.max",
        D::Board,
        K::Limit,
        Some(U::Volts),
        P::Specification,
        true,
        "Highest voltage this rail is specified to accept, as declared by the sensor source. Null where no limit was declared.",
    ));
    add(Entity::new(
        "board.fan.<none>",
        D::Board,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no motherboard fan was reported, carrying why.",
    ));
    add(Entity::new(
        "board.fan.<unreadable>",
        D::Board,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when a motherboard sensor device was enumerated but its fans could not be read, naming the device and the error.",
    ));
    add(Entity::new(
        "board.fan.{n}.label",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Measured,
        false,
        "Label the sensor source gives this fan, such as CPU Fan or Case Fan 1.",
    ));
    add(Entity::new(
        "board.fan.{n}.rpm",
        D::Board,
        K::Measurement,
        Some(U::Rpm),
        P::Measured,
        true,
        "Current speed of this fan. Null where the fan is stopped or its speed cannot be measured.",
    ));
    add(Entity::new(
        "board.fan.{n}.pwm",
        D::Board,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Current pulse-width modulation duty for this fan, from 0 to 255. Null where the fan has no PWM control.",
    ));
    add(Entity::new(
        "board.fan.{n}.min_rpm",
        D::Board,
        K::Limit,
        Some(U::Rpm),
        P::Specification,
        true,
        "Lowest speed this fan is specified to run at, as declared by the sensor source. Null where no limit was declared.",
    ));
    add(Entity::new(
        "board.fan.{n}.max_rpm",
        D::Board,
        K::Limit,
        Some(U::Rpm),
        P::Specification,
        true,
        "Highest speed this fan is specified to reach, as declared by the sensor source. Null where no limit was declared.",
    ));
    add(Entity::new(
        "board.fan.{n}.controllable",
        D::Board,
        K::Identity,
        None,
        P::Measured,
        true,
        "Whether this fan's speed can be set. Null where the sensor source did not determine it.",
    ));

    // ── Batteries ───────────────────────────────────────────────────────────
    add(Entity::new(
        "power.battery.<none>",
        D::Power,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no battery was enumerated, carrying why.",
    ));
    add(Entity::new(
        "power.battery.{n}.manufacturer",
        D::Power,
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "Battery manufacturer, as the battery reports it.",
    ));
    add(Entity::new(
        "power.battery.{n}.model",
        D::Power,
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "Battery model name, as the battery reports it.",
    ));
    add(Entity::new(
        "power.battery.{n}.technology",
        D::Power,
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "Cell chemistry the battery reports, such as Li-ion or Li-poly.",
    ));
    add(Entity::new(
        "power.battery.{n}.serial_number",
        D::Power,
        K::Identity,
        Some(U::Identifier),
        P::Specification,
        true,
        "Serial number the battery reports for itself. Null where it reports none.",
    ));
    add(Entity::new(
        "power.battery.{n}.state",
        D::Power,
        K::Measurement,
        Some(U::Identifier),
        P::Measured,
        true,
        "Charging state: charging, discharging, full or notcharging. Null where the battery did not say.",
    ));
    add(Entity::new(
        "power.battery.{n}.health",
        D::Power,
        K::Measurement,
        Some(U::Identifier),
        P::Measured,
        true,
        "Health the battery reports: good, fair or poor. Null where the battery did not say.",
    ));
    add(Entity::new(
        "power.battery.{n}.voltage",
        D::Power,
        K::Measurement,
        Some(U::Volts),
        P::Measured,
        true,
        "Present terminal voltage of the battery.",
    ));
    add(Entity::new(
        "power.battery.{n}.power",
        D::Power,
        K::Measurement,
        Some(U::Watts),
        P::Measured,
        true,
        "Power the battery is currently drawing or supplying.",
    ));
    add(Entity::new(
        "power.battery.{n}.energy_now",
        D::Power,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Energy the battery holds now, in watt-hours. The unit is not one the ontology names, so it is stated here.",
    ));
    add(Entity::new(
        "power.battery.{n}.energy_full",
        D::Power,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Energy the battery holds when fully charged, in watt-hours. Falls as the battery wears.",
    ));
    add(Entity::new(
        "power.battery.{n}.energy_full_design",
        D::Power,
        K::Limit,
        Some(U::Count),
        P::Specification,
        true,
        "Energy the battery was designed to hold when new, in watt-hours, as the battery reports it.",
    ));
    add(Entity::new(
        "power.battery.{n}.time_to_empty",
        D::Power,
        K::Measurement,
        Some(U::Seconds),
        P::Measured,
        true,
        "Estimated time until the battery is empty at its present draw. Null while charging or where unreported.",
    ));
    add(Entity::new(
        "power.battery.{n}.time_to_full",
        D::Power,
        K::Measurement,
        Some(U::Seconds),
        P::Measured,
        true,
        "Estimated time until the battery is full at its present charge rate. Null while discharging or where unreported.",
    ));
    add(Entity::new(
        "power.battery.{n}.cycle_count",
        D::Power,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Charge cycles the battery has completed, as its own counter reports them.",
    ));
    add(Entity::new(
        "power.battery.{n}.temperature",
        D::Power,
        K::Measurement,
        Some(U::Celsius),
        P::Measured,
        true,
        "Temperature the battery reports for itself.",
    ));

    // ── Power supplies: mains, USB, UPS and the batteries behind them ───────
    add(Entity::new(
        "power.supply.<none>",
        D::Power,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no power supply was enumerated, carrying why.",
    ));
    add(Entity::new(
        "power.supply.{n}.type",
        D::Power,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Kind of supply: mains, battery, usb, usbpd or ups.",
    ));
    add(Entity::new(
        "power.supply.{n}.online",
        D::Power,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether this supply is currently online, such as a mains adapter plugged in.",
    ));
    add(Entity::new(
        "power.supply.{n}.status",
        D::Power,
        K::Measurement,
        Some(U::Identifier),
        P::Measured,
        true,
        "Charging status the supply reports: charging, discharging, full or notcharging.",
    ));
    add(Entity::new(
        "power.supply.{n}.health",
        D::Power,
        K::Measurement,
        Some(U::Identifier),
        P::Measured,
        true,
        "Health the supply reports, such as good or overheat. Null where it did not say.",
    ));
    add(Entity::new(
        "power.supply.{n}.voltage",
        D::Power,
        K::Measurement,
        Some(U::Volts),
        P::Measured,
        true,
        "Present voltage of the supply.",
    ));
    add(Entity::new(
        "power.supply.{n}.current",
        D::Power,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Present current of the supply, in amperes. Negative while the supply is discharging. The unit is not one the ontology names, so it is stated here.",
    ));
    add(Entity::new(
        "power.supply.{n}.power",
        D::Power,
        K::Measurement,
        Some(U::Watts),
        P::Measured,
        true,
        "Power the supply is currently delivering or drawing.",
    ));
    add(Entity::new(
        "power.supply.{n}.energy_now",
        D::Power,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Energy the supply holds now, in watt-hours. The unit is not one the ontology names, so it is stated here.",
    ));
    add(Entity::new(
        "power.supply.{n}.energy_full",
        D::Power,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Energy the supply holds when fully charged, in watt-hours.",
    ));
    add(Entity::new(
        "power.supply.{n}.energy_full_design",
        D::Power,
        K::Limit,
        Some(U::Count),
        P::Specification,
        true,
        "Energy the supply was designed to hold when new, in watt-hours, as it reports.",
    ));
    add(Entity::new(
        "power.supply.{n}.capacity",
        D::Power,
        K::Measurement,
        Some(U::Percent),
        P::Measured,
        true,
        "Charge level of the supply as a percentage of its full capacity.",
    ));
    add(Entity::new(
        "power.supply.{n}.cycle_count",
        D::Power,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Charge cycles the supply has completed, as its own counter reports them.",
    ));
    add(Entity::new(
        "power.supply.{n}.serial_number",
        D::Power,
        K::Identity,
        Some(U::Identifier),
        P::Specification,
        true,
        "Serial number the supply reports for itself. Null where it reports none.",
    ));
}

pub(super) fn resolve(out: &mut Vec<Reading>) {
    resolve_identity(out);
    resolve_firmware_detail(out);
    resolve_tpm_detail(out);
    resolve_chassis(out);
    resolve_motherboard_sensors(out);
    resolve_batteries(out);
    resolve_power_supplies(out);
}

// ── Helpers shared with `peripherals` ───────────────────────────────────────

/// Strings that firmware fills in where a vendor left a field unset. They are
/// stand-ins, not identities: an all-zero or all-F UUID is the SMBIOS "not set"
/// value, and "To Be Filled By O.E.M." is what boards ship with in serial fields.
fn is_standin(value: &str) -> bool {
    // One list for the reader and the ontology. It used to live only here, so
    // the chassis reader passed these strings through.
    crate::datacenter::is_firmware_placeholder(value)
}

/// A text reading. A blank or absent value is unavailable with `why`. A
/// firmware-declared value that is a stand-in is unavailable too, since passing
/// it through would present "To Be Filled By O.E.M." as a serial number.
pub(super) fn text_reading(
    out: &mut Vec<Reading>,
    id: impl Into<String>,
    value: Option<&str>,
    unit: U,
    why: &str,
    provenance: P,
) {
    let id = id.into();
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => out.push(Reading::unavailable(id, Some(unit), why)),
        Some(v) if provenance == P::Specification && is_standin(v) => {
            out.push(Reading::unavailable(
                id,
                Some(unit),
                format!(
                    "the firmware filled this field with {v:?}, a stand-in rather than a value"
                ),
            ))
        }
        Some(v) => push_str_as(out, id, v, unit, provenance),
    }
}

/// An enumerated state as its lowercase name. `Unknown` and a missing value are
/// the reader failing to decide, so they are unavailable rather than a state.
pub(super) fn enum_reading(
    out: &mut Vec<Reading>,
    id: impl Into<String>,
    debug_name: Option<String>,
    why: &str,
) {
    let id = id.into();
    match debug_name {
        Some(name) if name != "Unknown" => push_id(out, id, &name.to_lowercase()),
        _ => out.push(Reading::unavailable(id, Some(U::Identifier), why)),
    }
}

/// A JSON number rounded to three places, or `None` for a value that is not finite.
pub(super) fn finite(value: f64) -> Option<serde_json::Value> {
    value
        .is_finite()
        .then(|| serde_json::json!((value * 1000.0).round() / 1000.0))
}

// ── Resolvers ───────────────────────────────────────────────────────────────

fn resolve_identity(out: &mut Vec<Reading>) {
    const IDS: [(&str, U); 6] = [
        ("board.serial", U::Identifier),
        ("board.uuid", U::Identifier),
        ("board.version", U::Text),
        ("board.bios.version", U::Text),
        ("board.bios.release_date", U::Text),
        ("board.bios.revision", U::Text),
    ];

    let info = match crate::motherboard::get_system_info() {
        Ok(info) => info,
        Err(e) => {
            for (id, unit) in IDS {
                out.push(Reading::unavailable(
                    id,
                    Some(unit),
                    format!("the platform identity reader failed: {e}"),
                ));
            }
            return;
        }
    };

    let why = "the firmware's identity tables did not report this field on this platform";
    let spec = P::Specification;
    text_reading(
        out,
        "board.serial",
        info.serial_number.as_deref(),
        U::Identifier,
        why,
        spec,
    );
    text_reading(
        out,
        "board.uuid",
        info.uuid.as_deref(),
        U::Identifier,
        why,
        spec,
    );
    text_reading(
        out,
        "board.version",
        info.board_version.as_deref(),
        U::Text,
        why,
        spec,
    );
    text_reading(
        out,
        "board.bios.version",
        info.bios.version.as_deref(),
        U::Text,
        why,
        spec,
    );
    text_reading(
        out,
        "board.bios.release_date",
        info.bios.release_date.as_deref(),
        U::Text,
        why,
        spec,
    );
    text_reading(
        out,
        "board.bios.revision",
        info.bios.revision.as_deref(),
        U::Text,
        why,
        spec,
    );
}

fn resolve_firmware_detail(out: &mut Vec<Reading>) {
    let inventory = match crate::firmware::FirmwareInventory::new() {
        Ok(i) => i,
        Err(e) => {
            out.push(Reading::unavailable(
                "board.firmware.serial",
                Some(U::Identifier),
                format!("firmware inventory unavailable: {e}"),
            ));
            return;
        }
    };

    text_reading(
        out,
        "board.firmware.serial",
        inventory.system_serial.as_deref(),
        U::Identifier,
        "the firmware inventory did not report a system serial number on this platform",
        P::Specification,
    );

    let items = inventory.items();
    if items.is_empty() {
        out.push(Reading::unavailable(
            "board.firmware.<none>",
            None,
            "the firmware inventory enumerated no entries on this machine",
        ));
        return;
    }

    for (i, entry) in items.iter().enumerate() {
        let base = format!("board.firmware.{i}");
        text_reading(
            out,
            format!("{base}.vendor"),
            entry.vendor.as_deref(),
            U::Text,
            "the source did not name a vendor for this firmware entry",
            P::Specification,
        );
        text_reading(
            out,
            format!("{base}.date"),
            entry.date.as_deref(),
            U::Text,
            "the source did not give a release date for this firmware entry",
            P::Specification,
        );
        text_reading(
            out,
            format!("{base}.device"),
            Some(entry.device.as_str()),
            U::Identifier,
            "the firmware entry names no device",
            P::Measured,
        );
        push_opt(
            out,
            format!("{base}.updateable"),
            entry.updateable.map(|u| serde_json::json!(u)),
            None,
            "the source did not say whether the operating system can update this entry",
        );
    }
}

fn resolve_tpm_detail(out: &mut Vec<Reading>) {
    const IDS: [(&str, Option<U>); 4] = [
        ("board.tpm.device", Some(U::Identifier)),
        ("board.tpm.firmware_version", Some(U::Text)),
        ("board.tpm.algorithms", None),
        ("board.tpm.pcr_banks", Some(U::Count)),
    ];

    let monitor = match crate::tpm::TpmMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            for (id, unit) in IDS {
                out.push(Reading::unavailable(
                    id,
                    unit,
                    format!("TPM enumeration failed: {e}"),
                ));
            }
            return;
        }
    };

    let Some(tpm) = monitor.tpm() else {
        for (id, unit) in IDS {
            out.push(Reading::unavailable(
                id,
                unit,
                "no TPM present on this machine",
            ));
        }
        return;
    };

    text_reading(
        out,
        "board.tpm.device",
        Some(tpm.device.as_str()),
        U::Identifier,
        "the TPM reports no device name",
        P::Measured,
    );
    text_reading(
        out,
        "board.tpm.firmware_version",
        Some(tpm.firmware_version.as_str()),
        U::Text,
        "the TPM does not report a firmware version to this reader",
        P::Measured,
    );
    match &tpm.algorithms {
        Some(list) => {
            let names: Vec<&String> = list
                .iter()
                .filter(|a| !names_an_absence(a.as_str()))
                .collect();
            out.push(Reading::measured(
                "board.tpm.algorithms",
                serde_json::json!(names),
                None,
            ));
        }
        None => out.push(Reading::unavailable(
            "board.tpm.algorithms",
            None,
            "the TPM's algorithm list was not read on this platform",
        )),
    }
    push_opt(
        out,
        "board.tpm.pcr_banks",
        tpm.pcr_banks.map(|n| serde_json::json!(n)),
        Some(U::Count),
        "the PCR bank count was not read on this platform",
    );
}

fn resolve_chassis(out: &mut Vec<Reading>) {
    const IDS: [(&str, U); 6] = [
        ("board.chassis.serial", U::Identifier),
        ("board.chassis.asset_tag", U::Identifier),
        ("board.chassis.sku", U::Identifier),
        ("board.chassis.version", U::Text),
        ("board.chassis.manufacturer", U::Text),
        ("board.chassis.product_name", U::Text),
    ];

    let chassis = match crate::datacenter::ChassisInfo::detect() {
        Ok(c) => c,
        Err(e) => {
            for (id, unit) in IDS {
                out.push(Reading::unavailable(
                    id,
                    Some(unit),
                    format!("chassis detection failed: {e}"),
                ));
            }
            return;
        }
    };

    // Read live by the chassis reader, so Measured. The reader has already
    // dropped placeholders; `text_reading` only checks the empty case here.
    // The reader returns `None` for an absent field and for a firmware
    // placeholder ("System Product Name" on this board), so the reason names both.
    let why =
        "the chassis reader obtained no value: the field is absent or holds a firmware placeholder";
    let measured = P::Measured;
    text_reading(
        out,
        "board.chassis.serial",
        chassis.serial_number.as_deref(),
        U::Identifier,
        why,
        measured,
    );
    text_reading(
        out,
        "board.chassis.asset_tag",
        chassis.asset_tag.as_deref(),
        U::Identifier,
        why,
        measured,
    );
    text_reading(
        out,
        "board.chassis.sku",
        chassis.sku.as_deref(),
        U::Identifier,
        why,
        measured,
    );
    text_reading(
        out,
        "board.chassis.version",
        chassis.version.as_deref(),
        U::Text,
        why,
        measured,
    );
    text_reading(
        out,
        "board.chassis.manufacturer",
        chassis.manufacturer.as_deref(),
        U::Text,
        why,
        measured,
    );
    text_reading(
        out,
        "board.chassis.product_name",
        chassis.product_name.as_deref(),
        U::Text,
        why,
        measured,
    );
}

/// Voltage rails and fans, from every motherboard sensor device. One device
/// failing is reported as `<unreadable>` naming it, and the others still publish.
fn resolve_motherboard_sensors(out: &mut Vec<Reading>) {
    let devices = match crate::motherboard::enumerate_sensors() {
        Ok(d) => d,
        Err(e) => {
            for prefix in ["board.voltage", "board.fan"] {
                out.push(Reading::unavailable(
                    format!("{prefix}.<none>"),
                    None,
                    format!("motherboard sensor enumeration failed: {e}"),
                ));
            }
            return;
        }
    };

    let mut rails = Vec::new();
    let mut voltage_errors = Vec::new();
    let mut fans = Vec::new();
    let mut fan_errors = Vec::new();
    for device in &devices {
        let name = device.name().to_string();
        match device.voltage_rails() {
            Ok(list) => rails.extend(list),
            Err(e) => voltage_errors.push(format!("{name}: {e}")),
        }
        match device.fans() {
            Ok(list) => fans.extend(list),
            Err(e) => fan_errors.push(format!("{name}: {e}")),
        }
    }

    const NO_RAILS: &str = concat!(
        "no voltage rail was reported by the motherboard sensor source. On Windows ",
        "this means LibreHardwareMonitor or OpenHardwareMonitor is not running, because ",
        "WMI does not expose voltages"
    );
    if !voltage_errors.is_empty() {
        out.push(Reading::unavailable(
            "board.voltage.<unreadable>",
            None,
            voltage_errors.join("; "),
        ));
    } else if rails.is_empty() {
        out.push(Reading::unavailable("board.voltage.<none>", None, NO_RAILS));
    }
    for (i, rail) in rails.iter().enumerate() {
        let base = format!("board.voltage.{i}");
        push_text(out, format!("{base}.label"), &rail.label);
        push_opt(
            out,
            format!("{base}.voltage"),
            rail.voltage.and_then(|v| finite(f64::from(v))),
            Some(U::Volts),
            "the rail is listed by the sensor source but its voltage could not be sampled",
        );
        push_spec_opt(
            out,
            format!("{base}.min"),
            rail.min.and_then(|v| finite(f64::from(v))),
            Some(U::Volts),
            "the sensor source declares no lower limit for this rail",
        );
        push_spec_opt(
            out,
            format!("{base}.max"),
            rail.max.and_then(|v| finite(f64::from(v))),
            Some(U::Volts),
            "the sensor source declares no upper limit for this rail",
        );
    }

    const NO_FANS: &str = concat!(
        "no fan was reported by the motherboard sensor source. On Windows this means ",
        "LibreHardwareMonitor is not running and the WMI fan class is empty"
    );
    if !fan_errors.is_empty() {
        out.push(Reading::unavailable(
            "board.fan.<unreadable>",
            None,
            fan_errors.join("; "),
        ));
    } else if fans.is_empty() {
        out.push(Reading::unavailable("board.fan.<none>", None, NO_FANS));
    }
    for (i, fan) in fans.iter().enumerate() {
        let base = format!("board.fan.{i}");
        push_text(out, format!("{base}.label"), &fan.label);
        push_opt(
            out,
            format!("{base}.rpm"),
            fan.rpm.map(|r| serde_json::json!(r)),
            Some(U::Rpm),
            "the fan is stopped or its speed cannot be measured from this source",
        );
        push_opt(
            out,
            format!("{base}.pwm"),
            fan.pwm.map(|p| serde_json::json!(p)),
            Some(U::Count),
            "this fan exposes no PWM duty cycle",
        );
        push_spec_opt(
            out,
            format!("{base}.min_rpm"),
            fan.min_rpm.map(|r| serde_json::json!(r)),
            Some(U::Rpm),
            "the sensor source declares no minimum speed for this fan",
        );
        push_spec_opt(
            out,
            format!("{base}.max_rpm"),
            fan.max_rpm.map(|r| serde_json::json!(r)),
            Some(U::Rpm),
            "the sensor source declares no maximum speed for this fan",
        );
        push_opt(
            out,
            format!("{base}.controllable"),
            fan.controllable.map(|c| serde_json::json!(c)),
            None,
            "the sensor source did not determine whether this fan can be controlled",
        );
    }
}

fn resolve_batteries(out: &mut Vec<Reading>) {
    let monitor = match crate::battery::BatteryMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "power.battery.<none>",
                None,
                format!("battery enumeration failed: {e}"),
            ));
            return;
        }
    };

    let batteries = monitor.batteries();
    if batteries.is_empty() {
        out.push(Reading::unavailable(
            "power.battery.<none>",
            None,
            "no battery present (desktop or always-AC machine)",
        ));
        return;
    }

    for (i, b) in batteries.iter().enumerate() {
        let base = format!("power.battery.{i}");
        let why = "the battery did not report this value";
        text_reading(
            out,
            format!("{base}.manufacturer"),
            b.manufacturer.as_deref(),
            U::Text,
            why,
            P::Specification,
        );
        text_reading(
            out,
            format!("{base}.model"),
            b.model.as_deref(),
            U::Text,
            why,
            P::Specification,
        );
        text_reading(
            out,
            format!("{base}.technology"),
            b.technology.as_deref(),
            U::Text,
            why,
            P::Specification,
        );
        text_reading(
            out,
            format!("{base}.serial_number"),
            b.serial_number.as_deref(),
            U::Identifier,
            why,
            P::Specification,
        );
        enum_reading(
            out,
            format!("{base}.state"),
            b.state.as_ref().map(|s| format!("{s:?}")),
            "the battery did not say whether it is charging",
        );
        enum_reading(
            out,
            format!("{base}.health"),
            b.health.as_ref().map(|h| format!("{h:?}")),
            "the battery did not report its health",
        );
        push_opt(
            out,
            format!("{base}.voltage"),
            b.voltage_mv.and_then(|mv| finite(f64::from(mv) / 1000.0)),
            Some(U::Volts),
            why,
        );
        push_opt(
            out,
            format!("{base}.power"),
            b.power_w.and_then(|w| finite(f64::from(w))),
            Some(U::Watts),
            why,
        );
        push_opt(
            out,
            format!("{base}.energy_now"),
            b.energy_wh.and_then(|e| finite(f64::from(e))),
            Some(U::Count),
            why,
        );
        push_opt(
            out,
            format!("{base}.energy_full"),
            b.energy_full_wh.and_then(|e| finite(f64::from(e))),
            Some(U::Count),
            why,
        );
        push_spec_opt(
            out,
            format!("{base}.energy_full_design"),
            b.energy_full_design_wh.and_then(|e| finite(f64::from(e))),
            Some(U::Count),
            why,
        );
        push_opt(
            out,
            format!("{base}.time_to_empty"),
            b.time_to_empty.map(|d| serde_json::json!(d.as_secs())),
            Some(U::Seconds),
            "the battery did not estimate time to empty",
        );
        push_opt(
            out,
            format!("{base}.time_to_full"),
            b.time_to_full.map(|d| serde_json::json!(d.as_secs())),
            Some(U::Seconds),
            "the battery did not estimate time to full",
        );
        push_opt(
            out,
            format!("{base}.cycle_count"),
            b.cycle_count.map(|c| serde_json::json!(c)),
            Some(U::Count),
            "the battery does not report a cycle count",
        );
        push_opt(
            out,
            format!("{base}.temperature"),
            b.temperature_c.and_then(|t| finite(f64::from(t))),
            Some(U::Celsius),
            "the battery does not report a temperature",
        );
    }
}

fn resolve_power_supplies(out: &mut Vec<Reading>) {
    let monitor = match crate::power_supply::PowerSupplyMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "power.supply.<none>",
                None,
                format!("power supply enumeration failed: {e}"),
            ));
            return;
        }
    };

    let supplies = monitor.supplies();
    if supplies.is_empty() {
        out.push(Reading::unavailable(
            "power.supply.<none>",
            None,
            "no power supply was enumerated on this machine",
        ));
        return;
    }

    for (i, s) in supplies.iter().enumerate() {
        let base = format!("power.supply.{i}");
        enum_reading(
            out,
            format!("{base}.type"),
            Some(format!("{:?}", s.supply_type)),
            "the supply did not report its kind",
        );
        push_opt(
            out,
            format!("{base}.online"),
            s.online.map(|o| serde_json::json!(o)),
            None,
            "the supply did not report whether it is online",
        );
        enum_reading(
            out,
            format!("{base}.status"),
            s.status.as_ref().map(|c| format!("{c:?}")),
            "the supply did not report its charging status",
        );
        enum_reading(
            out,
            format!("{base}.health"),
            s.health.as_ref().map(|h| format!("{h:?}")),
            "the supply did not report its health",
        );
        let why = "the supply did not report this value";
        push_opt(
            out,
            format!("{base}.voltage"),
            s.voltage_v().and_then(|v| finite(f64::from(v))),
            Some(U::Volts),
            why,
        );
        push_opt(
            out,
            format!("{base}.current"),
            s.current_a().and_then(|c| finite(f64::from(c))),
            None,
            why,
        );
        push_opt(
            out,
            format!("{base}.power"),
            s.power_w().and_then(|w| finite(f64::from(w))),
            Some(U::Watts),
            why,
        );
        push_opt(
            out,
            format!("{base}.energy_now"),
            s.energy_wh().and_then(|e| finite(f64::from(e))),
            Some(U::Count),
            why,
        );
        push_opt(
            out,
            format!("{base}.energy_full"),
            s.energy_full_wh().and_then(|e| finite(f64::from(e))),
            Some(U::Count),
            why,
        );
        push_spec_opt(
            out,
            format!("{base}.energy_full_design"),
            s.energy_full_design_wh().and_then(|e| finite(f64::from(e))),
            Some(U::Count),
            why,
        );
        push_opt(
            out,
            format!("{base}.capacity"),
            s.capacity_percent.map(|c| serde_json::json!(c)),
            Some(U::Percent),
            why,
        );
        push_opt(
            out,
            format!("{base}.cycle_count"),
            s.cycle_count.map(|c| serde_json::json!(c)),
            Some(U::Count),
            "the supply does not report a cycle count",
        );
        text_reading(
            out,
            format!("{base}.serial_number"),
            s.serial_number.as_deref(),
            U::Identifier,
            "the supply reports no serial number",
            P::Specification,
        );
    }
}
