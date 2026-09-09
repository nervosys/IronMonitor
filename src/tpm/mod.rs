//! Trusted Platform Module (TPM) monitoring.
//!
//! Detects TPM presence, version, manufacturer, and capabilities.
//!
//! # Platform Support
//!
//! - **Linux**: Reads `/sys/class/tpm/`, `/dev/tpm*`
//! - **Windows**: Uses WMI (`Win32_Tpm`) and registry
//! - **macOS**: Checks Secure Enclave presence (Apple's TPM equivalent)
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::tpm::TpmMonitor;
//!
//! let monitor = TpmMonitor::new().unwrap();
//! if let Some(tpm) = monitor.tpm() {
//!     println!("TPM {} - v{} by {}", tpm.device, tpm.version, tpm.manufacturer);
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::error::IronError;

/// TPM specification version
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TpmVersion {
    /// TPM 1.2
    V1_2,
    /// TPM 2.0
    V2_0,
    /// Apple Secure Enclave (T1/T2/Apple Silicon)
    SecureEnclave,
    /// Unknown version
    Unknown,
}

/// TPM status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TpmStatus {
    /// TPM is enabled and ready
    Enabled,
    /// TPM exists but is disabled in BIOS/firmware
    Disabled,
    /// TPM is in a locked state
    Locked,
    /// Status unknown
    Unknown,
}

/// Information about the TPM device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpmInfo {
    /// Device name (e.g., "tpm0")
    pub device: String,
    /// TPM version
    pub version: TpmVersion,
    /// Current status
    pub status: TpmStatus,
    /// Manufacturer name
    pub manufacturer: String,
    /// Firmware version
    pub firmware_version: String,
    /// Device path
    pub device_path: String,
    /// Whether the TPM is the system's primary/active TPM
    pub is_primary: bool,
    /// Supported algorithms (SHA-1, SHA-256, RSA, ECC, etc.).
    ///
    /// `None` when the platform was not asked or did not say. An empty `Vec` is
    /// a different answer: enumerated, and none were reported. Until 6.0.0 a
    /// Linux TPM whose `pcr-*` directories were unreadable was given the
    /// standard TPM 2.0 list under a comment reading "Default for TPM 2.0",
    /// Windows derived the list from the specification version, and macOS
    /// asserted `AES-256`, `SHA-256` and `ECC-P256` for every Secure Enclave.
    pub algorithms: Option<Vec<String>>,
    /// PCR bank count. `None` when it was not read -- never 24, which was the
    /// Windows constant, and never 0, which was both "no banks" and "did not
    /// look" on three platforms.
    pub pcr_banks: Option<u32>,
    /// Whether platform integrity measurements are active.
    ///
    /// `None` when it could not be established. `Some(true)` on Windows means a
    /// Measured Boot log was found, not that a TPM exists: the previous value
    /// was a literal `true` under the comment "Windows with TPM implies
    /// measured boot", which is an inference, and an unsound one -- a machine
    /// can have a TPM with measured boot switched off.
    pub measured_boot: Option<bool>,
}

/// Monitor for TPM devices
pub struct TpmMonitor {
    tpm_info: Option<TpmInfo>,
}

/// The TPM specification a Windows security device node declares.
///
/// The ACPI hardware id is the declaration: `MSFT0101` is the TPM 2.0 device
/// and `PNP0C31` the TPM 1.2 one, both assigned rather than descriptive. This
/// is not the friendly name "Trusted Platform Module 2.0" that sits beside it
/// -- reading the version out of a display string is the substring guessing
/// this crate removes wherever it finds it, and the same node would answer in a
/// localised name on another machine.
///
/// The list may hold several security devices; on the development host it holds
/// the AMD PSP alongside the TPM. Anything that is not one of the two known ids
/// is passed over rather than treated as a TPM of unknown version.
#[cfg(target_os = "windows")]
fn version_from_node(pnp_ids: &str) -> Option<TpmVersion> {
    pnp_ids.lines().find_map(|line| {
        let upper = line.trim().to_uppercase();
        if upper.starts_with(r"ACPI\MSFT0101") {
            Some(TpmVersion::V2_0)
        } else if upper.starts_with(r"ACPI\PNP0C31") {
            Some(TpmVersion::V1_2)
        } else {
            None
        }
    })
}

