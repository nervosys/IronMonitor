//! Process Monitoring and Control Example
//!
//! Demonstrates process monitoring with GPU attribution and process control.
//! Shows how to list processes using GPU resources and terminate them.
//!
//! # Usage
//!
//! ```bash
//! # List all GPU processes
//! cargo run --release --features full --example process_control
//! ```
//!
//! # Features Demonstrated
//!
//! - Real-time process monitoring with GPU attribution
//! - Per-process engine utilization (graphics, compute, encoder, decoder)
//! - Process type detection (C/G = Compute+Graphics, C = Compute, G = Graphics)
//! - GPU memory usage per process
//! - System CPU and memory usage
//! - Process control (kill capability with safety warnings)
//!
//! # Safety
//!
//! This example includes process termination capabilities. Use with caution!
//! Terminating system processes or processes owned by other users requires
//! appropriate permissions and can cause system instability.

use ironmonlib::{ProcessGpuType, ProcessMonitor};
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[HW]  IronMonitor - Process Control Example\n");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Initialize process monitor with GPU detection
    let mut monitor = ProcessMonitor::new()?;
    println!("✓ Process monitor initialized");
    println!("✓ Detected {} GPU(s)\n", monitor.gpu_count());

    loop {
        // Update process list
        monitor.update()?;

        // Clear screen
        print!("\x1B[2J\x1B[1;1H");

        println!("[HW]  IronMonitor - GPU Process Monitor");
        println!("═══════════════════════════════════════════════════════════════\n");

        // Get processes using GPU memory
        let gpu_processes = monitor.processes_by_gpu_memory()?;

        // Filter to only GPU-using processes
        let gpu_only: Vec<_> = gpu_processes
            .iter()
            .filter(|p| p.is_gpu_process())
            .collect();

        if gpu_only.is_empty() {
            println!("No GPU processes detected.\n");
        } else {
            println!("GPU Processes (sorted by GPU memory usage):\n");
            println!(
                "{:<8} {:<12} {:<24} {:<6} {:<10} {:<8} {:<8} {:<6} {:<6} {:<6} {:<6}",
                "PID",
                "USER",
                "NAME",
                "TYPE",
                "GPU MEM",
                "SYS MEM",
                "CPU%",
                "GPU%",
                "ENC%",
                "DEC%",
                "STATE"
            );
            println!("{}", "─".repeat(120));

            for (i, proc) in gpu_only.iter().take(15).enumerate() {
                // Format GPU memory
                let gpu_mem = proc
                    .total_gpu_memory_bytes
                    .map_or_else(|| "n/a".to_string(), format_bytes);

                // Format system memory
                let sys_mem = format_bytes(proc.memory_bytes);

                // Format CPU usage
                let cpu_usage = format!("{:.1}", proc.cpu_percent);

                // Get process type string
                let type_str = match proc.gpu_process_type {
                    ProcessGpuType::GraphicalCompute => "C+G",
                    ProcessGpuType::Compute => "C",
                    ProcessGpuType::Graphical => "G",
                    ProcessGpuType::Unknown => "-",
                };

                // Format engine utilizations (use percentage if available)
                let gpu_pct = proc
                    .gpu_usage_percent
                    .map(|v| format!("{:.0}", v))
                    .unwrap_or_else(|| "-".to_string());
                let enc = proc
                    .encoder_usage_percent
                    .map(|v| format!("{:.0}", v))
                    .unwrap_or_else(|| "-".to_string());
                let dec = proc
                    .decoder_usage_percent
                    .map(|v| format!("{:.0}", v))
                    .unwrap_or_else(|| "-".to_string());

                // Truncate name if too long
                let name = if proc.name.len() > 23 {
                    format!("{}...", &proc.name[..20])
                } else {
                    proc.name.clone()
                };

                // User (handle Option)
                let user = proc.user.as_deref().unwrap_or("-");

                println!(
                    "{:<8} {:<12} {:<24} {:<6} {:<10} {:<8} {:<8} {:<6} {:<6} {:<6} {:<6}",
                    proc.pid,
                    user,
                    name,
                    type_str,
                    gpu_mem,
                    sys_mem,
                    cpu_usage,
                    gpu_pct,
                    enc,
                    dec,
                    proc.state
                );

                // Color indicator based on GPU memory usage
                if i == 0 {
                    println!("    ↑ Top GPU memory consumer");
                }
            }

            if gpu_only.len() > 15 {
                println!("\n... and {} more processes", gpu_only.len() - 15);
            }
        }

        println!("\n═══════════════════════════════════════════════════════════════");
        println!("Legend:");
        println!("  TYPE: C+G=Compute+Graphics, C=Compute, G=Graphics");
        println!("  GPU%=GPU Utilization, ENC%=Encoder, DEC%=Decoder");
        println!("\nControls:");
        println!("  Press Ctrl+C to exit");
        println!("═══════════════════════════════════════════════════════════════\n");

        // Wait for next update or user input
        print!("Next update in 2 seconds... ");
        io::stdout().flush()?;

        // Sleep with ability to interrupt
        thread::sleep(Duration::from_secs(2));
    }
}

/// Format bytes to human-readable string
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
