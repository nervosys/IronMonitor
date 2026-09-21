//! Unified GPU Monitoring Example
//!
//! This example demonstrates monitoring all GPU vendors (NVIDIA, AMD, Intel)
//! using IronMonitor's unified Device trait interface.
//!
//! # Usage
//!
//! ```bash
//! # Monitor all GPU types
//! cargo run --example all_gpus --features nvidia,amd,intel
//!
//! # Monitor specific vendor
//! cargo run --example all_gpus --features nvidia
//! cargo run --example all_gpus --features amd
//! cargo run --example all_gpus --features intel
//! ```

use ironmonlib::gpu::traits::{Device, Vendor};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("═══════════════════════════════════════════════════════════");
    println!("       IronMonitor - Unified GPU Monitoring");
    println!("═══════════════════════════════════════════════════════════\n");

    let mut all_devices: Vec<Box<dyn Device>> = Vec::new();
    // Annotated because every insertion sits behind a vendor feature: with none
    // enabled there is nothing for inference to work from and the example fails to
    // compile, which only shows up on a build that is not `--all-features`.
    let mut vendor_counts: std::collections::HashMap<Vendor, usize> =
        std::collections::HashMap::new();

    // Enumerate NVIDIA GPUs
    #[cfg(feature = "nvidia")]
    {
        print!("[SCAN] Scanning for NVIDIA GPUs... ");
        match ironmonlib::gpu::nvidia_new::enumerate() {
            Ok(devices) => {
                let count = devices.len();
                println!("[OK] Found {}", count);
                vendor_counts.insert(Vendor::Nvidia, count);
                for device in devices {
                    all_devices.push(Box::new(device) as Box<dyn Device>);
                }
            }
            Err(e) => {
                println!("⚠️  None ({})", e);
                vendor_counts.insert(Vendor::Nvidia, 0);
            }
        }
    }

    // Enumerate AMD GPUs
    #[cfg(feature = "amd")]
    {
        print!("[SCAN] Scanning for AMD GPUs... ");
        match ironmonlib::gpu::amd_rocm::enumerate() {
            Ok(mut devices) => {
                let count = devices.len();
                println!("[OK] Found {}", count);
                vendor_counts.insert(Vendor::Amd, count);
                all_devices.append(&mut devices);
            }
            Err(e) => {
                println!("⚠️  None ({})", e);
                vendor_counts.insert(Vendor::Amd, 0);
            }
        }
    }

    // Enumerate Intel GPUs
    #[cfg(feature = "intel")]
    {
        print!("[SCAN] Scanning for Intel GPUs... ");
        match ironmonlib::gpu::intel_levelzero::enumerate() {
            Ok(mut devices) => {
                let count = devices.len();
                println!("[OK] Found {}", count);
                vendor_counts.insert(Vendor::Intel, count);
                all_devices.append(&mut devices);
            }
            Err(e) => {
                println!("⚠️  None ({})", e);
                vendor_counts.insert(Vendor::Intel, 0);
            }
        }
    }

    println!();

    if all_devices.is_empty() {
        println!("[ERROR] No GPUs detected!");
        println!("\nThis could mean:");
        println!("  - No supported GPUs are installed");
        println!("  - GPU drivers are not loaded");
        println!("  - Insufficient permissions to access GPU devices");
        return Ok(());
    }

    println!(
        "[INFO] Summary: {} total GPU(s) detected",
        all_devices.len()
    );
    for (vendor, count) in &vendor_counts {
        if *count > 0 {
            println!("   {} {} GPU(s)", count, vendor_name(*vendor));
        }
    }
    println!();

    // Monitor each GPU
    for (i, device) in all_devices.iter().enumerate() {
        println!("┌─────────────────────────────────────────────────────────┐");
        println!(
            "│ GPU #{} - {}                                       ",
            i,
            vendor_name(device.vendor())
        );
        println!("└─────────────────────────────────────────────────────────┘");

        print_gpu_info(device.as_ref())?;
        println!();
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("                  Monitoring Complete");
    println!("═══════════════════════════════════════════════════════════");

    Ok(())
}

