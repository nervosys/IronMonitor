//! Test example for TSDB functionality
//!
//! Tests the time-series database by recording and reading back data

use ironmonlib::tsdb::{
    format_size, MetricsRecorder, ProcessSnapshot, SystemSnapshot, TimeSeriesDb,
};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TSDB Recording Test ===\n");

    let db_path = "test_tsdb_example.db";

    // Clean up any existing test file
    let _ = std::fs::remove_file(db_path);

    // Create recorder with small size for testing
    let mut recorder = MetricsRecorder::new(
        db_path,
        10 * 1024 * 1024, // 10 MB
        Duration::from_millis(500),
        10,
    )?;

    println!("Recording 10 test snapshots...");

    for i in 0..10 {
        let timestamp = TimeSeriesDb::now_millis();

        // Create test snapshot
        let snapshot = SystemSnapshot {
            timestamp,
            cpu_percent: Some(25.0 + (i as f32 * 5.0)),
            // One core whose idle time was not read, so the round trip covers
            // an absent per-core entry as well as present ones.
            cpu_per_core: vec![Some(20.0), Some(30.0), None, Some(35.0)],
            memory_used: Some(8_000_000_000 + (i as u64 * 100_000_000)),
            memory_total: Some(16_000_000_000),
            swap_used: Some(500_000_000),
            swap_total: Some(8_000_000_000),
            gpu_percent: vec![Some(50.0 + (i as f32 * 2.0))],
            gpu_memory_used: vec![Some(4_000_000_000)],
            // A second adapter with no sensor, so the round trip covers an
            // absent column as well as a present one.
            gpu_temperature: vec![Some(65.0 + (i as f32)), None],
            gpu_power_mw: vec![Some(150_000 + (i as u32 * 1000)), None],
            net_rx_bps: Some(1_000_000 + (i as u64 * 100_000)),
            net_tx_bps: Some(500_000 + (i as u64 * 50_000)),
            processes: vec![ProcessSnapshot {
                pid: 1234,
                name: "test_process".to_string(),
                cpu_percent: 10.0 + (i as f32),
                memory_bytes: 500_000_000,
                gpu_memory_bytes: Some(1_000_000_000),
                gpu_percent: Some(25.0),
                disk_read_bps: Some(10_000_000),
                disk_write_bps: Some(5_000_000),
                net_rx_bps: Some(100_000),
                net_tx_bps: Some(50_000),
            }],
        };

        recorder.record_snapshot(snapshot)?;
        println!("  Recorded snapshot {} at {}", i + 1, timestamp);

        std::thread::sleep(Duration::from_millis(100));
    }

    // Close recorder to flush
    recorder.close()?;
    println!("\nRecorder closed.");

    // Now read back
    println!("\n=== Reading Back Data ===\n");

    let mut db = TimeSeriesDb::new(db_path, 0)?;
    let stats = db.stats();

    println!("Database Stats:");
    println!("  Path: {}", stats.path.display());
    println!("  Max Size: {}", format_size(stats.max_size));
    println!(
        "  Current Size: {} ({:.1}%)",
        format_size(stats.current_size),
        stats.usage_percent()
    );
    println!("  Record Count: {}", stats.record_count);

    if let Some(first) = stats.first_timestamp {
        println!("  First Timestamp: {}", first);
    }
    if let Some(last) = stats.last_timestamp {
        println!("  Last Timestamp: {}", last);
    }
    if let Some(span) = stats.time_span() {
        println!("  Time Span: {}", span);
    }

    // Read all snapshots
    let snapshots = db.read_all_system_snapshots()?;
    println!("\nRead {} snapshots:", snapshots.len());

    for (i, s) in snapshots.iter().enumerate() {
        let util = match s.gpu_percent.first().copied().flatten() {
            Some(u) => format!("{:.1}%", u),
            None => "not read".to_string(),
        };
        let temp = match s.gpu_temperature.first().copied().flatten() {
            Some(t) => format!("{:.0}°C", t),
            None => "no sensor".to_string(),
        };
        let cpu = match s.cpu_percent {
            Some(p) => format!("{p:.1}%"),
            None => "not read".to_string(),
        };
        let mem = match (s.memory_used, s.memory_total) {
            (Some(used), Some(total)) if total > 0 => {
                format!("{:.1}%", (used as f64 / total as f64) * 100.0)
            }
            _ => "not read".to_string(),
        };
        let cores = s
            .cpu_per_core
            .iter()
            .map(|c| match c {
                Some(v) => format!("{v:.0}"),
                None => "-".to_string(),
            })
            .collect::<Vec<_>>()
            .join("/");
        println!(
            "  [{}] CPU: {} ({}) | MEM: {} | GPU: {} | Temp: {} | Net: {} rx/s {} tx/s",
            i + 1,
            cpu,
            cores,
            mem,
            util,
            temp,
            s.net_rx_bps.map(format_size).unwrap_or_else(|| "—".into()),
            s.net_tx_bps.map(format_size).unwrap_or_else(|| "—".into())
        );
    }

    // Clean up
    let _ = std::fs::remove_file(db_path);
    println!("\nTest database cleaned up.");

    println!("\n=== Test Complete ===");
    Ok(())
}
