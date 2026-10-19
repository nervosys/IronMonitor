// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 nervosys

//! IPMI/BMC Sensor Interface

use serde::{Deserialize, Serialize};
use std::process::Command;

/// IPMI sensor types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpmiSensorType {
    Temperature,
    Voltage,
    Fan,
    Power,
    Current,
    Other,
}

/// An IPMI sensor reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpmiSensor {
    pub name: String,
    pub sensor_type: IpmiSensorType,
    /// The reading, `None` where ipmitool printed `na` for it.
    pub value: Option<f64>,
    pub unit: String,
    pub status: super::SensorStatus,
    pub lower_critical: Option<f64>,
    pub upper_critical: Option<f64>,
}

/// BMC (Baseboard Management Controller) information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmcInfo {
    pub firmware_version: String,
    pub manufacturer: String,
    pub ip_address: Option<String>,
    pub mac_address: Option<String>,
}

/// Power reading from IPMI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerReading {
    pub current_watts: f64,
    pub minimum_watts: Option<f64>,
    pub maximum_watts: Option<f64>,
    pub average_watts: Option<f64>,
}

/// System Event Log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelEntry {
    pub id: u32,
    pub timestamp: String,
    pub sensor: String,
    pub event: String,
    pub severity: SelSeverity,
}

/// SEL entry severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelSeverity {
    Info,
    Warning,
    Critical,
}

/// IPMI controller for reading sensors via ipmitool
pub struct IpmiController {
    host: Option<String>,
    user: Option<String>,
    pass: Option<String>,
}

impl Default for IpmiController {
    fn default() -> Self {
        Self::new()
    }
}

impl IpmiController {
    pub fn new() -> Self {
        Self {
            host: None,
            user: None,
            pass: None,
        }
    }

    pub fn remote(host: String, user: String, pass: String) -> Self {
        Self {
            host: Some(host),
            user: Some(user),
            pass: Some(pass),
        }
    }

    /// `ipmitool` with the remote target, if any.
    ///
    /// The password goes in `IPMI_PASSWORD` for `-E`, not on the command line.
    /// It was passed as `-P <password>`, where every local user can read it
    /// from the process list for as long as the command runs.
    fn ipmitool(&self) -> Command {
        let mut cmd = Command::new("ipmitool");
        if let (Some(h), Some(u), Some(p)) = (&self.host, &self.user, &self.pass) {
            cmd.args(["-H", h, "-U", u, "-E"]);
            cmd.env("IPMI_PASSWORD", p);
        }
        cmd
    }

