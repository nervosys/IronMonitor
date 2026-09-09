//! Test example to verify new Windows process fields
//!
//! Shows thread count, handle count, I/O bytes, parent PID, and other new fields

use ironmonlib::{ProcessMonitor, Result};

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

fn main() -> Result<()> {
    println!("=== Testing New Windows Process Fields ===\n");

    let mut monitor = ProcessMonitor::new()?;
    let processes = monitor.processes()?;

    println!("Found {} total processes\n", processes.len());

    // Sort by thread count to show interesting processes
    let mut procs: Vec<_> = processes.iter().collect();
    procs.sort_by_key(|p| std::cmp::Reverse(p.thread_count));

    println!("Top 20 processes by thread count:");
    println!("{:-<110}", "");
    println!(
        "{:>7} | {:>6} | {:>5} | {:>7} | {:>10} | {:>10} | {:>10} | {:<30}",
        "PID", "PARENT", "THRD", "HANDLES", "I/O READ", "I/O WRITE", "PRIV MEM", "NAME"
    );
    println!("{:-<110}", "");

    for p in procs.iter().take(20) {
        let parent = p
            .parent_pid
            .map(|pid: u32| pid.to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:>7} | {:>6} | {:>5} | {:>7} | {:>10} | {:>10} | {:>10} | {:<30}",
            p.pid,
            parent,
            p.thread_count,
            p.handle_count,
            format_bytes(p.io_read_bytes),
            format_bytes(p.io_write_bytes),
            format_bytes(p.private_bytes),
            p.name.chars().take(30).collect::<String>()
        );
    }

    println!("\n{:-<110}", "");

    // Show processes with highest I/O
    procs.sort_by(|a, b| {
        (b.io_read_bytes + b.io_write_bytes).cmp(&(a.io_read_bytes + a.io_write_bytes))
    });

    println!("\nTop 10 processes by total I/O:");
    println!("{:-<110}", "");
    println!(
        "{:>7} | {:>5} | {:>12} | {:>12} | {:>12} | {:<30}",
        "PID", "THRD", "I/O READ", "I/O WRITE", "TOTAL I/O", "NAME"
    );
    println!("{:-<110}", "");

    for p in procs.iter().take(10) {
        let total_io = p.io_read_bytes + p.io_write_bytes;
        println!(
            "{:>7} | {:>5} | {:>12} | {:>12} | {:>12} | {:<30}",
            p.pid,
            p.thread_count,
            format_bytes(p.io_read_bytes),
            format_bytes(p.io_write_bytes),
            format_bytes(total_io),
            p.name.chars().take(30).collect::<String>()
        );
    }

    // Show some processes with start time
    println!("\n\nSample processes with start time (Unix timestamp):");
    println!("{:-<90}", "");
    for p in procs.iter().take(5) {
        if let Some(start) = p.start_time {
            // Convert Unix timestamp to readable time
            let datetime = chrono::DateTime::from_timestamp(start as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| format!("{}", start));
            println!("PID {} ({}): started at {}", p.pid, p.name, datetime);
        }
    }

    println!("\nDone!");
    Ok(())
}
