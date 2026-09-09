//! Time each GPU adapter's per-tick query separately.
//!
//! The collector's GPU stage costs far more than NVML alone should, and the machine
//! that showed it has two NVIDIA cards plus an AMD integrated GPU served by WMI
//! performance counters. This attributes the cost per device.

fn main() {
    let collection = ironmonlib::gpu::GpuCollection::auto_detect().expect("enumerate GPUs");
    println!("{} GPU(s)\n", collection.gpus().len());

    for round in 0..3 {
        println!("round {round}:");
        let all = std::time::Instant::now();
        let results = collection.snapshot_all_partial();
        let all_elapsed = all.elapsed();

        for (i, r) in results.iter().enumerate() {
            match r {
                Ok(info) => println!(
                    "  [{i}] {:<34} ok",
                    info.static_info.name.chars().take(34).collect::<String>()
                ),
                Err(e) => println!("  [{i}] query failed: {e}"),
            }
        }
        println!("  concurrent snapshot_all_partial: {:?}", all_elapsed);

        // Serial, per device, splitting static from dynamic to attribute the cost.
        for (i, gpu) in collection.gpus().iter().enumerate() {
            let t = std::time::Instant::now();
            let s = gpu.static_info();
            let static_ms = t.elapsed().as_secs_f64() * 1000.0;

            let t = std::time::Instant::now();
            let _ = gpu.dynamic_info();
            let dynamic_ms = t.elapsed().as_secs_f64() * 1000.0;

            let t = std::time::Instant::now();
            let procs = gpu.processes().map(|p| p.len()).unwrap_or(0);
            let procs_ms = t.elapsed().as_secs_f64() * 1000.0;

            let name = s.map(|i| i.name).unwrap_or_else(|_| "<error>".into());
            println!(
                "  [{i}] {:<30} static {:>7.1} ms  dynamic {:>7.1} ms  processes {:>7.1} ms ({procs})",
                name.chars().take(30).collect::<String>(),
                static_ms,
                dynamic_ms,
                procs_ms
            );
        }
        println!();
    }
}
