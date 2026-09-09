//! Example: USB device event monitoring
//!
//! Demonstrates polling for USB device connect/disconnect events.
//!
//! Run with: cargo run --example usb_events --features cli

use ironmonlib::usb::{UsbEvent, UsbMonitor};
use std::thread;
use std::time::Duration;

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
    println!("USB Device Event Monitor");
    println!("========================");
    println!("Watching for USB device connect/disconnect events...");
    println!("Plug in or remove a USB device to see events.");
    println!("Press Ctrl+C to exit.\n");

    let mut monitor = UsbMonitor::new()?;

    // Initial device list
    println!("Currently connected devices:");
    for device in monitor.devices() {
        println!(
            "  {} - {} ({:?})",
            ids(device),
            device.product.as_deref().unwrap_or("Unknown"),
            device.speed
        );
    }
    println!();

    // Poll for events
    loop {
        thread::sleep(Duration::from_secs(1));

        let events = monitor.poll_events()?;

        for event in events {
            match event {
                UsbEvent::Connected(device) => {
                    println!(
                        "[+] CONNECTED: {} - {} ({:?})",
                        ids(&device),
                        device.product.as_deref().unwrap_or("Unknown"),
                        device.speed
                    );
                }
                UsbEvent::Disconnected(device) => {
                    println!(
                        "[-] DISCONNECTED: {} - {} ({:?})",
                        ids(&device),
                        device.product.as_deref().unwrap_or("Unknown"),
                        device.speed
                    );
                }
            }
        }
    }
}
