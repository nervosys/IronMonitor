//! Firmware inventory — BIOS, UEFI, EC, ME, NIC firmware, storage firmware.
//!
//! Collects firmware versions for all system components, security status,
//! and infers firmware age and update urgency.
//!
//! # Platform Support
//!
//! - **Linux**: DMI/SMBIOS (`/sys/class/dmi/id/`), `fwupdmgr`, `ethtool -i`
//! - **Windows**: WMI (`Win32_BIOS`), `MSFT_Firmware`, registry
//! - **macOS**: `system_profiler SPHardwareDataType`, `ioreg`
//!
//! ## Inference
//!
//! Firmware dates are parsed to estimate age. Known vulnerability databases
//! (e.g., BIOS date < 2023 = likely unpatched Spectre/Meltdown mitigations)
//! produce a risk score.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::firmware::FirmwareInventory;
//!
//! let inventory = FirmwareInventory::new().unwrap();
//! for fw in inventory.items() {
//!     println!("{}: {} v{} ({})", fw.component, fw.vendor, fw.version, fw.date);
//! }
//! match inventory.risk_score() {
//!     Some(score) => println!("Inferred risk score: {}/100", score),
//!     None => println!("No firmware entries were read; no risk score"),
//! }
//! ```

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// Firmware component type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FirmwareComponent {
    /// System BIOS / UEFI
    SystemBios,
    /// Intel Management Engine (ME) or AMD PSP
    ManagementEngine,
    /// Embedded Controller (EC)
    EmbeddedController,
    /// Trusted Platform Module (TPM)
    Tpm,
    /// Network adapter firmware
    Nic,
    /// Storage controller / disk firmware
    Storage,
    /// GPU VBIOS
    GpuVbios,
    /// Thunderbolt controller
    Thunderbolt,
    /// Bluetooth controller
    Bluetooth,
    /// WiFi adapter
    Wifi,
    /// Base Management Controller (BMC/IPMI)
    Bmc,
    /// CPU microcode
    CpuMicrocode,
    /// Other
    Other(String),
}

/// UEFI Secure Boot status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecureBootStatus {
    Enabled,
    Disabled,
    NotSupported,
    Unknown,
}

/// Boot mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootMode {
    UEFI,
    Legacy,
    Unknown,
}

/// A single firmware entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareEntry {
    /// Component type
    pub component: FirmwareComponent,
    /// Vendor / manufacturer
    pub vendor: String,
    /// Version string
    pub version: String,
    /// Release date (YYYY-MM-DD or vendor format)
    pub date: String,
    /// Device path or identifier
    pub device: String,
    /// Updateable via OS mechanisms?
    pub updateable: bool,
    /// Estimated age in days (inferred from date)
    pub estimated_age_days: Option<u32>,
    /// Security risk level (0 = low, 100 = critical)
    pub inferred_risk_score: u8,
}

/// Firmware inventory for the system.
pub struct FirmwareInventory {
    entries: Vec<FirmwareEntry>,
    secure_boot: SecureBootStatus,
    boot_mode: BootMode,
    system_vendor: String,
    system_product: String,
    /// System serial number.
    pub system_serial: String,
}

impl FirmwareInventory {
    pub fn new() -> Result<Self, IronError> {
        let mut inv = Self {
            entries: Vec::new(),
            secure_boot: SecureBootStatus::Unknown,
            boot_mode: BootMode::Unknown,
            system_vendor: String::new(),
            system_product: String::new(),
            system_serial: String::new(),
        };
        inv.refresh()?;
        Ok(inv)
    }

    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.entries.clear();

        #[cfg(target_os = "linux")]
        self.refresh_linux();

        #[cfg(target_os = "windows")]
        self.refresh_windows()?;

        #[cfg(target_os = "macos")]
        self.refresh_macos();

        // Infer risk scores for all entries
        for entry in &mut self.entries {
            entry.estimated_age_days = Self::estimate_age(&entry.date);
            entry.inferred_risk_score = Self::infer_risk(entry);
        }

