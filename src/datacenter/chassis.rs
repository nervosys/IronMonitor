// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 nervosys

//! Server Chassis Information via DMI/SMBIOS

use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::fs;

/// Chassis type from SMBIOS Type 3
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChassisType {
    Desktop,
    LowProfileDesktop,
    PizzaBox,
    MiniTower,
    Tower,
    Portable,
    Laptop,
    Notebook,
    Handheld,
    DockingStation,
    AllInOne,
    SubNotebook,
    LunchBox,
    MainServer,
    Expansion,
    SubChassis,
    BusExpansion,
    Peripheral,
    RaidChassis,
    RackMount,
    SealedCase,
    MultiSystem,
    CompactPci,
    AdvancedTca,
    Blade,
    BladeEnclosure,
    Tablet,
    Convertible,
    Detachable,
    IoTGateway,
    EmbeddedPc,
    MiniPc,
    StickPc,
    Other,
    Unknown,
}

/// Form factor derived from chassis type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormFactor {
    RackMount,
    Tower,
    Blade,
    Desktop,
    Laptop,
    MiniPc,
    Tablet,
    Embedded,
    Other,
    Unknown,
}

/// Physical location in rack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerLocation {
    pub rack_id: Option<String>,
    pub rack_unit: Option<u32>,
    pub datacenter: Option<String>,
    pub room: Option<String>,
}

/// Complete chassis info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChassisInfo {
    pub chassis_type: ChassisType,
    pub form_factor: FormFactor,
    pub manufacturer: String,
    pub product_name: String,
    pub serial_number: Option<String>,
    pub asset_tag: Option<String>,
    pub version: Option<String>,
    pub sku: Option<String>,
    pub location: ServerLocation,
}

impl ChassisInfo {
    pub fn detect() -> Result<Self, super::DatacenterError> {
        #[cfg(target_os = "linux")]
        {
            Self::detect_linux()
        }
        #[cfg(target_os = "windows")]
        {
            Self::detect_windows()
        }
        #[cfg(target_os = "macos")]
        {
            Self::detect_macos()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            Err(super::DatacenterError::Other("Unsupported platform".into()))
        }
    }

