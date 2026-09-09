//! CPU Frequency Scaling Monitor
//!
//! Demonstrates:
//! - CPU frequency monitoring
//! - Governor detection and control
//! - Turbo boost status
//! - CPU idle states (C-states)
//! - Energy preferences (Intel/AMD P-state)

use ironmonlib::cpufreq::{
    available_governors, cpufreq_summary, list_cpus, CpuFreqMonitor, EnergyPreference, Governor,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("⚡ CPU Frequency Scaling Monitor");
    println!("{}", "=".repeat(70));

    // Get summary first
    match cpufreq_summary() {
        Ok(summary) => {
            println!("\n📊 CPU Summary");
            println!("├─ Total CPUs: {}", summary.total_cpus);
            println!("├─ Online CPUs: {}", summary.online_cpus);

            if let Some(ref model) = summary.cpu_model {
                // Truncate long model names
                let model_short = if model.len() > 50 {
                    format!("{}...", &model[..50])
                } else {
                    model.clone()
                };
                println!("├─ Model: {}", model_short);
            }

            if let Some(ref driver) = summary.scaling_driver {
                println!("├─ Scaling driver: {}", driver);
            }

            if let Some(ref gov) = summary.governor {
                println!("├─ Governor: {}", gov);
            }

            println!(
                "├─ Frequency range: {} - {} MHz",
                summary.min_freq_mhz, summary.max_freq_mhz
            );
            println!("├─ Average freq: {} MHz", summary.avg_freq_mhz);
            println!(
                "└─ Turbo boost: {}",
                if summary.turbo_enabled {
                    "✅ Enabled"
                } else {
                    "❌ Disabled"
                }
            );
        }
        Err(e) => {
            println!("\n⚠️  Could not get CPU summary: {}", e);
        }
    }

    // Create monitor for detailed info
    let monitor = match CpuFreqMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            println!("\n❌ Could not create CPU frequency monitor: {}", e);
            println!("   This may require root access on some systems.");
            return Ok(());
        }
    };

    // Show available governors
    println!("\n🎛️  Available Governors");
    println!("{}", "-".repeat(70));

    match available_governors() {
        Ok(govs) => {
            if govs.is_empty() {
                println!("No governors available (cpufreq may not be supported)");
            } else {
                for gov in &govs {
                    let current = monitor.current_governor();
                    let marker = if current.as_ref() == Some(gov) {
                        " ← current"
                    } else {
                        ""
                    };

                    let desc = match gov {
                        Governor::Performance => "Max frequency always",
                        Governor::Powersave => "Min frequency always",
                        Governor::Ondemand => "Dynamic scaling (legacy)",
                        Governor::Conservative => "Gradual scaling",
                        Governor::Userspace => "Manual control",
                        Governor::Schedutil => "Scheduler-driven (recommended)",
                        Governor::IntelPstate => "Intel P-state driver",
                        Governor::AmdPstate => "AMD P-state driver",
                        Governor::Interactive => "Interactive (Android)",
                        Governor::Unknown(_) => "Unknown governor",
                    };

                    println!("   {:<15} - {}{}", gov.to_string(), desc, marker);
                }
            }
        }
        Err(e) => {
            println!("Could not get governors: {}", e);
        }
    }

    // Turbo boost status
    println!("\n🚀 Turbo Boost Status");
    println!("{}", "-".repeat(70));

    let turbo = monitor.turbo_status();
    println!(
        "   Available: {}",
        if turbo.available { "✅ Yes" } else { "❌ No" }
    );
    println!(
        "   Enabled: {}",
        if turbo.enabled { "✅ Yes" } else { "❌ No" }
    );
    println!(
        "   Controllable: {}",
        if turbo.controllable {
            "✅ Yes"
        } else {
            "❌ No"
        }
    );
    if let Some(boost) = turbo.boost_mhz {
        println!("   Boost: +{} MHz", boost);
    }

    // Per-CPU details
    println!("\n🔢 Per-CPU Frequency Info");
    println!("{}", "-".repeat(70));

    match list_cpus() {
        Ok(cpus) => {
            // Group CPUs by online status
            let online: Vec<_> = cpus.iter().filter(|c| c.online).collect();
            let offline: Vec<_> = cpus.iter().filter(|c| !c.online).collect();

            // Header
            println!(
                "   {:>4} │ {:>8} │ {:>8} │ {:>8} │ {:>6} │ {:>12}",
                "CPU", "Current", "Min", "Max", "Usage", "Governor"
            );
            println!(
                "   {:─>4}─┼─{:─>8}─┼─{:─>8}─┼─{:─>8}─┼─{:─>6}─┼─{:─>12}",
                "", "", "", "", "", ""
            );

            for cpu in &online {
                let freq_bar = create_freq_bar(cpu.freq_percent(), 6);
                let gov_str = cpu.governor.to_string();
                let gov_short = if gov_str.len() > 12 {
                    format!("{}...", &gov_str[..9])
                } else {
                    gov_str
                };

                let turbo_marker = match cpu.is_turbo() {
                    Some(true) => "🔥",
                    Some(false) => "  ",
                    // No base frequency, so no answer — not "not boosting".
                    None => " ?",
                };

                println!(
                    "   {:>4} │ {:>6} {} │ {:>6} │ {:>6} │ {} │ {:<12}",
                    cpu.id,
                    cpu.current_freq_mhz,
                    turbo_marker,
                    cpu.min_freq_khz / 1000,
                    cpu.max_freq_khz / 1000,
                    freq_bar,
                    gov_short
                );
            }

            // Show offline CPUs
            if !offline.is_empty() {
                println!(
                    "\n   Offline CPUs: {:?}",
                    offline.iter().map(|c| c.id).collect::<Vec<_>>()
                );
            }

            // CPU frequency visualization
            if !online.is_empty() {
                println!("\n📊 CPU Frequency Distribution");
                println!("{}", "-".repeat(70));

                let max_freq = online.iter().map(|c| c.max_freq_khz).max().unwrap_or(1);

                for cpu in online.iter().take(16) {
                    // Limit to 16 CPUs for display
                    let bar_len = ((cpu.current_freq_khz as f64 / max_freq as f64) * 40.0) as usize;
                    let bar = "█".repeat(bar_len);
                    let turbo = match cpu.is_turbo() {
                        Some(true) => "🔥",
                        Some(false) => "",
                        None => "?",
                    };

                    println!(
                        "   CPU{:>2} │{}│ {} MHz {}",
                        cpu.id, bar, cpu.current_freq_mhz, turbo
                    );
                }

                if online.len() > 16 {
                    println!("   ... and {} more CPUs", online.len() - 16);
                }
            }
        }
        Err(e) => {
            println!("Could not list CPUs: {}", e);
        }
    }

    // Energy preferences (Intel/AMD P-state)
    println!("\n⚡ Energy Performance Preferences");
    println!("{}", "-".repeat(70));

    let cpus = monitor.cpus();
    if let Some(cpu) = cpus.iter().find(|c| c.online) {
        if let Some(ref epp) = cpu.energy_preference {
            println!("   Current: {:?}", epp);
        }

        if !cpu.available_energy_preferences.is_empty() {
            println!("   Available preferences:");
            for pref in &cpu.available_energy_preferences {
                let desc = match pref {
                    EnergyPreference::Performance => "Maximum performance, highest power",
                    EnergyPreference::BalancePerformance => "Favor performance over power",
                    EnergyPreference::BalancePower => "Favor power over performance",
                    EnergyPreference::Power => "Maximum power saving",
                };
                println!("     • {:20} - {}", pref.to_string(), desc);
            }
        } else {
            println!("   No energy preferences available (not using P-state driver)");
        }
    }

    // Idle states (C-states)
    println!("\n😴 CPU Idle States (C-states)");
    println!("{}", "-".repeat(70));

    if let Some(cpu) = cpus.iter().find(|c| c.online && !c.idle_states.is_empty()) {
        println!("   States for CPU{}:", cpu.id);
        println!(
            "   {:>8} │ {:>12} │ {:>12} │ {:>10} │ {:>8}",
            "State", "Latency (µs)", "Usage", "Time (ms)", "Status"
        );
        println!(
            "   {:─>8}─┼─{:─>12}─┼─{:─>12}─┼─{:─>10}─┼─{:─>8}",
            "", "", "", "", ""
        );

        for state in &cpu.idle_states {
            let status = if state.enabled { "✅ On" } else { "❌ Off" };
            let time_ms = state.time_us / 1000;

            println!(
                "   {:>8} │ {:>12} │ {:>12} │ {:>10} │ {:>8}",
                state.name, state.latency_us, state.usage, time_ms, status
            );
        }

        // Show description if available
        for state in &cpu.idle_states {
            if let Some(ref desc) = state.desc {
                println!("\n   {} - {}", state.name, desc);
            }
        }
    } else {
        println!("   No CPU idle states available");
    }

    // Recommendations
    println!("\n💡 Usage Tips");
    println!("{}", "-".repeat(70));
    println!("   • For maximum performance: Use 'performance' governor or P-state EPP");
    println!("   • For power saving: Use 'powersave' governor or 'power' EPP");
    println!("   • For balanced use: 'schedutil' adapts to workload (recommended)");
    println!("   • Disable turbo to reduce heat on laptops");
    println!("   • Disable deep C-states for low-latency workloads");

    // Platform-specific notes
    #[cfg(target_os = "linux")]
    {
        println!("\n📝 Linux Notes");
        println!("{}", "-".repeat(70));
        println!("   • Governor control requires root access");
        println!("   • P-state EPP available with intel_pstate or amd_pstate drivers");
        println!("   • Check /sys/devices/system/cpu/ for raw values");
        println!("   • Use cpupower or cpufrequtils for command-line control");
    }

    #[cfg(target_os = "windows")]
    {
        println!("\n📝 Windows Notes");
        println!("{}", "-".repeat(70));
        println!("   • Use Power Options control panel for power plan");
        println!("   • High Performance = Performance governor");
        println!("   • Balanced = Schedutil equivalent");
        println!("   • Power Saver = Powersave governor");
    }

    Ok(())
}

/// Create a visual frequency bar
fn create_freq_bar(percent: f32, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f32) as usize;
    let empty = width.saturating_sub(filled);

    format!("{}{}", "▓".repeat(filled), "░".repeat(empty))
}
