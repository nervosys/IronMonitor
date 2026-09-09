// Boot Configuration Example for IronMonitor
//
// Demonstrates boot configuration and startup item monitoring.

use ironmonlib::{
    boot_summary, format_uptime, BootMonitor, BootType, StartupItemStatus, StartupItemType,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Boot Configuration Monitor");
    println!("{}", "═".repeat(60));

    let monitor = BootMonitor::new()?;

    // === Boot Type ===
    println!("\n💾 Boot Configuration");
    println!("────────────────────────────────────────────────────────────");

    let boot_emoji = match monitor.boot_info.boot_type {
        BootType::Legacy => "📟",
        BootType::Uefi => "🔌",
        BootType::SecureBoot => "🔒",
        BootType::Unknown => "❓",
    };

    println!(
        "   {} Boot Type:   {}",
        boot_emoji, monitor.boot_info.boot_type
    );
    println!(
        "   🛡️  Secure Boot: {}",
        match monitor.is_secure_boot() {
            Some(true) => "✅ Enabled",
            Some(false) => "❌ Disabled",
            None => "— not read",
        }
    );

    if let Some(ref device) = monitor.boot_info.boot_device {
        println!("   💿 Boot Device: {}", device);
    }

    if let Some(ref bootloader) = monitor.boot_info.bootloader {
        println!("   🔧 Bootloader:  {}", bootloader);
    }

    if let Some(ref efi) = monitor.boot_info.efi_partition {
        println!("   📁 EFI Partition: {}", efi);
    }

    // === Boot Time ===
    println!("\n⏱️  Boot Time Analysis");
    println!("────────────────────────────────────────────────────────────");

    let bt = &monitor.boot_time;

    if bt.total.as_secs() > 0 {
        println!("   Total Boot Time: {}", format_uptime(bt.total));
    }

    if let Some(firmware) = bt.firmware {
        println!("   ├─ Firmware:   {}", format_uptime(firmware));
    }
    if let Some(bootloader) = bt.bootloader {
        println!("   ├─ Bootloader: {}", format_uptime(bootloader));
    }
    if let Some(kernel) = bt.kernel {
        println!("   ├─ Kernel:     {}", format_uptime(kernel));
    }
    if let Some(userspace) = bt.userspace {
        println!("   └─ Userspace:  {}", format_uptime(userspace));
    }

    println!();
    println!("   🕐 System Uptime: {}", format_uptime(bt.uptime));

    if let Some(boot_time) = bt.boot_timestamp {
        println!(
            "   📅 Last Boot:     {}",
            boot_time.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }

    // === Kernel Parameters (Linux) ===
    #[cfg(target_os = "linux")]
    {
        let kp = &monitor.kernel_params;
        if !kp.cmdline.is_empty() {
            println!("\n🔧 Kernel Parameters");
            println!("────────────────────────────────────────────────────────────");

            if let Some(ref root) = kp.root {
                println!("   Root: {}", root);
            }

            println!("   Quiet: {}", if kp.quiet { "Yes" } else { "No" });
            println!("   Splash: {}", if kp.splash { "Yes" } else { "No" });

            println!("\n   Full command line:");
            // Word wrap long command line
            let max_width = 55;
            let mut current_line = String::from("   ");
            for word in kp.cmdline.split_whitespace() {
                if current_line.len() + word.len() + 1 > max_width {
                    println!("{}", current_line);
                    current_line = format!("   {}", word);
                } else {
                    if current_line.len() > 3 {
                        current_line.push(' ');
                    }
                    current_line.push_str(word);
                }
            }
            if current_line.len() > 3 {
                println!("{}", current_line);
            }
        }
    }

    // === Startup Items Summary ===
    println!("\n📋 Startup Items Summary");
    println!("────────────────────────────────────────────────────────────");

    let enabled = monitor.enabled_count();
    let disabled = monitor.disabled_count();
    let total = monitor.startup_items.len();

    println!("   Total Items:   {}", total);
    println!("   ✅ Enabled:    {}", enabled);
    println!("   ❌ Disabled:   {}", disabled);

    // Group by type
    let service_count = monitor.items_by_type(StartupItemType::Service).len();
    let app_count = monitor.items_by_type(StartupItemType::Application).len();
    let task_count = monitor.items_by_type(StartupItemType::ScheduledTask).len();
    let registry_count = monitor.items_by_type(StartupItemType::Registry).len();

    if service_count > 0 || app_count > 0 || task_count > 0 {
        println!();
        println!("   By Type:");
        if service_count > 0 {
            println!("   ⚙️  Services:        {}", service_count);
        }
        if app_count > 0 {
            println!("   🖥️  Applications:    {}", app_count);
        }
        if task_count > 0 {
            println!("   📅 Scheduled Tasks: {}", task_count);
        }
        if registry_count > 0 {
            println!("   📝 Registry:        {}", registry_count);
        }
    }

    // === Startup Items List ===
    if !monitor.startup_items.is_empty() {
        println!("\n📝 Startup Items (first 20)");
        println!("────────────────────────────────────────────────────────────");
        println!("   {:30} {:12} {:12}", "NAME", "TYPE", "STATUS");
        println!("   {}", "─".repeat(56));

        for item in monitor.startup_items.iter().take(20) {
            let status_emoji = match item.status {
                StartupItemStatus::Enabled => "✅",
                StartupItemStatus::Disabled => "❌",
                StartupItemStatus::Unknown => "❓",
            };

            let type_str = match item.item_type {
                StartupItemType::Service => "service",
                StartupItemType::Application => "app",
                StartupItemType::ScheduledTask => "task",
                StartupItemType::Registry => "registry",
                StartupItemType::KernelModule => "module",
                StartupItemType::Driver => "driver",
                StartupItemType::Unknown => "unknown",
            };

            let name = if item.name.len() > 28 {
                format!("{}...", &item.name[..25])
            } else {
                item.name.clone()
            };

            println!(
                "   {:30} {:12} {} {:10}",
                name,
                type_str,
                status_emoji,
                match item.status {
                    StartupItemStatus::Enabled => "enabled",
                    StartupItemStatus::Disabled => "disabled",
                    StartupItemStatus::Unknown => "unknown",
                }
            );
        }

        if total > 20 {
            println!("   ... and {} more items", total - 20);
        }
    }

    // === Boot Recommendations ===
    println!("\n💡 Recommendations");
    println!("────────────────────────────────────────────────────────────");

    // No recommendation without a reading. Advising someone to enable Secure
    // Boot because the flag could not be read is the whole defect in one line.
    match monitor.is_secure_boot() {
        Some(false) => {
            println!("   ⚠️  Secure Boot is disabled");
            println!("      Consider enabling for better security");
        }
        Some(true) => println!("   ✅ Secure Boot is enabled - good!"),
        None => println!("   ·  Secure Boot state was not read, so no advice here"),
    }

    if enabled > 50 {
        println!("   ⚠️  {} startup items enabled", enabled);
        println!("      Consider disabling unused items for faster boot");
    }

    if monitor.boot_time.total.as_secs() > 60 {
        println!("   ⚠️  Boot time is over 60 seconds");
        println!("      Review startup items to improve boot speed");
    }

    // === Quick Summary ===
    println!("\n📊 Quick Summary");
    println!("────────────────────────────────────────────────────────────");

    let summary = boot_summary()?;
    println!("   Boot Type:       {}", summary.boot_type);
    println!(
        "   Secure Boot:     {}",
        match summary.secure_boot {
            Some(true) => "Yes",
            Some(false) => "No",
            None => "not read",
        }
    );
    if summary.boot_time_secs > 0.0 {
        println!("   Boot Time:       {:.1}s", summary.boot_time_secs);
    }
    println!(
        "   Uptime:          {}",
        format_uptime(std::time::Duration::from_secs_f64(summary.uptime_secs))
    );
    println!(
        "   Startup Items:   {} enabled, {} disabled",
        summary.enabled_startup_items, summary.disabled_startup_items
    );

    // === Tips ===
    println!("\n💡 Tips");
    println!("────────────────────────────────────────────────────────────");

    #[cfg(target_os = "linux")]
    {
        println!("   Linux commands:");
        println!("   • systemd-analyze        # Show boot time");
        println!("   • systemd-analyze blame  # Show slow services");
        println!("   • systemctl list-unit-files --state=enabled");
    }

    #[cfg(windows)]
    {
        println!("   Windows tools:");
        println!("   • Task Manager → Startup tab");
        println!("   • msconfig → Startup services");
        println!("   • bcdedit → Boot configuration");
    }

    #[cfg(target_os = "macos")]
    {
        println!("   macOS commands:");
        println!("   • launchctl list         # Show launch daemons");
        println!("   • System Preferences → Users → Login Items");
    }

    println!("\n📝 API Examples");
    println!("────────────────────────────────────────────────────────────");
    println!("   // Create boot monitor");
    println!("   let monitor = BootMonitor::new()?;");
    println!();
    println!("   // Check boot type");
    println!("   if monitor.is_uefi() {{");
    println!("       println!(\"Running in UEFI mode\");");
    println!("   }}");
    println!();
    println!("   // Check secure boot");
    println!("   if monitor.is_secure_boot() {{");
    println!("       println!(\"Secure Boot enabled\");");
    println!("   }}");
    println!();
    println!("   // Get startup items");
    println!("   for item in &monitor.startup_items {{");
    println!("       println!(\"{{}}: {{}}\", item.name, item.status);");
    println!("   }}");

    Ok(())
}
