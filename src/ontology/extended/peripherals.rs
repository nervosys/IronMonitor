//! USB, display, camera, input, sensor and Bluetooth detail the base resolvers omit.

#[allow(unused_imports)]
use super::super::resolve::*;
#[allow(unused_imports)]
use super::super::{Domain as D, Entity, EntityKind as K, Provenance as P, Unit as U};
use super::board::{enum_reading, finite, text_reading};

pub(super) fn declare(add: &mut dyn FnMut(Entity)) {
    // ── USB ─────────────────────────────────────────────────────────────────
    add(Entity::new(
        "usb.{addr}.serial_number",
        D::Usb,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Serial number string the device reports in its descriptor. Null where the device publishes none.",
    ));
    add(Entity::new(
        "usb.{addr}.bus_number",
        D::Usb,
        K::Identity,
        Some(U::Count),
        P::Measured,
        true,
        "Number of the USB bus the device hangs off. Null where the platform does not report one, as for some virtual devices.",
    ));
    add(Entity::new(
        "usb.{addr}.port_number",
        D::Usb,
        K::Identity,
        Some(U::Count),
        P::Measured,
        true,
        "Port on the parent hub the device is plugged into. Null where the platform does not report one.",
    ));
    add(Entity::new(
        "usb.{addr}.description",
        D::Usb,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Description the platform gives the device, which can differ from its product string. Null where the platform gives none.",
    ));

    // ── Displays ────────────────────────────────────────────────────────────
    add(Entity::new(
        "board.display.{n}.manufacturer",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "Manufacturer code the display's own EDID block declares. Null where no EDID was read.",
    ));
    add(Entity::new(
        "board.display.{n}.physical_width_mm",
        D::Board,
        K::Limit,
        Some(U::Count),
        P::Specification,
        true,
        "Width of the visible image area in millimetres, as the display's EDID declares it. Null where no EDID size was read.",
    ));
    add(Entity::new(
        "board.display.{n}.physical_height_mm",
        D::Board,
        K::Limit,
        Some(U::Count),
        P::Specification,
        true,
        "Height of the visible image area in millimetres, as the display's EDID declares it. Null where no EDID size was read.",
    ));
    add(Entity::new(
        "board.display.{n}.scale_factor",
        D::Board,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Operating-system display scaling as a dimensionless ratio, where 1.0 is 100 percent. Null where the platform reports none.",
    ));
    add(Entity::new(
        "board.display.{n}.bits_per_pixel",
        D::Board,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Colour depth of the display's current mode, in bits per pixel. Null where the mode was not readable.",
    ));
    add(Entity::new(
        "board.display.{n}.brightness",
        D::Board,
        K::Measurement,
        Some(U::Percent),
        P::Measured,
        true,
        "Backlight level as a percentage of the maximum, read from the panel's backlight control. Null where the display exposes no backlight level.",
    ));
    add(Entity::new(
        "board.display.{n}.hdr",
        D::Board,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "HDR mode the display is using, such as hdr10, hdr10_plus or dolby_vision, or null where it was not read. No reader fills it: the Windows advanced-colour query says whether HDR is on, not which format, so this stays null. Use hdr_supported and hdr_enabled instead. Null means unknown, not standard dynamic range.",
    ));
    add(Entity::new(
        "board.display.{n}.hdr_supported",
        D::Board,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether the display can use HDR (advanced colour), as the Windows display configuration reports it. Null where the platform does not report it, which is Linux and macOS today, or where the query did not answer for this display.",
    ));
    add(Entity::new(
        "board.display.{n}.hdr_enabled",
        D::Board,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether HDR (advanced colour) is switched on for the display, as the Windows display configuration reports it. Null where the platform does not report it, which is Linux and macOS today, or where the query did not answer for this display.",
    ));
    add(Entity::new(
        "board.display.{n}.bits_per_color_channel",
        D::Board,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Bits per colour channel the display path is driving, as the Windows display configuration reports it. Null where the platform does not report it, which is Linux and macOS today, or where the query did not answer for this display.",
    ));

    // ── Cameras ─────────────────────────────────────────────────────────────
    add(Entity::new(
        "board.camera.{n}.vendor",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Vendor of the camera as the platform reports it. Null where the platform names none.",
    ));
    add(Entity::new(
        "board.camera.{n}.formats",
        D::Board,
        K::Identity,
        None,
        P::Measured,
        true,
        "JSON array of the pixel formats the camera offers, such as YUYV, MJPG or H264, as its driver enumerates them. Null where they were not read: only the Linux V4L2 path enumerates them.",
    ));

    // ── Audio endpoints ─────────────────────────────────────────────────────
    add(Entity::new(
        "board.audio.{n}.enabled",
        D::Board,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether the audio endpoint is enabled, from the endpoint's device state on Windows. Null where the platform does not report an enable state, which is Linux and macOS today.",
    ));

    // ── Input devices ───────────────────────────────────────────────────────
    add(Entity::new(
        "board.input.{n}.capabilities",
        D::Board,
        K::Identity,
        None,
        P::Measured,
        true,
        "JSON array of the capabilities the input device advertises, such as its event types. Null where the platform reports none.",
    ));

    // ── Sensor devices ──────────────────────────────────────────────────────
    add(Entity::new(
        "board.sensor.{n}.device",
        D::Board,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Device node or identifier the sensor is exposed through.",
    ));
    add(Entity::new(
        "board.sensor.{n}.vendor",
        D::Board,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Vendor of the sensor chip as the platform reports it. Null where none is reported.",
    ));

    // ── Bluetooth: adapters and the devices they know ───────────────────────
    add(Entity::new(
        "network.bluetooth.{n}.address",
        D::Network,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Bluetooth address of the adapter, the radio's own hardware address.",
    ));
    add(Entity::new(
        "network.bluetooth.device.<none>",
        D::Network,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no paired or known Bluetooth device was enumerated, carrying why.",
    ));
    add(Entity::new(
        "network.bluetooth.device.{n}.address",
        D::Network,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Bluetooth address of a paired or known remote device.",
    ));
    add(Entity::new(
        "network.bluetooth.device.{n}.name",
        D::Network,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Name the remote device advertises, or the name it was paired under. Null where it has none.",
    ));
    add(Entity::new(
        "network.bluetooth.device.{n}.device_type",
        D::Network,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Kind of device from its class of device: computer, phone, headset, speaker, keyboard, mouse or game_controller. Null where it did not identify itself.",
    ));
    add(Entity::new(
        "network.bluetooth.device.{n}.state",
        D::Network,
        K::Measurement,
        Some(U::Identifier),
        P::Measured,
        true,
        "Connection state: connected, paired, discovered or disconnected.",
    ));
    add(Entity::new(
        "network.bluetooth.device.{n}.battery_percent",
        D::Network,
        K::Measurement,
        Some(U::Percent),
        P::Measured,
        true,
        "Battery level the device reports over Bluetooth. Null where the device reports none.",
    ));
}

