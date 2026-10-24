//! Linux temperature monitoring

use crate::core::temperature::{TemperatureSensor, TemperatureStats};
use crate::error::Result;
use crate::platform::common::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const THERMAL_ZONE_PATH: &str = "/sys/class/thermal";

/// Read temperature statistics
pub fn read_temperature_stats() -> Result<TemperatureStats> {
    let mut stats = TemperatureStats::new()?;

    // Read thermal zones
    stats.sensors = read_thermal_zones()?;

    Ok(stats)
}

fn read_thermal_zones() -> Result<HashMap<String, TemperatureSensor>> {
    let mut sensors = HashMap::new();

    if !Path::new(THERMAL_ZONE_PATH).exists() {
        return Ok(sensors);
    }

    for entry in fs::read_dir(THERMAL_ZONE_PATH)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let filename = entry.file_name();
        let name_str = filename.to_string_lossy();

        if !name_str.starts_with("thermal_zone") {
            continue;
        }

        if let Ok(sensor) = read_thermal_zone(&path) {
            // Get zone name/type
            let zone_type =
                read_file_string(path.join("type")).unwrap_or_else(|_| name_str.to_string());

            sensors.insert(zone_type, sensor);
        }
    }

    Ok(sensors)
}

fn read_thermal_zone(zone_path: &Path) -> Result<TemperatureSensor> {
    let temp_path = zone_path.join("temp");

    // Read temperature in millidegrees Celsius
    let temp_millidegrees = read_file_u32(&temp_path)?;
    let temp = temp_millidegrees as f32 / 1000.0;

    // Trip points are identified by their `_type` file, not by position. This
    // used to read trip 0 as the max and trip 1 as the critical point, and the
    // order is the driver's: trip 0 is "critical" on some boards.
    let mut trips: Vec<(String, u32)> = Vec::new();
    for i in 0..32 {
        let kind = match read_file_string(zone_path.join(format!("trip_point_{i}_type"))) {
            Ok(kind) => kind,
            Err(_) => break,
        };
        if let Ok(temp) = read_file_u32(zone_path.join(format!("trip_point_{i}_temp"))) {
            trips.push((kind, temp));
        }
    }
    let borrowed: Vec<(&str, u32)> = trips.iter().map(|(k, t)| (k.as_str(), *t)).collect();
    let (max, crit) = max_and_crit(&borrowed);

    Ok(TemperatureSensor {
        online: true,
        temp,
        max,
        crit,
    })
}

/// The max and critical temperatures (degrees C) from typed trip points.
///
/// "hot" is the max; "passive" stands in for it only when there is no "hot"
/// point. "critical" is the crit. A zone with no such typed point has neither,
/// rather than whatever sat at position 0 or 1.
fn max_and_crit(trips: &[(&str, u32)]) -> (Option<f32>, Option<f32>) {
    let celsius = |millideg: u32| millideg as f32 / 1000.0;
    let find = |wanted: &str| {
        trips
            .iter()
            .find(|(kind, _)| kind.trim() == wanted)
            .map(|(_, t)| celsius(*t))
    };
    let max = find("hot").or_else(|| find("passive"));
    let crit = find("critical");
    (max, crit)
}

#[cfg(test)]
mod tests {
    use super::max_and_crit;

    #[test]
    fn trip_points_are_matched_by_type_not_position() {
        // Critical first, as on boards whose trip 0 is the critical point.
        let trips = [("critical", 105_000), ("hot", 95_000), ("passive", 85_000)];
        assert_eq!(max_and_crit(&trips), (Some(95.0), Some(105.0)));
    }

    #[test]
    fn passive_stands_in_for_hot_only_when_hot_is_absent() {
        let trips = [("passive", 85_000), ("critical", 105_000)];
        assert_eq!(max_and_crit(&trips), (Some(85.0), Some(105.0)));
    }

    #[test]
    fn a_zone_with_no_typed_trip_point_has_no_limits() {
        let trips = [("active", 60_000)];
        assert_eq!(max_and_crit(&trips), (None, None));
        assert_eq!(max_and_crit(&[]), (None, None));
    }
}
