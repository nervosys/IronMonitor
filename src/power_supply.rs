//! Power Supply and Battery Monitoring
//!
//! This module provides monitoring for power supplies, batteries, and AC adapters.
//! Inspired by jetson_stats power monitoring and Linux `/sys/class/power_supply/`.
//!
//! # Examples
//!
//! ## List all power supplies
//!
//! ```no_run
//! use ironmonitor::power_supply::{PowerSupplyMonitor, PowerSupplyType};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let monitor = PowerSupplyMonitor::new()?;
//! for supply in monitor.supplies() {
//!     println!("{}: {:?}", supply.name, supply.supply_type);
//!     if let Some(capacity) = supply.capacity_percent {
//!         println!("  Capacity: {}%", capacity);
//!     }
//!     if let Some(power) = supply.power_now_mw {
//!         println!("  Power: {:.2}W", power as f64 / 1000.0);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};

/// Type of power supply
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerSupplyType {
    /// AC adapter / mains power
    Mains,
    /// Battery (rechargeable)
    Battery,
    /// USB power delivery
    Usb,
    /// USB-C PD
    UsbPd,
    /// Uninterruptible Power Supply
    Ups,
    /// Unknown type
    Unknown,
}

impl std::fmt::Display for PowerSupplyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PowerSupplyType::Mains => write!(f, "Mains"),
            PowerSupplyType::Battery => write!(f, "Battery"),
            PowerSupplyType::Usb => write!(f, "USB"),
            PowerSupplyType::UsbPd => write!(f, "USB-PD"),
            PowerSupplyType::Ups => write!(f, "UPS"),
            PowerSupplyType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Battery charging status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargingStatus {
    /// Battery is charging
    Charging,
    /// Battery is discharging
    Discharging,
    /// Battery is full
    Full,
    /// Not charging (plugged in but not charging)
    NotCharging,
    /// Unknown status
    Unknown,
}

impl std::fmt::Display for ChargingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChargingStatus::Charging => write!(f, "Charging"),
            ChargingStatus::Discharging => write!(f, "Discharging"),
            ChargingStatus::Full => write!(f, "Full"),
            ChargingStatus::NotCharging => write!(f, "Not Charging"),
            ChargingStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Battery health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatteryHealth {
    /// Battery is in good condition
    Good,
    /// Battery is overheating
    Overheat,
    /// Battery is dead
    Dead,
    /// Over voltage condition
    OverVoltage,
    /// Unspecified failure
    UnspecifiedFailure,
    /// Battery is cold
    Cold,
    /// Watchdog timer expired
    WatchdogTimerExpire,
    /// Safety timer expired
    SafetyTimerExpire,
    /// Unknown health status
    Unknown,
}

impl std::fmt::Display for BatteryHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatteryHealth::Good => write!(f, "Good"),
            BatteryHealth::Overheat => write!(f, "Overheat"),
            BatteryHealth::Dead => write!(f, "Dead"),
            BatteryHealth::OverVoltage => write!(f, "Over Voltage"),
            BatteryHealth::UnspecifiedFailure => write!(f, "Failure"),
            BatteryHealth::Cold => write!(f, "Cold"),
            BatteryHealth::WatchdogTimerExpire => write!(f, "Watchdog Expired"),
            BatteryHealth::SafetyTimerExpire => write!(f, "Safety Timer Expired"),
            BatteryHealth::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Information about a power supply
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerSupplyInfo {
    /// Name of the power supply
    pub name: String,
    /// Type of power supply
    pub supply_type: PowerSupplyType,
    /// Whether the supply is online/connected
    /// Whether this supply is currently delivering power.
    ///
    /// `None` when the platform did not say. Windows documents
    /// `SYSTEM_POWER_STATUS::ACLineStatus` as 0 offline, 1 online and **255
    /// unknown**, and this was `status.ACLineStatus == 1` -- so a host that
    /// answered "unknown" was recorded as running on battery. The rest of that
    /// function already handles every other sentinel the struct defines (255
    /// for `BatteryLifePercent`, `0xFFFFFFFF` for both time fields, 128 for "no
    /// battery"), which is what made this one worth fixing rather than the
    /// function worth rewriting.
    pub online: Option<bool>,
    /// Charging status (for batteries). `None` if the platform did not report it.
    pub status: Option<ChargingStatus>,
    /// Battery health (for batteries). `None` if the platform did not report it.
    pub health: Option<BatteryHealth>,
    /// Current capacity percentage (0-100)
    pub capacity_percent: Option<u8>,
    /// Voltage now in millivolts
    pub voltage_now_mv: Option<u32>,
    /// Voltage minimum design in millivolts
    pub voltage_min_mv: Option<u32>,
    /// Current now in microamps (positive = charging, negative = discharging)
    pub current_now_ua: Option<i32>,
    /// Power now in milliwatts. Linux reads `power_now` in microwatts and divides
    /// by 1000; the doc said microwatts, which the field never held, and
    /// `power_w()` divides by 1000 as well.
    pub power_now_mw: Option<u32>,
    /// Energy now in microwatt-hours
    pub energy_now_uwh: Option<u64>,
    /// Energy full in microwatt-hours
    pub energy_full_uwh: Option<u64>,
    /// Energy full design in microwatt-hours
    pub energy_full_design_uwh: Option<u64>,
    /// Charge now in microamp-hours
    pub charge_now_uah: Option<u64>,
    /// Charge full in microamp-hours
    pub charge_full_uah: Option<u64>,
    /// Charge full design in microamp-hours
    pub charge_full_design_uah: Option<u64>,
    /// Temperature in tenths of degrees Celsius
    pub temperature_tenths_c: Option<i32>,
    /// Manufacturer name
    pub manufacturer: Option<String>,
    /// Model name
    pub model_name: Option<String>,
    /// Serial number
    pub serial_number: Option<String>,
    /// Technology (Li-ion, NiMH, etc.)
    pub technology: Option<String>,
    /// Cycle count (number of charge cycles)
    pub cycle_count: Option<u32>,
    /// Time to empty in minutes (estimated)
    pub time_to_empty_min: Option<u32>,
    /// Time to full in minutes (estimated)
    pub time_to_full_min: Option<u32>,
}

