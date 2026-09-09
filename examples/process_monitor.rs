//! Process Monitor Example
//!
//! Demonstrates unified process monitoring with GPU attribution.
//! Shows all running processes with their CPU, memory, and GPU usage.

use ironmonlib::{ProcessMonitor, Result};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    println!("=== IronMonitor - Process Monitoring ===\n");

    // Create process monitor with GPU attribution
    let mut monitor = ProcessMonitor::new()?;

    println!("Process monitor initialized");
    println!("Monitoring processes every 2 seconds (Ctrl+C to exit)\n");

    loop {
        // Get all processes sorted by GPU memory usage
        let gpu_procs = monitor.processes_by_gpu_memory()?;

        // Display top GPU-using processes
        println!("\n+--------+--------------------------+-------+---------+-------+-----------+");
        println!("|                    TOP GPU-USING PROCESSES                              |");
        println!("+---------+--------------------------+-------+---------+-------+----------+");
        println!("|  PID   | Name                     |  CPU% | Mem(MB) |  GPU  | GPU Mem  |");
        println!("+---------+--------------------------+-------+---------+-------+----------+");

        let mut count = 0;
        for proc in gpu_procs.iter().filter(|p| p.is_gpu_process()).take(10) {
            let name = if proc.name.len() > 24 {
                format!("{}...", &proc.name[..21])
            } else {
                format!("{:<24}", proc.name)
            };

            let gpus = proc
                .gpu_indices
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",");

            println!(
                "| {:>6} | {} | {:>5.1} | {:>7.1} | {:>5} | {:>6.1} MB |",
                proc.pid,
                name,
                proc.cpu_usage(),
                proc.memory_mb(),
                gpus,
                proc.gpu_memory_mb()
                    .map_or_else(|| "n/a".to_string(), |mb| format!("{mb:.1}"))
            );
            count += 1;
        }

        if count == 0 {
            println!("|              No GPU-using processes found                              |");
        }

        println!("+---------+--------------------------+-------+---------+-------+----------+");

        // Display top CPU-using processes
        let cpu_procs = monitor.processes_by_cpu()?;
        println!("\n+--------+--------------------------+-------+---------+----------------+");
        println!("|                    TOP CPU-USING PROCESSES                          |");
        println!("+---------+--------------------------+-------+---------+---------------+");
        println!("|  PID   | Name                     |  CPU% | Mem(MB) | State/Priority|");
        println!("+---------+--------------------------+-------+---------+---------------+");

        for proc in cpu_procs.iter().take(10) {
            let name = if proc.name.len() > 24 {
                format!("{}...", &proc.name[..21])
            } else {
                format!("{:<24}", proc.name)
            };

            let state_prio = if let Some(prio) = proc.priority {
                format!("{} / {:>3}", proc.state, prio)
            } else {
                format!("{} / N/A", proc.state)
            };

            println!(
                "| {:>6} | {} | {:>5.1} | {:>7.1} | {:>13} |",
                proc.pid,
                name,
                proc.cpu_usage(),
                proc.memory_mb(),
                state_prio
            );
        }

        println!("+---------+--------------------------+-------+---------+---------------+");

        // Display summary statistics
        let total_procs = monitor.process_count()?;
        let gpu_proc_count = monitor.gpu_process_count()?;

        println!("\n+------------------------------------------------------------+");
        println!("|                     SUMMARY STATISTICS                     |");
        println!("+-------------------------------------------------+----------+");
        println!(
            "| Total Processes:                                | {:>8} |",
            total_procs
        );
        println!(
            "| GPU-Using Processes:                            | {:>8} |",
            gpu_proc_count
        );
        println!("+-------------------------------------------------+----------+");

        // Wait before next update
        thread::sleep(Duration::from_secs(2));
    }
}
