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
    /// Whether devices in this group are isolated from one another.
    ///
    /// `Some(true)` only for a single-device group, where isolation is definitional.
    /// `None` for a multi-device group: whether they are isolated depends on PCIe
    /// ACS, which this reader does not read. It used to be `devices.len() <= 1`,
    /// which reported a multi-device group as `false` as if ACS had been checked.
    pub isolated: Option<bool>,
    /// Whether suitable for VFIO passthrough. Follows `isolated`: `None` where
    /// isolation is unknown, so such a group is not counted as a candidate.
    pub passthrough_candidate: Option<bool>,
}

/// Isolation and passthrough suitability for a group's devices. Only a
/// single-device group is known to be isolated; for any other group the answer
/// depends on ACS, which is not read, so both values are `None`.
#[cfg(any(target_os = "linux", test))]
fn group_isolation(devices: &[IommuDevice]) -> (Option<bool>, Option<bool>) {
    if devices.len() != 1 {
        return (None, None);
    }
    let candidate = !devices[0].bdf.ends_with(".0") || devices[0].driver.is_some();
    (Some(true), Some(candidate))
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
    /// Whether interrupt remapping is enabled, or `None` where it was not read.
    ///
    /// This was `/sys/class/iommu` existing, which says an IOMMU driver is
    /// loaded and nothing about remapping, and a `false` recommendation
    /// ("interrupt remapping may not be active") was derived from it. Remapping
    /// is only readable from the IOMMU's own registers or the kernel log, which
    /// this reader does not consult, so the field is unknown.
    pub interrupt_remapping: Option<bool>,
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
            .filter(|g| g.passthrough_candidate == Some(true))
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
                // No IOMMU, so there is no remapping state to read.
                interrupt_remapping: None,
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
        let passthrough = groups
            .iter()
            .filter(|g| g.passthrough_candidate == Some(true))
            .count() as u32;

        // Not read. See the field's doc: `/sys/class/iommu` existing is not
        // evidence of remapping, so no value is inferred from it.
        let interrupt_remapping: Option<bool> = None;

        let dmar_present = std::path::Path::new("/sys/firmware/acpi/tables/DMAR").exists()
            || std::path::Path::new("/sys/firmware/acpi/tables/IVRS").exists();

        let mut recommendations = Vec::new();
        // Only a reading of `false` can say remapping is off, so only that
        // reading produces this advice.
        if interrupt_remapping == Some(false) {
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
        // The ACPI tables name the technology directly: DMAR is Intel, IVRS is AMD.
        if std::path::Path::new("/sys/firmware/acpi/tables/DMAR").exists() {
            return IommuType::IntelVtd;
        }
        if std::path::Path::new("/sys/firmware/acpi/tables/IVRS").exists() {
            return IommuType::AmdVi;
        }

        // With no table, the driver that bound each IOMMU unit publishes its own
        // attribute directory under /sys/class/iommu/<unit>/. The kernel command
        // line was matched here before, which also matched `intel_iommu=off`
        // and so named a technology from a setting that turns it off.
        let mut driver_dirs: Vec<&str> = Vec::new();
        if let Ok(units) = std::fs::read_dir("/sys/class/iommu") {
            for unit in units.flatten() {
                for name in ["intel-iommu", "amd-iommu"] {
                    if unit.path().join(name).is_dir() {
                        driver_dirs.push(name);
                    }
                }
            }
        }
        iommu_type_from_driver_dirs(&driver_dirs)
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

        let (isolated, passthrough_candidate) = group_isolation(&devices);

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

/// The technology named by the IOMMU driver directories under
/// `/sys/class/iommu/<unit>/`, given as their names.
///
/// `Unknown` unless exactly one technology's directory is present. Both present
/// is not a case a single-vendor machine produces, so it is not guessed at.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn iommu_type_from_driver_dirs(names: &[&str]) -> IommuType {
    let intel = names.contains(&"intel-iommu");
    let amd = names.contains(&"amd-iommu");
    match (intel, amd) {
        (true, false) => IommuType::IntelVtd,
        (false, true) => IommuType::AmdVi,
        _ => IommuType::Unknown,
    }
}

// There is no `Default` for `IommuMonitor`. Its `Default` built an overview that
// said `enabled: false` and no IOMMU type, which reads as "no IOMMU" when the
// reader simply failed. Callers use `new()` and see the error.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_driver_directory_names_its_technology() {
        assert_eq!(
            iommu_type_from_driver_dirs(&["intel-iommu"]),
            IommuType::IntelVtd
        );
        assert_eq!(
            iommu_type_from_driver_dirs(&["amd-iommu"]),
            IommuType::AmdVi
        );
    }

    #[test]
    fn no_driver_directory_is_unknown_not_a_guess() {
        assert_eq!(iommu_type_from_driver_dirs(&[]), IommuType::Unknown);
    }

    #[test]
    fn two_technologies_are_unknown() {
        assert_eq!(
            iommu_type_from_driver_dirs(&["intel-iommu", "amd-iommu"]),
            IommuType::Unknown
        );
    }

    /// `intel_iommu=off` in the kernel command line is not an Intel IOMMU. The
    /// command line is no longer consulted at all; this pins that the names
    /// the driver directories use are the only ones that count.
    #[test]
    fn a_name_that_only_resembles_a_driver_is_not_one() {
        assert_eq!(
            iommu_type_from_driver_dirs(&["intel_iommu", "amd_iommu"]),
            IommuType::Unknown
        );
    }

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
            isolated: Some(true),
            passthrough_candidate: Some(true),
        };
        assert_eq!(group.isolated, Some(true));
        assert_eq!(group.devices.len(), 1);
    }

    /// ACS is not read, so a multi-device group's isolation is unknown, not false.
    #[test]
    fn multi_device_group_isolation_is_unknown_not_false() {
        let dev = |bdf: &str| IommuDevice {
            bdf: bdf.into(),
            description: "d".into(),
            driver: None,
            vfio_bound: false,
        };
        let multi = vec![dev("0000:01:00.0"), dev("0000:01:00.1")];
        assert_eq!(group_isolation(&multi), (None, None));
        assert_eq!(group_isolation(&[]), (None, None));

        // A single device at function 0 with no driver is not a candidate; with a
        // driver, or at a non-zero function, it is.
        assert_eq!(
            group_isolation(&[dev("0000:02:00.0")]),
            (Some(true), Some(false))
        );
        assert_eq!(
            group_isolation(&[dev("0000:02:00.1")]),
            (Some(true), Some(true))
        );
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
            isolated: None,
            passthrough_candidate: None,
        };
        assert_eq!(group.isolated, None);
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