    /// Run `ipmitool` with `args`, failing with its own message if it fails.
    fn run(&self, args: &[&str]) -> Result<String, super::DatacenterError> {
        let output = self.ipmitool().args(args).output().map_err(|e| {
            super::DatacenterError::IpmiError(format!("Failed to run ipmitool: {}", e))
        })?;
        if !output.status.success() {
            return Err(super::DatacenterError::IpmiError(format!(
                "ipmitool {} exited {}: {}",
                args.join(" "),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn read_sensors(&self) -> Result<Vec<IpmiSensor>, super::DatacenterError> {
        match self.run(&["sensor", "list"]) {
            Ok(stdout) => Ok(parse_sensor_list(&stdout)),
            Err(_) => self.read_sensors_sysfs(),
        }
    }

    fn read_sensors_sysfs(&self) -> Result<Vec<IpmiSensor>, super::DatacenterError> {
        // Populated only on Linux; `mut` is unused on other platforms.
        #[allow(unused_mut)]
        let mut sensors = Vec::new();
        #[cfg(target_os = "linux")]
        {
            use std::fs;
            use std::path::Path;
            let hwmon = Path::new("/sys/class/hwmon");
            if let Ok(entries) = fs::read_dir(hwmon) {
                for entry in entries.flatten() {
                    let name_path = entry.path().join("name");
                    if let Ok(name) = fs::read_to_string(&name_path) {
                        if name.trim().contains("ipmi") {
                            // Read temp/fan/voltage inputs
                            for i in 1..=16 {
                                let temp_path = entry.path().join(format!("temp{}_input", i));
                                if let Ok(val) = fs::read_to_string(&temp_path) {
                                    if let Ok(v) = val.trim().parse::<f64>() {
                                        let label = fs::read_to_string(
                                            entry.path().join(format!("temp{}_label", i)),
                                        )
                                        .unwrap_or_else(|_| format!("Temp {}", i));
                                        sensors.push(IpmiSensor {
                                            name: label.trim().to_string(),
                                            sensor_type: IpmiSensorType::Temperature,
                                            value: Some(v / 1000.0),
                                            unit: "degrees C".into(),
                                            // hwmon's `_input` carries no
                                            // threshold state. This said Ok
                                            // for every sensor, unread.
                                            status: super::SensorStatus::Unknown,
                                            lower_critical: None,
                                            upper_critical: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(sensors)
    }

    pub fn bmc_info(&self) -> Result<BmcInfo, super::DatacenterError> {
        let stdout = self.run(&["bmc", "info"])?;
        let mut firmware = String::new();
        let mut manufacturer = String::new();

        for line in stdout.lines() {
            if line.contains("Firmware Revision") {
                firmware = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.contains("Manufacturer Name") {
                manufacturer = line.split(':').nth(1).unwrap_or("").trim().to_string();
            }
        }

        Ok(BmcInfo {
            firmware_version: firmware,
            manufacturer,
            ip_address: None,
            mac_address: None,
        })
    }

    pub fn power_reading(&self) -> Result<PowerReading, super::DatacenterError> {
        parse_power_reading(&self.run(&["dcmi", "power", "reading"])?)
    }
}

/// `ipmitool sensor list`: `name | value | unit | status | lnr | lcr | lnc |
/// unc | ucr | unr`.
fn parse_sensor_list(stdout: &str) -> Vec<IpmiSensor> {
    let mut sensors = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() < 4 {
            continue;
        }

        let name = parts[0].to_string();
        // `na` for a sensor with no reading -- an absent drive bay, a fan
        // header with nothing on it. This made it 0.0: a fan at 0 RPM and a
        // temperature of 0 degrees, beside thresholds it was nowhere near.
        let value = parts[1].parse::<f64>().ok();
        let unit = parts[2].to_string();
        let status_str = parts[3];

        let sensor_type = match unit.to_lowercase().as_str() {
            u if u.contains("degrees") || u.contains("celsius") => IpmiSensorType::Temperature,
            u if u.contains("volts") => IpmiSensorType::Voltage,
            u if u.contains("rpm") => IpmiSensorType::Fan,
            u if u.contains("watts") => IpmiSensorType::Power,
            u if u.contains("amps") => IpmiSensorType::Current,
            _ => IpmiSensorType::Other,
        };

        let status = match status_str.to_lowercase().as_str() {
            "ok" => super::SensorStatus::Ok,
            s if s.contains("warn") => super::SensorStatus::Warning,
            s if s.contains("crit") => super::SensorStatus::Critical,
            s if s.contains("nr") => super::SensorStatus::NonRecoverable,
            _ => super::SensorStatus::Unknown,
        };

        let lower_critical = parts.get(5).and_then(|s| s.parse().ok());
        let upper_critical = parts.get(8).and_then(|s| s.parse().ok());

        sensors.push(IpmiSensor {
            name,
            sensor_type,
            value,
            unit,
            status,
            lower_critical,
            upper_critical,
        });
    }
    sensors
}

/// `ipmitool dcmi power reading`. An output with no instantaneous reading is
/// an error: it was 0 W, a server drawing no power.
fn parse_power_reading(stdout: &str) -> Result<PowerReading, super::DatacenterError> {
    let mut current = None;
    let mut minimum = None;
    let mut maximum = None;
    let mut average = None;

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 2 {
            let val = parts[1]
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok());
            if line.contains("Instantaneous") || line.contains("Current") {
                current = current.or(val);
            } else if line.contains("Minimum") {
                minimum = val;
            } else if line.contains("Maximum") {
                maximum = val;
            } else if line.contains("Average") {
                average = val;
            }
        }
    }

    let current_watts = current.ok_or_else(|| {
        super::DatacenterError::IpmiError(
            "ipmitool dcmi power reading gave no instantaneous power reading".into(),
        )
    })?;
    Ok(PowerReading {
        current_watts,
        minimum_watts: minimum,
        maximum_watts: maximum,
        average_watts: average,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sensor_with_no_reading_has_no_value() {
        let sensors = parse_sensor_list(concat!(
            "CPU Temp         | 45.000     | degrees C  | ok    | 0.000     | 0.000     | 0.000     | 90.000    | 95.000    | 100.000\n",
            "FAN 4            | na         | RPM        | na    | na        | 300.000   | 500.000   | na        | na        | na\n",
        ));
        assert_eq!(sensors.len(), 2);
        assert_eq!(sensors[0].value, Some(45.0));
        assert_eq!(sensors[0].upper_critical, Some(95.0));
        assert_eq!(sensors[1].value, None, "`na` is not 0 RPM");
        assert_eq!(sensors[1].sensor_type, IpmiSensorType::Fan);
    }

    #[test]
    fn no_instantaneous_reading_is_an_error_not_zero_watts() {
        let reading = parse_power_reading(concat!(
            "    Instantaneous power reading:                   212 Watts\n",
            "    Minimum during sampling period:                 98 Watts\n",
            "    Maximum during sampling period:                389 Watts\n",
            "    Average power reading over sample period:      205 Watts\n",
        ))
        .unwrap();
        assert_eq!(reading.current_watts, 212.0);
        assert_eq!(reading.maximum_watts, Some(389.0));
        assert!(parse_power_reading("    Power reading state is: deactivated\n").is_err());
    }
}
