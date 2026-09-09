//! Audio device monitoring example
//!
//! Demonstrates how to enumerate audio devices and check their status.
//!
//! Run with: cargo run --example audio_monitor

use ironmonlib::audio::AudioMonitor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Audio Monitor Example ===\n");

    let monitor = AudioMonitor::new()?;

    // Get master volume
    if let Some(volume) = monitor.master_volume() {
        println!("Master Volume: {}%", volume);
    } else {
        println!("Master Volume: Not available");
    }

    // Check mute status
    match monitor.is_muted() {
        Some(m) => println!("Muted: {}", if m { "Yes" } else { "No" }),
        None => println!("Muted: Not available"),
    }

    println!();

    // List all audio devices
    let devices = monitor.devices();
    println!("Found {} audio device(s):\n", devices.len());

    for device in devices {
        let direction = match device.device_type {
            ironmonlib::audio::AudioDeviceType::Output => "Output",
            ironmonlib::audio::AudioDeviceType::Input => "Input",
            ironmonlib::audio::AudioDeviceType::Duplex => "Duplex",
        };
        let default = match device.is_default {
            Some(true) => " (Default)",
            Some(false) => "",
            // The default endpoint is a COM call this crate does not make; it
            // is not "not the default".
            None => "",
        };
        let enabled = if device.is_enabled { "" } else { " [Disabled]" };

        println!(
            "  {} {}{}{}",
            if device.is_output { "🔊" } else { "🎤" },
            device.name,
            default,
            enabled
        );
        println!("    ID: {}", device.id);
        println!("    Type: {}", direction);
        if let Some(vol) = device.volume {
            println!("    Volume: {}%", vol);
        }
        println!();
    }

    Ok(())
}
