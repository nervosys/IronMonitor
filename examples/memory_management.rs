// Memory Management Example for IronMonitor
//
// Demonstrates comprehensive memory and swap monitoring.

use ironmonitor::{format_bytes, memory_summary, MemoryMonitor, MemoryPressure, SwapType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧠 Memory & Swap Monitor");
    println!("{}", "═".repeat(60));

    // Get detailed memory information
    let monitor = MemoryMonitor::new()?;

    // === Memory Information ===
    println!("\n📊 Memory Overview");
    println!("────────────────────────────────────────────────────────────");

    let mem = &monitor.memory;
    let pressure = monitor.pressure();

    match pressure {
        Some(p) => {
            println!("   {} Memory Pressure: {:?}", p.emoji(), p);
            println!("   {}", p.description());
        }
        None => println!("   Memory Pressure: not read"),
    }
    println!();
    println!("   Total:      {:>12}", format_bytes(mem.total));
    println!(
        "   Used:       {:>12} ({})",
        shown_bytes(mem.used),
        shown_percent(mem.usage_percent())
    );
    println!("   Available:  {:>12}", shown_bytes(mem.available));
    println!("   Free:       {:>12}", shown_bytes(mem.free));

    if mem.cached.is_some() || mem.buffers.is_some() {
        println!();
        println!("   Cached:     {:>12}", shown_bytes(mem.cached));
        println!("   Buffers:    {:>12}", shown_bytes(mem.buffers));
    }

    if mem.active.is_some() || mem.inactive.is_some() {
        println!();
        println!("   Active:     {:>12}", shown_bytes(mem.active));
        println!("   Inactive:   {:>12}", shown_bytes(mem.inactive));
    }

    // === Swap Information ===
    println!("\n💾 Swap Information");
    println!("────────────────────────────────────────────────────────────");

    let swap = &monitor.swap;

    if swap.has_swap() {
        match monitor.swap_pressure() {
            Some(p) => println!("   {} Swap Pressure: {:?}", p.emoji(), p),
            None => println!("   Swap Pressure: not read"),
        }
        println!();
        println!("   Total:      {:>12}", format_bytes(swap.total));
        println!(
            "   Used:       {:>12} ({})",
            format_bytes(swap.used),
            shown_percent(swap.usage_percent())
        );
        println!("   Free:       {:>12}", format_bytes(swap.free));

        if !swap.devices.is_empty() {
            println!("\n   📁 Swap Devices:");
            for device in &swap.devices {
                let type_str = match device.swap_type {
                    SwapType::File => "📄",
                    SwapType::Partition => "💽",
                    SwapType::Zram => "🗜️",
                    SwapType::Unknown => "❓",
                };
                println!(
                    "      {} {} - {} ({} used, priority {})",
                    type_str,
                    device.path,
                    shown_bytes(device.total_bytes),
                    shown_percent(device.usage_percent()),
                    device
                        .priority
                        .map_or_else(|| "not read".to_string(), |p| p.to_string())
                );
            }
        }

        // ZRAM information
        if let Some(ref zram) = swap.zram {
            println!("\n   🗜️ ZRAM (Compressed RAM):");
            println!("      Device:      {}", zram.device);
            println!("      Disk Size:   {}", format_bytes(zram.disksize));
            println!("      Original:    {}", format_bytes(zram.original_bytes));
            println!("      Compressed:  {}", format_bytes(zram.compressed_bytes));
            println!("      Ratio:       {}", shown_ratio(zram.compression_ratio));
            println!(
                "      Algorithm:   {}",
                zram.algorithm.as_deref().unwrap_or("not read")
            );
            println!(
                "      Savings:     {} ({})",
                format_bytes(zram.memory_savings()),
                shown_percent(zram.savings_percent())
            );
        }
    } else {
        println!("   ⚠️  No swap configured");
        println!("   💡 Consider enabling swap for better memory management");
    }

    // === VM Statistics ===
    let stats = &monitor.stats;
    if stats.page_faults_minor.is_some() || stats.swap_in.is_some() {
        println!("\n📈 VM Statistics");
        println!("────────────────────────────────────────────────────────────");
        println!(
            "   Page faults (minor): {}",
            shown_count(stats.page_faults_minor)
        );
        println!(
            "   Page faults (major): {}",
            shown_count(stats.page_faults_major)
        );
        println!("   Swap in:             {}", shown_count(stats.swap_in));
        println!("   Swap out:            {}", shown_count(stats.swap_out));
        if stats.oom_kills.is_some_and(|n| n > 0) {
            println!("   ⚠️  OOM kills:        {}", shown_count(stats.oom_kills));
        }
    }

    // === VM Settings ===
    if !monitor.vm_settings.is_empty() {
        println!("\n⚙️  VM Settings");
        println!("────────────────────────────────────────────────────────────");
        for (key, value) in &monitor.vm_settings {
            let description = match key.as_str() {
                "swappiness" => format!("Swappiness: {} (0=avoid swap, 100=aggressive)", value),
                "vfs_cache_pressure" => format!("VFS Cache Pressure: {}", value),
                "dirty_ratio" => format!("Dirty Ratio: {}%", value),
                "dirty_background_ratio" => format!("Dirty Background Ratio: {}%", value),
                "overcommit_memory" => {
                    let mode = match value.as_str() {
                        "0" => "heuristic",
                        "1" => "always",
                        "2" => "never",
                        _ => value,
                    };
                    format!("Overcommit: {}", mode)
                }
                _ => format!("{}: {}", key, value),
            };
            println!("   {}", description);
        }
    }

    // === Top Memory Consumers ===
    println!("\n🔝 Top Memory Consumers");
    println!("────────────────────────────────────────────────────────────");
    println!(
        "   {:>6}  {:>10}  {:>10}  {:>6}  Name",
        "PID", "RSS", "Virtual", "Mem%"
    );
    println!("   {}", "─".repeat(54));

    let top_procs = monitor.top_processes(10);
    for proc in top_procs {
        println!(
            "   {:>6}  {:>10}  {:>10}  {:>5.1}%  {}",
            proc.pid,
            format_bytes(proc.rss),
            format_bytes(proc.vms),
            proc.memory_percent,
            if proc.name.len() > 20 {
                &proc.name[..20]
            } else {
                &proc.name
            }
        );
    }

    // === Health Score ===
    println!("\n🏥 Memory Health");
    println!("────────────────────────────────────────────────────────────");

    match monitor.health_score() {
        None => println!("   Health Score: not read"),
        Some(score) => {
            let health_bar = create_health_bar(score);
            let health_emoji = match score {
                90..=100 => "💚",
                70..=89 => "💛",
                50..=69 => "🧡",
                _ => "❤️",
            };

            println!("   {} Health Score: {}/100", health_emoji, score);
            println!("   {}", health_bar);
        }
    }

    // === Quick Summary ===
    println!("\n📋 Quick Summary");
    println!("────────────────────────────────────────────────────────────");

    let summary = memory_summary()?;
    let used = summary
        .available_memory
        .map(|available| summary.total_memory.saturating_sub(available));
    println!(
        "   Memory: {} used ({} / {})",
        shown_percent(summary.memory_percent),
        shown_bytes(used),
        format_bytes(summary.total_memory)
    );

    if summary.total_swap > 0 {
        println!(
            "   Swap:   {} used ({} / {})",
            shown_percent(summary.swap_percent),
            format_bytes(summary.used_swap),
            format_bytes(summary.total_swap)
        );
    }

    if summary.has_zram {
        if let Some(ratio) = summary.zram_ratio {
            println!("   ZRAM:   {:.2}x compression ratio", ratio);
        }
    }

    // === Tips ===
    println!("\n💡 Tips");
    println!("────────────────────────────────────────────────────────────");

    match pressure {
        Some(MemoryPressure::Low) => {
            println!("   ✅ Memory usage is healthy");
        }
        Some(MemoryPressure::Medium) => {
            println!("   ⚠️  Consider closing unused applications");
        }
        Some(MemoryPressure::High) => {
            println!("   🟠 High memory pressure detected");
            println!("   • Close memory-intensive applications");
            println!("   • Consider increasing swap or adding ZRAM");
        }
        Some(MemoryPressure::Critical) => {
            println!("   🔴 CRITICAL: System may become unstable!");
            println!("   • Close applications immediately");
            println!("   • Save your work");
            println!("   • Consider rebooting if system becomes unresponsive");
        }
        None => println!("   Memory pressure not read"),
    }

    #[cfg(target_os = "linux")]
    {
        println!();
        println!("   Linux commands:");
        println!("   • sudo sysctl vm.swappiness=10  # Reduce swap usage");
        println!("   • sudo sync; echo 3 | sudo tee /proc/sys/vm/drop_caches  # Clear caches");
    }

    #[cfg(windows)]
    {
        println!();
        println!("   Windows tips:");
        println!("   • Use Task Manager (Ctrl+Shift+Esc) to see details");
        println!("   • Adjust virtual memory in System Properties");
    }

    println!("\n📝 API Examples");
    println!("────────────────────────────────────────────────────────────");
    println!("   // Create monitor");
    println!("   let monitor = MemoryMonitor::new()?;");
    println!();
    println!("   // Get memory usage");
    println!("   let usage = monitor.memory.usage_percent();");
    println!("   let pressure = monitor.pressure();");
    println!();
    println!("   // Check swap");
    println!("   if monitor.swap.has_zram() {{");
    println!("       let ratio = monitor.swap.zram.as_ref().and_then(|z| z.compression_ratio);");
    println!("   }}");
    println!();
    println!("   // Get top processes");
    println!("   let top = monitor.top_processes(10);");

    Ok(())
}

fn create_health_bar(score: u32) -> String {
    let filled = (score as usize / 5).min(20);
    let empty = 20 - filled;

    let color = match score {
        90..=100 => "🟩",
        70..=89 => "🟨",
        50..=69 => "🟧",
        _ => "🟥",
    };

    format!(
        "   [{}{}] {}%",
        color.repeat(filled),
        "⬜".repeat(empty),
        score
    )
}

/// What an unread value prints as. Zero would be a reading, so it is never used instead.
fn shown_bytes(value: Option<u64>) -> String {
    value.map_or_else(|| "not read".to_string(), format_bytes)
}

fn shown_percent(value: Option<f64>) -> String {
    value.map_or_else(|| "not read".to_string(), |p| format!("{:.1}%", p))
}

fn shown_ratio(value: Option<f64>) -> String {
    value.map_or_else(|| "not read".to_string(), |r| format!("{:.2}x", r))
}

fn shown_count(value: Option<u64>) -> String {
    value.map_or_else(|| "not read".to_string(), |n| n.to_string())
}
