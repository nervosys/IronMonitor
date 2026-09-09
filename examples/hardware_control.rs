//! Hardware Control APIs Example
//!
//! Demonstrates the hardware control capabilities for audio and Bluetooth.
//!
//! **Note**: These are stub implementations that update internal state but don't
//! actually control hardware. Full implementation requires platform-specific APIs.
//!
//! Run with: `cargo run --example hardware_control --features cli`

use ironmonlib::{AudioMonitor, BluetoothMonitor};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Hardware Control APIs Demo ===\n");

    // Audio Control Demo
    println!("--- Audio Control ---");
    let mut audio = AudioMonitor::new()?;

    // Show current state. Both read `None`: nothing in IronMonitor reads the system
    // mixer on any platform yet.
    println!("Initial master volume: {:?}", audio.master_volume());
    println!("Initial mute state: {:?}", audio.is_muted());

    // These used to assign a field and return Ok(()), so this example printed
    // "New master volume: Some(75)" while the machine's volume did not move.
    // They report the truth now, and this shows it rather than dying on `?`.
    println!(
        "
Setting master volume to 75%..."
    );
    match audio.set_master_volume(75) {
        Ok(()) => println!("New master volume: {:?}", audio.master_volume()),
        Err(e) => println!("  declined: {e}"),
    }

    println!(
        "
Muting audio..."
    );
    match audio.set_mute(true) {
        Ok(()) => println!("Muted: {:?}", audio.is_muted()),
        Err(e) => println!("  declined: {e}"),
    }

    // Device-specific volume (if devices exist)
    let device_id = audio.devices().first().map(|d| d.id.clone());
    if let Some(id) = device_id {
        let name = audio
            .devices()
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        println!("\nSetting volume for device '{}' to 50%...", name);
        audio.set_device_volume(&id, 50)?;
        println!("Device volume updated");
    }

    // Try invalid volume (will error)
    println!("\nTrying to set invalid volume (150%)...");
    match audio.set_master_volume(150) {
        Ok(_) => println!("Unexpected success"),
        Err(e) => println!("Expected error: {}", e),
    }

    // Bluetooth Control Demo
    println!("\n--- Bluetooth Control ---");
    let mut bt = BluetoothMonitor::new()?;

    println!("\nAttempting to pair with device AA:BB:CC:DD:EE:FF...");
    bt.pair_device("AA:BB:CC:DD:EE:FF")?;
    println!("Pair command sent (stub)");

    println!("\nAttempting to connect to device AA:BB:CC:DD:EE:FF...");
    bt.connect_device("AA:BB:CC:DD:EE:FF")?;
    println!("Connect command sent");

    println!("\nAttempting to disconnect from device AA:BB:CC:DD:EE:FF...");
    bt.disconnect_device("AA:BB:CC:DD:EE:FF")?;
    println!("Disconnect command sent");

    println!("\nAttempting to unpair device AA:BB:CC:DD:EE:FF...");
    bt.unpair_device("AA:BB:CC:DD:EE:FF")?;
    println!("Unpair command sent");

    // Try invalid MAC address
    println!("\nTrying invalid MAC address format...");
    match bt.pair_device("invalid-address") {
        Ok(_) => println!("Unexpected success"),
        Err(e) => println!("Expected error: {}", e),
    }

    println!("\n=== Hardware Control Demo Complete ===");
    println!("\nNote: These are stub implementations. Real hardware control");
    println!("requires platform-specific APIs.");

    Ok(())
}