    #[cfg(target_os = "linux")]
    fn detect_linux() -> Result<Self, super::DatacenterError> {
        let dmi = "/sys/class/dmi/id";
        let read = |name: &str| -> String {
            fs::read_to_string(format!("{}/{}", dmi, name))
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let read_opt = |name: &str| -> Option<String> {
            let v = read(name);
            if v.is_empty() || v == "Not Specified" || v == "Default string" {
                None
            } else {
                Some(v)
            }
        };

        let chassis_type_byte: u8 = fs::read_to_string(format!("{}/chassis_type", dmi))
            .unwrap_or_default()
            .trim()
            .parse()
            .unwrap_or(2);

        let chassis_type = Self::from_smbios_byte(chassis_type_byte);
        let form_factor = Self::infer_form_factor(chassis_type, &read("product_name"));

        Ok(ChassisInfo {
            chassis_type,
            form_factor,
            manufacturer: read("sys_vendor"),
            product_name: read("product_name"),
            serial_number: read_opt("product_serial"),
            asset_tag: read_opt("chassis_asset_tag"),
            version: read_opt("product_version"),
            sku: read_opt("product_sku"),
            location: ServerLocation {
                rack_id: std::env::var("IRONMON_RACK_ID").ok(),
                rack_unit: std::env::var("IRONMON_RACK_UNIT")
                    .ok()
                    .and_then(|v| v.parse().ok()),
                datacenter: std::env::var("IRONMON_DATACENTER").ok(),
                room: std::env::var("IRONMON_ROOM").ok(),
            },
        })
    }

    #[cfg(target_os = "windows")]
    fn detect_windows() -> Result<Self, super::DatacenterError> {
        // SMBIOS system identity as the firmware published it at boot. This used to
        // shell out to `wmic`, which Microsoft removed in Windows 11 24H2 — the
        // subprocess failed to spawn and every field came back an empty string, so a
        // named chassis was indistinguishable from an unnamed one.
        const BIOS_KEY: &str = r"HARDWARE\DESCRIPTION\System\BIOS";
        let smbios = |value: &str| -> String {
            crate::platform::windows::read_registry_string(BIOS_KEY, value).unwrap_or_default()
        };

        // The registry carries system make and model but not the serial, which lives
        // only in SMBIOS as WMI surfaces it.
        let wmi_info = crate::motherboard::get_system_info().ok();

        Ok(ChassisInfo {
            chassis_type: ChassisType::Unknown,
            form_factor: FormFactor::Unknown,
            manufacturer: smbios("SystemManufacturer"),
            product_name: smbios("SystemProductName"),
            serial_number: wmi_info
                .as_ref()
                .and_then(|i| i.serial_number.clone())
                .filter(|s| !s.trim().is_empty()),
            asset_tag: None,
            version: None,
            sku: None,
            location: ServerLocation {
                rack_id: None,
                rack_unit: None,
                datacenter: None,
                room: None,
            },
        })
    }

    #[cfg(target_os = "macos")]
    fn detect_macos() -> Result<Self, super::DatacenterError> {
        let sp = std::process::Command::new("system_profiler")
            .args(["SPHardwareDataType", "-detailLevel", "mini"])
            .output()
            .map_err(|e| super::DatacenterError::Other(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&sp.stdout);
        let get = |key: &str| -> String {
            stdout
                .lines()
                .find(|l| l.contains(key))
                .and_then(|l| l.split(':').nth(1))
                .unwrap_or("")
                .trim()
                .to_string()
        };

        Ok(ChassisInfo {
            chassis_type: ChassisType::Unknown,
            form_factor: FormFactor::Desktop,
            manufacturer: "Apple".into(),
            product_name: get("Model Name"),
            serial_number: Some(get("Serial Number")).filter(|s| !s.is_empty()),
            asset_tag: None,
            version: None,
            sku: Some(get("Model Identifier")).filter(|s| !s.is_empty()),
            location: ServerLocation {
                rack_id: None,
                rack_unit: None,
                datacenter: None,
                room: None,
            },
        })
    }

    #[allow(dead_code)]
    fn from_smbios_byte(b: u8) -> ChassisType {
        match b {
            3 => ChassisType::Desktop,
            4 => ChassisType::LowProfileDesktop,
            5 => ChassisType::PizzaBox,
            6 => ChassisType::MiniTower,
            7 => ChassisType::Tower,
            8 => ChassisType::Portable,
            9 => ChassisType::Laptop,
            10 => ChassisType::Notebook,
            11 => ChassisType::Handheld,
            12 => ChassisType::DockingStation,
            13 => ChassisType::AllInOne,
            14 => ChassisType::SubNotebook,
            17 => ChassisType::MainServer,
            23 => ChassisType::RackMount,
            24 => ChassisType::SealedCase,
            25 => ChassisType::MultiSystem,
            28 => ChassisType::Blade,
            29 => ChassisType::BladeEnclosure,
            30 => ChassisType::Tablet,
            31 => ChassisType::Convertible,
            32 => ChassisType::Detachable,
            33 => ChassisType::IoTGateway,
            34 => ChassisType::EmbeddedPc,
            35 => ChassisType::MiniPc,
            36 => ChassisType::StickPc,
            1 => ChassisType::Other,
            _ => ChassisType::Unknown,
        }
    }

    #[allow(dead_code)]
    fn infer_form_factor(ct: ChassisType, product: &str) -> FormFactor {
        let p = product.to_lowercase();
        match ct {
            ChassisType::RackMount | ChassisType::MainServer => FormFactor::RackMount,
            ChassisType::Tower | ChassisType::MiniTower => FormFactor::Tower,
            ChassisType::Blade | ChassisType::BladeEnclosure => FormFactor::Blade,
            ChassisType::Laptop | ChassisType::Notebook | ChassisType::SubNotebook => {
                FormFactor::Laptop
            }
            ChassisType::Tablet | ChassisType::Convertible | ChassisType::Detachable => {
                FormFactor::Tablet
            }
            ChassisType::MiniPc | ChassisType::StickPc => FormFactor::MiniPc,
            ChassisType::EmbeddedPc | ChassisType::IoTGateway => FormFactor::Embedded,
            ChassisType::Desktop | ChassisType::LowProfileDesktop | ChassisType::AllInOne => {
                FormFactor::Desktop
            }
            _ => {
                if p.contains("rack") || p.contains("server") {
                    FormFactor::RackMount
                } else if p.contains("blade") {
                    FormFactor::Blade
                } else if p.contains("nuc") || p.contains("mini") {
                    FormFactor::MiniPc
                } else {
                    FormFactor::Unknown
                }
            }
        }
    }
}
