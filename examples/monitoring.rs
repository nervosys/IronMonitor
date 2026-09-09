//! Continuous monitoring example using IronMonitor

use ironmonlib::gpu::GpuCollection;
use std::error::Error;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Continuous GPU Monitoring ===");
    println!("Press Ctrl+C to stop\n");

    let gpus = GpuCollection::auto_detect()?;

    if gpus.is_empty() {
        println!("No GPUs detected!");
        return Ok(());
    }

    loop {
        // Get snapshot of all GPUs
        let snapshots = gpus.snapshot_all()?;

        // Clear line and print stats
        print!("\r");

        // Print concise GPU stats
        let gpu_info: Vec<String> = snapshots
            .iter()
            .enumerate()
            .map(|(idx, info)| {
                let clocks = info.dynamic_info.clocks.graphics.unwrap_or(0);
                let power = info
                    .dynamic_info
                    .power
                    .draw
                    .map(|p| format!("{:.1}W", p as f64 / 1000.0))
                    .unwrap_or_else(|| "N/A".to_string());
                // "N/A" for an absent utilization, matching the power figure
                // built just above.
                let util = info
                    .dynamic_info
                    .utilization
                    .map(|u| format!("{u}%"))
                    .unwrap_or_else(|| "N/A".to_string());
                format!("GPU{}: {}@{}MHz {}", idx, util, clocks, power)
            })
            .collect();

        print!("{}", gpu_info.join(" | "));

        use std::io::{self, Write};
        io::stdout().flush()?;

        // Sleep for interval
        thread::sleep(Duration::from_millis(500));
    }
}
