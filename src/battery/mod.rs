//! Battery monitoring module
//!
//! Provides cross-platform battery information by delegating to the PowerSupplyMonitor.
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Battery charging state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargingState {
    Charging,
    Discharging,
    Full,
    NotCharging,
    Unknown,
}

/// Battery health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatteryHealth {
    Good,
    Fair,
    Poor,
    Unknown,
}

/// Battery information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    pub name: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub technology: Option<String>,
    pub serial_number: Option<String>,
    /// State of charge, or `None` where it was not read.
    ///
    /// This was `f32` and an unread charge became `0.0`, via an
    /// `unwrap_or(0)` over the `Option<u8>` that `power_supply.rs` models
    /// correctly. **Zero is not a neutral wrong answer here**: it means "about
    /// to shut down", and `power.battery.percentage` published it as a
    /// measurement, so an agent asked whether it could start a long job on a
    /// laptop with an unreadable gauge would have been told the battery was
    /// flat.
    pub charge_percent: Option<f32>,
    pub state: ChargingState,
    pub health: BatteryHealth,
    pub voltage_mv: Option<u32>,
    pub power_w: Option<f32>,
    pub energy_wh: Option<f32>,
    pub energy_full_wh: Option<f32>,
    pub energy_full_design_wh: Option<f32>,
    pub time_to_empty: Option<Duration>,
    pub time_to_full: Option<Duration>,
    pub cycle_count: Option<u32>,
    pub temperature_c: Option<f32>,
    pub wear_level_percent: Option<f32>,
}

/// Battery monitor
///
/// `Default` is derived now that `ac_connected` is an `Option`: the hand-written
/// impl existed to seed it with `true`, an unread machine reported as plugged in.
#[derive(Default)]
pub struct BatteryMonitor {
    batteries: Vec<BatteryInfo>,
    /// Whether mains power is connected, or `None` if that was not read.
    ac_connected: Option<bool>,
}

impl BatteryMonitor {
    pub fn new() -> Result<Self, crate::error::IronError> {
        let mut monitor = Self {
            batteries: Vec::new(),
            ac_connected: None,
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    pub fn refresh(&mut self) -> Result<(), crate::error::IronError> {
        self.batteries.clear();
        self.ac_connected = None;

        // Delegate to the comprehensive PowerSupplyMonitor
        if let Ok(ps_monitor) = crate::power_supply::PowerSupplyMonitor::new() {
            self.ac_connected = ps_monitor.on_ac_power();

            for supply in ps_monitor.supplies() {
                if !supply.is_battery() {
                    continue;
                }

                let state = match supply.status {
                    crate::power_supply::ChargingStatus::Charging => ChargingState::Charging,
                    crate::power_supply::ChargingStatus::Discharging => ChargingState::Discharging,
                    crate::power_supply::ChargingStatus::Full => ChargingState::Full,
                    crate::power_supply::ChargingStatus::NotCharging => ChargingState::NotCharging,
                    crate::power_supply::ChargingStatus::Unknown => ChargingState::Unknown,
                };

                let health = match supply.health {
                    crate::power_supply::BatteryHealth::Good => BatteryHealth::Good,
                    crate::power_supply::BatteryHealth::Dead
                    | crate::power_supply::BatteryHealth::UnspecifiedFailure => BatteryHealth::Poor,
                    crate::power_supply::BatteryHealth::Overheat
                    | crate::power_supply::BatteryHealth::OverVoltage
                    | crate::power_supply::BatteryHealth::Cold => BatteryHealth::Fair,
                    _ => BatteryHealth::Unknown,
                };

                self.batteries.push(BatteryInfo {
                    name: supply.name.clone(),
                    manufacturer: supply.manufacturer.clone(),
                    model: supply.model_name.clone(),
                    technology: supply.technology.clone(),
                    serial_number: supply.serial_number.clone(),
                    // The `Option` that `power_supply` went to the trouble of
                    // producing, kept rather than flattened to a sentinel.
                    charge_percent: supply.capacity_percent.map(f32::from),
                    state,
                    health,
                    voltage_mv: supply.voltage_now_mv,
                    power_w: supply.power_w(),
                    energy_wh: supply.energy_wh(),
                    energy_full_wh: supply.energy_full_wh(),
                    energy_full_design_wh: supply.energy_full_design_wh(),
                    time_to_empty: supply
                        .time_to_empty_min
                        .map(|m| Duration::from_secs(m as u64 * 60)),
                    time_to_full: supply
                        .time_to_full_min
                        .map(|m| Duration::from_secs(m as u64 * 60)),
                    cycle_count: supply.cycle_count,
                    temperature_c: supply.temperature_celsius(),
                    wear_level_percent: supply.wear_level_percent(),
                });
            }
        }

        Ok(())
    }

    pub fn batteries(&self) -> &[BatteryInfo] {
        &self.batteries
    }
    pub fn ac_connected(&self) -> Option<bool> {
        self.ac_connected
    }
    pub fn has_battery(&self) -> bool {
        !self.batteries.is_empty()
    }
    pub fn primary_battery(&self) -> Option<&BatteryInfo> {
        self.batteries.first()
    }
}