fn vendor_name(vendor: Vendor) -> &'static str {
    match vendor {
        Vendor::Nvidia => "NVIDIA",
        Vendor::Amd => "AMD",
        Vendor::Intel => "Intel",
        Vendor::Apple => "Apple",
    }
}

fn print_gpu_info(device: &dyn Device) -> Result<(), Box<dyn std::error::Error>> {
    // Basic info
    println!("\n[MB] Device Information:");
    println!("  Vendor:  {}", vendor_name(device.vendor()));
    println!("  Index:   {}", device.index());
    println!(
        "  Name:    {}",
        device.name().unwrap_or_else(|_| "Unknown".to_string())
    );

    if let Ok(driver) = device.driver_version() {
        println!("  Driver:  {}", driver);
    }

    if let Ok(pci) = device.pci_info() {
        println!("  PCI:     {}", pci.bus_id);
    }

    // Temperature
    if let Ok(temp) = device.temperature() {
        print!("\n[TEMP]  Temperature: ");
        let primary = temp.primary();
        if let Some(t) = primary {
            print!("{:.1}°C", t);
            if t > 80.0 {
                print!(" ⚠️ ");
            }
            println!();
        } else {
            println!("N/A");
        }
    }

    // Power
    // `> 0.0` was how this told "unreported" from "reported": the fields were
    // bare, so a missing reading arrived as zero and the example simply hid it.
    // Now it asks the question directly.
    if let Ok(power) = device.power() {
        if power.current.is_some() || power.limit.is_some() {
            println!("\n[VOLT] Power:");
            if let Some(draw) = power.current {
                print!("  Draw:    {draw:.2}W");
                if let Some(limit) = power.limit.filter(|l| *l > 0.0) {
                    print!(" ({:.0}%)", (draw / limit) * 100.0);
                }
                println!();
            }
            if let Some(limit) = power.limit {
                println!("  Limit:   {limit:.2}W");
            }
        }
    }

    // Clocks
    //
    // These sections were guarded by `> 0` because the reader could not say a
    // value was unread and wrote zeros instead. The guard worked as a sentinel
    // and cost something real: a card genuinely idling at 0 MHz, or a GPU
    // genuinely at 0% utilisation, was hidden from this output entirely. With
    // absence expressible, the check is `is_some` and a true zero prints.
    if let Ok(clocks) = device.clocks() {
        if clocks.graphics.is_some() || clocks.memory.is_some() {
            println!("\n🔄 Clocks:");
            if let Some(mhz) = clocks.graphics {
                println!("  GPU:     {} MHz", mhz);
            }
            if let Some(mhz) = clocks.memory {
                println!("  Memory:  {} MHz", mhz);
            }
        }
    }

    // Utilization
    if let Ok(util) = device.utilization() {
        if util.gpu.is_some() || util.memory.is_some() {
            println!("\n📈 Utilization:");
            if let Some(pct) = util.gpu {
                println!("  GPU:     {:.1}%", pct);
            }
            if let Some(pct) = util.memory {
                println!("  Memory:  {:.1}%", pct);
            }
        }
    }

    // Memory
    if let Ok(mem) = device.memory() {
        if let Some(total) = mem.total {
            println!("\n💾 Memory:");
            const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
            println!("  Total:   {:.2} GB", total as f64 / GIB);
            match (mem.used, mem.utilization_percent()) {
                (Some(used), Some(pct)) => {
                    println!("  Used:    {:.2} GB ({:.0}%)", used as f64 / GIB, pct)
                }
                (Some(used), None) => println!("  Used:    {:.2} GB", used as f64 / GIB),
                (None, _) => println!("  Used:    unavailable - not reported by the driver"),
            }
        }
    }

    // Fan
    if let Ok(Some(fan)) = device.fan_speed() {
        println!("\n[FAN] Fan:");
        match fan {
            ironmonlib::gpu::traits::FanSpeed::Rpm(rpm) => {
                println!("  Speed:   {} RPM", rpm);
            }
            ironmonlib::gpu::traits::FanSpeed::Percent(percent) => {
                println!("  Speed:   {}%", percent);
            }
        }
    }

    Ok(())
}