impl PowerSupplyInfo {
    /// Create a new PowerSupplyInfo with default values
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            supply_type: PowerSupplyType::Unknown,
            online: None,
            status: None,
            health: None,
            capacity_percent: None,
            voltage_now_mv: None,
            voltage_min_mv: None,
            current_now_ua: None,
            power_now_mw: None,
            energy_now_uwh: None,
            energy_full_uwh: None,
            energy_full_design_uwh: None,
            charge_now_uah: None,
            charge_full_uah: None,
            charge_full_design_uah: None,
            temperature_tenths_c: None,
            manufacturer: None,
            model_name: None,
            serial_number: None,
            technology: None,
            cycle_count: None,
            time_to_empty_min: None,
            time_to_full_min: None,
        }
    }

    /// Get temperature in Celsius (if available)
    pub fn temperature_celsius(&self) -> Option<f32> {
        self.temperature_tenths_c.map(|t| t as f32 / 10.0)
    }

    /// Get voltage in volts (if available)
    pub fn voltage_v(&self) -> Option<f32> {
        self.voltage_now_mv.map(|v| v as f32 / 1000.0)
    }

    /// Get current in amps (if available)
    pub fn current_a(&self) -> Option<f32> {
        self.current_now_ua.map(|c| c as f32 / 1_000_000.0)
    }

    /// Get power in watts (if available)
    pub fn power_w(&self) -> Option<f32> {
        self.power_now_mw.map(|p| p as f32 / 1000.0)
    }

    /// Get energy now in watt-hours (if available)
    pub fn energy_wh(&self) -> Option<f32> {
        self.energy_now_uwh.map(|e| e as f32 / 1_000_000.0)
    }

    /// Get energy full in watt-hours (if available)
    pub fn energy_full_wh(&self) -> Option<f32> {
        self.energy_full_uwh.map(|e| e as f32 / 1_000_000.0)
    }

    /// Get energy full design in watt-hours (if available)
    pub fn energy_full_design_wh(&self) -> Option<f32> {
        self.energy_full_design_uwh.map(|e| e as f32 / 1_000_000.0)
    }

    /// Calculate battery wear level as percentage (100% = like new)
    pub fn wear_level_percent(&self) -> Option<f32> {
        match (self.energy_full_uwh, self.energy_full_design_uwh) {
            (Some(full), Some(design)) if design > 0 => Some((full as f32 / design as f32) * 100.0),
            _ => match (self.charge_full_uah, self.charge_full_design_uah) {
                (Some(full), Some(design)) if design > 0 => {
                    Some((full as f32 / design as f32) * 100.0)
                }
                _ => None,
            },
        }
    }

    /// Check if this is a battery
    pub fn is_battery(&self) -> bool {
        self.supply_type == PowerSupplyType::Battery
    }

    /// Check if AC power is connected
    pub fn is_on_ac_power(&self) -> Option<bool> {
        if self.supply_type != PowerSupplyType::Mains {
            return Some(false);
        }
        self.online
    }
}

/// Power supply monitor
pub struct PowerSupplyMonitor {
    supplies: Vec<PowerSupplyInfo>,
}

impl PowerSupplyMonitor {
    /// Create a new power supply monitor
    pub fn new() -> Result<Self> {
        let supplies = Self::enumerate_supplies()?;
        Ok(Self { supplies })
    }

    /// Get all power supplies
    pub fn supplies(&self) -> &[PowerSupplyInfo] {
        &self.supplies
    }

    /// Get mutable reference to supplies for updating
    pub fn supplies_mut(&mut self) -> &mut Vec<PowerSupplyInfo> {
        &mut self.supplies
    }

    /// Update all power supply information
    pub fn update(&mut self) -> Result<()> {
        self.supplies = Self::enumerate_supplies()?;
        Ok(())
    }

    /// Get the primary battery (if any)
    pub fn primary_battery(&self) -> Option<&PowerSupplyInfo> {
        self.supplies
            .iter()
            .find(|s| s.supply_type == PowerSupplyType::Battery)
    }

    /// Get AC adapter info (if any)
    pub fn ac_adapter(&self) -> Option<&PowerSupplyInfo> {
        self.supplies
            .iter()
            .find(|s| s.supply_type == PowerSupplyType::Mains)
    }

    /// Check if running on AC power.
    ///
    /// `None` when no mains supply was enumerated, or when the one that was
    /// did not report its state. An empty supply list is a failed enumeration
    /// as often as it is a machine with no power supplies, and this used to
    /// answer `false` -- "on battery" -- for both.
    pub fn on_ac_power(&self) -> Option<bool> {
        let mains: Vec<&PowerSupplyInfo> = self
            .supplies
            .iter()
            .filter(|s| s.supply_type == PowerSupplyType::Mains)
            .collect();
        if mains.is_empty() {
            return None;
        }
        // Any mains supply that says it is online settles it. Otherwise the
        // answer is `false` only if at least one of them actually said so.
        if mains.iter().any(|s| s.online == Some(true)) {
            Some(true)
        } else if mains.iter().any(|s| s.online == Some(false)) {
            Some(false)
        } else {
            None
        }
    }

    /// Check if running on battery.
    ///
    /// `None` when the AC state is unknown: "not on mains" cannot be concluded
    /// from "nobody said whether it is on mains".
    pub fn on_battery(&self) -> Option<bool> {
        let has_battery = self
            .supplies
            .iter()
            .any(|s| s.supply_type == PowerSupplyType::Battery);
        Some(!self.on_ac_power()? && has_battery)
    }

    /// Get total system power consumption in watts (if available)
    pub fn total_power_w(&self) -> Option<f32> {
        let mut total = 0.0f32;
        let mut found = false;

        for supply in &self.supplies {
            if let Some(power) = supply.power_w() {
                total += power;
                found = true;
            }
        }

        if found {
            Some(total)
        } else {
            None
        }
    }

