//! System Stats Example - Linux/BSD style monitoring
//!
//! Demonstrates load average, vmstat, uptime, and other system-wide statistics
//! similar to tools like htop, vmstat, and uptime.
//!
//! Run with: cargo run --example system_stats

use ironmonitor::SystemStats;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═══════════════════════════════════════════════════════════════════╗");
    println!("║           IronMonitor - System Stats (Linux/BSD Style)        ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");
    println!();

    // Get system stats
    let stats = SystemStats::new()?;

    // Hostname and kernel
    if let Some(ref hostname) = stats.hostname {
        println!("🖥  Hostname: {}", hostname);
    }
    if let Some(ref kernel) = stats.kernel_version {
        println!("🐧 Kernel: {}", kernel);
    }
    println!();

    // Uptime (like 'uptime' command)
    println!("━━━━━━━━━━━━━━━━━━━━ UPTIME ━━━━━━━━━━━━━━━━━━━━");
    println!("⏱  {}", stats.uptime_string());
    if let Some(idle) = stats.idle_seconds {
        println!("💤 Idle: {} seconds (sum across all CPUs)", idle);
    }
    println!();

    // Load Average (like 'uptime' / 'htop')
    println!("━━━━━━━━━━━━━━━━━━━ LOAD AVERAGE ━━━━━━━━━━━━━━━━━");
    if let Some(ref load) = stats.load_average {
        println!(
            "⚖  Load Average: {} (1min, 5min, 15min)",
            stats.load_string()
        );
        println!("   1 min:  {:.2}", load.one);
        println!("   5 min:  {:.2}", load.five);
        println!("   15 min: {:.2}", load.fifteen);
    } else {
        println!("   Load average not available on this platform");
    }
    println!();

    // Process info (like 'htop' header)
    println!("━━━━━━━━━━━━━━━━━━━━ TASKS ━━━━━━━━━━━━━━━━━━━━━━");
    // Each half is printed only where the platform reports it: Windows gives a
    // total and no runnable count, macOS gives neither.
    match stats.total_processes {
        Some(total) => println!("📊 Tasks: {total} total"),
        None => println!("📊 Tasks: total not reported on this platform"),
    }
    match stats.running_processes {
        Some(running) => println!("📊       {running} running"),
        None => println!("📊       runnable count not reported on this platform"),
    }
    match stats.num_cpus {
        Some(n) => println!("💻 CPUs: {n}"),
        None => println!("💻 CPUs: not reported on this platform"),
    }
    println!();

    // CPU Time breakdown (like 'top' or 'htop')
    println!("━━━━━━━━━━━━━━━━━━ CPU TIME (%) ━━━━━━━━━━━━━━━━━━");
    if let Some(ref cpu_time) = stats.cpu_time {
        let total = cpu_time.total() as f64;
        if total > 0.0 {
            println!(
                "   user (us):    {:>6.1}%",
                (cpu_time.user as f64 / total) * 100.0
            );
            println!(
                "   system (sy):  {:>6.1}%",
                (cpu_time.system as f64 / total) * 100.0
            );
            println!(
                "   nice (ni):    {:>6.1}%",
                (cpu_time.nice as f64 / total) * 100.0
            );
            println!(
                "   idle (id):    {:>6.1}%",
                (cpu_time.idle as f64 / total) * 100.0
            );
            println!(
                "   iowait (wa):  {:>6.1}%",
                (cpu_time.iowait as f64 / total) * 100.0
            );
            println!(
                "   irq (hi):     {:>6.1}%",
                (cpu_time.irq as f64 / total) * 100.0
            );
            println!(
                "   softirq (si): {:>6.1}%",
                (cpu_time.softirq as f64 / total) * 100.0
            );
            if cpu_time.steal > 0 {
                println!(
                    "   steal (st):   {:>6.1}%",
                    (cpu_time.steal as f64 / total) * 100.0
                );
            }
        }
    } else {
        println!("   CPU time breakdown not available");
    }
    println!();

    // VMstat-style info
    println!("━━━━━━━━━━━━━━━━━━━ VMSTAT ━━━━━━━━━━━━━━━━━━━━━━");
    if let Some(ref vm) = stats.vm_stats {
        // Counters are totals since boot; `--` marks a counter that was not read.
        println!("🔀 Context Switches: {}", opt(vm.context_switches));
        println!("⚡ Interrupts:       {}", opt(vm.interrupts));
        println!("📥 Pages In:         {}", opt(vm.pages_in));
        println!("📤 Pages Out:        {}", opt(vm.pages_out));
        println!("💾 Swap In:          {}", opt(vm.swap_in));
        println!("💾 Swap Out:         {}", opt(vm.swap_out));
        println!("🔄 Procs Running:    {}", opt(vm.processes_running));
        println!("⏸  Procs Blocked:    {}", opt(vm.processes_blocked));
        println!(
            "🆕 Procs Created:    {} (forks since boot)",
            opt(vm.processes_created)
        );
    } else {
        println!("   VMstat info not available");
    }
    println!();

    // Live monitoring - show context switch rate
    println!("━━━━━━━━━━━━━━━ LIVE MONITORING (5s) ━━━━━━━━━━━━━");
    println!("Monitoring context switches and interrupts per second...\n");

    let mut prev_ctx = stats.vm_stats.as_ref().and_then(|v| v.context_switches);
    let mut prev_int = stats.vm_stats.as_ref().and_then(|v| v.interrupts);

    for i in 1..=5 {
        thread::sleep(Duration::from_secs(1));

        if let Ok(new_stats) = SystemStats::new() {
            if let Some(ref vm) = new_stats.vm_stats {
                // A rate needs this sample and the previous one; otherwise it is unknown.
                let ctx_rate = vm
                    .context_switches
                    .zip(prev_ctx)
                    .map(|(now, prev)| now.saturating_sub(prev));
                let int_rate = vm
                    .interrupts
                    .zip(prev_int)
                    .map(|(now, prev)| now.saturating_sub(prev));

                println!(
                    "[{}/5] ctx/s: {:>8}  |  int/s: {:>8}  |  running: {}  |  blocked: {}",
                    i,
                    opt(ctx_rate),
                    opt(int_rate),
                    opt(vm.processes_running),
                    opt(vm.processes_blocked)
                );

                prev_ctx = vm.context_switches;
                prev_int = vm.interrupts;
            }

            // Show load if available
            if let Some(ref load) = new_stats.load_average {
                println!(
                    "      load: {:.2} {:.2} {:.2}",
                    load.one, load.five, load.fifteen
                );
            }
        }
    }

    println!("\n✅ System stats demo complete!");
    Ok(())
}

/// A counter for display, or `--` where it was not read.
fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "--".to_string(), |x| x.to_string())
}