impl TpmMonitor {
    /// Create a new TpmMonitor and detect TPM.
    pub fn new() -> Result<Self, IronError> {
        let mut monitor = Self { tpm_info: None };
        monitor.refresh()?;
        Ok(monitor)
    }

    /// Refresh TPM detection.
    ///
    /// Returns `Err` when the detection failed, and `Ok` -- with or without a
    /// TPM -- only when it ran. The difference is not cosmetic here: the
    /// resolver publishes `board.tpm.present` as a **measured** `false` when
    /// this returns `Ok` and finds nothing, under a comment reading "a
    /// successful enumeration that found nothing is a reading: this machine has
    /// no TPM". That premise was false, because until 6.0.0 this could not
    /// report a failure at all, and "the query did not run" was published as
    /// "this machine has no TPM".
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.tpm_info = None;

        #[cfg(target_os = "linux")]
        self.refresh_linux()?;

        #[cfg(target_os = "windows")]
        self.refresh_windows()?;

        #[cfg(target_os = "macos")]
        self.refresh_macos()?;

        Ok(())
    }

    /// Get the detected TPM info, if any.
    pub fn tpm(&self) -> Option<&TpmInfo> {
        self.tpm_info.as_ref()
    }

    /// Returns true if a TPM is present.
    pub fn has_tpm(&self) -> bool {
        self.tpm_info.is_some()
    }

    /// Returns true if TPM 2.0 is available.
    pub fn has_tpm2(&self) -> bool {
        self.tpm_info
            .as_ref()
            .map(|t| t.version == TpmVersion::V2_0)
            .unwrap_or(false)
    }

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) -> Result<(), IronError> {
        let tpm_class = std::path::Path::new("/sys/class/tpm");
        if !tpm_class.exists() {
            // A real answer: this kernel exposes no TPM class at all.
            return Ok(());
        }

        let entries = std::fs::read_dir(tpm_class)
            .map_err(|e| IronError::System(format!("cannot read /sys/class/tpm: {e}")))?;
        {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("tpm") {
                    continue;
                }

                let base = entry.path();

                // Read TPM version from tpm_version_major
                let version =
                    if let Ok(major) = std::fs::read_to_string(base.join("tpm_version_major")) {
                        match major.trim() {
                            "2" => TpmVersion::V2_0,
                            "1" => TpmVersion::V1_2,
                            _ => TpmVersion::Unknown,
                        }
                    } else {
                        // Fallback: check caps or device path
                        if std::path::Path::new("/dev/tpmrm0").exists() {
                            TpmVersion::V2_0
                        } else if std::path::Path::new("/dev/tpm0").exists() {
                            TpmVersion::V1_2
                        } else {
                            TpmVersion::Unknown
                        }
                    };

                // Read manufacturer from caps
                let caps = std::fs::read_to_string(base.join("caps")).unwrap_or_default();
                let mut manufacturer = String::new();
                let mut firmware_version = String::new();
                for line in caps.lines() {
                    if let Some(m) = line.strip_prefix("Manufacturer:") {
                        manufacturer = m.trim().to_string();
                    }
                    if let Some(v) = line.strip_prefix("Firmware version:") {
                        firmware_version = v.trim().to_string();
                    }
                }

                // Fallback vendor detection from device description
                if manufacturer.is_empty() {
                    manufacturer = std::fs::read_to_string(base.join("device/description"))
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                }

                // PCR banks. A directory that is absent and one that will not
                // open are different answers, and both used to be zero.
                let pcr_path = base.join("pcr-sha256");
                let pcr_banks = if pcr_path.exists() {
                    std::fs::read_dir(&pcr_path).ok().map(|d| d.count() as u32)
                } else {
                    None
                };

                // Algorithms, from the `pcr-*` directories the driver exposes.
                // An empty result is left empty: it used to be replaced with
                // the standard TPM 2.0 list, which is a specification talking,
                // not this device.
                let mut algorithms = Vec::new();
                for algo in &["sha1", "sha256", "sha384", "sha512", "sm3-256"] {
                    if base.join(format!("pcr-{}", algo)).exists() {
                        algorithms.push(algo.to_uppercase());
                    }
                }

                // Measured boot check (IMA or tpm_bios). Both paths existing is
                // evidence that measurements are active; neither existing is
                // not evidence that they are not, so the negative is `None`.
                let measured_boot = (std::path::Path::new("/sys/kernel/security/ima").exists()
                    || std::path::Path::new("/sys/kernel/security/tpm0/binary_bios_measurements")
                        .exists())
                .then_some(true);

                self.tpm_info = Some(TpmInfo {
                    device: name,
                    version,
                    status: TpmStatus::Enabled,
                    manufacturer,
                    firmware_version,
                    device_path: format!("/dev/{}", entry.file_name().to_string_lossy()),
                    is_primary: true,
                    algorithms: Some(algorithms),
                    pcr_banks,
                    measured_boot,
                });
                break; // Usually only one TPM
            }
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), IronError> {
        // Two sources, and the first is expected to fail on an ordinary
        // account: `root/cimv2/Security/MicrosoftTpm` answers "Access denied"
        // without elevation. So its failure is carried, not raised -- the
        // registry check below is what decides -- and only both failing is an
        // error. This is the same rule as `usb::refresh_windows`.
        const WMI_TPM: &str = "Get-CimInstance -Namespace 'root/cimv2/Security/MicrosoftTpm' -ClassName Win32_Tpm -ErrorAction SilentlyContinue | Select-Object IsActivated_InitialValue, IsEnabled_InitialValue, IsOwned_InitialValue, ManufacturerIdTxt, ManufacturerVersion, SpecVersion, PhysicalPresenceVersionInfo | ConvertTo-Json -Compress";
        const TPM_SERVICE: &str = r#"if (Test-Path 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\TPM') { 'present' } else { 'absent' }"#;
        // The security device's own node, which an ordinary account can read.
        // Its ACPI hardware id names the specification: `MSFT0101` is the
        // TPM 2.0 device and `PNP0C31` the TPM 1.2 one. See `version_from_node`.
        const TPM_DEVICE_NODE: &str = "Get-CimInstance Win32_PnPEntity -Filter \"PNPClass='SecurityDevices'\" | Select-Object -ExpandProperty PNPDeviceID";

        let wmi =
            crate::core::command::capture_json("powershell", &["-NoProfile", "-Command", WMI_TPM]);
        if let Ok(Some(val)) = &wmi {
            let spec = val["SpecVersion"].as_str().unwrap_or("");
            let version = if spec.starts_with("2.0") || spec.contains("2.0") {
                TpmVersion::V2_0
            } else if spec.starts_with("1.2") || spec.contains("1.2") {
                TpmVersion::V1_2
            } else {
                TpmVersion::Unknown
            };

            let enabled = val["IsEnabled_InitialValue"].as_bool().unwrap_or(false);
            let activated = val["IsActivated_InitialValue"].as_bool().unwrap_or(false);
            let status = if enabled && activated {
                TpmStatus::Enabled
            } else if enabled {
                TpmStatus::Locked
            } else {
                TpmStatus::Disabled
            };

            let manufacturer = val["ManufacturerIdTxt"].as_str().unwrap_or("").to_string();
            let firmware_version = val["ManufacturerVersion"]
                .as_str()
                .unwrap_or("")
                .to_string();

            self.tpm_info = Some(TpmInfo {
                device: "tpm0".into(),
                version,
                status,
                manufacturer,
                firmware_version,
                device_path: r"\\.\TPM".into(),
                is_primary: true,
                // Win32_Tpm publishes neither. The algorithm list here was
                // derived from `SpecVersion` -- what a TPM of that version is
                // required to support, not what this one reports -- and
                // `pcr_banks` was the constant 24.
                algorithms: None,
                pcr_banks: None,
                measured_boot: Self::measured_boot_windows(),
            });
            return Ok(());
        }

        // Fallback: check registry for TPM existence
        let registry =
            crate::core::command::capture("powershell", &["-NoProfile", "-Command", TPM_SERVICE]);
        let present = match (&registry, wmi) {
            (Ok(text), _) => text.trim() == "present",
            (Err(registry_err), Err(wmi_err)) => {
                return Err(IronError::System(format!(
                    "no TPM detection succeeded: the WMI class said {wmi_err}; the registry check \
                     said {registry_err}"
                )))
            }
            (Err(registry_err), Ok(_)) => {
                // The privileged query ran and reported no TPM object; the
                // registry check is what would have confirmed it, and it did
                // not run. Neither answer is available.
                return Err(IronError::System(format!(
                    "the TPM service registry check could not be made: {registry_err}"
                )));
            }
        };

        if present {
            // The version is readable here without elevation, from the ACPI
            // hardware id of the security device node. `Win32_Tpm.SpecVersion`
            // above is the better source and answers "Access denied" on an
            // ordinary account, which left this path reporting `Unknown` and
            // the ontology saying "a TPM is present but its specification
            // version could not be determined" -- while `ACPI\MSFT0101\1` sat
            // in the device tree, readable, saying 2.0.
            let version = crate::core::command::capture(
                "powershell",
                &["-NoProfile", "-Command", TPM_DEVICE_NODE],
            )
            .ok()
            .and_then(|text| version_from_node(&text))
            .unwrap_or(TpmVersion::Unknown);

            self.tpm_info = Some(TpmInfo {
                device: "tpm0".into(),
                version,
                // Still unknown, and deliberately: a started driver is not the
                // TPM's own enabled flag, and `TPM_PT_PERMANENT` is what that
                // question asks. Reporting "enabled" because a device node is
                // in state OK would answer a different question in this one's
                // name -- and for a security property that is the wrong way to
                // be wrong.
                status: TpmStatus::Unknown,
                manufacturer: String::new(),
                firmware_version: String::new(),
                device_path: r"\\.\TPM".into(),
                is_primary: true,
                // This path learned that the TPM service is registered and
                // which specification the device node declares.
                algorithms: None,
                pcr_banks: None,
                measured_boot: Self::measured_boot_windows(),
            });
        }

        Ok(())
    }

    /// Whether Windows measured this boot, from the Measured Boot log.
    ///
    /// The boot loader writes the TCG log to `C:\Windows\Logs\MeasuredBoot`
    /// when measurements are active, so a `.log` there is evidence that they
    /// are. Its absence is not evidence that they are not -- the directory can
    /// be cleared, and policy can discard the logs -- so this answers
    /// `Some(true)` or `None` and never `Some(false)`.
    #[cfg(target_os = "windows")]
    fn measured_boot_windows() -> Option<bool> {
        let dir = std::path::Path::new(r"C:\Windows\Logs\MeasuredBoot");
        let logged = std::fs::read_dir(dir).ok()?.flatten().any(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("log"))
        });
        logged.then_some(true)
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), IronError> {
        // macOS uses Secure Enclave instead of discrete TPM
        // T1 chip (2016 MBP), T2 chip (2018+), Apple Silicon (M1+) all have SE
        let text = crate::core::command::capture("system_profiler", &["SPHardwareDataType"])?;
        {
            {
                let has_se = text.contains("Apple M")
                    || text.contains("Apple T2")
                    || text.contains("Apple T1");
                if has_se {
                    self.tpm_info = Some(TpmInfo {
                        device: "sep0".into(),
                        version: TpmVersion::SecureEnclave,
                        status: TpmStatus::Enabled,
                        manufacturer: "Apple".into(),
                        firmware_version: String::new(),
                        device_path: String::new(),
                        is_primary: true,
                        // `system_profiler` reports a chip name and nothing
                        // about the Secure Enclave's capabilities. The list
                        // here was what an SEP is documented to support.
                        algorithms: None,
                        // The SEP has no PCRs, which is why 0 was never a bank
                        // count. Secure boot on Apple silicon is a separate
                        // property this reader does not consult.
                        pcr_banks: None,
                        measured_boot: None,
                    });
                }
            }
        }
        Ok(())
    }
}

