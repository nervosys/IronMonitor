//! Apple Silicon GPU monitoring example
//!
//! Run with: cargo run --release --features apple --example apple_monitor
//!
//! Requires macOS with Apple Silicon (M1/M2/M3/M4).
//! Uses powermetrics for GPU utilization, frequency, and power data.
//! Note: powermetrics requires root/sudo for full metrics.

use ironmonlib::gpu::GpuCollection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Apple Silicon GPU Monitor ===\n");

    let gpus = GpuCollection::auto_detect()?;

    if gpus.is_empty() {
        println!("No Apple GPUs detected.");
        println!("This example requires macOS with Apple Silicon.");
        return Ok(());
    }

    for gpu in gpus.gpus() {
        let info = gpu.info()?;
        let s = &info.static_info;
        let d = &info.dynamic_info;

        println!("GPU: {}", s.name);
        println!("  Vendor:       {:?}", s.vendor);
        println!("  Integrated:   {}", s.integrated);
        if let Some(cores) = s.shader_cores {
            println!("  GPU Cores:    {}", cores);
        }

        match d.utilization {
            Some(u) => println!("\n  Utilization:  {u}%"),
            None => println!("\n  Utilization:  not reported"),
        }
        if let Some(freq) = d.clocks.graphics {
            println!("  Frequency:    {} MHz", freq);
        }
        if let Some(max) = d.clocks.graphics_max {
            println!("  Max Freq:     {} MHz", max);
        }
        if let Some(power) = d.power.draw {
            println!("  Power Draw:   {:.1} W", power as f64 / 1000.0);
        }
        if let Some(pct) = d.power.usage_percent {
            println!("  Power Usage:  {}%", pct);
        }
        println!();
    }

    Ok(())
}
