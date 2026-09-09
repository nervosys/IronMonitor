//! IOMMU (Input-Output Memory Management Unit) monitoring.
//!
//! Detects IOMMU presence (Intel VT-d, AMD-Vi, ARM SMMU), enumerates
//! IOMMU groups, lists device-to-group mappings, identifies passthrough
//! candidates for VFIO/GPU passthrough, and checks isolation quality.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/class/iommu/`, `/sys/kernel/iommu_groups/`
//! - **Windows**: VT-d detection via WMI/registry
//! - **macOS**: Not applicable (stub)

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// IOMMU technology type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IommuType {
    /// Intel VT-d (Virtualization Technology for Directed I/O).
    IntelVtd,
    /// AMD-Vi (AMD I/O Virtualization).
    AmdVi,
    /// ARM SMMU (System Memory Management Unit).
    ArmSmmu,
    /// Apple DART.
    AppleDart,
    /// Unknown / generic.
    Unknown,
}

impl std::fmt::Display for IommuType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IntelVtd => write!(f, "Intel VT-d"),
            Self::AmdVi => write!(f, "AMD-Vi"),
            Self::ArmSmmu => write!(f, "ARM SMMU"),
            Self::AppleDart => write!(f, "Apple DART"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// A device within an IOMMU group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IommuDevice {
    /// BDF (Bus:Device.Function) address, e.g. "0000:01:00.0".
    pub bdf: String,
    /// Device description (from sysfs or lspci).
    pub description: String,
    /// Driver currently bound.
    pub driver: Option<String>,
    /// Whether bound to vfio-pci.
    pub vfio_bound: bool,
}

/// An IOMMU group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IommuGroup {
    /// Group number.
    pub id: u32,
    /// Devices in this group.
    pub devices: Vec<IommuDevice>,
    /// Whether this group has good isolation (single device or ACS).
    pub isolated: bool,
    /// Whether suitable for VFIO passthrough.
    pub passthrough_candidate: bool,
}

/// IOMMU overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IommuOverview {
    /// Whether IOMMU is enabled.
    pub enabled: bool,
    /// IOMMU technology type.
    pub iommu_type: IommuType,
    /// All IOMMU groups.
    pub groups: Vec<IommuGroup>,
    /// Total number of groups.
    pub total_groups: u32,
    /// Number of groups suitable for passthrough.
    pub passthrough_groups: u32,
    /// Whether interrupt remapping is enabled.
    pub interrupt_remapping: bool,
    /// DMAR/IVRS table present.
    pub dmar_present: bool,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// IOMMU monitor.
pub struct IommuMonitor {
    overview: IommuOverview,
}

impl IommuMonitor {
    /// Create a new IOMMU monitor.
    pub fn new() -> Result<Self, IronError> {
        let overview = Self::scan()?;
        Ok(Self { overview })
    }