impl Default for TpmMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self { tpm_info: None })
    }
}

impl std::fmt::Display for TpmVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1_2 => write!(f, "1.2"),
            Self::V2_0 => write!(f, "2.0"),
            Self::SecureEnclave => write!(f, "Secure Enclave"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructing the monitor either enumerates or says why it could not.
    ///
    /// This asserted `is_ok()`, which was true by construction until 6.0.0
    /// because `refresh` could not fail. Now that it can, the contract is
    /// weaker and more useful: **whatever happens, the caller can tell which
    /// happened.** A failure has to carry a reason, because a reason is the
    /// whole difference between "this machine has none" and "nobody looked".
    #[test]
    fn test_tpm_monitor_creation() {
        match TpmMonitor::new() {
            Ok(_monitor) => {}
            Err(e) => {
                let why = e.to_string();
                assert!(
                    why.len() > 10,
                    "enumeration failed without saying why: {why:?}"
                );
            }
        }
    }

    #[test]
    fn test_tpm_monitor_default() {
        let monitor = TpmMonitor::default();
        let _ = monitor.has_tpm();
        let _ = monitor.has_tpm2();
    }

    #[test]
    fn test_tpm_version_display() {
        assert_eq!(TpmVersion::V2_0.to_string(), "2.0");
        assert_eq!(TpmVersion::V1_2.to_string(), "1.2");
        assert_eq!(TpmVersion::SecureEnclave.to_string(), "Secure Enclave");
    }

    #[test]
    fn test_tpm_info_serialization() {
        let info = TpmInfo {
            device: "tpm0".into(),
            version: TpmVersion::V2_0,
            status: TpmStatus::Enabled,
            manufacturer: "IFX".into(),
            firmware_version: "7.85".into(),
            device_path: "/dev/tpm0".into(),
            is_primary: true,
            algorithms: Some(vec!["SHA-256".into(), "RSA".into()]),
            pcr_banks: Some(24),
            measured_boot: Some(true),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("tpm0"));
        let _: TpmInfo = serde_json::from_str(&json).unwrap();
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_version_tests {
    use super::{version_from_node, TpmVersion};

    /// The exact output of the device-node query on the development host,
    /// where the AMD PSP is listed first and the TPM second.
    #[test]
    fn the_acpi_hardware_id_names_the_specification() {
        let listing =
            "PCI\\VEN_1022&DEV_1649&SUBSYS_88771043&REV_00\\4&1EBE6A9C&0&0241\nACPI\\MSFT0101\\1\n";
        assert_eq!(version_from_node(listing), Some(TpmVersion::V2_0));
    }

    #[test]
    fn a_tpm_12_node_reads_as_12() {
        assert_eq!(
            version_from_node("ACPI\\PNP0C31\\0"),
            Some(TpmVersion::V1_2)
        );
    }

    #[test]
    fn a_listing_with_no_tpm_node_states_nothing() {
        // A security device that is not a TPM must not be read as a TPM of
        // unknown version: the caller reports the absence instead.
        assert_eq!(
            version_from_node("PCI\\VEN_1022&DEV_1649&SUBSYS_88771043&REV_00\\4&1EBE6A9C&0&0241"),
            None
        );
        assert_eq!(version_from_node(""), None);
    }
}
