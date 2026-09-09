//! Simple example using IronMonitor's new unified API

use ironmonlib::gpu::GpuCollection;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== IronMonitor - Basic Example ===\n");

    // Auto-detect all available GPUs
    let gpus = GpuCollection::auto_detect()?;

    println!("Found {} GPU(s)\n", gpus.len());

    // Get snapshot of all GPUs
    println!("=== GPU Information ===");
    for (idx, info) in gpus.snapshot_all()?.iter().enumerate() {
        println!("GPU {}: {}", idx, info.static_info.name);
        println!("  Vendor: {:?}", info.static_info.vendor);
        // A dash where the device reported no figure, rather than `0 / 0 MB`.
        let mb =
            |v: Option<u64>| v.map_or_else(|| "-".to_string(), |b| (b / 1024 / 1024).to_string());
        println!(
            "  Memory: {} / {} MB",
            mb(info.dynamic_info.memory.used),
            mb(info.dynamic_info.memory.total)
        );
        match info.dynamic_info.utilization {
            Some(u) => println!("  Utilization: {u}%"),
            None => println!("  Utilization: not reported"),
        }
        if let Some(temp) = info.dynamic_info.thermal.temperature {
            println!("  Temperature: {}°C", temp);
        }
        if let Some(power_draw) = info.dynamic_info.power.draw {
            println!("  Power: {:.1}W", power_draw as f64 / 1000.0);
        }

        if !info.dynamic_info.processes.is_empty() {
            println!("  Processes: {}", info.dynamic_info.processes.len());
        }
        println!();
    }

    Ok(())
}
