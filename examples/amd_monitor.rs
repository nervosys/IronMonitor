//! AMD GPU Monitoring Example

#[cfg(feature = "amd")]
use ironmonlib::gpu::amd_rocm;

#[cfg(feature = "amd")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("IronMonitor - AMD GPU Monitoring\n");

    let devices = match amd_rocm::enumerate() {
        Ok(devs) => devs,
        Err(e) => {
            eprintln!("Failed to enumerate AMD GPUs: {:?}", e);
            return Ok(());
        }
    };

    println!("Found {} AMD GPU(s)\n", devices.len());

    for device in devices.iter() {
        println!("GPU #{}", device.index());
        println!(
            "  Name: {}",
            device.name().unwrap_or_else(|_| "Unknown".to_string())
        );

        if let Ok(temp) = device.temperature() {
            if let Some(edge) = temp.edge {
                println!("  Temperature: {:.1}C", edge);
            }
        }

        if let Ok(power) = device.power() {
            match power.current {
                Some(w) => println!("  Power: {w:.2}W"),
                None => println!("  Power: not reported"),
            }
        }

        if let Ok(util) = device.utilization() {
            println!("  GPU Utilization: {:.1}%", util.gpu);
        }

        if let Ok(mem) = device.memory() {
            println!(
                "  VRAM: {} MB / {} MB",
                mem.used / (1024 * 1024),
                mem.total / (1024 * 1024)
            );
        }

        println!();
    }

    Ok(())
}

#[cfg(not(feature = "amd"))]
fn main() {
    eprintln!("This example requires the amd feature.");
    eprintln!("Run with: cargo run --example amd_monitor --features amd");
}
