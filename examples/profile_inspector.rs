//! Profile Inspector demo — NVIDIA Profile Inspector / XTU / Ryzen Master /
//! nvme-cli style read-only enumeration of vendor driver settings.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --features full --example profile_inspector
//! cargo run --release --features full --example profile_inspector -- gpu
//! cargo run --release --features full --example profile_inspector -- search xmp
//! ```

use ironmonlib::profile::{ProfileInspector, Subsystem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut inspector = ProfileInspector::new();

    match args.first().map(String::as_str) {
        None => print_summary(&mut inspector),
        Some("search") => {
            let query = args.get(1).cloned().unwrap_or_default();
            let snap = inspector.snapshot_all();
            println!("Searching profile settings for {:?}\n", query);
            for (sub, group, setting) in snap.search(&query) {
                println!(
                    "[{}] {} / {}",
                    sub.as_str(),
                    group.device,
                    group.display_name
                );
                println!(
                    "  {:<30} = {}{}",
                    setting.display_name,
                    setting.value,
                    setting
                        .unit
                        .as_deref()
                        .map(|u| format!(" {}", u))
                        .unwrap_or_default()
                );
            }
        }
        Some(name) => match Subsystem::parse(name) {
            Some(sub) => print_subsystem(&mut inspector, sub),
            None => {
                eprintln!(
                    "Unknown subsystem: {}. Valid: gpu, cpu, nvme, display, memory",
                    name
                );
                std::process::exit(2);
            }
        },
    }
    Ok(())
}

fn print_summary(inspector: &mut ProfileInspector) {
    let snap = inspector.snapshot_all();
    println!(
        "Hardware Profile Inspector — {} groups, {} settings\n",
        snap.total_groups(),
        snap.total_settings()
    );
    for sub in Subsystem::ALL {
        let groups = snap.providers.get(sub).cloned().unwrap_or_default();
        let settings: usize = groups.iter().map(|g| g.settings.len()).sum();
        println!(
            "  {:<10} {:>2} groups, {:>4} settings",
            sub.as_str(),
            groups.len(),
            settings
        );
    }
}

fn print_subsystem(inspector: &mut ProfileInspector, sub: Subsystem) {
    let groups = inspector.snapshot(sub);
    println!("== {} ==", sub);
    for group in &groups {
        println!("\n▸ {} ({})", group.device, group.display_name);
        println!("  source: {}", group.source);
        for s in &group.settings {
            let unit = s
                .unit
                .as_deref()
                .map(|u| format!(" {}", u))
                .unwrap_or_default();
            println!("    {:<28} = {}{}", s.id, s.value, unit);
        }
        for n in &group.notes {
            println!("    • {}", n);
        }
    }
}
