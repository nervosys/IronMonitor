//! USB device monitoring example
//!
//! Demonstrates how to enumerate USB devices and get their details.
//!
//! Run with: cargo run --example usb_monitor

use ironmonlib::usb::{UsbDeviceClass, UsbMonitor, UsbSpeed};

/// `vvvv:pppp`, or `no usb ids` for an entry that carries none — a root hub's
/// PnP id has no `VID_` at all.
fn ids(device: &ironmonlib::usb::UsbDevice) -> String {
    match (device.vendor_id, device.product_id) {
        (None, None) => "no usb ids".to_string(),
        (v, p) => format!(
            "{}:{}",
            v.map_or("----".to_string(), |v| format!("{v:04x}")),
            p.map_or("----".to_string(), |p| format!("{p:04x}"))
        ),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== USB Monitor Example ===\n");

    let monitor = UsbMonitor::new()?;

    let devices = monitor.devices();
    println!("Found {} USB device(s):\n", devices.len());

    for device in devices {
        let class_emoji = match device.class {
            UsbDeviceClass::Audio => "🔊",
            UsbDeviceClass::Communication => "📡",
            UsbDeviceClass::Hid => "🖱️",
            UsbDeviceClass::Printer => "🖨️",
            UsbDeviceClass::MassStorage => "💾",
            UsbDeviceClass::Hub => "🔌",
            UsbDeviceClass::Video => "📹",
            UsbDeviceClass::Wireless => "📶",
            UsbDeviceClass::Vendor => "🏭",
            UsbDeviceClass::Unknown => "❓",
        };

        let speed_str = match device.speed {
            UsbSpeed::Low => "Low (1.5 Mbps)",
            UsbSpeed::Full => "Full (12 Mbps)",
            UsbSpeed::High => "High (480 Mbps)",
            UsbSpeed::Super => "SuperSpeed (5 Gbps)",
            UsbSpeed::SuperPlus => "SuperSpeed+ (10 Gbps)",
            UsbSpeed::SuperPlusx2 => "SuperSpeed+ 20 (20 Gbps)",
            UsbSpeed::Usb4 => "USB4 (40 Gbps)",
            UsbSpeed::Unknown => "Unknown",
        };

        let name = device
            .product
            .as_deref()
            .or(device.description.as_deref())
            .unwrap_or("Unknown Device");

        println!("{} {}", class_emoji, name);
        println!("  VID:PID: {}", ids(device));

        if let Some(manufacturer) = &device.manufacturer {
            println!("  Manufacturer: {}", manufacturer);
        }

        if let Some(serial) = &device.serial_number {
            println!("  Serial: {}", serial);
        }

        println!("  Class: {:?}", device.class);
        println!("  Speed: {}", speed_str);
        // The address is the stable key and is always present; bus and port
        // are reported only where the platform gives them, which on Windows is
        // nowhere. They printed "0/0" for every device before this was an
        // `Option`.
        println!("  Address: {}", device.address);
        let slot = |v: Option<u8>| match v {
            Some(x) => x.to_string(),
            None => "-".to_string(),
        };
        println!(
            "  Bus/Port: {}/{}",
            slot(device.bus_number),
            slot(device.port_number)
        );
        println!();
    }

    // Summary by class
    if !devices.is_empty() {
        println!("Devices by Class:");

        let count_by_class = |class: UsbDeviceClass| {
            devices
                .iter()
                .filter(|d| std::mem::discriminant(&d.class) == std::mem::discriminant(&class))
                .count()
        };

        let classes = [
            ("Hubs", UsbDeviceClass::Hub),
            ("Storage", UsbDeviceClass::MassStorage),
            ("HID (Keyboard/Mouse)", UsbDeviceClass::Hid),
            ("Audio", UsbDeviceClass::Audio),
            ("Video", UsbDeviceClass::Video),
            ("Communication", UsbDeviceClass::Communication),
            ("Wireless", UsbDeviceClass::Wireless),
            ("Printer", UsbDeviceClass::Printer),
            ("Vendor-specific", UsbDeviceClass::Vendor),
            ("Unknown", UsbDeviceClass::Unknown),
        ];

        for (name, class) in classes {
            let count = count_by_class(class);
            if count > 0 {
                println!("  {}: {}", name, count);
            }
        }

        // Speed summary
        println!("\nDevices by Speed:");
        let speeds = [
            ("USB4 40 Gbps", UsbSpeed::Usb4),
            ("SuperSpeed+ 20 Gbps", UsbSpeed::SuperPlusx2),
            ("SuperSpeed+ 10 Gbps", UsbSpeed::SuperPlus),
            ("SuperSpeed 5 Gbps", UsbSpeed::Super),
            ("High Speed 480 Mbps", UsbSpeed::High),
            ("Full Speed 12 Mbps", UsbSpeed::Full),
            ("Low Speed 1.5 Mbps", UsbSpeed::Low),
        ];

        for (name, speed) in speeds {
            let count = devices
                .iter()
                .filter(|d| std::mem::discriminant(&d.speed) == std::mem::discriminant(&speed))
                .count();
            if count > 0 {
                println!("  {}: {}", name, count);
            }
        }
    }

    Ok(())
}
