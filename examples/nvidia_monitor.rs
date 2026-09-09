//! NVIDIA GPU Monitoring Example
//!
//! Demonstrates the unified GPU monitoring interface with NVIDIA GPUs.
//! Run with: cargo run --example nvidia_monitor --features nvidia

use ironmonlib::gpu::traits::*;

#[cfg(feature = "nvidia")]
use ironmonlib::gpu::nvidia_new;

#[cfg(not(feature = "nvidia"))]
fn main() {
    eprintln!("This example requires the 'nvidia' feature");
    eprintln!("Run with: cargo run --example nvidia_monitor --features nvidia");
    std::process::exit(1);
}

#[cfg(feature = "nvidia")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== IronMonitor: NVIDIA GPU Example ===\n");

    // Enumerate all NVIDIA GPUs
    let devices = match nvidia_new::enumerate() {
        Ok(devs) => devs,
        Err(e) => {
            eprintln!("Failed to enumerate NVIDIA GPUs: {}", e);
            eprintln!("Make sure you have:");
            eprintln!("  1. NVIDIA drivers installed");
            eprintln!("  2. NVML library available (nvidia-ml)");
            eprintln!("  3. Sufficient permissions to access GPU");
            return Err(e.into());
        }
    };

    println!("Found {} NVIDIA GPU(s)\n", devices.len());

    for device in &devices {
        print_device_info(device)?;
    }

    Ok(())
}