    /// Enumerate all power supplies on the system
    #[cfg(target_os = "linux")]
    fn enumerate_supplies() -> Result<Vec<PowerSupplyInfo>> {
        use std::fs;
        use std::path::Path;

        let mut supplies = Vec::new();
        let power_supply_path = Path::new("/sys/class/power_supply");

        // A missing class directory means this kernel has no power-supply
        // subsystem to read. That is a failed read, not a machine with no supplies.
        if !power_supply_path.exists() {
            return Err(IronError::System(
                "/sys/class/power_supply does not exist".to_string(),
            ));
        }

        let entries = fs::read_dir(power_supply_path)
            .map_err(|e| IronError::System(format!("Failed to read power_supply: {}", e)))?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();

            let mut info = PowerSupplyInfo::new(&name);

            // Read type
            if let Ok(type_str) = fs::read_to_string(path.join("type")) {
                info.supply_type = match type_str.trim().to_lowercase().as_str() {
                    "mains" => PowerSupplyType::Mains,
                    "battery" => PowerSupplyType::Battery,
                    "usb" => PowerSupplyType::Usb,
                    "usb_pd" | "usb-pd" => PowerSupplyType::UsbPd,
                    "ups" => PowerSupplyType::Ups,
                    _ => PowerSupplyType::Unknown,
                };
            }

            // Read online status. Only the kernel's "0" and "1" are readings;
            // anything else is withheld rather than read as offline.
            if let Ok(online_str) = fs::read_to_string(path.join("online")) {
                info.online = match online_str.trim() {
                    "1" => Some(true),
                    "0" => Some(false),
                    _ => None,
                };
            }

            // Read status
            if let Ok(status_str) = fs::read_to_string(path.join("status")) {
                info.status = Some(match status_str.trim().to_lowercase().as_str() {
                    "charging" => ChargingStatus::Charging,
                    "discharging" => ChargingStatus::Discharging,
                    "full" => ChargingStatus::Full,
                    "not charging" => ChargingStatus::NotCharging,
                    // Read, but not one of the kernel's values for a charging state.
                    _ => ChargingStatus::Unknown,
                });
            }

            // Read health
            if let Ok(health_str) = fs::read_to_string(path.join("health")) {
                info.health = Some(match health_str.trim().to_lowercase().as_str() {
                    "good" => BatteryHealth::Good,
                    "overheat" => BatteryHealth::Overheat,
                    "dead" => BatteryHealth::Dead,
                    "overvoltage" | "over voltage" => BatteryHealth::OverVoltage,
                    "unspecified failure" => BatteryHealth::UnspecifiedFailure,
                    "cold" => BatteryHealth::Cold,
                    "watchdog timer expire" => BatteryHealth::WatchdogTimerExpire,
                    "safety timer expire" => BatteryHealth::SafetyTimerExpire,
                    _ => BatteryHealth::Unknown,
                });
            }

            // Read capacity. Above 100 is not a percentage: withheld, not clamped.
            if let Ok(cap_str) = fs::read_to_string(path.join("capacity")) {
                if let Ok(cap) = cap_str.trim().parse::<u8>() {
                    if cap <= 100 {
                        info.capacity_percent = Some(cap);
                    }
                }
            }

            // Read voltage_now (microvolts)
            if let Ok(volt_str) = fs::read_to_string(path.join("voltage_now")) {
                if let Ok(volt_uv) = volt_str.trim().parse::<u64>() {
                    info.voltage_now_mv = Some((volt_uv / 1000) as u32);
                }
            }

            // Read voltage_min_design
            if let Ok(volt_str) = fs::read_to_string(path.join("voltage_min_design")) {
                if let Ok(volt_uv) = volt_str.trim().parse::<u64>() {
                    info.voltage_min_mv = Some((volt_uv / 1000) as u32);
                }
            }

            // Read current_now (microamps)
            if let Ok(curr_str) = fs::read_to_string(path.join("current_now")) {
                if let Ok(curr) = curr_str.trim().parse::<i32>() {
                    info.current_now_ua = Some(curr);
                }
            }

            // Read power_now (microwatts)
            if let Ok(power_str) = fs::read_to_string(path.join("power_now")) {
                if let Ok(power) = power_str.trim().parse::<u64>() {
                    info.power_now_mw = Some((power / 1000) as u32);
                }
            }

            // Read energy values
            if let Ok(energy_str) = fs::read_to_string(path.join("energy_now")) {
                if let Ok(energy) = energy_str.trim().parse::<u64>() {
                    info.energy_now_uwh = Some(energy);
                }
            }

            if let Ok(energy_str) = fs::read_to_string(path.join("energy_full")) {
                if let Ok(energy) = energy_str.trim().parse::<u64>() {
                    info.energy_full_uwh = Some(energy);
                }
            }

            if let Ok(energy_str) = fs::read_to_string(path.join("energy_full_design")) {
                if let Ok(energy) = energy_str.trim().parse::<u64>() {
                    info.energy_full_design_uwh = Some(energy);
                }
            }

            // Read charge values
            if let Ok(charge_str) = fs::read_to_string(path.join("charge_now")) {
                if let Ok(charge) = charge_str.trim().parse::<u64>() {
                    info.charge_now_uah = Some(charge);
                }
            }

            if let Ok(charge_str) = fs::read_to_string(path.join("charge_full")) {
                if let Ok(charge) = charge_str.trim().parse::<u64>() {
                    info.charge_full_uah = Some(charge);
                }
            }

            if let Ok(charge_str) = fs::read_to_string(path.join("charge_full_design")) {
                if let Ok(charge) = charge_str.trim().parse::<u64>() {
                    info.charge_full_design_uah = Some(charge);
                }
            }

            // Read temperature (tenths of degrees C)
            if let Ok(temp_str) = fs::read_to_string(path.join("temp")) {
                if let Ok(temp) = temp_str.trim().parse::<i32>() {
                    info.temperature_tenths_c = Some(temp);
                }
            }

            // Read manufacturer
            if let Ok(mfr_str) = fs::read_to_string(path.join("manufacturer")) {
                let mfr = mfr_str.trim();
                if !mfr.is_empty() {
                    info.manufacturer = Some(mfr.to_string());
                }
            }

            // Read model name
            if let Ok(model_str) = fs::read_to_string(path.join("model_name")) {
                let model = model_str.trim();
                if !model.is_empty() {
                    info.model_name = Some(model.to_string());
                }
            }

            // Read serial number
            if let Ok(serial_str) = fs::read_to_string(path.join("serial_number")) {
                let serial = serial_str.trim();
                if !serial.is_empty() {
                    info.serial_number = Some(serial.to_string());
                }
            }

            // Read technology
            if let Ok(tech_str) = fs::read_to_string(path.join("technology")) {
                let tech = tech_str.trim();
                if !tech.is_empty() {
                    info.technology = Some(tech.to_string());
                }
            }

            // Read cycle count
            if let Ok(cycle_str) = fs::read_to_string(path.join("cycle_count")) {
                if let Ok(cycles) = cycle_str.trim().parse::<u32>() {
                    info.cycle_count = Some(cycles);
                }
            }

            // Read time to empty. The sysfs value is in seconds.
            if let Ok(time_str) = fs::read_to_string(path.join("time_to_empty_avg")) {
                if let Ok(time) = time_str.trim().parse::<u32>() {
                    info.time_to_empty_min = Some(time / 60);
                }
            }

            // Read time to full. The sysfs value is in seconds.
            if let Ok(time_str) = fs::read_to_string(path.join("time_to_full_avg")) {
                if let Ok(time) = time_str.trim().parse::<u32>() {
                    info.time_to_full_min = Some(time / 60);
                }
            }

            supplies.push(info);
        }

