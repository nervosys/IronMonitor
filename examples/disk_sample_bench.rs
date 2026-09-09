//! Time one full sampling of every disk — the work the GUI used to do inline.
//!
//! `refresh_cached_disk_data` ran this on the UI thread every 2 seconds, which is
//! what made the window lock up on a fixed beat.

fn main() {
    let t = std::time::Instant::now();
    let disks = ironmonlib::disk::enumerate_disks().unwrap_or_default();
    println!(
        "enumerate_disks: {:?} ({} disks)\n",
        t.elapsed(),
        disks.len()
    );

    for round in 0..3 {
        let round_start = std::time::Instant::now();
        for disk in &disks {
            let name = disk.name().to_string();
            let t = std::time::Instant::now();
            let _ = disk.info();
            let info_ms = t.elapsed().as_secs_f64() * 1000.0;

            let t = std::time::Instant::now();
            let _ = disk.io_stats();
            let io_ms = t.elapsed().as_secs_f64() * 1000.0;

            let t = std::time::Instant::now();
            let _ = disk.health();
            let health_ms = t.elapsed().as_secs_f64() * 1000.0;

            let t = std::time::Instant::now();
            let _ = disk.filesystem_info();
            let fs_ms = t.elapsed().as_secs_f64() * 1000.0;

            let t = std::time::Instant::now();
            let _ = disk.temperature();
            let temp_ms = t.elapsed().as_secs_f64() * 1000.0;

            println!(
                "  [{round}] {name:<22} info {info_ms:>7.1}  io {io_ms:>7.1}  \
                 health {health_ms:>7.1}  fs {fs_ms:>7.1}  temp {temp_ms:>7.1}  (ms)"
            );
        }
        println!(
            "  [{round}] full sampling of all disks: {:.1} ms\n",
            round_start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
