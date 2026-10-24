// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 nervosys

//! Rack topology, PDU power, and cooling zone modeling

use serde::{Deserialize, Serialize};

/// Power phase info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerPhase {
    pub phase: u8,
    pub voltage: f64,
    pub current: f64,
    pub power_watts: f64,
}

/// PDU outlet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PduOutlet {
    pub index: u32,
    pub label: String,
    pub state: bool,
    pub current_amps: Option<f64>,
    pub power_watts: Option<f64>,
}

/// Power Distribution Unit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PduInfo {
    pub name: String,
    pub model: Option<String>,
    pub total_power_watts: f64,
    pub max_power_watts: f64,
    pub phases: Vec<PowerPhase>,
    pub outlets: Vec<PduOutlet>,
}

/// Cooling zone status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoolingStatus {
    Normal,
    Elevated,
    Warning,
    Critical,
    Unknown,
}

/// Cooling zone
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoolingZone {
    pub name: String,
    pub status: CoolingStatus,
    pub temperature_celsius: Option<f64>,
    pub target_temperature: Option<f64>,
    pub humidity_percent: Option<f64>,
    pub airflow_cfm: Option<f64>,
}

/// Rack information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RackInfo {
    pub rack_id: String,
    pub location: String,
    /// Rack height in units, or `None` when `IRONMON_RACK_UNITS` was not set.
    /// Used to report 42U, the standard full rack, which nobody configured.
    pub total_units: Option<u32>,
    /// Occupied units, or `None` when `IRONMON_RACK_USED_UNITS` was not set.
    /// Used to report 0, which reads as an empty rack.
    pub used_units: Option<u32>,
    pub pdus: Vec<PduInfo>,
    pub cooling_zones: Vec<CoolingZone>,
    /// Sum of the PDUs' power, or `None` when no PDU is listed. Used to report
    /// 0 W, which reads as a rack drawing nothing.
    pub total_power_watts: Option<f64>,
    /// Power budget, or `None` when `IRONMON_RACK_MAX_POWER` was not set. Used
    /// to report 0 W, which made every unconfigured budget look like one.
    pub max_power_watts: Option<f64>,
    pub tags: std::collections::HashMap<String, String>,
}

impl RackInfo {
    pub fn builder(rack_id: impl Into<String>) -> RackInfoBuilder {
        RackInfoBuilder {
            rack_id: rack_id.into(),
            location: String::new(),
            total_units: None,
            used_units: None,
            pdus: Vec::new(),
            cooling_zones: Vec::new(),
            max_power_watts: None,
            tags: std::collections::HashMap::new(),
        }
    }

    /// Draw as a percentage of the rack's power budget, or `None` when either
    /// the budget or the draw was not read.
    pub fn power_utilization(&self) -> Option<f64> {
        let max = self.max_power_watts.filter(|w| *w > 0.0)?;
        let total = self.total_power_watts?;
        Some((total / max) * 100.0)
    }

    /// Occupied units as a percentage of the rack's height, or `None` when
    /// either the height or the occupancy was not configured.
    pub fn space_utilization(&self) -> Option<f64> {
        let total = self.total_units.filter(|&t| t > 0)?;
        let used = self.used_units?;
        Some((used as f64 / total as f64) * 100.0)
    }

    /// Detect from environment variables
    pub fn from_env() -> Option<Self> {
        let rack_id = std::env::var("IRONMON_RACK_ID").ok()?;
        let mut builder = Self::builder(rack_id);

        if let Ok(loc) = std::env::var("IRONMON_RACK_LOCATION") {
            builder = builder.location(loc);
        }
        if let Ok(units) = std::env::var("IRONMON_RACK_UNITS") {
            if let Ok(u) = units.parse() {
                builder = builder.total_units(u);
            }
        }
        if let Ok(used) = std::env::var("IRONMON_RACK_USED_UNITS") {
            if let Ok(u) = used.parse() {
                builder = builder.used_units(u);
            }
        }
        if let Ok(max) = std::env::var("IRONMON_RACK_MAX_POWER") {
            if let Ok(p) = max.parse() {
                builder = builder.max_power_watts(p);
            }
        }
        Some(builder.build())
    }
}

pub struct RackInfoBuilder {
    rack_id: String,
    location: String,
    total_units: Option<u32>,
    used_units: Option<u32>,
    pdus: Vec<PduInfo>,
    cooling_zones: Vec<CoolingZone>,
    max_power_watts: Option<f64>,
    tags: std::collections::HashMap<String, String>,
}

impl RackInfoBuilder {
    pub fn location(mut self, loc: impl Into<String>) -> Self {
        self.location = loc.into();
        self
    }

    pub fn total_units(mut self, u: u32) -> Self {
        self.total_units = Some(u);
        self
    }

    pub fn used_units(mut self, u: u32) -> Self {
        self.used_units = Some(u);
        self
    }

    pub fn max_power_watts(mut self, w: f64) -> Self {
        self.max_power_watts = Some(w);
        self
    }

    pub fn add_pdu(mut self, pdu: PduInfo) -> Self {
        self.pdus.push(pdu);
        self
    }

    pub fn add_cooling_zone(mut self, zone: CoolingZone) -> Self {
        self.cooling_zones.push(zone);
        self
    }

    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> RackInfo {
        // No PDU means no draw reading, not a draw of zero.
        let total_power_watts =
            (!self.pdus.is_empty()).then(|| self.pdus.iter().map(|p| p.total_power_watts).sum());
        RackInfo {
            rack_id: self.rack_id,
            location: self.location,
            total_units: self.total_units,
            used_units: self.used_units,
            total_power_watts,
            max_power_watts: self.max_power_watts,
            pdus: self.pdus,
            cooling_zones: self.cooling_zones,
            tags: self.tags,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pdu(watts: f64) -> PduInfo {
        PduInfo {
            name: "pdu0".into(),
            model: None,
            total_power_watts: watts,
            max_power_watts: 0.0,
            phases: Vec::new(),
            outlets: Vec::new(),
        }
    }

    #[test]
    fn an_unconfigured_rack_has_no_units_or_power() {
        let rack = RackInfo::builder("r1").build();
        assert_eq!(rack.total_units, None);
        assert_eq!(rack.used_units, None);
        assert_eq!(rack.total_power_watts, None);
        assert_eq!(rack.max_power_watts, None);
        assert_eq!(rack.space_utilization(), None);
        assert_eq!(rack.power_utilization(), None);
    }

    #[test]
    fn a_height_without_occupancy_is_not_an_empty_rack() {
        let rack = RackInfo::builder("r1").total_units(42).build();
        assert_eq!(rack.space_utilization(), None);
    }

    #[test]
    fn a_budget_without_a_pdu_reading_is_not_a_zero_draw() {
        let rack = RackInfo::builder("r1").max_power_watts(10_000.0).build();
        assert_eq!(rack.total_power_watts, None);
        assert_eq!(rack.power_utilization(), None);

        let rack = RackInfo::builder("r1")
            .max_power_watts(10_000.0)
            .add_pdu(pdu(2_500.0))
            .build();
        assert_eq!(rack.power_utilization(), Some(25.0));
    }

    #[test]
    fn configured_units_give_a_percentage() {
        let rack = RackInfo::builder("r1")
            .total_units(42)
            .used_units(21)
            .build();
        assert_eq!(rack.space_utilization(), Some(50.0));
    }
}