        Ok(supplies)
    }

    #[cfg(target_os = "windows")]
    fn enumerate_supplies() -> Result<Vec<PowerSupplyInfo>> {
        let mut supplies = Vec::new();
        // The battery `GetSystemPowerStatus` describes, held until it is joined
        // to the batteries WMI describes.
        let mut status_battery: Option<PowerSupplyInfo> = None;

        // Use Windows API to get battery information
        unsafe {
            use ::windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

            let mut status = SYSTEM_POWER_STATUS::default();
            // A failed call is a failed read, not a machine with no supplies.
            if let Err(e) = GetSystemPowerStatus(&mut status) {
                return Err(IronError::System(format!(
                    "GetSystemPowerStatus failed: {}",
                    e
                )));
            }

            // AC Line Status
            let mut ac_info = PowerSupplyInfo::new("AC");
            ac_info.supply_type = PowerSupplyType::Mains;
            // 0 offline, 1 online, 255 unknown.
            ac_info.online = match status.ACLineStatus {
                0 => Some(false),
                1 => Some(true),
                _ => None,
            };
            supplies.push(ac_info);

            // Battery
            if status.BatteryFlag != 128 {
                // 128 = No battery
                let mut battery_info = PowerSupplyInfo::new("Battery");
                battery_info.supply_type = PowerSupplyType::Battery;
                // `online` stays unset: this API reports the AC line, not whether
                // the battery is delivering power.

                // Capacity
                if status.BatteryLifePercent != 255 {
                    battery_info.capacity_percent = Some(status.BatteryLifePercent);
                }

                // Status. BatteryFlag 255 is "unknown". It also has the charging
                // bit set, so it is excluded before that bit is tested.
                if status.BatteryFlag != 255 {
                    battery_info.status = if status.BatteryFlag & 8 != 0 {
                        Some(ChargingStatus::Charging)
                    } else if status.ACLineStatus == 1 && status.BatteryLifePercent == 100 {
                        Some(ChargingStatus::Full)
                    } else if status.ACLineStatus == 0 {
                        Some(ChargingStatus::Discharging)
                    } else if status.ACLineStatus == 1 {
                        Some(ChargingStatus::NotCharging)
                    } else {
                        None
                    };
                }

                // Time remaining
                if status.BatteryLifeTime != 0xFFFFFFFF {
                    battery_info.time_to_empty_min = Some(status.BatteryLifeTime / 60);
                }

                // `BatteryFullLifeTime` is how long a full battery lasts, not the
                // time until full, so it is not a `time_to_full_min`. Windows
                // reports no time to full here, and the field stays unset.

                status_battery = Some(battery_info);
            }
        }

        supplies.extend(combine_windows_batteries(
            status_battery,
            read_wmi_batteries(),
        ));
        Ok(supplies)
    }

    #[cfg(target_os = "macos")]
    fn enumerate_supplies() -> Result<Vec<PowerSupplyInfo>> {
        use std::process::Command;

        let mut supplies = Vec::new();

        // Use pmset to get battery info. A run that fails is a failed read, not
        // a machine with no power supplies.
        let output = Command::new("pmset")
            .args(["-g", "batt"])
            .output()
            .map_err(|e| IronError::CommandFailed(format!("pmset -g batt failed: {}", e)))?;
        if !output.status.success() {
            return Err(IronError::CommandFailed(format!(
                "pmset -g batt exited with {}",
                output.status
            )));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse AC adapter
        let mut ac_info = PowerSupplyInfo::new("AC");
        ac_info.supply_type = PowerSupplyType::Mains;
        // Only a "drawing from" line says where power comes from; output
        // without one is unknown, not "on battery".
        ac_info.online = crate::power_profile::parse_pmset_power_source(&stdout);
        supplies.push(ac_info);

        // Parse battery. Battery lines start with the battery's name, e.g.
        // " -InternalBattery-0 (id=...)\t87%; discharging; 5:23 remaining". The
        // header line can say "Battery Power", so the name is what is matched.
        for line in stdout.lines() {
            if line.contains("InternalBattery") || line.trim_start().starts_with('-') {
                supplies.push(parse_pmset_battery_line(line));
                break;
            }
        }

        Ok(supplies)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn enumerate_supplies() -> Result<Vec<PowerSupplyInfo>> {
        // No reader exists for this platform. That is not the same as "no
        // power supplies here", so it is reported as an error.
        Err(IronError::UnsupportedPlatform(
            "power supply enumeration is not implemented for this platform".to_string(),
        ))
    }
}

impl Default for PowerSupplyMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            supplies: Vec::new(),
        })
    }
}

