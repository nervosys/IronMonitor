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
    pub total_units: u32,
    pub used_units: u32,
    pub pdus: Vec<PduInfo>,
    pub cooling_zones: Vec<CoolingZone>,
    pub total_power_watts: f64,
    pub max_power_watts: f64,
    pub tags: std::collections::HashMap<String, String>,
}

impl RackInfo {
    pub fn builder(rack_id: impl Into<String>) -> RackInfoBuilder {
        RackInfoBuilder {
            rack_id: rack_id.into(),
            location: String::new(),
            total_units: 42,
            used_units: 0,
            pdus: Vec::new(),
            cooling_zones: Vec::new(),
            max_power_watts: 0.0,
            tags: std::collections::HashMap::new(),
        }
    }

    /// Draw as a percentage of the rack's power budget, or `None` when no
    /// budget has been configured.
    ///
    /// Both of these returned `0.0` for the unconfigured case, which reads as
    /// "this rack is drawing nothing" — the most reassuring possible answer to
    /// a question nobody supplied the inputs for. `max_power_watts` defaults to
    /// `0.0` in the builder precisely because it is not knowable without being
    /// told, so the unconfigured case is the common one.
    pub fn power_utilization(&self) -> Option<f64> {
        if self.max_power_watts <= 0.0 {
            return None;
        }
        Some((self.total_power_watts / self.max_power_watts) * 100.0)
    }

    /// Occupied units as a percentage of the rack's height, or `None` when the
    /// height is zero. See [`Self::power_utilization`].
    ///
    /// Note that the builder's default height is 42U, the standard full rack,
    /// so a caller who never set one gets a percentage against an assumed size
    /// rather than `None`.
    pub fn space_utilization(&self) -> Option<f64> {
        if self.total_units == 0 {
            return None;
        }
        Some((self.used_units as f64 / self.total_units as f64) * 100.0)
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
    total_units: u32,
    used_units: u32,
    pdus: Vec<PduInfo>,
    cooling_zones: Vec<CoolingZone>,
    max_power_watts: f64,
    tags: std::collections::HashMap<String, String>,
}

impl RackInfoBuilder {
    pub fn location(mut self, loc: impl Into<String>) -> Self {
        self.location = loc.into();
        self
    }

    pub fn total_units(mut self, u: u32) -> Self {
        self.total_units = u;
        self
    }

    pub fn used_units(mut self, u: u32) -> Self {
        self.used_units = u;
        self
    }

    pub fn max_power_watts(mut self, w: f64) -> Self {
        self.max_power_watts = w;
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
        let total_power: f64 = self.pdus.iter().map(|p| p.total_power_watts).sum();
        RackInfo {
            rack_id: self.rack_id,
            location: self.location,
            total_units: self.total_units,
            used_units: self.used_units,
            total_power_watts: total_power,
            max_power_watts: self.max_power_watts,
            pdus: self.pdus,
            cooling_zones: self.cooling_zones,
            tags: self.tags,
        }
    }
}
