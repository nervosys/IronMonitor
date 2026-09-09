//! Time `MonitoringBackend` construction and update — the path behind `ironmon cli`.

fn main() {
    println!("--- step breakdown ---");
    breakdown();
    println!("--- whole path ---");
    let t = std::time::Instant::now();
    let mut backend = match ironmonlib::backend::MonitoringBackend::new() {
        Ok(b) => b,
        Err(e) => {
            println!("new() failed: {e}");
            return;
        }
    };
    println!("MonitoringBackend::new():  {:>8.1} ms", ms(t));

    for round in 0..3 {
        let t = std::time::Instant::now();
        let _ = backend.update();
        println!("update() round {round}:       {:>8.1} ms", ms(t));
    }

    let t = std::time::Instant::now();
    let _ = backend.gpu_static_info();
    let _ = backend.gpu_dynamic_info();
    println!("gpu_*_info() accessors:    {:>8.1} ms", ms(t));
}

fn ms(t: std::time::Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Time each step `MonitoringBackend::with_config` performs, in its order.
#[allow(dead_code)]
fn breakdown() {
    macro_rules! step {
        ($label:expr, $body:expr) => {{
            let t = std::time::Instant::now();
            let v = $body;
            println!("  {:<28} {:>8.1} ms", $label, ms(t));
            v
        }};
    }

    let gpus = step!(
        "GpuCollection::auto_detect",
        ironmonlib::gpu::GpuCollection::auto_detect().ok()
    );
    if let Some(ref g) = gpus {
        step!("gpus.snapshot_all", g.snapshot_all().unwrap_or_default());
    }
    step!(
        "ProcessMonitor::new",
        ironmonlib::ProcessMonitor::new().ok()
    );
    step!(
        "NetworkMonitor::new",
        ironmonlib::network_monitor::NetworkMonitor::new().ok()
    );
    step!(
        "ConnectionMonitor::new",
        ironmonlib::connections::ConnectionMonitor::new().ok()
    );
    step!(
        "disk::enumerate_disks",
        ironmonlib::disk::enumerate_disks()
            .unwrap_or_default()
            .len()
    );
    step!(
        "motherboard::enumerate_sensors",
        ironmonlib::motherboard::enumerate_sensors()
            .unwrap_or_default()
            .len()
    );
    step!(
        "motherboard::get_system_info",
        ironmonlib::motherboard::get_system_info().ok()
    );
    step!(
        "motherboard::get_driver_versions",
        ironmonlib::motherboard::get_driver_versions()
            .unwrap_or_default()
            .len()
    );
    step!(
        "SystemStats::new",
        ironmonlib::system_stats::SystemStats::new().ok()
    );
    step!(
        "AgentConfig::auto_detect",
        ironmonlib::agent::AgentConfig::auto_detect().ok()
    );
}
