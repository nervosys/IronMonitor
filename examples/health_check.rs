//! System Health Check Example
//!
//! Demonstrates comprehensive system health scoring like a system doctor.
//!
//! Run: cargo run --release --features nvidia --example health_check

use ironmonlib::{
    health_score, quick_health_check, HealthCheck, HealthStatus, HealthThresholds, SystemHealth,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║            🏥 System Health Check - Diagnostics Report             ║");
    println!("╠════════════════════════════════════════════════════════════════════╣");

    // Quick health check first
    println!("║                                                                    ║");
    let status = quick_health_check();
    let score = health_score();
    print_overall_status(score, &status);

    println!("║                                                                    ║");
    println!("╠════════════════════════════════════════════════════════════════════╣");
    println!("║  📋 Detailed Health Report                                         ║");
    println!("╠════════════════════════════════════════════════════════════════════╣");

    // Full health check with default thresholds
    let health = SystemHealth::check()?;

    // Group checks by category
    let mut cpu_checks: Vec<&HealthCheck> = Vec::new();
    let mut memory_checks: Vec<&HealthCheck> = Vec::new();
    let mut gpu_checks: Vec<&HealthCheck> = Vec::new();
    let mut storage_checks: Vec<&HealthCheck> = Vec::new();

    for check in &health.checks {
        match check.category.as_str() {
            "CPU" => cpu_checks.push(check),
            "Memory" => memory_checks.push(check),
            "GPU" => gpu_checks.push(check),
            "Storage" => storage_checks.push(check),
            _ => {}
        }
    }

    // Print CPU checks
    if !cpu_checks.is_empty() {
        println!("║                                                                    ║");
        println!("║  💻 CPU                                                            ║");
        for check in cpu_checks {
            print_check(check);
        }
    }

    // Print Memory checks
    if !memory_checks.is_empty() {
        println!("║                                                                    ║");
        println!("║  🧠 Memory                                                         ║");
        for check in memory_checks {
            print_check(check);
        }
    }

    // Print GPU checks
    if !gpu_checks.is_empty() {
        println!("║                                                                    ║");
        println!("║  🎮 GPU                                                            ║");
        for check in gpu_checks {
            print_check(check);
        }
    }

    // Print Storage checks
    if !storage_checks.is_empty() {
        println!("║                                                                    ║");
        println!("║  💾 Storage                                                        ║");
        for check in storage_checks {
            print_check(check);
        }
    }

    println!("║                                                                    ║");
    println!("╠════════════════════════════════════════════════════════════════════╣");
    println!("║  📊 Summary                                                        ║");
    println!("╠════════════════════════════════════════════════════════════════════╣");

    println!("║    ✅ Healthy: {:<53} ║", health.healthy_count);
    println!("║    ⚠️  Warning: {:<52} ║", health.warning_count);
    println!("║    🔴 Critical: {:<52} ║", health.critical_count);
    println!("║                                                                    ║");

    if health.has_critical() {
        println!("║  ⚠️  ATTENTION: Critical issues detected! Action required.        ║");
    } else if health.has_warnings() {
        println!("║  ℹ️  Some warnings detected. Consider reviewing the above.        ║");
    } else {
        println!("║  ✅ All systems operating within normal parameters.               ║");
    }

    println!("╚════════════════════════════════════════════════════════════════════╝");

    // Demo custom thresholds
    println!();
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║  🔧 Custom Thresholds Example                                      ║");
    println!("╠════════════════════════════════════════════════════════════════════╣");

    let strict_thresholds = HealthThresholds {
        cpu_warning: 70.0,
        cpu_critical: 85.0,
        memory_warning: 70.0,
        memory_critical: 85.0,
        gpu_temp_warning: 75,
        gpu_temp_critical: 85,
        ..Default::default()
    };

    println!("║  Using stricter thresholds:                                        ║");
    println!("║    CPU: Warning at 70%, Critical at 85%                            ║");
    println!("║    Memory: Warning at 70%, Critical at 85%                         ║");
    println!("║    GPU Temp: Warning at 75°C, Critical at 85°C                     ║");
    println!("║                                                                    ║");

    let strict_health = SystemHealth::check_with_thresholds(&strict_thresholds)?;
    let (strict_status_icon, strict_status_text) = match strict_health.status {
        HealthStatus::Healthy => ("✅", "HEALTHY"),
        HealthStatus::Good => ("🟢", "GOOD"),
        HealthStatus::Warning => ("⚠️ ", "WARNING"),
        HealthStatus::Critical => ("🔴", "CRITICAL"),
        HealthStatus::Unknown => ("❓", "UNKNOWN"),
    };

    println!(
        "║  {} Strict Score: {:>3}/100 - {}                                   ║",
        strict_status_icon, strict_health.score, strict_status_text
    );

    println!("╚════════════════════════════════════════════════════════════════════╝");

    // Print the summary method
    println!();
    println!("Health Summary: {}", health.summary());

    Ok(())
}

fn print_overall_status(score: Option<u8>, status: &HealthStatus) {
    let (icon, text, bar_char) = match status {
        HealthStatus::Healthy => ("✅", "EXCELLENT", '█'),
        HealthStatus::Good => ("🟢", "GOOD", '▓'),
        HealthStatus::Warning => ("⚠️ ", "WARNING", '▒'),
        HealthStatus::Critical => ("🔴", "CRITICAL", '░'),
        HealthStatus::Unknown => ("❓", "UNKNOWN", '?'),
    };

    // No score is not a score of zero — a failed check used to render an
    // empty bar reading 0/100, which is the worst possible health.
    let Some(score) = score else {
        println!(
            "║  {icon} Overall Health Score: [{:<20}] not run              ║",
            ""
        );
        println!("║     Status: {:<56} ║", text);
        return;
    };

    // Create score bar
    let bar_len = (score as f32 / 5.0) as usize;
    let bar: String = bar_char.to_string().repeat(bar_len);
    let empty: String = "░".repeat(20 - bar_len);

    println!(
        "║  {} Overall Health Score: [{:<20}] {:>3}/100              ║",
        icon,
        format!("{}{}", bar, empty),
        score
    );
    println!("║     Status: {:<56} ║", text);
}

fn print_check(check: &HealthCheck) {
    let icon = match check.status {
        HealthStatus::Healthy => "✅",
        HealthStatus::Good => "🟢",
        HealthStatus::Warning => "⚠️ ",
        HealthStatus::Critical => "🔴",
        HealthStatus::Unknown => "❓",
    };

    // Truncate message if too long
    let msg = if check.message.len() > 50 {
        format!("{}...", &check.message[..47])
    } else {
        check.message.clone()
    };

    println!("║      {} {:<61} ║", icon, msg);
}