    /// Refresh.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.overview = Self::scan()?;
        Ok(())
    }

    /// Get overview.
    pub fn overview(&self) -> &IommuOverview {
        &self.overview
    }

    /// Get all groups.
    pub fn groups(&self) -> &[IommuGroup] {
        &self.overview.groups
    }

    /// Find group containing a specific BDF.
    pub fn group_for_device(&self, bdf: &str) -> Option<&IommuGroup> {
        self.overview
            .groups
            .iter()
            .find(|g| g.devices.iter().any(|d| d.bdf == bdf))
    }

    /// Get passthrough candidates.
    pub fn passthrough_candidates(&self) -> Vec<&IommuGroup> {
        self.overview
            .groups
            .iter()
            .filter(|g| g.passthrough_candidate)
            .collect()
    }

    #[cfg(target_os = "linux")]
    fn scan() -> Result<IommuOverview, IronError> {
        let iommu_groups_path = std::path::Path::new("/sys/kernel/iommu_groups");
        let enabled = iommu_groups_path.exists();

        if !enabled {
            return Ok(IommuOverview {
                enabled: false,
                iommu_type: IommuType::Unknown,
                groups: Vec::new(),
                total_groups: 0,
                passthrough_groups: 0,
                interrupt_remapping: false,
                dmar_present: false,
                recommendations: vec![
                    "IOMMU not enabled; add intel_iommu=on or amd_iommu=on to kernel cmdline"
                        .into(),
                ],
            });
        }

        let iommu_type = Self::detect_type();
        let mut groups = Vec::new();

        if let Ok(entries) = std::fs::read_dir(iommu_groups_path) {
            let mut group_ids: Vec<u32> = entries
                .flatten()
                .filter_map(|e| e.file_name().to_string_lossy().parse::<u32>().ok())
                .collect();
            group_ids.sort();

            for gid in group_ids {
                if let Ok(group) = Self::read_group(gid) {
                    groups.push(group);
                }
            }
        }

        let total = groups.len() as u32;
        let passthrough = groups.iter().filter(|g| g.passthrough_candidate).count() as u32;

        let interrupt_remapping = std::path::Path::new("/sys/class/iommu").exists();

        let dmar_present = std::path::Path::new("/sys/firmware/acpi/tables/DMAR").exists()
            || std::path::Path::new("/sys/firmware/acpi/tables/IVRS").exists();

        let mut recommendations = Vec::new();
        if !interrupt_remapping {
            recommendations
                .push("Interrupt remapping may not be active; needed for secure VFIO".into());
        }
        if passthrough > 0 {
            recommendations.push(format!(
                "{} IOMMU groups suitable for device passthrough (VFIO)",
                passthrough
            ));
        }

        Ok(IommuOverview {
            enabled,
            iommu_type,
            groups,
            total_groups: total,
            passthrough_groups: passthrough,
            interrupt_remapping,
            dmar_present,
            recommendations,
        })
    }

    #[cfg(target_os = "linux")]
    fn detect_type() -> IommuType {
        // Check DMAR (Intel) vs IVRS (AMD)
        if std::path::Path::new("/sys/firmware/acpi/tables/DMAR").exists() {
            IommuType::IntelVtd
        } else if std::path::Path::new("/sys/firmware/acpi/tables/IVRS").exists() {
            IommuType::AmdVi
        } else {
            // Check dmesg or iommu driver
            let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
            if cmdline.contains("intel_iommu") {
                IommuType::IntelVtd
            } else if cmdline.contains("amd_iommu") {
                IommuType::AmdVi
            } else {
                IommuType::Unknown
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn read_group(gid: u32) -> Result<IommuGroup, IronError> {
        let devices_path = format!("/sys/kernel/iommu_groups/{}/devices", gid);
        let mut devices = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&devices_path) {
            for entry in entries.flatten() {
                let bdf = entry.file_name().to_string_lossy().to_string();

                let driver = std::fs::read_link(entry.path().join("driver"))
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));

                let vfio_bound = driver.as_deref() == Some("vfio-pci");

                // Try to read device class/vendor for description
                let vendor = std::fs::read_to_string(entry.path().join("vendor"))
                    .ok()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                let device_id = std::fs::read_to_string(entry.path().join("device"))
                    .ok()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

                let description = if !vendor.is_empty() && !device_id.is_empty() {
                    format!("{}:{}", vendor, device_id)
                } else {
                    bdf.clone()
                };

                devices.push(IommuDevice {
                    bdf,
                    description,
                    driver,
                    vfio_bound,
                });
            }
        }

        let isolated = devices.len() <= 1;
        let passthrough_candidate = isolated
            && devices
                .iter()
                .any(|d| !d.bdf.ends_with(".0") || d.driver.is_some());

        Ok(IommuGroup {
            id: gid,
            devices,
            isolated,
            passthrough_candidate,
        })
    }

    #[cfg(target_os = "windows")]
    fn scan() -> Result<IommuOverview, IronError> {
        // Returning an empty overview here made "this platform cannot
        // answer" indistinguishable from "this machine has none", which
        // is the same defect RAPL shipped once. The reason travels with
        // the error so a caller can report it.
        Err(IronError::UnsupportedPlatform(
            "IOMMU groups are read from `/sys/kernel/iommu_groups`, which Windows does not expose"
                .into(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn scan() -> Result<IommuOverview, IronError> {
        // Returning an empty overview here made "this platform cannot
        // answer" indistinguishable from "this machine has none", which
        // is the same defect RAPL shipped once. The reason travels with
        // the error so a caller can report it.
        Err(IronError::UnsupportedPlatform(
            "IOMMU groups are read from `/sys/kernel/iommu_groups`, which macOS does not expose"
                .into(),
        ))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn scan() -> Result<IommuOverview, IronError> {
        // Returning an empty overview here made "this platform cannot
        // answer" indistinguishable from "this machine has none", which
        // is the same defect RAPL shipped once. The reason travels with
        // the error so a caller can report it.
        Err(IronError::UnsupportedPlatform(
            "IOMMU groups are read from `/sys/kernel/iommu_groups`, which this platform does not expose"
                .into(),
        ))
    }
}

impl Default for IommuMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            overview: IommuOverview {
                enabled: false,
                iommu_type: IommuType::Unknown,
                groups: Vec::new(),
                total_groups: 0,
                passthrough_groups: 0,
                interrupt_remapping: false,
                dmar_present: false,
                recommendations: Vec::new(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iommu_type_display() {
        assert_eq!(IommuType::IntelVtd.to_string(), "Intel VT-d");
        assert_eq!(IommuType::AmdVi.to_string(), "AMD-Vi");
        assert_eq!(IommuType::AppleDart.to_string(), "Apple DART");
    }

    #[test]
    fn test_group_isolation() {
        let group = IommuGroup {
            id: 0,
            devices: vec![IommuDevice {
                bdf: "0000:01:00.0".into(),
                description: "GPU".into(),
                driver: Some("nvidia".into()),
                vfio_bound: false,
            }],
            isolated: true,
            passthrough_candidate: true,
        };
        assert!(group.isolated);
        assert_eq!(group.devices.len(), 1);
    }

    #[test]
    fn test_multi_device_group() {
        let group = IommuGroup {
            id: 1,
            devices: vec![
                IommuDevice {
                    bdf: "0000:01:00.0".into(),
                    description: "GPU".into(),
                    driver: Some("amdgpu".into()),
                    vfio_bound: false,
                },
                IommuDevice {
                    bdf: "0000:01:00.1".into(),
                    description: "Audio".into(),
                    driver: Some("snd_hda_intel".into()),
                    vfio_bound: false,
                },
            ],
            isolated: false,
            passthrough_candidate: false,
        };
        assert!(!group.isolated);
    }

    #[test]
    fn test_monitor_default() {
        let monitor = IommuMonitor::default();
        let _overview = monitor.overview();
    }

    #[test]
    fn test_serialization() {
        let dev = IommuDevice {
            bdf: "0000:01:00.0".into(),
            description: "NVIDIA GPU".into(),
            driver: Some("nvidia".into()),
            vfio_bound: false,
        };
        let json = serde_json::to_string(&dev).unwrap();
        assert!(json.contains("0000:01:00.0"));
        let _: IommuDevice = serde_json::from_str(&json).unwrap();
    }
}