/// Summary of system power state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerSummary {
    /// Whether running on AC power. `None` if that was not established.
    pub on_ac_power: Option<bool>,
    /// Whether running on battery. `None` if that was not established.
    pub on_battery: Option<bool>,
    /// Primary battery capacity (if any)
    pub battery_percent: Option<u8>,
    /// Primary battery status. `None` if not read.
    pub battery_status: Option<ChargingStatus>,
    /// Battery health. `None` if not read.
    pub battery_health: Option<BatteryHealth>,
    /// Estimated time remaining on battery (minutes)
    pub time_remaining_min: Option<u32>,
    /// Total system power draw (watts)
    pub power_draw_w: Option<f32>,
    /// Battery wear level (100% = like new)
    pub battery_wear_percent: Option<f32>,
}

impl PowerSummary {
    /// Get power summary from monitor
    pub fn from_monitor(monitor: &PowerSupplyMonitor) -> Self {
        let battery = monitor.primary_battery();

        Self {
            on_ac_power: monitor.on_ac_power(),
            on_battery: monitor.on_battery(),
            battery_percent: battery.and_then(|b| b.capacity_percent),
            battery_status: battery.and_then(|b| b.status),
            battery_health: battery.and_then(|b| b.health),
            time_remaining_min: battery.and_then(|b| b.time_to_empty_min),
            power_draw_w: monitor.total_power_w(),
            battery_wear_percent: battery.and_then(|b| b.wear_level_percent()),
        }
    }
}

/// Get a quick power summary
pub fn power_summary() -> Result<PowerSummary> {
    let monitor = PowerSupplyMonitor::new()?;
    Ok(PowerSummary::from_monitor(&monitor))
}

/// Check if running on AC power
pub fn is_on_ac_power() -> Option<bool> {
    PowerSupplyMonitor::new().ok().and_then(|m| m.on_ac_power())
}

/// Check if running on battery
pub fn is_on_battery() -> Option<bool> {
    PowerSupplyMonitor::new().ok().and_then(|m| m.on_battery())
}

/// Get battery percentage (if available)
pub fn battery_percent() -> Option<u8> {
    PowerSupplyMonitor::new()
        .ok()
        .and_then(|m| m.primary_battery().and_then(|b| b.capacity_percent))
}

/// The batteries WMI describes in `root\WMI`, joined by `InstanceName`.
///
/// Four classes are read in one PowerShell pass: `BatteryStaticData` (design
/// capacity, manufacturer, serial number, chemistry), `BatteryFullChargedCapacity`,
/// `BatteryStatus` (remaining capacity, voltage, charge and discharge rate) and
/// `BatteryCycleCount`. A desktop has none of these instances and the list is
/// empty, which is a reading. A failed query is also empty, and the batteries it
/// would have described are not guessed at.
#[cfg(windows)]
fn read_wmi_batteries() -> Vec<PowerSupplyInfo> {
    const QUERY: &str = concat!(
        "$ns = 'root/wmi'; ",
        "[pscustomobject]@{ ",
        "Static = @(Get-CimInstance -Namespace $ns -ClassName BatteryStaticData -ErrorAction SilentlyContinue | ",
        "Select-Object InstanceName, DesignedCapacity, ManufactureName, SerialNumber, Chemistry); ",
        "Full = @(Get-CimInstance -Namespace $ns -ClassName BatteryFullChargedCapacity -ErrorAction SilentlyContinue | ",
        "Select-Object InstanceName, FullChargedCapacity); ",
        "Status = @(Get-CimInstance -Namespace $ns -ClassName BatteryStatus -ErrorAction SilentlyContinue | ",
        "Select-Object InstanceName, RemainingCapacity, Voltage, ChargeRate, DischargeRate); ",
        "Cycle = @(Get-CimInstance -Namespace $ns -ClassName BatteryCycleCount -ErrorAction SilentlyContinue | ",
        "Select-Object InstanceName, CycleCount) ",
        "} | ConvertTo-Json -Depth 4 -Compress"
    );
    crate::core::command::capture_json("powershell", &["-NoProfile", "-Command", QUERY])
        .ok()
        .flatten()
        .map(|json| batteries_from_wmi(&json))
        .unwrap_or_default()
}

/// Rows of one WMI class in the captured JSON, keyed by `InstanceName`.
#[cfg(any(windows, test))]
fn wmi_rows_by_instance(
    json: &serde_json::Value,
    key: &str,
) -> std::collections::HashMap<String, serde_json::Value> {
    json.get(key)
        .map(crate::core::command::json_items)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| {
            let id = row.get("InstanceName")?.as_str()?.to_string();
            Some((id, row))
        })
        .collect()
}

/// A WMI battery integer, or `None` where it is the API's unknown marker.
///
/// The battery API reports `0xFFFFFFFF` for an unknown capacity or voltage.
#[cfg(any(windows, test))]
fn wmi_u32(value: Option<&serde_json::Value>) -> Option<u32> {
    let n = value?.as_u64()?;
    if n == 0xFFFF_FFFF {
        return None;
    }
    u32::try_from(n).ok()
}

/// A charge or discharge rate in mW. The battery API reports `0x80000000` for an
/// unknown rate, and an idle battery reports 0 on both, which is a reading.
#[cfg(any(windows, test))]
fn wmi_rate(
    discharge: Option<&serde_json::Value>,
    charge: Option<&serde_json::Value>,
) -> Option<u32> {
    let rate = |v| wmi_u32(v).filter(|&r| r != 0x8000_0000);
    match (rate(discharge), rate(charge)) {
        // At most one of the two is non-zero, so the larger is the rate.
        (Some(d), Some(c)) => Some(d.max(c)),
        (Some(d), None) if d > 0 => Some(d),
        (None, Some(c)) if c > 0 => Some(c),
        _ => None,
    }
}