pub(super) fn resolve(out: &mut Vec<Reading>) {
    resolve_usb_detail(out);
    resolve_display_detail(out);
    resolve_camera_detail(out);
    resolve_audio_detail(out);
    resolve_input_detail(out);
    resolve_sensor_detail(out);
    resolve_bluetooth_detail(out);
}

// Each reader is constructed once here. When a reader fails or enumerates
// nothing, the base resolver has already said so with a `<none>` diagnostic
// covering these prefixes, so there is nothing to add and returning is correct.

fn resolve_usb_detail(out: &mut Vec<Reading>) {
    let Ok(monitor) = crate::usb::UsbMonitor::new() else {
        return;
    };
    for dev in monitor.devices() {
        // The same id segment the base `usb.{addr}.*` rows use, so these join onto them.
        let base = format!("usb.{}", id_segment(&dev.address));
        text_reading(
            out,
            format!("{base}.serial_number"),
            dev.serial_number.as_deref(),
            U::Identifier,
            "the device publishes no serial number string",
            P::Measured,
        );
        push_opt(
            out,
            format!("{base}.bus_number"),
            dev.bus_number.map(|b| serde_json::json!(b)),
            Some(U::Count),
            "the platform reports no bus number for this device",
        );
        push_opt(
            out,
            format!("{base}.port_number"),
            dev.port_number.map(|p| serde_json::json!(p)),
            Some(U::Count),
            "the platform reports no port number for this device",
        );
        text_reading(
            out,
            format!("{base}.description"),
            dev.description.as_deref(),
            U::Text,
            "the platform gives this device no description",
            P::Measured,
        );
    }
}

