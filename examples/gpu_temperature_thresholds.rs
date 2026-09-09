//! GPU Temperature Threshold Monitoring Example
//!
//! Demonstrates temperature threshold detection and status monitoring
//! for NVIDIA, AMD, and Intel GPUs.

use ironmonlib::gpu::GpuCollection;
use ironmonlib::UnifiedMonitor;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("GPU Temperature Threshold Monitoring\n");
    println!("============================================================\n");

    // Initialize monitor
    let _monitor = UnifiedMonitor::new()?;

    // Discover all GPUs
    let gpus = GpuCollection::auto_detect()?;
    println!("Found {} GPU(s)\n", gpus.len());

    // Display threshold information for each GPU
    for (idx, gpu) in gpus.gpus().iter().enumerate() {
        println!(
            "GPU #{}: {}",
            idx,
            gpu.name().unwrap_or_else(|_| "Unknown".to_string())
        );
        println!("  Vendor: {}", gpu.vendor());

        // Get dynamic info including temperature data
        match gpu.dynamic_info() {
            Ok(info) => {
                // Display current temperatures
                println!("\n  Current Temperature:");
                if let Some(temp) = info.thermal.temperature {
                    println!("    GPU: {:.1}°C", temp);
                }

                // Display temperature thresholds
                if info.thermal.max_temperature.is_some()
                    || info.thermal.critical_temperature.is_some()
                {
                    println!("\n  Temperature Thresholds:");
                    if let Some(max_temp) = info.thermal.max_temperature {
                        println!("    Max Operating: {:.1}°C", max_temp);
                    }
                    if let Some(critical) = info.thermal.critical_temperature {
                        println!("    Critical:      {:.1}°C", critical);
                    }

                    // Display temperature status
                    if let Some(current) = info.thermal.temperature {
                        let status = if let Some(critical) = info.thermal.critical_temperature {
                            if current >= critical {
                                "🔥 CRITICAL"
                            } else if current >= (critical - 10) {
                                "⚠ HOT"
                            } else {
                                "✓ Normal"
                            }
                        } else if let Some(max_temp) = info.thermal.max_temperature {
                            if current >= max_temp {
                                "⚠ Above Max"
                            } else if current >= (max_temp - 10) {
                                "⚠ Hot"
                            } else {
                                "✓ Normal"
                            }
                        } else {
                            if current >= 90 {
                                "⚠ Hot"
                            } else if current >= 80 {
                                "⚠ Warm"
                            } else {
                                "✓ Normal"
                            }
                        };

                        println!("\n  Status: {}", status);

                        // Display margin to critical
                        if let Some(critical) = info.thermal.critical_temperature {
                            let margin = critical - current;
                            if margin > 0 {
                                println!("  Margin to Critical: {:.1}°C", margin);
                            } else {
                                println!("  🔥 CRITICAL! ({:.1}°C over threshold)", margin.abs());
                            }
                        }
                    }
                } else {
                    println!("\n  ℹ Temperature thresholds not available for this GPU");
                }

                // Display fan speed if available
                if let Some(fan_speed) = info.thermal.fan_speed {
                    println!("\n  Fan Speed: {}%", fan_speed);
                }
                if let Some(fan_rpm) = info.thermal.fan_rpm {
                    println!("  Fan RPM: {}", fan_rpm);
                }
            }
            Err(e) => {
                println!("  Error reading GPU info: {}", e);
            }
        }

        println!();
    }

    // Live monitoring mode
    println!("\n============================================================");
    println!("Live Temperature Monitoring (press Ctrl+C to stop)\n");
    println!("GPU | Temp (°C) | Fan %  | Status");
    println!("----|-----------|--------|----------");

    loop {
        for (idx, gpu) in gpus.gpus().iter().enumerate() {
            if let Ok(info) = gpu.dynamic_info() {
                let temp = info.thermal.temperature.unwrap_or(0);
                let fan = info.thermal.fan_speed.unwrap_or(0);

                // Determine status based on temperature
                let status = if let Some(critical) = info.thermal.critical_temperature {
                    if temp >= critical {
                        "CRITICAL 🔥"
                    } else if temp >= (critical - 10) {
                        "HOT ⚠"
                    } else {
                        "OK ✓"
                    }
                } else if let Some(max_temp) = info.thermal.max_temperature {
                    if temp >= max_temp {
                        "Above Max ⚠"
                    } else if temp >= (max_temp - 10) {
                        "Warm ⚠"
                    } else {
                        "OK ✓"
                    }
                } else {
                    if temp >= 90 {
                        "HOT ⚠"
                    } else if temp >= 80 {
                        "Warm"
                    } else {
                        "OK ✓"
                    }
                };

                println!("{:3} | {:9} | {:5}% | {}", idx, temp, fan, status);
            }
        }

        // Update every 2 seconds
        thread::sleep(Duration::from_secs(2));

        // Move cursor up to overwrite previous output
        print!("\x1b[{}A\r", gpus.len());
    }
}
