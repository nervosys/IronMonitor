//! Process Monitoring Example using IronMonitor

use ironmonlib::ProcessMonitor;
use std::error::Error;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    println!("IronMonitor - Process Monitoring Example");
    println!("============================================\n");

    let mut monitor = ProcessMonitor::new()?;

    // Get all processes
    let processes = monitor.processes()?;

    println!("Total processes: {}", processes.len());
    println!("GPU processes: {}", monitor.gpu_process_count()?);
    println!();

    // Display top GPU consumers
    let gpu_procs = monitor.processes_by_gpu_memory()?;

    if !gpu_procs.is_empty() {
        println!("Top GPU Memory Consumers:");
        println!(
            "{:<8} {:<12} {:<8} {:<12} {:<20}",
            "PID", "USER", "GPU MEM(MB)", "CPU%", "NAME"
        );
        println!("{}", "-".repeat(70));

        for process in gpu_procs.iter().take(10) {
            println!(
                "{:<8} {:<12} {:<12} {:<12.1} {:<20}",
                process.pid,
                process.user.as_deref().unwrap_or("unknown"),
                process
                    .total_gpu_memory_bytes
                    .map_or_else(|| "n/a".to_string(), |b| (b / 1024 / 1024).to_string()),
                process.cpu_percent,
                process.name
            );
        }
    } else {
        println!("No GPU processes found");
    }

    println!("\n");

    // Display top CPU consumers
    let cpu_procs = monitor.processes_by_cpu()?;

    println!("Top CPU Consumers:");
    println!(
        "{:<8} {:<12} {:<12} {:<12} {:<20}",
        "PID", "USER", "CPU%", "MEM(MB)", "NAME"
    );
    println!("{}", "-".repeat(70));

    for process in cpu_procs.iter().take(10) {
        println!(
            "{:<8} {:<12} {:<12.1} {:<12} {:<20}",
            process.pid,
            process.user.as_deref().unwrap_or("unknown"),
            process.cpu_percent,
            process.memory_bytes / 1024 / 1024,
            process.name
        );
    }

    // Monitor for a few seconds
    println!("\nMonitoring for 5 seconds...");
    for i in 1..=5 {
        thread::sleep(Duration::from_secs(1));
        let count = monitor.process_count()?;
        let gpu_count = monitor.gpu_process_count()?;
        println!(
            "[{}s] Processes: {}, GPU processes: {}",
            i, count, gpu_count
        );
    }

    println!("\nDone!");
    Ok(())
}
