//! Linux power monitoring

use crate::core::power::{PowerRail, PowerStats, TotalPower};
use crate::error::Result;
use crate::platform::common::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const INA3221_PATH: &str = "/sys/bus/i2c/drivers/ina3221x";

/// Read power statistics
pub fn read_power_stats() -> Result<PowerStats> {
    let mut stats = PowerStats::empty();

    // Try to read INA3221 sensors (Jetson)
    if let Ok(rails) = read_ina3221_rails() {
        stats.rails = rails;

        // Calculate total. `Option` sum: one online rail with no reading
        // makes the total unknown rather than silently smaller. `.sum()` over
        // an iterator of `Option` does this for free.
        let total_power: Option<u32> = stats
            .rails
            .values()
            .filter(|r| r.online)
            .map(|r| r.power)
            .sum();

        stats.total = TotalPower {
            power: total_power,
            average: total_power, // Note: Use PowerAverageTracker for time-series averaging
        };
    }

    Ok(stats)
}

fn read_ina3221_rails() -> Result<HashMap<String, PowerRail>> {
    let mut rails = HashMap::new();

    if !Path::new(INA3221_PATH).exists() {
        return Ok(rails);
    }

    // Read INA3221 devices
    for entry in fs::read_dir(INA3221_PATH)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        // Read hwmon path
        let hwmon_path = path.join("hwmon");
        if !hwmon_path.exists() {
            continue;
        }

        for hwmon_entry in fs::read_dir(&hwmon_path)? {
            let hwmon_entry = hwmon_entry?;
            let hwmon_device = hwmon_entry.path();

            // Read all channels
            for i in 1..=3 {
                if let Ok(rail) = read_ina3221_channel(&hwmon_device, i) {
                    let label = read_file_string(hwmon_device.join(format!("in{}_label", i)))
                        .unwrap_or_else(|_| format!("Rail {}", i));

                    rails.insert(label, rail);
                }
            }
        }
    }

    Ok(rails)
}

fn read_ina3221_channel(hwmon_path: &Path, channel: u32) -> Result<PowerRail> {
    let voltage_path = hwmon_path.join(format!("in{}_input", channel));
    let current_path = hwmon_path.join(format!("curr{}_input", channel));

    let voltage = read_file_u32(&voltage_path).ok();
    let current = read_file_u32(&current_path).ok();

    // Calculate power (V * I). Derived, so it needs both: with `unwrap_or(0)`
    // on either input this produced a rail drawing exactly 0 W, which is what a
    // rail that is switched off also looks like.
    let power = match (voltage, current) {
        (Some(v), Some(i)) => Some((v as u64 * i as u64 / 1000) as u32), // mV * mA / 1000 = mW
        _ => None,
    };

    // Read limits if available
    let warn = read_file_u32(hwmon_path.join(format!("curr{}_crit", channel))).ok();
    let crit = read_file_u32(hwmon_path.join(format!("curr{}_max", channel))).ok();

    Ok(PowerRail {
        online: true,
        sensor_type: "INA3221".to_string(),
        voltage,
        current,
        power,
        // Seeded from the instantaneous reading, and `None` where there is
        // none; `PowerAverageTracker` replaces it once it has samples.
        average: power,
        warn,
        crit,
    })
}