        Ok(())
    }

    pub fn items(&self) -> &[FirmwareEntry] {
        &self.entries
    }

    pub fn secure_boot_status(&self) -> &SecureBootStatus {
        &self.secure_boot
    }

    pub fn boot_mode(&self) -> &BootMode {
        &self.boot_mode
    }

    pub fn system_vendor(&self) -> &str {
        &self.system_vendor
    }

    pub fn system_product(&self) -> &str {
        &self.system_product
    }

    /// Overall inferred firmware risk score (max across all components), or
    /// `None` when no firmware entry was read.
    ///
    /// This returned `0` for an empty inventory, and zero is the *safest*
    /// score there is: a machine whose firmware could not be read at all
    /// would have been reported as carrying no risk.
    pub fn risk_score(&self) -> Option<u8> {
        self.entries.iter().map(|e| e.inferred_risk_score).max()
    }

    /// Average firmware age in days.
    pub fn average_firmware_age_days(&self) -> Option<f64> {
        let ages: Vec<f64> = self
            .entries
            .iter()
            .filter_map(|e| e.estimated_age_days.map(|d| d as f64))
            .collect();
        if ages.is_empty() {
            None
        } else {
            Some(ages.iter().sum::<f64>() / ages.len() as f64)
        }
    }

    /// Firmware entries that need attention (risk > 50).
    pub fn high_risk_entries(&self) -> Vec<&FirmwareEntry> {
        self.entries
            .iter()
            .filter(|e| e.inferred_risk_score > 50)
            .collect()
    }

    /// Estimate age from date string.
    fn estimate_age(date: &str) -> Option<u32> {
        // Try common date formats: YYYY-MM-DD, MM/DD/YYYY, YYYYMMDD
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();

        let (year, month, day) = Self::parse_date(date)?;
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }

        // Exact days since the Unix epoch. This counted every month as 30 days
        // and every year as 365, which drifted by about two weeks a year -- a
        // BIOS dated 2025-09-19 read 360 days old on 2026-09-28, when it was
        // 374, on the other side of `infer_risk`'s 365-day threshold. It did
        // not show while the Windows date never parsed; it does now.
        let fw_days = Self::days_from_civil(year as i64, month, day);
        let now_days = (now / 86400) as i64;
        (fw_days >= 0 && now_days >= fw_days).then(|| (now_days - fw_days) as u32)
    }

    /// Days from 1970-01-01 to the given proleptic Gregorian date (Howard
    /// Hinnant's `days_from_civil`).
    fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
        let y = if month <= 2 { year - 1 } else { year };
        let era = (if y >= 0 { y } else { y - 399 }) / 400;
        let yoe = y - era * 400;
        let m = month as i64;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    fn parse_date(date: &str) -> Option<(u32, u32, u32)> {
        // YYYY-MM-DD
        if date.len() >= 10 && date.chars().nth(4) == Some('-') {
            let parts: Vec<&str> = date.split('-').collect();
            if parts.len() >= 3 {
                let y = parts[0].parse().ok()?;
                let m = parts[1].parse().ok()?;
                let d = parts[2].parse().ok()?;
                return Some((y, m, d));
            }
        }

        // MM/DD/YYYY
        if date.contains('/') {
            let parts: Vec<&str> = date.split('/').collect();
            if parts.len() >= 3 {
                let m = parts[0].parse().ok()?;
                let d = parts[1].parse().ok()?;
                let y = parts[2].parse().ok()?;
                return Some((y, m, d));
            }
        }

        // YYYYMMDD
        if date.len() == 8 && date.chars().all(|c| c.is_ascii_digit()) {
            let y = date[0..4].parse().ok()?;
            let m = date[4..6].parse().ok()?;
            let d = date[6..8].parse().ok()?;
            return Some((y, m, d));
        }

        None
    }

    /// Infer security risk from firmware entry.
    fn infer_risk(entry: &FirmwareEntry) -> u8 {
        let mut risk: u8 = 0;

        // Age-based risk
        if let Some(age_days) = entry.estimated_age_days {
            risk = match age_days {
                0..=180 => 0,      // < 6 months: low risk
                181..=365 => 10,   // 6-12 months
                366..=730 => 25,   // 1-2 years
                731..=1095 => 45,  // 2-3 years
                1096..=1825 => 65, // 3-5 years
                _ => 85,           // 5+ years: high risk
            };
        }

        // Component-specific risk amplifiers
        match &entry.component {
            FirmwareComponent::SystemBios => {
                // BIOS is critical — amplify risk
                risk = risk.saturating_add(10);
            }
            FirmwareComponent::ManagementEngine => {
                // ME has had many CVEs — highest risk
                risk = risk.saturating_add(15);
            }
            FirmwareComponent::CpuMicrocode => {
                // CPU microcode patches Spectre/Meltdown
                risk = risk.saturating_add(10);
            }
            FirmwareComponent::Bmc => {
                // BMC = remote management, high attack surface
                risk = risk.saturating_add(15);
            }
            FirmwareComponent::Tpm => {
                risk = risk.saturating_add(5);
            }
            _ => {}
        }

        risk.min(100)
    }

    /// Boot mode and Secure Boot from sysfs alone -- no helper processes.
    #[cfg(target_os = "linux")]
    fn linux_boot_mode_and_secure_boot() -> (BootMode, SecureBootStatus) {
        if !std::path::Path::new("/sys/firmware/efi").exists() {
            return (BootMode::Legacy, SecureBootStatus::NotSupported);
        }
        let sb_path = "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
        let secure_boot = match std::fs::read(sb_path) {
            // Last byte: 1 = enabled, 0 = disabled. An efivar that reads back
            // empty is neither, and the `else` used to call it disabled --
            // `SecureBootStatus::Unknown` exists for this.
            Ok(data) => match data.last() {
                Some(1) => SecureBootStatus::Enabled,
                Some(0) => SecureBootStatus::Disabled,
                _ => SecureBootStatus::Unknown,
            },
            Err(_) => SecureBootStatus::Unknown,
        };
        (BootMode::UEFI, secure_boot)
    }

    /// Only the Secure Boot state, without the rest of the inventory.
    ///
    /// On Linux that is one efivar. The full inventory also runs `fwupdmgr`,
    /// which on a machine without a reachable fwupd daemon waits 25 s for a
    /// D-Bus activation that never comes -- and the Secure Boot entity was
    /// paying that, a second time after `board.firmware`, on every snapshot.
    pub fn read_secure_boot() -> Result<SecureBootStatus, IronError> {
        #[cfg(target_os = "linux")]
        {
            Ok(Self::linux_boot_mode_and_secure_boot().1)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self::new()?.secure_boot_status().clone())
        }
    }

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) {
        let dmi = std::path::Path::new("/sys/class/dmi/id");

        // System info
        self.system_vendor = std::fs::read_to_string(dmi.join("sys_vendor"))
            .unwrap_or_default()
            .trim()
            .to_string();
        self.system_product = std::fs::read_to_string(dmi.join("product_name"))
            .unwrap_or_default()
            .trim()
            .to_string();
        self.system_serial = std::fs::read_to_string(dmi.join("product_serial"))
            .unwrap_or_default()
            .trim()
            .to_string();

        // BIOS
        let bios_vendor = std::fs::read_to_string(dmi.join("bios_vendor"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let bios_version = std::fs::read_to_string(dmi.join("bios_version"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let bios_date = std::fs::read_to_string(dmi.join("bios_date"))
            .unwrap_or_default()
            .trim()
            .to_string();

        if !bios_vendor.is_empty() {
            self.entries.push(FirmwareEntry {
                component: FirmwareComponent::SystemBios,
                vendor: bios_vendor,
                version: bios_version,
                date: bios_date,
                device: "System BIOS".into(),
                updateable: true,
                estimated_age_days: None,
                inferred_risk_score: 0,
            });
        }

        // Secure boot check
        let (boot_mode, secure_boot) = Self::linux_boot_mode_and_secure_boot();
        self.boot_mode = boot_mode;
        self.secure_boot = secure_boot;

        // CPU microcode
        if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in cpuinfo.lines() {
                if line.starts_with("microcode") {
                    if let Some(ver) = line.split(':').nth(1) {
                        self.entries.push(FirmwareEntry {
                            component: FirmwareComponent::CpuMicrocode,
                            vendor: self.system_vendor.clone(),
                            version: ver.trim().to_string(),
                            date: String::new(),
                            device: "CPU Microcode".into(),
                            updateable: true,
                            estimated_age_days: None,
                            inferred_risk_score: 0,
                        });
                        break;
                    }
                }
            }
        }

        // Network adapter firmware via ethtool
        if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
            for entry in entries.flatten() {
                let iface = entry.file_name().to_string_lossy().to_string();
                if iface == "lo" {
                    continue;
                }
                if let Ok(output) = std::process::Command::new("ethtool")
                    .args(["-i", &iface])
                    .output()
                {
                    let text = String::from_utf8(output.stdout).unwrap_or_default();
                    let mut version = String::new();
                    let mut driver = String::new();
                    for line in text.lines() {
                        if let Some(v) = line.strip_prefix("firmware-version: ") {
                            version = v.trim().to_string();
                        }
                        if let Some(d) = line.strip_prefix("driver: ") {
                            driver = d.trim().to_string();
                        }
                    }
                    if !version.is_empty() {
                        self.entries.push(FirmwareEntry {
                            component: FirmwareComponent::Nic,
                            vendor: driver,
                            version,
                            date: String::new(),
                            device: iface,
                            updateable: false,
                            estimated_age_days: None,
                            inferred_risk_score: 0,
                        });
                    }
                }
            }
        }

        // Try fwupdmgr for additional firmware. Bounded: without a reachable
        // fwupd daemon it waits 25 s on D-Bus activation and then fails, and a
        // running daemon answers in well under the limit.
        if let Ok(text) = crate::core::command::capture_with_timeout(
            "fwupdmgr",
            &["get-devices", "--json"],
            std::time::Duration::from_secs(8),
        ) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(devices) = json.get("Devices").and_then(|d| d.as_array()) {
                    for dev in devices {
                        let name = dev.get("Name").and_then(|n| n.as_str()).unwrap_or("");
                        let vendor = dev.get("Vendor").and_then(|v| v.as_str()).unwrap_or("");
                        let version = dev.get("Version").and_then(|v| v.as_str()).unwrap_or("");

                        if !name.is_empty() && !version.is_empty() {
                            let component = if name.to_lowercase().contains("thunderbolt") {
                                FirmwareComponent::Thunderbolt
                            } else if name.to_lowercase().contains("tpm") {
                                FirmwareComponent::Tpm
                            } else if name.to_lowercase().contains("bmc")
                                || name.to_lowercase().contains("ipmi")
                            {
                                FirmwareComponent::Bmc
                            } else {
                                FirmwareComponent::Other(name.to_string())
                            };

                            self.entries.push(FirmwareEntry {
                                component,
                                vendor: vendor.to_string(),
                                version: version.to_string(),
                                date: String::new(),
                                device: name.to_string(),
                                updateable: dev
                                    .get("Flags")
                                    .and_then(|f| f.as_array())
                                    .map(|flags| {
                                        flags.iter().any(|f| f.as_str() == Some("updatable"))
                                    })
                                    .unwrap_or(false),
                                estimated_age_days: None,
                                inferred_risk_score: 0,
                            });
                        }
                    }
                }
            }
        }
    }

    /// Read what Windows publishes about this machine's firmware.
    ///
    /// This was one function running four PowerShell queries in sequence, each
    /// wrapped in `if let Ok(output)`. Two of the four **enumerate** — the BIOS
    /// and the storage devices both push `FirmwareEntry` values — and two only
    /// **decorate** what the others found, setting the system vendor and the
    /// Secure Boot state. Mixing them meant a failure of any one was
    /// indistinguishable from a failure of all four, and an empty entry list is
    /// what the resolver publishes as a fact about the machine.
    ///
    /// They are four methods now, and the rule from `usb::refresh_windows`
    /// applies to the two that enumerate: either succeeding is enough to trust
    /// an empty result, and both failing is an error naming both reasons. The
    /// two that decorate are allowed to fail quietly, because their absence is
    /// already expressible — `system_vendor` stays empty and `secure_boot`
    /// stays `Unknown`, which is what `Confirm-SecureBootUEFI` does anyway
    /// without elevation.
    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), IronError> {
        // `GetFirmwareType` answers this, and this crate already wraps it --
        // `boot_config` calls the same helper and its comment says why:
        // "claiming Legacy on a failed query is how the old code got it wrong".
        // Here the assumption ran the other way, under the comment "modern
        // Windows is almost always UEFI", which is true and is not a reading.
        // A machine that boots legacy BIOS is exactly the machine an agent
        // asking this question cares about, and it was the one being told UEFI.
        self.boot_mode = match crate::platform::windows::firmware_type() {
            Some(crate::boot_config::BootType::Uefi) => BootMode::UEFI,
            Some(crate::boot_config::BootType::Legacy) => BootMode::Legacy,
            _ => BootMode::Unknown,
        };

        let bios = self.read_windows_bios();
        let storage = self.read_windows_storage_firmware();

        // Decoration: neither of these adds an entry, and both have a resting
        // value that already means "not established".
        self.read_windows_system_identity();
        self.read_windows_secure_boot();

        match (bios, storage) {
            (Err(bios_err), Err(storage_err)) => Err(IronError::System(format!(
                "no firmware source could be read: Win32_BIOS said {bios_err}; \
                 MSFT_PhysicalDisk said {storage_err}"
            ))),
            _ => Ok(()),
        }
    }

    /// The system BIOS entry, from `Win32_BIOS`.
    #[cfg(target_os = "windows")]
    fn read_windows_bios(&mut self) -> Result<(), IronError> {
        // In-process WMI; each of these reads was a PowerShell session.
        let rows = crate::platform::windows::wmi_query(
            "root\\CIMV2",
            "SELECT Manufacturer, SMBIOSBIOSVersion, ReleaseDate FROM Win32_BIOS",
        )?;
        let Some(row) = rows.first() else {
            return Ok(());
        };
        let vendor = crate::platform::windows::wmi_str(row, "Manufacturer");
        let version = crate::platform::windows::wmi_str(row, "SMBIOSBIOSVersion");
        // A CIM datetime, `yyyymmddHHMMSS.mmmmmm+UUU`, kept as `YYYY-MM-DD`.
        //
        // Through PowerShell it arrived as `/Date(ms)/` and was stored as Unix
        // seconds, which `parse_date` does not read -- so `estimated_age_days`
        // was never computed for a Windows BIOS. This form it does read.
        let raw = crate::platform::windows::wmi_str(row, "ReleaseDate");
        let clean_date = if raw.len() >= 8 && raw[..8].bytes().all(|b| b.is_ascii_digit()) {
            format!("{}-{}-{}", &raw[0..4], &raw[4..6], &raw[6..8])
        } else {
            String::new()
        };

        if !vendor.is_empty() {
            self.entries.push(FirmwareEntry {
                component: FirmwareComponent::SystemBios,
                vendor: vendor.to_string(),
                version: version.to_string(),
                date: clean_date,
                device: "System BIOS".into(),
                updateable: true,
                estimated_age_days: None,
                inferred_risk_score: 0,
            });
        }
        Ok(())
    }

    /// One entry per drive that reports a firmware revision.
    #[cfg(target_os = "windows")]
    fn read_windows_storage_firmware(&mut self) -> Result<(), IronError> {
        // `MSFT_PhysicalDisk` is the class `Get-PhysicalDisk` reads.
        let rows = crate::platform::windows::wmi_query(
            "root\\Microsoft\\Windows\\Storage",
            "SELECT FriendlyName, Manufacturer, FirmwareVersion FROM MSFT_PhysicalDisk",
        )?;
        for disk in &rows {
            let name = crate::platform::windows::wmi_str(disk, "FriendlyName");
            let vendor = crate::platform::windows::wmi_str(disk, "Manufacturer");
            let fw = crate::platform::windows::wmi_str(disk, "FirmwareVersion");

            if !fw.is_empty() {
                self.entries.push(FirmwareEntry {
                    component: FirmwareComponent::Storage,
                    vendor: vendor.to_string(),
                    version: fw.to_string(),
                    date: String::new(),
                    device: name.to_string(),
                    updateable: false,
                    estimated_age_days: None,
                    inferred_risk_score: 0,
                });
            }
        }
        Ok(())
    }

    /// System vendor and model. Decoration: the resting value is an empty
    /// string, which already reads as "not established".
    #[cfg(target_os = "windows")]
    fn read_windows_system_identity(&mut self) {
        if let Ok(rows) = crate::platform::windows::wmi_query(
            "root\\CIMV2",
            "SELECT Manufacturer, Model FROM Win32_ComputerSystem",
        ) {
            if let Some(row) = rows.first() {
                self.system_vendor =
                    crate::platform::windows::wmi_str(row, "Manufacturer").to_string();
                self.system_product = crate::platform::windows::wmi_str(row, "Model").to_string();
            }
        }
    }

    /// Secure Boot state.
    ///
    /// `Confirm-SecureBootUEFI` needs elevation and exits non-zero without it,
    /// which is the ordinary case rather than a fault -- so this is decoration
    /// and leaves `SecureBootStatus::Unknown` alone. `system.boot.secure_boot`
    /// is resolved from the registry instead, which is readable unelevated.
    #[cfg(target_os = "windows")]
    fn read_windows_secure_boot(&mut self) {
        // Refused unelevated, as the comment above says; asking cost a
        // PowerShell session to learn it. Skipped when the process is known
        // not to be elevated.
        if crate::platform::windows::is_elevated() == Some(false) {
            return;
        }
        if let Ok(text) = crate::core::command::capture(
            "powershell",
            &["-NoProfile", "-Command", "Confirm-SecureBootUEFI"],
        ) {
            self.secure_boot = match text.trim() {
                "True" => SecureBootStatus::Enabled,
                "False" => SecureBootStatus::Disabled,
                _ => SecureBootStatus::Unknown,
            };
        }
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) {
        // Every Intel Mac boots EFI and none boots legacy BIOS, so the answer
        // to "UEFI or legacy BIOS" is settled there by the architecture. Apple
        // silicon boots iBoot, which is neither, and this used to answer UEFI
        // for those too.
        self.boot_mode = if cfg!(target_arch = "x86_64") {
            BootMode::UEFI
        } else {
            BootMode::Unknown
        };

        if let Ok(output) = std::process::Command::new("system_profiler")
            .args(["SPHardwareDataType"])
            .output()
        {
            let text = String::from_utf8(output.stdout).unwrap_or_default();
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with("Model Name:") {
                    self.system_product = line.split(':').nth(1).unwrap_or("").trim().to_string();
                }
                if line.starts_with("Boot ROM Version:") {
                    let version = line.split(':').nth(1).unwrap_or("").trim().to_string();
                    self.entries.push(FirmwareEntry {
                        component: FirmwareComponent::SystemBios,
                        vendor: "Apple".into(),
                        version,
                        date: String::new(),
                        device: "Boot ROM".into(),
                        updateable: true,
                        estimated_age_days: None,
                        inferred_risk_score: 0,
                    });
                }
            }
        }

        self.system_vendor = "Apple".to_string();

        // T2 / Secure Enclave
        if let Ok(output) = std::process::Command::new("system_profiler")
            .args(["SPiBridgeDataType"])
            .output()
        {
            let text = String::from_utf8(output.stdout).unwrap_or_default();
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with("Firmware Version:") || line.starts_with("Build Version:") {
                    let version = line.split(':').nth(1).unwrap_or("").trim().to_string();
                    self.entries.push(FirmwareEntry {
                        component: FirmwareComponent::EmbeddedController,
                        vendor: "Apple".into(),
                        version,
                        date: String::new(),
                        device: "T2/Secure Enclave".into(),
                        updateable: true,
                        estimated_age_days: None,
                        inferred_risk_score: 0,
                    });
                    break;
                }
            }
        }
    }
}

