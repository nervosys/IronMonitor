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
//! use ironmonlib::power_supply::{PowerSupplyMonitor, PowerSupplyType};
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

use crate::error::Result;
// Only the Linux enumeration path constructs an error value directly.
#[cfg(target_os = "linux")]
use crate::error::IronError;
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
    /// Charging status (for batteries)
    pub status: ChargingStatus,
    /// Battery health (for batteries)
    pub health: BatteryHealth,
    /// Current capacity percentage (0-100)
    pub capacity_percent: Option<u8>,
    /// Voltage now in millivolts
    pub voltage_now_mv: Option<u32>,
    /// Voltage minimum design in millivolts
    pub voltage_min_mv: Option<u32>,
    /// Current now in microamps (positive = charging, negative = discharging)
    pub current_now_ua: Option<i32>,
    /// Power now in microwatts
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
            status: ChargingStatus::Unknown,
            health: BatteryHealth::Unknown,
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

        if !power_supply_path.exists() {
            return Ok(supplies);
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

            // Read online status
            if let Ok(online_str) = fs::read_to_string(path.join("online")) {
                info.online = Some(online_str.trim() == "1");
            }

            // Read status
            if let Ok(status_str) = fs::read_to_string(path.join("status")) {
                info.status = match status_str.trim().to_lowercase().as_str() {
                    "charging" => ChargingStatus::Charging,
                    "discharging" => ChargingStatus::Discharging,
                    "full" => ChargingStatus::Full,
                    "not charging" => ChargingStatus::NotCharging,
                    _ => ChargingStatus::Unknown,
                };
            }

            // Read health
            if let Ok(health_str) = fs::read_to_string(path.join("health")) {
                info.health = match health_str.trim().to_lowercase().as_str() {
                    "good" => BatteryHealth::Good,
                    "overheat" => BatteryHealth::Overheat,
                    "dead" => BatteryHealth::Dead,
                    "overvoltage" | "over voltage" => BatteryHealth::OverVoltage,
                    "cold" => BatteryHealth::Cold,
                    _ => BatteryHealth::Unknown,
                };
            }

            // Read capacity
            if let Ok(cap_str) = fs::read_to_string(path.join("capacity")) {
                if let Ok(cap) = cap_str.trim().parse::<u8>() {
                    info.capacity_percent = Some(cap.min(100));
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

            // Read time to empty
            if let Ok(time_str) = fs::read_to_string(path.join("time_to_empty_avg")) {
                if let Ok(time) = time_str.trim().parse::<u32>() {
                    info.time_to_empty_min = Some(time);
                }
            }

            // Read time to full
            if let Ok(time_str) = fs::read_to_string(path.join("time_to_full_avg")) {
                if let Ok(time) = time_str.trim().parse::<u32>() {
                    info.time_to_full_min = Some(time);
                }
            }

            supplies.push(info);
        }

        Ok(supplies)
    }

    #[cfg(target_os = "windows")]
    fn enumerate_supplies() -> Result<Vec<PowerSupplyInfo>> {
        let mut supplies = Vec::new();

        // Use Windows API to get battery information
        unsafe {
            use ::windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

            let mut status = SYSTEM_POWER_STATUS::default();
            if GetSystemPowerStatus(&mut status).is_ok() {
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
                    battery_info.online = Some(true);

                    // Capacity
                    if status.BatteryLifePercent != 255 {
                        battery_info.capacity_percent = Some(status.BatteryLifePercent);
                    }

                    // Status
                    if status.BatteryFlag & 8 != 0 {
                        battery_info.status = ChargingStatus::Charging;
                    } else if status.ACLineStatus == 1 && status.BatteryLifePercent == 100 {
                        battery_info.status = ChargingStatus::Full;
                    } else {
                        battery_info.status = ChargingStatus::Discharging;
                    }

                    // Time remaining
                    if status.BatteryLifeTime != 0xFFFFFFFF {
                        battery_info.time_to_empty_min = Some(status.BatteryLifeTime / 60);
                    }

                    if status.BatteryFullLifeTime != 0xFFFFFFFF {
                        battery_info.time_to_full_min = Some(status.BatteryFullLifeTime / 60);
                    }

                    supplies.push(battery_info);
                }
            }
        }

        Ok(supplies)
    }

    #[cfg(target_os = "macos")]
    fn enumerate_supplies() -> Result<Vec<PowerSupplyInfo>> {
        use std::process::Command;

        let mut supplies = Vec::new();

        // Use pmset to get battery info
        if let Ok(output) = Command::new("pmset").args(["-g", "batt"]).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Parse AC adapter
            let mut ac_info = PowerSupplyInfo::new("AC");
            ac_info.supply_type = PowerSupplyType::Mains;
            ac_info.online = Some(stdout.contains("AC Power"));
            supplies.push(ac_info);

            // Parse battery
            for line in stdout.lines() {
                if line.contains("InternalBattery") || line.contains("Battery") {
                    let mut battery_info = PowerSupplyInfo::new("Battery");
                    battery_info.supply_type = PowerSupplyType::Battery;
                    battery_info.online = Some(true);

                    // Parse percentage
                    if let Some(pct_start) = line.find(char::is_numeric) {
                        let pct_str: String = line[pct_start..]
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect();
                        if let Ok(pct) = pct_str.parse::<u8>() {
                            battery_info.capacity_percent = Some(pct.min(100));
                        }
                    }

                    // Parse status
                    if line.contains("charging") && !line.contains("not charging") {
                        battery_info.status = ChargingStatus::Charging;
                    } else if line.contains("discharging") {
                        battery_info.status = ChargingStatus::Discharging;
                    } else if line.contains("charged") {
                        battery_info.status = ChargingStatus::Full;
                    }

                    // Parse time remaining
                    if let Some(time_idx) = line.find(':') {
                        let time_part = &line[time_idx..];
                        if let Some(remaining) = time_part.find("remaining") {
                            let time_str = &time_part[1..remaining].trim();
                            let parts: Vec<&str> = time_str.split(':').collect();
                            if parts.len() == 2 {
                                if let (Ok(hours), Ok(mins)) =
                                    (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                                {
                                    battery_info.time_to_empty_min = Some(hours * 60 + mins);
                                }
                            }
                        }
                    }

                    supplies.push(battery_info);
                    break;
                }
            }
        }

        Ok(supplies)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn enumerate_supplies() -> Result<Vec<PowerSupplyInfo>> {
        Ok(Vec::new())
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
    /// Primary battery status
    pub battery_status: ChargingStatus,
    /// Battery health
    pub battery_health: BatteryHealth,
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
            battery_status: battery.map(|b| b.status).unwrap_or(ChargingStatus::Unknown),
            battery_health: battery.map(|b| b.health).unwrap_or(BatteryHealth::Unknown),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_supply_info_new_defaults() {
        let info = PowerSupplyInfo::new("BAT0");
        assert_eq!(info.name, "BAT0");
        assert_eq!(info.supply_type, PowerSupplyType::Unknown);
        assert_eq!(info.online, None);
        assert_eq!(info.status, ChargingStatus::Unknown);
        assert_eq!(info.health, BatteryHealth::Unknown);
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