fn resolve_display_detail(out: &mut Vec<Reading>) {
    let Ok(monitor) = crate::display::DisplayMonitor::new() else {
        return;
    };
    for (i, d) in monitor.displays().iter().enumerate() {
        let base = format!("board.display.{i}");
        text_reading(
            out,
            format!("{base}.manufacturer"),
            d.manufacturer.as_deref(),
            U::Text,
            "no EDID manufacturer was read for this display",
            P::Specification,
        );
        push_spec_opt(
            out,
            format!("{base}.physical_width_mm"),
            d.physical_width_mm.map(|w| serde_json::json!(w)),
            Some(U::Count),
            "no EDID physical width was read for this display",
        );
        push_spec_opt(
            out,
            format!("{base}.physical_height_mm"),
            d.physical_height_mm.map(|h| serde_json::json!(h)),
            Some(U::Count),
            "no EDID physical height was read for this display",
        );
        push_opt(
            out,
            format!("{base}.scale_factor"),
            d.scale_factor.and_then(finite),
            None,
            "the platform reports no scaling factor for this display",
        );
        push_opt(
            out,
            format!("{base}.bits_per_pixel"),
            d.bits_per_pixel.map(|b| serde_json::json!(b)),
            Some(U::Count),
            "the display's current mode was not readable, so its colour depth is unknown",
        );
        push_opt(
            out,
            format!("{base}.brightness"),
            d.brightness.and_then(|b| finite(f64::from(b) * 100.0)),
            Some(U::Percent),
            "this display exposes no backlight level",
        );
        // No reader fills `hdr` yet, so this is always absent. It says so rather
        // than publishing the standard-range mode that the field used to hold.
        push_opt(
            out,
            format!("{base}.hdr"),
            d.hdr
                .map(|mode| serde_json::json!(format!("{mode:?}").to_lowercase())),
            Some(U::Identifier),
            concat!(
                "the display's HDR format is not read on any platform: the Windows ",
                "advanced-colour query reports only whether HDR is on, the DRM connector ",
                "properties and the macOS display profile are not parsed, so the mode is ",
                "unknown, not standard"
            ),
        );
        // Advanced-colour state is read only by the Windows DisplayConfig query.
        // Linux and macOS have no reader for it, and a Windows display the query
        // did not answer for is unread in the same way.
        push_opt(
            out,
            format!("{base}.hdr_supported"),
            d.hdr_supported.map(|b| serde_json::json!(b)),
            None,
            concat!(
                "the platform's display configuration did not report HDR support for this ",
                "display: only the Windows DisplayConfig query reads it"
            ),
        );
        push_opt(
            out,
            format!("{base}.hdr_enabled"),
            d.hdr_enabled.map(|b| serde_json::json!(b)),
            None,
            concat!(
                "the platform's display configuration did not report whether HDR is on for ",
                "this display: only the Windows DisplayConfig query reads it"
            ),
        );
        push_opt(
            out,
            format!("{base}.bits_per_color_channel"),
            d.bits_per_color_channel.map(|b| serde_json::json!(b)),
            Some(U::Count),
            concat!(
                "the platform's display configuration did not report the colour depth of this ",
                "display: only the Windows DisplayConfig query reads it"
            ),
        );
    }
}

fn resolve_camera_detail(out: &mut Vec<Reading>) {
    let Ok(monitor) = crate::camera::CameraMonitor::new() else {
        return;
    };
    for (i, c) in monitor.cameras().iter().enumerate() {
        text_reading(
            out,
            format!("board.camera.{i}.vendor"),
            c.vendor.as_deref(),
            U::Text,
            "the platform names no vendor for this camera",
            P::Measured,
        );
        push_opt(
            out,
            format!("board.camera.{i}.formats"),
            c.formats.as_ref().map(|f| serde_json::json!(f)),
            None,
            concat!(
                "this camera's pixel formats were not read: only the Linux V4L2 path ",
                "enumerates them, and on other platforms no reader does. On Linux, a ",
                "device that refuses the enumeration or lists no capture format is unread"
            ),
        );
    }
}

