//! Measure the snapshot pipeline: per-collector cost, concurrent vs serial tick cost,
//! and reader-side latency.
//!
//! Run with: `cargo run --release --example pipeline_bench`

use std::time::{Duration, Instant};

use ironmonlib::pipeline::{Collector, CollectorConfig};

fn main() {
    println!("Spawning collector...\n");

    let collector = Collector::spawn(CollectorConfig {
        interval: Duration::from_millis(500),
        ..Default::default()
    });
    let handle = collector.handle();

    // Phase 1: time to *any* data. The collector publishes a warm-up snapshot from
    // the collectors that need no driver setup, so this should be milliseconds even
    // though GPU enumeration takes seconds.
    let start = Instant::now();
    while handle.generation() == 0 && start.elapsed() < Duration::from_secs(15) {
        std::thread::sleep(Duration::from_millis(1));
    }

    if handle.generation() == 0 {
        eprintln!("collector produced no snapshot within 15s");
        return;
    }
    let warmup_latency = start.elapsed();
    let warmup = handle.latest();

    // Phase 2: time until every source is populated, which is gated by GPU driver
    // enumeration. This is what the old pipeline made you wait for before showing
    // anything at all.
    while handle.latest().gpu_dynamic.is_empty()
        && handle.latest().processes.is_empty()
        && start.elapsed() < Duration::from_secs(60)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    let full_latency = start.elapsed();

    println!("=== cold start ===");
    println!(
        "  first data (warm-up)   {:>8.1} ms",
        warmup_latency.as_secs_f64() * 1000.0
    );
    println!("    cpu present          {}", warmup.cpu.is_some());
    println!("    memory present       {}", warmup.memory.is_some());
    println!(
        "  fully populated        {:>8.1} ms",
        full_latency.as_secs_f64() * 1000.0
    );

    // Let a few ticks accumulate so rate-based collectors have deltas to work with.
    std::thread::sleep(Duration::from_secs(3));

    let snap = handle.latest();
    let t = &snap.timings;

    println!(
        "=== per-collector cost (generation {}) ===",
        snap.generation
    );
    println!("  cpu          {:>8.2} ms", t.cpu_us as f64 / 1000.0);
    println!("  memory       {:>8.2} ms", t.memory_us as f64 / 1000.0);
    println!("  gpu          {:>8.2} ms", t.gpu_us as f64 / 1000.0);
    println!("  processes    {:>8.2} ms", t.process_us as f64 / 1000.0);
    println!("  network      {:>8.2} ms", t.network_us as f64 / 1000.0);
    println!("  connections  {:>8.2} ms", t.connection_us as f64 / 1000.0);
    println!("  disks        {:>8.2} ms", t.disk_us as f64 / 1000.0);
    println!("  system       {:>8.2} ms", t.system_us as f64 / 1000.0);

    let serial_ms = t.serial_us() as f64 / 1000.0;
    let critical_ms = t.critical_path_us() as f64 / 1000.0;
    let actual_ms = snap.collect_us as f64 / 1000.0;

    println!("\n=== tick cost ===");
    println!("  serial (old update() path)  {:>8.2} ms", serial_ms);
    println!("  critical path (slowest)     {:>8.2} ms", critical_ms);
    println!("  actual concurrent tick      {:>8.2} ms", actual_ms);
    if actual_ms > 0.0 {
        println!(
            "  speedup vs serial           {:>8.2}x",
            serial_ms / actual_ms
        );
    }

    // Reader-side latency: what a render thread pays to obtain state.
    let iterations = 100_000;
    let read_start = Instant::now();
    let mut sink = 0u64;
    for _ in 0..iterations {
        sink = sink.wrapping_add(handle.latest().generation);
    }
    let per_read_ns = read_start.elapsed().as_nanos() as f64 / iterations as f64;

    let gen_start = Instant::now();
    for _ in 0..iterations {
        sink = sink.wrapping_add(handle.generation());
    }
    let per_gen_ns = gen_start.elapsed().as_nanos() as f64 / iterations as f64;

    println!("\n=== reader-side cost (what a frame pays) ===");
    println!("  latest()      {:>8.1} ns/read", per_read_ns);
    println!("  generation()  {:>8.1} ns/read", per_gen_ns);
    println!("  (checksum {sink})");

    println!("\n=== payload ===");
    println!("  processes    {}", snap.processes.len());
    println!("  connections  {}", snap.connections.len());
    println!("  disks        {}", snap.disks.len());
    println!("  interfaces   {}", snap.network.len());
    println!("  gpus         {}", snap.gpu_dynamic.len());
    let show = |v: Option<f32>| match v {
        Some(p) => format!("{p:.1}%"),
        None => "not read".to_string(),
    };
    println!("  cpu util     {}", show(snap.cpu_utilization()));
    println!("  mem util     {}", show(snap.memory_utilization()));
}