impl Default for FirmwareInventory {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            entries: Vec::new(),
            secure_boot: SecureBootStatus::Unknown,
            boot_mode: BootMode::Unknown,
            system_vendor: String::new(),
            system_product: String::new(),
            system_serial: String::new(),
        })
    }
}

impl std::fmt::Display for FirmwareComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SystemBios => write!(f, "System BIOS/UEFI"),
            Self::ManagementEngine => write!(f, "Management Engine"),
            Self::EmbeddedController => write!(f, "Embedded Controller"),
            Self::Tpm => write!(f, "TPM"),
            Self::Nic => write!(f, "Network Adapter"),
            Self::Storage => write!(f, "Storage"),
            Self::GpuVbios => write!(f, "GPU VBIOS"),
            Self::Thunderbolt => write!(f, "Thunderbolt"),
            Self::Bluetooth => write!(f, "Bluetooth"),
            Self::Wifi => write!(f, "WiFi"),
            Self::Bmc => write!(f, "BMC/IPMI"),
            Self::CpuMicrocode => write!(f, "CPU Microcode"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firmware_inventory_creation() {
        let inv = FirmwareInventory::new();
        assert!(inv.is_ok());
    }

    #[test]
    fn test_firmware_inventory_default() {
        let inv = FirmwareInventory::default();
        let _ = inv.risk_score();
        let _ = inv.average_firmware_age_days();
        let _ = inv.high_risk_entries();
    }

    /// Checked against Python's `datetime.date` subtraction.
    #[test]
    fn days_from_civil_is_exact() {
        for (y, m, d, days) in [
            (1970, 1, 1, 0),
            (2000, 2, 29, 11_016),
            (2000, 3, 1, 11_017),
            (2025, 9, 19, 20_350),
            (1969, 12, 31, -1),
        ] {
            assert_eq!(
                FirmwareInventory::days_from_civil(y, m, d),
                days,
                "{y}-{m}-{d}"
            );
        }
    }

    #[test]
    fn an_inventory_with_no_entries_has_no_risk_score_rather_than_zero_risk() {
        // Built directly: `default()` reads this machine's firmware first.
        let inv = FirmwareInventory {
            entries: Vec::new(),
            secure_boot: SecureBootStatus::Unknown,
            boot_mode: BootMode::Unknown,
            system_vendor: String::new(),
            system_product: String::new(),
            system_serial: String::new(),
        };
        assert_eq!(inv.risk_score(), None);
    }

    #[test]
    fn test_date_parsing() {
        assert_eq!(
            FirmwareInventory::parse_date("2024-01-15"),
            Some((2024, 1, 15))
        );
        assert_eq!(
            FirmwareInventory::parse_date("01/15/2024"),
            Some((2024, 1, 15))
        );
        assert_eq!(
            FirmwareInventory::parse_date("20240115"),
            Some((2024, 1, 15))
        );
    }

    #[test]
    fn test_risk_inference() {
        let mut entry = FirmwareEntry {
            component: FirmwareComponent::SystemBios,
            vendor: "Test".into(),
            version: "1.0".into(),
            date: String::new(),
            device: "BIOS".into(),
            updateable: true,
            estimated_age_days: Some(2000), // ~5.5 years
            inferred_risk_score: 0,
        };
        entry.inferred_risk_score = FirmwareInventory::infer_risk(&entry);
        assert!(entry.inferred_risk_score > 80); // Old BIOS = high risk
    }

    #[test]
    fn test_serialization() {
        let entry = FirmwareEntry {
            component: FirmwareComponent::SystemBios,
            vendor: "AMI".into(),
            version: "2.10".into(),
            date: "2024-01-15".into(),
            device: "System BIOS".into(),
            updateable: true,
            estimated_age_days: Some(365),
            inferred_risk_score: 20,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let _: FirmwareEntry = serde_json::from_str(&json).unwrap();
    }
}