fn resolve_audio_detail(out: &mut Vec<Reading>) {
    let Ok(monitor) = crate::audio::AudioMonitor::new() else {
        return;
    };
    for (i, d) in monitor.devices().iter().enumerate() {
        push_opt(
            out,
            format!("board.audio.{i}.enabled"),
            d.is_enabled.map(|e| serde_json::json!(e)),
            None,
            "the platform reports no enable state for this endpoint",
        );
    }
}

fn resolve_input_detail(out: &mut Vec<Reading>) {
    let Ok(monitor) = crate::input::InputMonitor::new() else {
        return;
    };
    for (i, d) in monitor.devices().iter().enumerate() {
        let capabilities: Vec<&String> = d
            .capabilities
            .iter()
            .filter(|c| !names_an_absence(c.as_str()))
            .collect();
        if capabilities.is_empty() {
            out.push(Reading::unavailable(
                format!("board.input.{i}.capabilities"),
                None,
                "the platform reported no capability list for this input device",
            ));
        } else {
            out.push(Reading::measured(
                format!("board.input.{i}.capabilities"),
                serde_json::json!(capabilities),
                None,
            ));
        }
    }
}

fn resolve_sensor_detail(out: &mut Vec<Reading>) {
    let Ok(monitor) = crate::sensors::SensorMonitor::new() else {
        return;
    };
    for (i, s) in monitor.sensors().iter().enumerate() {
        let base = format!("board.sensor.{i}");
        text_reading(
            out,
            format!("{base}.device"),
            Some(s.device.as_str()),
            U::Identifier,
            "the sensor names no device node",
            P::Measured,
        );
        text_reading(
            out,
            format!("{base}.vendor"),
            Some(s.vendor.as_str()),
            U::Text,
            "the platform names no vendor for this sensor",
            P::Measured,
        );
    }
}

fn resolve_bluetooth_detail(out: &mut Vec<Reading>) {
    let Ok(monitor) = crate::bluetooth::BluetoothMonitor::new() else {
        return;
    };

    for (i, a) in monitor.adapters().iter().enumerate() {
        text_reading(
            out,
            format!("network.bluetooth.{i}.address"),
            a.address.as_deref().and_then(bd_address),
            U::Identifier,
            "the platform did not report this adapter's hardware address",
            P::Measured,
        );
    }

    let devices = monitor.devices();
    if devices.is_empty() {
        out.push(Reading::unavailable(
            "network.bluetooth.device.<none>",
            None,
            "no paired or known Bluetooth device was enumerated on this machine",
        ));
        return;
    }

    for (i, d) in devices.iter().enumerate() {
        let base = format!("network.bluetooth.device.{i}");
        text_reading(
            out,
            format!("{base}.address"),
            d.address.as_deref().and_then(bd_address),
            U::Identifier,
            "the device record carries no Bluetooth address",
            P::Measured,
        );
        text_reading(
            out,
            format!("{base}.name"),
            d.name.as_deref(),
            U::Text,
            "the device advertises no name",
            P::Measured,
        );
        enum_reading(
            out,
            format!("{base}.device_type"),
            Some(format!("{:?}", d.device_type)),
            "the device's class of device did not identify its type",
        );
        // `Unknown` is published as unavailable with its reason, not as the
        // identifier "unknown", which reads as a measured state.
        enum_reading(
            out,
            format!("{base}.state"),
            Some(format!("{:?}", d.state)),
            "the platform did not report this device's connection state",
        );
        push_opt(
            out,
            format!("{base}.battery_percent"),
            d.battery_percent.map(|p| serde_json::json!(p)),
            Some(U::Percent),
            "the device reports no battery level",
        );
    }
}

/// A Bluetooth address is six hex octets. Anything else is not an address and
/// is not published as one. The adapter's address is already normalised by the
/// reader; the device address is checked here as it always was.
fn bd_address(raw: &str) -> Option<&str> {
    let v = raw.trim();
    let octets: Vec<&str> = v.split([':', '-']).collect();
    let is_address = octets.len() == 6
        && octets
            .iter()
            .all(|o| o.len() == 2 && o.chars().all(|c| c.is_ascii_hexdigit()));
    // All zeros is no device's address: the Windows reader writes it for a
    // peripheral whose PnP id carries none, and it was published as measured.
    let is_placeholder = octets.iter().all(|o| *o == "00");
    (is_address && !is_placeholder).then_some(v)
}
