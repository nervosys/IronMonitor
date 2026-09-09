//! Bluetooth device monitoring example
//!
//! Demonstrates how to enumerate Bluetooth adapters and paired devices.
//!
//! Run with: cargo run --example bluetooth_monitor

use ironmonlib::bluetooth::{BluetoothDeviceType, BluetoothMonitor, BluetoothState};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Bluetooth Monitor Example ===\n");

    let monitor = BluetoothMonitor::new()?;

    // Check availability
    if !monitor.is_available() {
        println!("Bluetooth is not available on this system.");
        println!("(This is expected on platforms without Bluetooth monitoring support)");
        return Ok(());
    }

    // List adapters
    let adapters = monitor.adapters();
    println!("Found {} Bluetooth adapter(s):\n", adapters.len());

    for adapter in adapters {
        println!("  📡 {}", adapter.name);
        println!("    ID: {}", adapter.id);
        println!("    Address: {}", adapter.address);
        println!(
            "    Powered: {}",
            if adapter.powered { "Yes" } else { "No" }
        );
        println!();
    }

    // List paired/discovered devices
    let devices = monitor.devices();
    println!("Found {} Bluetooth device(s):\n", devices.len());

    for device in devices {
        let type_emoji = match device.device_type {
            BluetoothDeviceType::Phone => "📱",
            BluetoothDeviceType::Computer => "💻",
            BluetoothDeviceType::Headset | BluetoothDeviceType::Speaker => "🎧",
            BluetoothDeviceType::Keyboard => "⌨️",
            BluetoothDeviceType::Mouse => "🖱️",
            BluetoothDeviceType::GameController => "🎮",
            BluetoothDeviceType::Unknown => "❓",
        };

        let state_str = match device.state {
            BluetoothState::Connected => "Connected",
            BluetoothState::Paired => "Paired",
            BluetoothState::Discovered => "Discovered",
            BluetoothState::Disconnected => "Disconnected",
        };

        let name = device.name.as_deref().unwrap_or("Unknown Device");
        println!("  {} {}", type_emoji, name);
        println!("    Address: {}", device.address);
        println!("    Type: {:?}", device.device_type);
        println!("    State: {}", state_str);
        if let Some(battery) = device.battery_percent {
            println!("    Battery: {}%", battery);
        }
        println!();
    }

    Ok(())
}