#[cfg(feature = "nvidia")]
fn print_device_info(device: &dyn Device) -> Result<(), Box<dyn std::error::Error>> {
    println!("===============================================");
    println!(
        "GPU {}: {} ({})",
        device.index(),
        device.name()?,
        device.vendor()
    );
    println!("===============================================\n");

    // Basic Info
    println!("[MB] Device Information:");
    println!("  UUID:           {}", device.uuid()?);
    println!("  Driver Version: {}", device.driver_version()?);

    let pci = device.pci_info()?;
    println!("  PCI Bus ID:     {}", pci.bus_id);
    if let Some(gen) = pci.pcie_generation {
        println!("  PCIe:           Gen{}", gen);
    }
    if let Some(width) = pci.pcie_link_width {
        println!("  PCIe Width:     x{}", width);
    }

    // Temperature
    println!("\n[TEMP]  Temperature:");
    let temp = device.temperature()?;
    if let Some(t) = temp.junction {
        println!("  GPU:            {:.1}°C", t);
    }
    if let Some(t) = temp.memory {
        println!("  Memory:         {:.1}°C", t);
    }
    if let Some(t) = temp.max() {
        println!("  Maximum:        {:.1}°C", t);
    }

    // Power
    println!("\n[VOLT] Power:");
    let power = device.power()?;
    let w = |v: Option<f32>| v.map_or_else(|| "not reported".to_string(), |x| format!("{x:.1} W"));
    println!("  Current Draw:   {}", w(power.current));
    println!("  Power Limit:    {}", w(power.limit));
    match (power.current, power.limit) {
        (Some(cur), Some(lim)) if lim > 0.0 => {
            println!("  Usage:          {:.1}%", (cur / lim) * 100.0)
        }
        _ => println!("  Usage:          not reported"),
    }
    println!(
        "  Available Range: {} - {}",
        w(power.min_limit),
        w(power.max_limit)
    );

    // Clocks
    println!("\n🔄 Clock Frequencies:");
    let clocks = device.clocks()?;
    println!("  Graphics Clock: {} MHz", clocks.graphics);
    println!("  Memory Clock:   {} MHz", clocks.memory);
    if let Some(sm) = clocks.sm {
        println!("  SM Clock:       {} MHz", sm);
    }
    if let Some(video) = clocks.video {
        println!("  Video Clock:    {} MHz", video);
    }

    // Utilization
    println!("\n[INFO] Utilization:");
    let util = device.utilization()?;
    println!("  GPU:            {:.1}%", util.gpu);
    println!("  Memory:         {:.1}%", util.memory);
    if let Some(enc) = util.encoder {
        println!("  Encoder:        {:.1}%", enc);
    }
    if let Some(dec) = util.decoder {
        println!("  Decoder:        {:.1}%", dec);
    }

    // Memory
    println!("\n💾 Memory:");
    let mem = device.memory()?;
    println!(
        "  Total:          {} GB",
        mem.total as f64 / 1024.0 / 1024.0 / 1024.0
    );
    println!(
        "  Used:           {} GB ({:.1}%)",
        mem.used as f64 / 1024.0 / 1024.0 / 1024.0,
        mem.utilization_percent()
    );
    println!(
        "  Free:           {} GB",
        mem.free as f64 / 1024.0 / 1024.0 / 1024.0
    );
    if let Some(bar1_total) = mem.bar1_total {
        println!(
            "  BAR1 Total:     {} MB",
            bar1_total as f64 / 1024.0 / 1024.0
        );
    }

    // Fan
    if let Ok(Some(fan)) = device.fan_speed() {
        println!("\n🌬️  Fan:");
        match fan {
            FanSpeed::Percent(p) => println!("  Speed:          {}%", p),
            FanSpeed::Rpm(r) => println!("  Speed:          {} RPM", r),
        }
    }

    // Performance State
    if let Ok(Some(pstate)) = device.performance_state() {
        println!("\n⚙️  Performance State: {}", pstate);
    }

    // Processes
    println!("\n[BIOS] GPU Processes:");
    match device.processes() {
        Ok(processes) => {
            if processes.is_empty() {
                println!("  No processes using this GPU");
            } else {
                println!("  {} process(es) running:", processes.len());
                for proc in processes {
                    let name = proc.name().unwrap_or_else(|_| "unknown".to_string());
                    let mem = proc
                        .gpu_memory_used()
                        .map(|m| format!("{} MB", m / 1024 / 1024))
                        .unwrap_or_else(|_| "N/A".to_string());
                    println!(
                        "    PID {}: {} ({:?}, {})",
                        proc.pid(),
                        name,
                        proc.process_type(),
                        mem
                    );
                }
            }
        }
        Err(e) => println!("  Failed to get processes: {}", e),
    }

    // Advanced Features (NVIDIA-specific)
    println!("\n🚀 Advanced Features:");

    // Compute Mode
    if let Ok(Some(compute_mode)) = device.compute_mode() {
        println!("  Compute Mode:   {:?}", compute_mode);
    }

    // Persistence Mode
    if let Ok(Some(persistence)) = device.persistence_mode() {
        println!(
            "  Persistence:    {}",
            if persistence { "Enabled" } else { "Disabled" }
        );
    }

    // MIG Mode
    if let Ok(mig) = device.mig_mode() {
        println!(
            "  MIG Mode:       {}",
            if mig.current { "Enabled" } else { "Disabled" }
        );
        if mig.current != mig.pending {
            println!(
                "    (Pending: {})",
                if mig.pending { "Enabled" } else { "Disabled" }
            );
        }
    }

    // ECC Errors
    if let Ok(ecc) = device.ecc_errors() {
        println!("  ECC Errors:");
        println!(
            "    Volatile:     {} SBE, {} DBE",
            ecc.volatile_single_bit, ecc.volatile_double_bit
        );
        println!(
            "    Aggregate:    {} SBE, {} DBE",
            ecc.aggregate_single_bit, ecc.aggregate_double_bit
        );
    }

    // NVLink
    if let Ok(nvlinks) = device.nvlink_status() {
        if !nvlinks.is_empty() {
            println!("  NVLink:");
            for link in nvlinks {
                println!(
                    "    Link {}: {:?} (v{}) -> {}",
                    link.link_id, link.state, link.version, link.remote_pci_bus_id
                );
            }
        }
    }

    println!();
    Ok(())
}
