//! CPU Monitoring Example
//!
//! This example demonstrates comprehensive CPU monitoring using IronMonitor.
//! It shows per-core tracking, frequency monitoring, utilization, and temperature
//! across all supported platforms (Linux, Windows, macOS).

use ironmonlib::silicon::{CpuClusterType, SiliconMonitor};

#[cfg(target_os = "linux")]
use ironmonlib::silicon::linux::LinuxSiliconMonitor;

#[cfg(target_os = "windows")]
use ironmonlib::silicon::windows::WindowsSiliconMonitor;

#[cfg(all(feature = "apple", target_os = "macos"))]
use ironmonlib::silicon::apple::AppleSiliconMonitor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "=== IronMonitor - CPU Monitoring Example ===
"
    );

    // Each supported platform hands its own monitor to `report`. Until 6.0.0
    // the selection bound a `monitor` local under three separate `cfg`s and the
    // rest of the body used it unconditionally, so on a target with no arm --
    // macOS without the `apple` feature -- the example failed to compile. The
    // early `return` in the fallback did not help: the code after it is still
    // type-checked.
    #[cfg(target_os = "linux")]
    return report(&LinuxSiliconMonitor::new()?);

    #[cfg(target_os = "windows")]
    return report(&WindowsSiliconMonitor::new()?);

    #[cfg(all(feature = "apple", target_os = "macos"))]
    return report(&AppleSiliconMonitor::new()?);

    #[cfg(not(any(
        target_os = "linux",
        target_os = "windows",
        all(feature = "apple", target_os = "macos")
    )))]
    {
        eprintln!("Platform not supported for this example");
        Ok(())
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    all(feature = "apple", target_os = "macos")
))]
fn report<M: SiliconMonitor>(monitor: &M) -> Result<(), Box<dyn std::error::Error>> {
    // Get CPU information
    // Utilization is a delta between two samples, so a single call has
    // nothing to difference against and reports `None`. Sample twice.
    let _ = monitor.cpu_info()?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    let (cores, clusters) = monitor.cpu_info()?;

    println!("CPU Cores: {}", cores.len());
    println!("CPU Clusters: {}\n", clusters.len());

    // Display cluster information
    println!("=== CPU Clusters ===");
    for cluster in &clusters {
        let cluster_name = match cluster.cluster_type {
            CpuClusterType::Performance => "Performance (P-cores)",
            CpuClusterType::Efficiency => "Efficiency (E-cores)",
            CpuClusterType::Standard => "Standard",
        };

        println!("\n{} Cluster:", cluster_name);
        println!("  Cores: {:?}", cluster.core_ids);
        match cluster.frequency_mhz {
            Some(mhz) => println!("  Average Frequency: {mhz} MHz"),
            None => println!("  Average Frequency: not measured"),
        }
        match cluster.utilization {
            Some(u) => println!("  Average Utilization: {u}%"),
            None => println!("  Average Utilization: not measured"),
        }

        if let Some(power) = cluster.power_watts {
            println!("  Power Consumption: {:.2} W", power);
        }
    }

    // Display per-core information
    println!("\n=== Per-Core Details ===");
    for core in &cores {
        let cluster_name = match core.cluster {
            CpuClusterType::Performance => "P",
            CpuClusterType::Efficiency => "E",
            CpuClusterType::Standard => "S",
        };

        print!("Core {:2} [{}]: ", core.id, cluster_name);
        match core.frequency_mhz {
            Some(mhz) => print!("{mhz:4} MHz, "),
            None => print!("   -  MHz, "),
        }
        match core.utilization {
            Some(u) => print!("{u:3}% util"),
            None => print!("  -  util"),
        }

        if let Some(temp) = core.temperature {
            print!(", {}°C", temp);
        }

        println!();
    }

    // Calculate and display summary statistics
    println!("\n=== Summary Statistics ===");

    let total_cores = cores.len();
    let avg_freq = ironmonlib::silicon::average_reported_mhz(&cores);
    let avg_util = ironmonlib::silicon::average_reported_util(&cores);

    println!("Total Cores: {}", total_cores);
    match avg_freq {
        Some(mhz) => println!("Average Frequency: {mhz} MHz"),
        None => println!("Average Frequency: not measured on this platform"),
    }
    match avg_util {
        Some(u) => println!("Average Utilization: {u}%"),
        None => println!("Average Utilization: not measured on this platform"),
    }

    // Temperature statistics (if available)
    let temps: Vec<i32> = cores.iter().filter_map(|c| c.temperature).collect();

    if !temps.is_empty() {
        let min_temp = temps.iter().min().unwrap();
        let max_temp = temps.iter().max().unwrap();
        let avg_temp = temps.iter().sum::<i32>() / temps.len() as i32;

        println!("\nTemperature Range: {}°C - {}°C", min_temp, max_temp);
        println!("Average Temperature: {}°C", avg_temp);
    }

    // I/O Controllers
    println!("\n=== I/O Controllers ===");
    match monitor.io_info() {
        Ok(controllers) if !controllers.is_empty() => {
            for ctrl in &controllers {
                let rate = ctrl.bandwidth_mbps.map_or_else(
                    || "not monitored".to_string(),
                    |mbps| format!("{mbps:.2} MB/s"),
                );
                let ceiling = ctrl
                    .max_bandwidth_mbps
                    .map_or_else(String::new, |m| format!(" of {m:.0} MB/s"));
                println!(
                    "  {} ({}): {}{}",
                    ctrl.name, ctrl.controller_type, rate, ceiling
                );
            }
        }
        Ok(_) => println!("  No I/O controllers detected"),
        Err(e) => println!("  Error reading I/O info: {}", e),
    }

    // Network Silicon
    println!("\n=== Network Interfaces ===");
    match monitor.network_info() {
        Ok(networks) if !networks.is_empty() => {
            for net in &networks {
                println!(
                    "  {} ({}): RX {:.2} MB/s, TX {:.2} MB/s, {} pkt/s",
                    net.interface,
                    net.link_speed_mbps.map_or_else(
                        || "link speed not read".to_string(),
                        |m| format!("{m} Mbps")
                    ),
                    net.rx_bandwidth_mbps,
                    net.tx_bandwidth_mbps,
                    net.packet_rate
                );
            }
        }
        Ok(_) => println!("  No network interfaces detected"),
        Err(e) => println!("  Error reading network info: {}", e),
    }

    // Platform-specific information
    #[cfg(target_os = "linux")]
    {
        println!("\n=== Linux-Specific Information ===");
        println!("CPU Governor: Check individual cores for governor settings");
        println!("Thermal Zones: Using hwmon and thermal_zone sensors");
    }

    #[cfg(target_os = "windows")]
    {
        println!("\n=== Windows-Specific Information ===");
        println!("Note: Temperature monitoring requires administrator privileges");
        println!("Using Performance Counters and WMI for monitoring");
    }

    #[cfg(all(feature = "apple", target_os = "macos"))]
    {
        println!("\n=== macOS-Specific Information ===");
        println!("Using powermetrics for comprehensive monitoring");
        println!("Hybrid architecture detected (P-cores + E-cores)");
    }

    Ok(())
}
