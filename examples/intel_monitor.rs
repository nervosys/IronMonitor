//! Intel GPU Monitoring Example
//!
//! This example demonstrates Intel GPU monitoring using IronMonitor.
//! Supports both integrated (iGPU) and discrete (Arc) GPUs.
//!
//! # Requirements
//!
//! - Linux system with Intel GPU
//! - i915 or xe kernel driver loaded
//! - Access to /sys/class/drm (no special permissions needed)
//!
//! # Usage
//!
//! ```bash
//! cargo run --example intel_monitor --features intel
//! ```

#[cfg(feature = "intel")]
use ironmonlib::gpu::intel_levelzero;

#[cfg(feature = "intel")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("IronMonitor - Intel GPU Monitoring\n");

    let devices = match intel_levelzero::enumerate() {
        Ok(devs) => devs,
        Err(e) => {
            eprintln!("Failed to enumerate Intel GPUs: {:?}", e);
            eprintln!("\nPossible causes:");
            eprintln!("  - No Intel GPUs present");
            eprintln!("  - i915/xe driver not loaded");
            eprintln!("  - /sys/class/drm not accessible");
            return Ok(());
        }
    };

    println!("Found {} Intel GPU(s)\n", devices.len());

    for device in devices.iter() {
        println!("GPU #{}", device.index());
        println!(
            "  Name: {}",
            device.name().unwrap_or_else(|_| "Unknown".to_string())
        );
        println!(
            "  Driver: {}",
            device
                .driver_version()
                .unwrap_or_else(|_| "Unknown".to_string())
        );

        if let Ok(pci) = device.pci_info() {
            println!("  PCI: {}", pci.bus_id);
        }

        if let Ok(temp) = device.temperature() {
            if let Some(junction) = temp.junction {
                println!("  Temperature: {:.1}C", junction);
            }
        }

        if let Ok(power) = device.power() {
            let w = |v: Option<f32>| {
                v.map_or_else(|| "not reported".to_string(), |x| format!("{x:.2}W"))
            };
            println!("  Power: {} / {}", w(power.current), w(power.limit));
        }

        if let Ok(clocks) = device.clocks() {
            // `if clocks.graphics > 0` until the type could say "not read".
            // That guard was treating a fabricated zero as a sentinel, which
            // worked and also silently hid a genuinely idle card.
            if let Some(mhz) = clocks.graphics {
                println!("  Clock: {} MHz", mhz);
            }
        }

        if let Ok(mem) = device.memory() {
            if let (Some(used), Some(total)) = (mem.used, mem.total) {
                println!(
                    "  Memory: {} MB / {} MB",
                    used / (1024 * 1024),
                    total / (1024 * 1024)
                );
            }
        }

        println!();
    }

    Ok(())
}

#[cfg(not(feature = "intel"))]
fn main() {
    eprintln!("This example requires the intel feature.");
    eprintln!("Run with: cargo run --example intel_monitor --features intel");
}
