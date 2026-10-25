// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 nervosys

//! Datacenter infrastructure monitoring
//!
//! Provides IPMI/BMC sensor reading, server chassis information via DMI/SMBIOS,
//! rack topology visualization, PDU power monitoring, and cooling zone tracking.

use serde::{Deserialize, Serialize};

pub mod chassis;
pub mod ipmi;
pub mod rack;

pub(crate) use chassis::is_firmware_placeholder;
pub use chassis::{ChassisInfo, ChassisType, FormFactor, ServerLocation};
pub use ipmi::{BmcInfo, IpmiController, IpmiSensor, IpmiSensorType, PowerReading, SelEntry};
pub use rack::{CoolingStatus, CoolingZone, PduInfo, PduOutlet, PowerPhase, RackInfo};

/// Error type for datacenter operations
#[derive(Debug)]
pub enum DatacenterError {
    IpmiError(String),
    DmiError(String),
    IoError(std::io::Error),
    Other(String),
}

impl std::fmt::Display for DatacenterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IpmiError(e) => write!(f, "IPMI error: {}", e),
            Self::DmiError(e) => write!(f, "DMI error: {}", e),
            Self::IoError(e) => write!(f, "I/O error: {}", e),
            Self::Other(e) => write!(f, "Datacenter error: {}", e),
        }
    }
}

impl std::error::Error for DatacenterError {}

impl From<std::io::Error> for DatacenterError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

/// Sensor status level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SensorStatus {
    Ok,
    Warning,
    Critical,
    NonRecoverable,
    Unknown,
}

/// Snapshot of all datacenter monitoring data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatacenterSnapshot {
    pub chassis: Option<ChassisInfo>,
    /// `None` when the IPMI read failed, which is not the same as `Some` of an
    /// empty list: that means the read succeeded and found no sensors.
    pub ipmi_sensors: Option<Vec<IpmiSensor>>,
    pub bmc: Option<BmcInfo>,
    pub power: Option<PowerReading>,
    pub rack: Option<RackInfo>,
}

/// Main datacenter monitoring interface
pub struct DatacenterMonitor {
    ipmi: IpmiController,
}

impl DatacenterMonitor {
    pub fn new() -> Result<Self, DatacenterError> {
        Ok(Self {
            ipmi: IpmiController::new(),
        })
    }

    pub fn chassis_info(&self) -> Result<ChassisInfo, DatacenterError> {
        ChassisInfo::detect()
    }

    pub fn ipmi_sensors(&self) -> Result<Vec<IpmiSensor>, DatacenterError> {
        self.ipmi.read_sensors()
    }

    pub fn bmc_info(&self) -> Result<BmcInfo, DatacenterError> {
        self.ipmi.bmc_info()
    }

    pub fn power_reading(&self) -> Result<PowerReading, DatacenterError> {
        self.ipmi.power_reading()
    }

    pub fn snapshot(&self) -> DatacenterSnapshot {
        DatacenterSnapshot {
            chassis: self.chassis_info().ok(),
            // A failed read is unreadable, not an empty sensor list.
            ipmi_sensors: self.ipmi_sensors().ok(),
            bmc: self.bmc_info().ok(),
            power: self.power_reading().ok(),
            rack: RackInfo::from_env(),
        }
    }
}
