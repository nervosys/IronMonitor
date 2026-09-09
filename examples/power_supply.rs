//! Power Supply and Battery Monitoring Example
//!
//! Demonstrates battery and AC adapter monitoring similar to TLP/powertop.
//!
//! Run: cargo run --release --example power_supply

use ironmonlib::{
    BatteryHealth, ChargingStatus, PowerSupplyInfo, PowerSupplyMonitor, PowerSupplyType,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║           ⚡ Power Supply Monitor - Battery & AC Status            ║");
    println!("╠════════════════════════════════════════════════════════════════════╣");

    // Create monitor
    let monitor = PowerSupplyMonitor::new()?;

    // Print summary using helper functions
    let summary = ironmonlib::power_summary()?;
    println!("║                                                                    ║");
    println!(
        "║  Quick Summary: AC={}, Battery={}                              ║",
        match summary.on_ac_power {
            Some(true) => "✅",
            Some(false) => "❌",
            None => "?",
        },
        match summary.on_battery {
            Some(true) => "✅",
            Some(false) => "❌",
            None => "?",
        }
    );

    // "Not on AC" is a reading only when something said so. It used to be
    // printed whenever the question could not be answered.
    match ironmonlib::is_on_ac_power() {
        Some(true) => {
            println!("║  Status: 🔌 Running on AC Power                                   ║")
        }
        Some(false) => {
            println!("║  Status: 🔋 Running on Battery                                    ║")
        }
        None => println!("║  Status: ·  Mains state not read                                  ║"),
    }

    if let Some(pct) = ironmonlib::battery_percent() {
        let bar_len = (pct as f32 / 5.0) as usize;
        let bar: String = "█".repeat(bar_len);
        let empty: String = "░".repeat(20 - bar_len);
        println!(
            "║  Battery Level: [{}{}] {:>3}%                             ║",
            bar, empty, pct
        );
    }

    println!("╠════════════════════════════════════════════════════════════════════╣");
    println!("║  Detailed Power Supplies                                           ║");
    println!("╠════════════════════════════════════════════════════════════════════╣");

    for supply in monitor.supplies() {
        print_supply_info(supply);
    }

    println!("╚════════════════════════════════════════════════════════════════════╝");
    Ok(())
}

fn print_supply_info(supply: &PowerSupplyInfo) {
    let type_icon = match supply.supply_type {
        PowerSupplyType::Battery => "🔋",
        PowerSupplyType::Mains => "🔌",
        PowerSupplyType::Usb | PowerSupplyType::UsbPd => "⚡",
        PowerSupplyType::Ups => "🏢",
        _ => "⚙️",
    };

    println!("║                                                                    ║");
    println!(
        "║  {} {:<62} ║",
        type_icon,
        format!("{} ({:?})", supply.name, supply.supply_type)
    );
    println!(
        "║    Online: {:<57} ║",
        match supply.online {
            Some(true) => "✅ Yes",
            Some(false) => "❌ No",
            None => "·  not read",
        }
    );

    if supply.supply_type == PowerSupplyType::Battery {
        // Status
        let status_str = match supply.status {
            ChargingStatus::Charging => "⚡ Charging",
            ChargingStatus::Discharging => "🔋 Discharging",
            ChargingStatus::Full => "✅ Full",
            ChargingStatus::NotCharging => "⏸️ Not Charging",
            _ => "❓ Unknown",
        };
        println!("║    Status: {:<57} ║", status_str);

        // Capacity
        if let Some(cap) = supply.capacity_percent {
            let bar_len = (cap as f32 / 5.0) as usize;
            let bar: String = "█".repeat(bar_len);
            let empty: String = "░".repeat(20 - bar_len);
            println!(
                "║    Capacity: [{}{}] {:>3}%                            ║",
                bar, empty, cap
            );
        }

        // Design capacity vs current (using methods)
        if let Some(design) = supply.energy_full_design_wh() {
            if let Some(current) = supply.energy_full_wh() {
                let wear = 100.0 - (current / design * 100.0);
                println!(
                    "║    Design Capacity: {:.2} Wh                                       ║",
                    design
                );
                println!(
                    "║    Current Capacity: {:.2} Wh ({:.1}% wear)                         ║",
                    current, wear
                );
            }
        }

        // Voltage (using method)
        if let Some(voltage) = supply.voltage_v() {
            println!(
                "║    Voltage: {:.2} V                                               ║",
                voltage
            );
        }

        // Current (using method)
        if let Some(current) = supply.current_a() {
            println!(
                "║    Current: {:.3} A                                              ║",
                current
            );
        }

        // Power draw (using method)
        if let Some(power) = supply.power_w() {
            println!(
                "║    Power Draw: {:.2} W                                           ║",
                power
            );
        }

        // Time estimates
        if let Some(mins) = supply.time_to_empty_min {
            let hours = mins / 60;
            let minutes = mins % 60;
            println!(
                "║    Time to Empty: {}h {}m                                         ║",
                hours, minutes
            );
        }
        if let Some(mins) = supply.time_to_full_min {
            let hours = mins / 60;
            let minutes = mins % 60;
            println!(
                "║    Time to Full: {}h {}m                                          ║",
                hours, minutes
            );
        }

        // Temperature (using method)
        if let Some(temp) = supply.temperature_celsius() {
            println!(
                "║    Temperature: {:.1}°C                                          ║",
                temp
            );
        }

        // Cycle count
        if let Some(cycles) = supply.cycle_count {
            println!(
                "║    Charge Cycles: {}                                           ║",
                cycles
            );
        }

        // Health
        let health_str = match supply.health {
            BatteryHealth::Good => "✅ Good",
            BatteryHealth::Overheat => "🔥 Overheating!",
            BatteryHealth::Dead => "💀 Dead",
            BatteryHealth::OverVoltage => "⚠️ Over Voltage",
            BatteryHealth::UnspecifiedFailure => "❌ Failure",
            BatteryHealth::Cold => "🥶 Cold",
            BatteryHealth::WatchdogTimerExpire => "⏰ Watchdog Expired",
            BatteryHealth::SafetyTimerExpire => "⏰ Safety Timer Expired",
            BatteryHealth::Unknown => "❓ Unknown",
        };
        println!("║    Health: {:<57} ║", health_str);

        // Technology
        if let Some(tech) = &supply.technology {
            println!("║    Technology: {:<53} ║", tech);
        }

        // Manufacturer
        if let Some(mfr) = &supply.manufacturer {
            println!("║    Manufacturer: {:<51} ║", mfr);
        }
    }
}
