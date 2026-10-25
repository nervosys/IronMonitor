//! Bluetooth device monitoring example
//!
//! Demonstrates how to enumerate Bluetooth adapters and paired devices.
//!
//! Run with: cargo run --example bluetooth_monitor

use ironmonitor::bluetooth::{BluetoothDeviceType, BluetoothMonitor, BluetoothState};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Bluetooth Monitor Example ===\n");

    let monitor = BluetoothMonitor::new()?;

    // Check availability
    match monitor.is_available() {
        Some(true) => {}
        Some(false) => {
            println!("No Bluetooth adapter was found on this system.");
            return Ok(());
        }
        None => {
            println!("Bluetooth adapters were not read on this platform.");
            println!("(No Bluetooth monitoring reader runs here)");
            return Ok(());
        }
    }

    // List adapters
    let adapters = monitor.adapters();
    println!("Found {} Bluetooth adapter(s):\n", adapters.len());

    for adapter in adapters {
        println!(
            "  📡 {}",
            adapter.name.as_deref().unwrap_or("name not read")
        );
        println!("    ID: {}", adapter.id);
        println!(
            "    Address: {}",
            adapter.address.as_deref().unwrap_or("not reported")
        );
        println!(
            "    Powered: {}",
            match adapter.powered {
                Some(true) => "Yes",
                Some(false) => "No",
                None => "not read",
            }
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
            BluetoothState::Unknown => "State not read",
        };

        let name = device.name.as_deref().unwrap_or("Unknown Device");
        println!("  {} {}", type_emoji, name);
        println!(
            "    Address: {}",
            device.address.as_deref().unwrap_or("not read")
        );
        println!("    Type: {:?}", device.device_type);
        println!("    State: {}", state_str);
        if let Some(battery) = device.battery_percent {
            println!("    Battery: {}%", battery);
        }
        println!();
    }

    Ok(())
}
