//! Fan control and monitoring example
//!
//! Demonstrates:
//! - Discovering system fans
//! - Reading fan speeds and RPM
//! - Displaying thermal zones
//! - Fan curve calculation

use ironmonlib::fan_control::{
    fan_summary, list_fans, list_thermal_zones, FanCurve, FanMonitor, FanProfile, FanType,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌀 Fan Control Monitor");
    println!("{}", "=".repeat(60));

    // Get fan summary
    match fan_summary() {
        Ok(summary) => {
            println!("\n📊 Fan Summary");
            println!("├─ Total fans: {}", summary.total_fans);
            println!("├─ Running fans: {}", summary.running_fans);
            println!("├─ Average speed: {:.1}%", summary.avg_speed_percent);

            if let Some(max_rpm) = summary.max_rpm {
                println!("├─ Max RPM: {}", max_rpm);
            }

            if !summary.stalled_fans.is_empty() {
                println!("├─ ⚠️  Stalled fans: {:?}", summary.stalled_fans);
            }

            if !summary.full_speed_fans.is_empty() {
                println!("└─ 🔥 Full speed fans: {:?}", summary.full_speed_fans);
            } else {
                println!("└─ All fans running normally");
            }
        }
        Err(e) => {
            println!("\n⚠️  Could not get fan summary: {}", e);
        }
    }

    // List all fans with details
    println!("\n🌀 Detected Fans");
    println!("{}", "-".repeat(60));

    match list_fans() {
        Ok(fans) => {
            if fans.is_empty() {
                println!("No fans detected (may require root access on Linux)");
            }

            for fan in &fans {
                let type_icon = match fan.fan_type {
                    FanType::Cpu => "🔵",
                    FanType::Gpu => "🟢",
                    FanType::Case => "⬜",
                    FanType::Psu => "🟡",
                    FanType::Chipset => "🟠",
                    FanType::System => "🔷",
                    FanType::Unknown => "⚪",
                };

                println!("\n{} {} ({})", type_icon, fan.name, fan.fan_type);

                // Speed bar
                let speed_bar = create_bar(fan.speed_percent, 20);
                println!("   Speed: {} {:.1}%", speed_bar, fan.speed_percent);

                // PWM value
                if let Some(pwm) = fan.pwm_value {
                    println!("   PWM: {}/255", pwm);
                }

                // RPM
                if let Some(rpm) = fan.rpm {
                    print!("   RPM: {}", rpm);
                    if let (Some(min), Some(max)) = (fan.rpm_min, fan.rpm_max) {
                        print!(" (range: {} - {})", min, max);
                    }
                    println!();

                    // Check for stalled fan
                    if rpm == 0 && fan.speed_percent > 10.0 {
                        println!("   ⚠️  Warning: Fan may be stalled!");
                    }
                }

                // Profile
                println!("   Profile: {}", fan.profile);
                if fan.available_profiles.len() > 1 {
                    println!(
                        "   Available: {:?}",
                        fan.available_profiles
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                    );
                }

                // Control mode
                println!("   Control: {:?}", fan.control_mode);
                println!(
                    "   Controllable: {}",
                    if fan.controllable { "✅" } else { "❌" }
                );

                // Linked temperature
                if let Some(temp) = fan.linked_temp_celsius {
                    println!("   Linked temp: {:.1}°C", temp);
                }

                // Efficiency (RPM per % speed)
                if let Some(efficiency) = fan.efficiency() {
                    println!("   Efficiency: {:.1} RPM/%", efficiency);
                }
            }
        }
        Err(e) => {
            println!("Could not enumerate fans: {}", e);
        }
    }

    // List thermal zones
    println!("\n🌡️  Thermal Zones");
    println!("{}", "-".repeat(60));

    match list_thermal_zones() {
        Ok(zones) => {
            if zones.is_empty() {
                println!("No thermal zones detected");
            }

            for zone in &zones {
                let temp_icon = if zone.temp_celsius > 80.0 {
                    "🔴"
                } else if zone.temp_celsius > 60.0 {
                    "🟠"
                } else if zone.temp_celsius > 40.0 {
                    "🟡"
                } else {
                    "🟢"
                };

                println!(
                    "\n{} {} ({}) - {:.1}°C",
                    temp_icon, zone.name, zone.zone_type, zone.temp_celsius
                );

                // Trip points
                if !zone.trip_points.is_empty() {
                    println!("   Trip points:");
                    for trip in &zone.trip_points {
                        let status = if zone.temp_celsius >= trip.temp_celsius {
                            "⚠️"
                        } else {
                            "✅"
                        };
                        println!(
                            "     {} {}: {:.1}°C",
                            status, trip.trip_type, trip.temp_celsius
                        );
                    }
                }

                // Policy
                if let Some(ref policy) = zone.policy {
                    println!("   Policy: {}", policy);
                }
            }
        }
        Err(e) => {
            println!("Could not enumerate thermal zones: {}", e);
        }
    }

    // Demonstrate fan curves
    println!("\n📈 Fan Curves");
    println!("{}", "-".repeat(60));

    let curves = [
        ("Silent", FanCurve::silent()),
        ("Quiet", FanCurve::quiet()),
        ("Performance", FanCurve::performance()),
    ];

    // Test temperatures
    let test_temps = [30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0];

    println!("\n   Temp   │ Silent │ Quiet  │ Performance");
    println!("   ───────┼────────┼────────┼─────────────");

    for temp in &test_temps {
        let silent_speed = curves[0].1.calculate_speed(*temp);
        let quiet_speed = curves[1].1.calculate_speed(*temp);
        let perf_speed = curves[2].1.calculate_speed(*temp);

        println!(
            "   {:>5.0}°C │ {:>5.0}% │ {:>5.0}% │ {:>5.0}%",
            temp, silent_speed, quiet_speed, perf_speed
        );
    }

    // Try creating a full monitor
    println!("\n🔧 Fan Monitor Status");
    println!("{}", "-".repeat(60));

    match FanMonitor::new() {
        Ok(monitor) => {
            println!("✅ Fan monitor initialized");
            println!("   Fans discovered: {}", monitor.fans().len());
            println!("   Thermal zones: {}", monitor.thermal_zones().len());

            // Show controllable fans
            let controllable: Vec<_> = monitor.fans().iter().filter(|f| f.controllable).collect();
            if !controllable.is_empty() {
                println!("\n   Controllable fans:");
                for fan in controllable {
                    println!("     • {} ({:?})", fan.name, fan.profile);
                }
            }
        }
        Err(e) => {
            println!("⚠️  Could not create fan monitor: {}", e);
        }
    }

    // Profile recommendations
    println!("\n💡 Profile Recommendations");
    println!("{}", "-".repeat(60));
    println!(
        "   {:12} - Zero RPM below 40°C, quiet operation",
        FanProfile::Silent
    );
    println!("   {:12} - Balanced noise and cooling", FanProfile::Quiet);
    println!(
        "   {:12} - Prioritize temperatures over noise",
        FanProfile::Cool
    );
    println!(
        "   {:12} - Maximum cooling at all times",
        FanProfile::Performance
    );
    println!("   {:12} - Direct PWM control (0-100%)", FanProfile::Manual);
    println!(
        "   {:12} - System-managed based on temperature",
        FanProfile::Auto
    );

    Ok(())
}

/// Create a visual progress bar
fn create_bar(percent: f32, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f32) as usize;
    let empty = width.saturating_sub(filled);

    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}
