use ironmonlib::gpu::GpuCollection;
use ironmonlib::Result;
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    println!("=== GPU Fan Monitor ===\n");
    println!("Monitoring fan speeds (percentage and RPM)...\n");
    println!("Press Ctrl+C to exit\n");

    // Initialize GPU collection
    let gpus = GpuCollection::auto_detect()?;

    if gpus.is_empty() {
        println!("No GPUs detected!");
        return Ok(());
    }

    println!("Found {} GPU(s)\n", gpus.len());

    loop {
        // Clear screen (simple method)
        print!("\x1B[2J\x1B[1;1H");

        println!("=== GPU Fan Monitor ===");
        println!(
            "Time: {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );

        for (i, gpu) in gpus.gpus().iter().enumerate() {
            let info = gpu.static_info()?;
            let dynamic = gpu.dynamic_info()?;

            println!("GPU {}: {}", i, info.name);
            println!("  Vendor: {:?}", info.vendor);

            let temp = &dynamic.thermal;
            println!("  Temperature: {}°C", temp.temperature.unwrap_or(0));

            // Display fan speed
            match (temp.fan_speed, temp.fan_rpm) {
                (Some(percent), Some(rpm)) => {
                    println!("  Fan Speed: {}% ({} RPM)", percent, rpm);

                    // Visual bar for percentage
                    let bar_width = 50;
                    let filled = (percent as usize * bar_width) / 100;
                    let bar = "█".repeat(filled) + &"░".repeat(bar_width - filled);
                    println!("  Fan Bar:   [{}] {}%", bar, percent);
                }
                (Some(percent), None) => {
                    println!("  Fan Speed: {}%", percent);

                    // Visual bar for percentage
                    let bar_width = 50;
                    let filled = (percent as usize * bar_width) / 100;
                    let bar = "█".repeat(filled) + &"░".repeat(bar_width - filled);
                    println!("  Fan Bar:   [{}] {}%", bar, percent);
                }
                (None, Some(rpm)) => {
                    println!("  Fan Speed: {} RPM", rpm);
                }
                (None, None) => {
                    println!("  Fan Speed: Not available (fanless or not supported)");
                }
            }

            if let Some(max_temp) = temp.max_temperature {
                println!("  Max Temp Threshold: {}°C", max_temp);
            }
            if let Some(crit_temp) = temp.critical_temperature {
                println!("  Critical Temp Threshold: {}°C", crit_temp);
            }

            // Display utilization
            match dynamic.utilization {
                Some(u) => println!("  GPU Utilization: {u}%"),
                None => println!("  GPU Utilization: not reported"),
            }

            // Display power
            if let Some(draw) = dynamic.power.draw {
                println!("  Power Usage: {:.2} W", draw);
            }
            if let Some(limit) = dynamic.power.limit {
                println!("  Power Limit: {:.2} W", limit);
            }

            println!();
        }

        println!("\nPress Ctrl+C to exit");

        // Update every 2 seconds
        thread::sleep(Duration::from_secs(2));
    }
}