/// The technology name for a `BatteryStaticData.Chemistry` value, which is four
/// ASCII bytes. A code not listed here is `None`, not a guess.
#[cfg(any(windows, test))]
fn technology_from_chemistry(chemistry: &serde_json::Value) -> Option<String> {
    let bytes: Vec<u8> = chemistry
        .as_array()?
        .iter()
        .filter_map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
        .collect();
    let code = String::from_utf8(bytes).ok()?;
    match code.trim_end_matches('\0') {
        "LION" => Some("Li-ion".to_string()),
        "LIPO" => Some("LiPo".to_string()),
        "NiMH" => Some("NiMH".to_string()),
        "NiCd" => Some("NiCd".to_string()),
        "PbAc" => Some("Lead-acid".to_string()),
        _ => None,
    }
}

/// Build one battery per `BatteryStaticData` instance, filling the rest from the
/// rows with the same `InstanceName`. A field whose class has no row for that
/// instance stays unset.
#[cfg(any(windows, test))]
fn batteries_from_wmi(json: &serde_json::Value) -> Vec<PowerSupplyInfo> {
    let full = wmi_rows_by_instance(json, "Full");
    let status = wmi_rows_by_instance(json, "Status");
    let cycle = wmi_rows_by_instance(json, "Cycle");
    // mWh to uWh, the unit the struct keeps.
    let mwh_to_uwh = |mwh: u32| u64::from(mwh) * 1000;

    let statics = json
        .get("Static")
        .map(crate::core::command::json_items)
        .unwrap_or_default();

    let mut batteries = Vec::new();
    for row in &statics {
        let Some(id) = row.get("InstanceName").and_then(|v| v.as_str()) else {
            continue;
        };
        let text = |v: Option<&serde_json::Value>| {
            v.and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let full = full.get(id);
        let status = status.get(id);
        let cycle = cycle.get(id);

        let mut info = PowerSupplyInfo::new(id);
        info.supply_type = PowerSupplyType::Battery;
        info.manufacturer = text(row.get("ManufactureName"));
        info.serial_number = text(row.get("SerialNumber"));
        info.technology = row.get("Chemistry").and_then(technology_from_chemistry);
        info.energy_full_design_uwh = wmi_u32(row.get("DesignedCapacity"))
            .filter(|&v| v > 0)
            .map(mwh_to_uwh);
        info.energy_full_uwh = full
            .and_then(|f| wmi_u32(f.get("FullChargedCapacity")))
            .filter(|&v| v > 0)
            .map(mwh_to_uwh);
        info.energy_now_uwh = status
            .and_then(|s| wmi_u32(s.get("RemainingCapacity")))
            .map(mwh_to_uwh);
        info.voltage_now_mv = status.and_then(|s| wmi_u32(s.get("Voltage")));
        info.power_now_mw =
            status.and_then(|s| wmi_rate(s.get("DischargeRate"), s.get("ChargeRate")));
        info.cycle_count = cycle.and_then(|c| wmi_u32(c.get("CycleCount")));
        batteries.push(info);
    }
    batteries
}

/// Join the battery `GetSystemPowerStatus` describes to the batteries WMI
/// describes.
///
/// `GetSystemPowerStatus` gives one combined percentage and charging state, and
/// no instance name. It is attributed to a battery only when WMI describes
/// exactly one, since then it is that battery's. With several batteries it is
/// dropped rather than given to one of them.
#[cfg(any(windows, test))]
fn combine_windows_batteries(
    status_battery: Option<PowerSupplyInfo>,
    mut wmi: Vec<PowerSupplyInfo>,
) -> Vec<PowerSupplyInfo> {
    match wmi.len() {
        0 => status_battery.into_iter().collect(),
        1 => {
            let mut battery = wmi.remove(0);
            if let Some(status) = status_battery {
                battery.capacity_percent = status.capacity_percent;
                battery.status = status.status;
                battery.time_to_empty_min = status.time_to_empty_min;
            }
            vec![battery]
        }
        _ => wmi,
    }
}

/// Read one pmset battery line, e.g.
/// `" -InternalBattery-0 (id=...)\t87%; discharging; 5:23 remaining present: true"`.
#[cfg(any(target_os = "macos", test))]
fn parse_pmset_battery_line(line: &str) -> PowerSupplyInfo {
    let mut battery_info = PowerSupplyInfo::new("Battery");
    battery_info.supply_type = PowerSupplyType::Battery;
    // `online` stays unset, as on Windows: pmset does not say whether the battery
    // is delivering power.

    // Parse percentage: the number directly before '%'. Digits earlier on the line
    // belong to the battery's name ("InternalBattery-0").
    if let Some(pct_end) = line.find('%') {
        let pct_str: String = line[..pct_end]
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if let Ok(pct) = pct_str.parse::<u8>() {
            if pct <= 100 {
                battery_info.capacity_percent = Some(pct);
            }
        }
    }

    // Parse status. "discharging" contains "charging", so it is tested first.
    if line.contains("discharging") {
        battery_info.status = Some(ChargingStatus::Discharging);
    } else if line.contains("not charging") {
        battery_info.status = Some(ChargingStatus::NotCharging);
    } else if line.contains("charging") {
        battery_info.status = Some(ChargingStatus::Charging);
    } else if line.contains("charged") {
        battery_info.status = Some(ChargingStatus::Full);
    }

    // Parse time remaining: the "H:MM" token before " remaining". A line without
    // one, such as "(no estimate)", leaves it unset. While charging, pmset's
    // remaining time is the time until full, not until empty.
    if let Some(rem) = line.find(" remaining") {
        if let Some(time_str) = line[..rem].rsplit(' ').next() {
            if let Some((h, m)) = time_str.split_once(':') {
                if let (Ok(hours), Ok(mins)) = (h.parse::<u32>(), m.parse::<u32>()) {
                    let minutes = Some(hours * 60 + mins);
                    match battery_info.status {
                        Some(ChargingStatus::Charging) => battery_info.time_to_full_min = minutes,
                        Some(ChargingStatus::Discharging) => {
                            battery_info.time_to_empty_min = minutes
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    battery_info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pmset_line_reads_percentage_status_and_time_to_empty() {
        let line =
            " -InternalBattery-0 (id=4653155)\t87%; discharging; 5:23 remaining present: true";
        let info = parse_pmset_battery_line(line);
        // The "0" in the battery's name is not the charge.
        assert_eq!(info.capacity_percent, Some(87));
        assert_eq!(info.status, Some(ChargingStatus::Discharging));
        assert_eq!(info.time_to_empty_min, Some(323));
        assert_eq!(info.time_to_full_min, None);
    }

    #[test]
    fn pmset_charging_time_is_time_to_full() {
        let line = " -InternalBattery-0 (id=4653155)\t52%; charging; 1:20 remaining present: true";
        let info = parse_pmset_battery_line(line);
        assert_eq!(info.status, Some(ChargingStatus::Charging));
        assert_eq!(info.time_to_full_min, Some(80));
        assert_eq!(info.time_to_empty_min, None);
    }

    #[test]
    fn pmset_line_without_estimate_leaves_times_unset() {
        let line =
            " -InternalBattery-0 (id=4653155)\t100%; AC attached; not charging present: true";
        let info = parse_pmset_battery_line(line);
        assert_eq!(info.capacity_percent, Some(100));
        assert_eq!(info.status, Some(ChargingStatus::NotCharging));
        assert_eq!(info.time_to_empty_min, None);
        assert_eq!(info.time_to_full_min, None);
    }

    #[test]
    fn test_power_supply_info_new_defaults() {
        let info = PowerSupplyInfo::new("BAT0");
        assert_eq!(info.name, "BAT0");
        assert_eq!(info.supply_type, PowerSupplyType::Unknown);
        assert_eq!(info.online, None);
        assert_eq!(info.status, None);
        assert_eq!(info.health, None);
        assert!(info.capacity_percent.is_none());
    }

    #[test]
    fn test_is_battery() {
        let mut info = PowerSupplyInfo::new("BAT0");
        assert!(!info.is_battery());
        info.supply_type = PowerSupplyType::Battery;
        assert!(info.is_battery());
        info.supply_type = PowerSupplyType::Mains;
        assert!(!info.is_battery());
    }

    #[test]
    fn test_unit_conversions() {
        let mut info = PowerSupplyInfo::new("BAT0");
        info.voltage_now_mv = Some(12000);
        assert_eq!(info.voltage_v(), Some(12.0));

        info.power_now_mw = Some(5000);
        assert_eq!(info.power_w(), Some(5.0));

        info.energy_now_uwh = Some(40_000_000);
        assert_eq!(info.energy_wh(), Some(40.0));

        info.temperature_tenths_c = Some(350);
        assert_eq!(info.temperature_celsius(), Some(35.0));

        info.current_now_ua = Some(2_000_000);
        assert_eq!(info.current_a(), Some(2.0));
    }

    #[test]
    fn test_wear_level_calculation() {
        let mut info = PowerSupplyInfo::new("BAT0");
        info.energy_full_uwh = Some(40_000_000);
        info.energy_full_design_uwh = Some(50_000_000);
        let wear = info.wear_level_percent().unwrap();
        assert!((wear - 80.0).abs() < 0.01);
    }

    #[test]
    fn test_charging_status_display() {
        assert_eq!(ChargingStatus::Charging.to_string(), "Charging");
        assert_eq!(ChargingStatus::Discharging.to_string(), "Discharging");
        assert_eq!(ChargingStatus::Full.to_string(), "Full");
        assert_eq!(ChargingStatus::NotCharging.to_string(), "Not Charging");
        assert_eq!(ChargingStatus::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn test_power_supply_type_display() {
        assert_eq!(PowerSupplyType::Mains.to_string(), "Mains");
        assert_eq!(PowerSupplyType::Battery.to_string(), "Battery");
        assert_eq!(PowerSupplyType::Usb.to_string(), "USB");
        assert_eq!(PowerSupplyType::UsbPd.to_string(), "USB-PD");
        assert_eq!(PowerSupplyType::Ups.to_string(), "UPS");
    }

    #[test]
    fn test_battery_health_display() {
        assert_eq!(BatteryHealth::Good.to_string(), "Good");
        assert_eq!(BatteryHealth::Dead.to_string(), "Dead");
        assert_eq!(BatteryHealth::Overheat.to_string(), "Overheat");
        assert_eq!(BatteryHealth::Cold.to_string(), "Cold");
    }

    #[test]
    fn test_power_supply_serde_roundtrip() {
        let info = PowerSupplyInfo::new("AC0");
        let json = serde_json::to_string(&info).unwrap();
        let parsed: PowerSupplyInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "AC0");
        assert_eq!(parsed.supply_type, PowerSupplyType::Unknown);
    }
}

#[cfg(test)]
mod wmi_battery_tests {
    use super::*;
    use serde_json::json;

    /// A laptop with two packs. The `Full` and `Status` rows are listed in the
    /// opposite order to `Static`, so only an `InstanceName` join gets them right.
    fn two_pack_laptop() -> serde_json::Value {
        json!({
            "Static": [
                {
                    "InstanceName": "ACPI\\PNP0C0A\\1_0",
                    "DesignedCapacity": 56000,
                    "ManufactureName": "SMP  ",
                    "SerialNumber": " 1234 ",
                    "Chemistry": [76, 73, 79, 78]
                },
                {
                    "InstanceName": "ACPI\\PNP0C0A\\2_0",
                    "DesignedCapacity": 30000,
                    "ManufactureName": "",
                    "SerialNumber": "9876",
                    "Chemistry": [76, 73, 80, 79]
                }
            ],
            "Full": [
                {"InstanceName": "ACPI\\PNP0C0A\\2_0", "FullChargedCapacity": 27500},
                {"InstanceName": "ACPI\\PNP0C0A\\1_0", "FullChargedCapacity": 51234}
            ],
            "Status": [
                {
                    "InstanceName": "ACPI\\PNP0C0A\\2_0",
                    "RemainingCapacity": 20000,
                    "Voltage": 12450,
                    "ChargeRate": 0,
                    "DischargeRate": 8000
                },
                {
                    "InstanceName": "ACPI\\PNP0C0A\\1_0",
                    "RemainingCapacity": 40000,
                    "Voltage": 12600,
                    "ChargeRate": 0,
                    "DischargeRate": 15000
                }
            ],
            "Cycle": [
                {"InstanceName": "ACPI\\PNP0C0A\\1_0", "CycleCount": 213},
                {"InstanceName": "ACPI\\PNP0C0A\\2_0", "CycleCount": 4}
            ]
        })
    }

    #[test]
    fn wmi_fields_are_joined_by_instance_name_and_converted_to_the_struct_units() {
        let batteries = batteries_from_wmi(&two_pack_laptop());
        assert_eq!(batteries.len(), 2);

        let a = &batteries[0];
        assert_eq!(a.name, "ACPI\\PNP0C0A\\1_0");
        assert_eq!(a.supply_type, PowerSupplyType::Battery);
        assert_eq!(a.energy_full_design_uwh, Some(56_000_000));
        assert_eq!(a.energy_full_uwh, Some(51_234_000));
        assert_eq!(a.energy_now_uwh, Some(40_000_000));
        assert_eq!(a.voltage_now_mv, Some(12600));
        assert_eq!(a.power_now_mw, Some(15000));
        assert_eq!(a.cycle_count, Some(213));
        assert_eq!(a.manufacturer.as_deref(), Some("SMP"));
        assert_eq!(a.serial_number.as_deref(), Some("1234"));
        assert_eq!(a.technology.as_deref(), Some("Li-ion"));

        let b = &batteries[1];
        assert_eq!(b.energy_full_uwh, Some(27_500_000));
        assert_eq!(b.energy_now_uwh, Some(20_000_000));
        assert_eq!(b.power_now_mw, Some(8000));
        assert_eq!(b.cycle_count, Some(4));
        assert_eq!(b.technology.as_deref(), Some("LiPo"));
        assert_eq!(b.manufacturer, None, "a blank manufacturer is not a name");
    }

    #[test]
    fn a_desktop_with_no_battery_instances_has_no_batteries() {
        let desktop = json!({"Static": [], "Full": [], "Status": [], "Cycle": []});
        assert!(batteries_from_wmi(&desktop).is_empty());
        // A single object is serialised without the array, and still a list.
        let single = json!({
            "Static": {"InstanceName": "X", "DesignedCapacity": 1},
            "Full": [], "Status": [], "Cycle": []
        });
        assert_eq!(batteries_from_wmi(&single).len(), 1);
        assert!(batteries_from_wmi(&json!({})).is_empty());
    }

    #[test]
    fn unknown_markers_and_missing_rows_are_withheld_not_zeroed() {
        let json = json!({
            "Static": [{
                "InstanceName": "pack",
                "DesignedCapacity": 0,
                "ManufactureName": "",
                "SerialNumber": "",
                "Chemistry": [88, 89, 90, 0]
            }],
            "Full": [],
            "Status": [{
                "InstanceName": "pack",
                "RemainingCapacity": 4294967295u64,
                "Voltage": 4294967295u64,
                "ChargeRate": 0,
                "DischargeRate": 2147483648u64
            }],
            "Cycle": []
        });
        let b = &batteries_from_wmi(&json)[0];
        assert_eq!(b.energy_full_design_uwh, None, "0 is not a capacity");
        assert_eq!(b.energy_full_uwh, None, "no FullChargedCapacity row");
        assert_eq!(b.energy_now_uwh, None, "0xFFFFFFFF is unknown");
        assert_eq!(b.voltage_now_mv, None, "0xFFFFFFFF is unknown");
        assert_eq!(b.power_now_mw, None, "an unknown rate is not 0 W");
        assert_eq!(b.cycle_count, None, "no CycleCount row");
        assert_eq!(
            b.technology, None,
            "an unlisted chemistry code is not guessed"
        );
        assert_eq!(b.manufacturer, None);
        assert_eq!(b.serial_number, None);
    }

    #[test]
    fn the_rate_is_the_non_zero_direction_and_idle_is_zero() {
        let n = |v: u64| json!(v);
        assert_eq!(wmi_rate(Some(&n(8000)), Some(&n(0))), Some(8000));
        assert_eq!(wmi_rate(Some(&n(0)), Some(&n(3000))), Some(3000));
        assert_eq!(wmi_rate(Some(&n(0)), Some(&n(0))), Some(0), "idle on AC");
        assert_eq!(wmi_rate(None, None), None);
        assert_eq!(wmi_rate(Some(&n(0)), None), None);
    }

    #[test]
    fn a_status_battery_joins_the_one_wmi_battery_and_no_more() {
        let mut status = PowerSupplyInfo::new("Battery");
        status.supply_type = PowerSupplyType::Battery;
        status.capacity_percent = Some(73);
        status.status = Some(ChargingStatus::Discharging);
        status.time_to_empty_min = Some(190);

        let wmi = batteries_from_wmi(&two_pack_laptop());
        let one = combine_windows_batteries(Some(status.clone()), vec![wmi[0].clone()]);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].capacity_percent, Some(73));
        assert_eq!(one[0].energy_full_uwh, Some(51_234_000), "WMI fields kept");

        // Two packs: the combined percentage is not either pack's, so neither gets it.
        let two = combine_windows_batteries(Some(status.clone()), wmi.clone());
        assert_eq!(two.len(), 2);
        assert!(two.iter().all(|b| b.capacity_percent.is_none()));

        // No WMI battery: the status API's battery stands as it did before.
        let alone = combine_windows_batteries(Some(status), Vec::new());
        assert_eq!(alone.len(), 1);
        assert_eq!(alone[0].capacity_percent, Some(73));
        assert_eq!(alone[0].energy_full_uwh, None);

        assert!(combine_windows_batteries(None, Vec::new()).is_empty());
    }
}
