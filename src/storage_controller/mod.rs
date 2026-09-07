//! Storage controller monitoring — RAID arrays, HBAs, NVMe controllers.
//!
//! # Platform Support
//!
//! - **Linux**: Reads `/sys/class/scsi_host/`, `/sys/class/nvme/`, `/proc/mdstat`
//! - **Windows**: Uses WMI (`Win32_SCSIController`, `Win32_IDEController`, `MSFT_StorageSubSystem`)
//! - **macOS**: Uses `system_profiler SPStorageDataType`, `SPNVMeDataType`
//!
//! # Examples
//!
//! ```no_run
//! use simonlib::storage_controller::StorageControllerMonitor;
//!
//! let monitor = StorageControllerMonitor::new().unwrap();
//! for ctrl in monitor.controllers() {
//!     println!("{}: {} ({:?})", ctrl.name, ctrl.driver, ctrl.interface);
//! }
//! for array in monitor.raid_arrays() {
//!     println!("RAID {}: {} - {:?}", array.level, array.name, array.status);
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::error::SimonError;

/// Storage interface type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageInterface {
    /// NVMe (Non-Volatile Memory Express)
    NVMe,
    /// AHCI / SATA controller
    AHCI,
    /// SAS (Serial Attached SCSI)
    SAS,
    /// SCSI (legacy parallel)
    SCSI,
    /// IDE / PATA (legacy)
    IDE,
    /// USB mass storage
    USB,
    /// Fibre Channel
    FibreChannel,
    /// iSCSI (network)
    ISCSI,
    /// Virtio (virtualized)
    Virtio,
    /// Unknown interface
    Unknown,
}

/// RAID level
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaidLevel {
    Raid0,
    Raid1,
    Raid5,
    Raid6,
    Raid10,
    Raid50,
    Raid60,
    JBOD,
    Linear,
    Unknown(String),
}

/// RAID array status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaidStatus {
    /// Healthy / active
    Active,
    /// Degraded (missing member)
    Degraded,
    /// Rebuilding / resyncing
    Rebuilding,
    /// Failed / inactive
    Failed,
    /// Unknown status
    Unknown,
}

/// NVMe controller details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvmeControllerInfo {
    /// Controller name (e.g., "nvme0")
    pub name: String,
    /// Model name
    pub model: String,
    /// Serial number
    pub serial: String,
    /// Firmware revision
    pub firmware: String,
    /// PCIe link speed (e.g., "8 GT/s")
    pub pcie_speed: String,
    /// PCIe link width (e.g., "x4")
    pub pcie_width: String,
    /// Number of namespaces
    pub namespace_count: u32,
    /// Transport type (e.g., "pcie", "tcp")
    pub transport: String,
}

/// Storage controller information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageControllerInfo {
    /// Controller name or identifier
    pub name: String,
    /// Vendor or manufacturer
    pub vendor: String,
    /// Model or product name
    pub model: String,
    /// Driver in use
    pub driver: String,
    /// Controller interface type
    pub interface: StorageInterface,
    /// PCI address (if applicable)
    pub pci_address: String,
    /// Number of ports
    pub ports: u32,
    /// NVMe-specific info (if NVMe controller)
    pub nvme_info: Option<NvmeControllerInfo>,
}

/// Software RAID array information (mdadm, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidArrayInfo {
    /// Array name (e.g., "md0")
    pub name: String,
    /// RAID level
    pub level: RaidLevel,
    /// Current status
    pub status: RaidStatus,
    /// Number of active devices
    pub active_devices: u32,
    /// Total expected devices
    pub total_devices: u32,
    /// Failed devices count
    pub failed_devices: u32,
    /// Spare devices count
    pub spare_devices: u32,
    /// Array size in bytes
    pub size_bytes: u64,
    /// Member device names
    pub members: Vec<String>,
    /// Resync progress percentage (0-100, None if not resyncing)
    pub resync_progress: Option<f32>,
}

/// Monitor for storage controllers and RAID arrays
pub struct StorageControllerMonitor {
    controllers: Vec<StorageControllerInfo>,
    raid_arrays: Vec<RaidArrayInfo>,
}

impl StorageControllerMonitor {
    /// Create a new StorageControllerMonitor and detect controllers.
    pub fn new() -> Result<Self, SimonError> {
        let mut monitor = Self {
            controllers: Vec::new(),
            raid_arrays: Vec::new(),
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    /// Refresh storage controller detection.
    /// Re-enumerate storage controllers and RAID arrays.
    ///
    /// Returns `Err` when the enumeration failed, and `Ok` with an empty list
    /// only when it succeeded and found nothing -- which the resolver publishes
    /// as `disk.controller.<none>`. See [`crate::core::command`].
    pub fn refresh(&mut self) -> Result<(), SimonError> {
        self.controllers.clear();
        self.raid_arrays.clear();

        #[cfg(target_os = "linux")]
        {
            self.refresh_nvme_linux()?;
            self.refresh_scsi_linux()?;
            self.refresh_mdstat_linux()?;
        }

        #[cfg(target_os = "windows")]
        self.refresh_windows()?;

        #[cfg(target_os = "macos")]
        self.refresh_macos()?;

        Ok(())
    }

    /// All detected storage controllers.
    pub fn controllers(&self) -> &[StorageControllerInfo] {
        &self.controllers
    }

    /// All detected RAID arrays.
    pub fn raid_arrays(&self) -> &[RaidArrayInfo] {
        &self.raid_arrays
    }

    /// Get NVMe controllers only.
    pub fn nvme_controllers(&self) -> Vec<&StorageControllerInfo> {
        self.controllers
            .iter()
            .filter(|c| c.interface == StorageInterface::NVMe)
            .collect()
    }

    /// Get controllers by interface type.
    pub fn controllers_by_interface(&self, iface: StorageInterface) -> Vec<&StorageControllerInfo> {
        self.controllers
            .iter()
            .filter(|c| c.interface == iface)
            .collect()
    }

    /// Check if any RAID arrays are degraded.
    pub fn has_degraded_raids(&self) -> bool {
        self.raid_arrays
            .iter()
            .any(|a| a.status == RaidStatus::Degraded || a.status == RaidStatus::Failed)
    }

    // ── Linux implementations ──

    #[cfg(target_os = "linux")]
    fn refresh_nvme_linux(&mut self) -> Result<(), SimonError> {
        let nvme_base = std::path::Path::new("/sys/class/nvme");
        if !nvme_base.exists() {
            // A real answer: this kernel exposes no such class.
            return Ok(());
        }

        let entries = std::fs::read_dir(nvme_base)
            .map_err(|e| SimonError::System(format!("cannot read /sys/class/nvme: {e}")))?;
        {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("nvme") {
                    continue;
                }
                let base = entry.path();
                let model = Self::read_trimmed(&base.join("model"));
                let serial = Self::read_trimmed(&base.join("serial"));
                let firmware = Self::read_trimmed(&base.join("firmware_rev"));
                let transport = Self::read_trimmed(&base.join("transport"));

                // Count namespaces
                let ns_count = std::fs::read_dir(&base)
                    .map(|rd| {
                        rd.flatten()
                            .filter(|e| {
                                e.file_name()
                                    .to_string_lossy()
                                    .starts_with(&format!("{}n", name))
                            })
                            .count() as u32
                    })
                    .unwrap_or(0);

                // PCIe info from device/
                let pcie_speed = Self::read_trimmed(&base.join("device/current_link_speed"));
                let pcie_width = Self::read_trimmed(&base.join("device/current_link_width"));

                let nvme_info = NvmeControllerInfo {
                    name: name.clone(),
                    model: model.clone(),
                    serial: serial.clone(),
                    firmware,
                    pcie_speed,
                    pcie_width,
                    namespace_count: ns_count,
                    transport: if transport.is_empty() {
                        "pcie".to_string()
                    } else {
                        transport
                    },
                };

                self.controllers.push(StorageControllerInfo {
                    name: name.clone(),
                    vendor: String::new(),
                    model,
                    driver: "nvme".to_string(),
                    interface: StorageInterface::NVMe,
                    pci_address: Self::read_pci_address(&base),
                    ports: ns_count,
                    nvme_info: Some(nvme_info),
                });
            }
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn refresh_scsi_linux(&mut self) -> Result<(), SimonError> {
        let scsi_base = std::path::Path::new("/sys/class/scsi_host");
        if !scsi_base.exists() {
            // A real answer: this kernel exposes no such class.
            return Ok(());
        }

        let entries = std::fs::read_dir(scsi_base)
            .map_err(|e| SimonError::System(format!("cannot read /sys/class/scsi_host: {e}")))?;
        {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let base = entry.path();
                let proc_name = Self::read_trimmed(&base.join("proc_name"));

                let interface = match proc_name.as_str() {
                    "ahci" => StorageInterface::AHCI,
                    "mpt3sas" | "mpt2sas" | "megaraid_sas" => StorageInterface::SAS,
                    "uas" | "usb-storage" => StorageInterface::USB,
                    "virtio_scsi" => StorageInterface::Virtio,
                    _ => {
                        if proc_name.contains("iscsi") {
                            StorageInterface::ISCSI
                        } else {
                            StorageInterface::SCSI
                        }
                    }
                };

                let model_name = Self::read_trimmed(&base.join("model_name"));

                self.controllers.push(StorageControllerInfo {
                    name,
                    vendor: String::new(),
                    model: model_name,
                    driver: proc_name,
                    interface,
                    pci_address: String::new(),
                    ports: 0,
                    nvme_info: None,
                });
            }
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn refresh_mdstat_linux(&mut self) -> Result<(), SimonError> {
        // A kernel with no md subsystem has no /proc/mdstat, which is a real
        // answer; a file that will not open is not.
        let content = match std::fs::read_to_string("/proc/mdstat") {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(SimonError::System(format!("cannot read /proc/mdstat: {e}"))),
        };

        let mut current: Option<RaidArrayInfo> = None;

        for line in content.lines() {
            let line = line.trim();

            // "md0 : active raid1 sda1[0] sdb1[1]"
            if line.starts_with("md") && line.contains(" : ") {
                // Save previous array
                if let Some(array) = current.take() {
                    self.raid_arrays.push(array);
                }

                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 4 {
                    continue;
                }

                let name = parts[0].to_string();
                let status_str = parts[2]; // "active" or "inactive"
                let level_str = if parts.len() > 3 { parts[3] } else { "" };

                let status = match status_str {
                    "active" => RaidStatus::Active,
                    "inactive" => RaidStatus::Failed,
                    _ => RaidStatus::Unknown,
                };

                let level = match level_str {
                    "raid0" => RaidLevel::Raid0,
                    "raid1" => RaidLevel::Raid1,
                    "raid5" => RaidLevel::Raid5,
                    "raid6" => RaidLevel::Raid6,
                    "raid10" => RaidLevel::Raid10,
                    "linear" => RaidLevel::Linear,
                    other => RaidLevel::Unknown(other.to_string()),
                };

                // Extract member devices (e.g., "sda1[0]", "sdb1[1]")
                let members: Vec<String> = parts[4..]
                    .iter()
                    .filter_map(|p| p.split('[').next().map(|s| s.to_string()))
                    .collect();

                current = Some(RaidArrayInfo {
                    name,
                    level,
                    status,
                    active_devices: members.len() as u32,
                    total_devices: members.len() as u32,
                    failed_devices: 0,
                    spare_devices: 0,
                    size_bytes: 0,
                    members,
                    resync_progress: None,
                });
            } else if let Some(ref mut array) = current {
                // "123456 blocks super 1.2 [2/2] [UU]"
                if line.contains("blocks") {
                    if let Some(blocks_str) = line.split_whitespace().next() {
                        if let Ok(blocks) = blocks_str.parse::<u64>() {
                            array.size_bytes = blocks * 1024;
                        }
                    }
                    // Parse [U_] or [UU] status
                    if let Some(bracket_pos) = line.rfind('[') {
                        let remainder = &line[bracket_pos + 1..];
                        if let Some(end) = remainder.find(']') {
                            let status_chars = &remainder[..end];
                            let failed = status_chars.chars().filter(|c| *c == '_').count() as u32;
                            if failed > 0 {
                                array.status = RaidStatus::Degraded;
                                array.failed_devices = failed;
                            }
                        }
                    }
                }
                // "recovery = 45.2% ..."
                if line.contains("recovery") || line.contains("resync") {
                    if let Some(pct_str) = line
                        .split('=')
                        .nth(1)
                        .and_then(|s| s.trim().split('%').next())
                    {
                        if let Ok(pct) = pct_str.trim().parse::<f32>() {
                            array.resync_progress = Some(pct);
                            array.status = RaidStatus::Rebuilding;
                        }
                    }
                }
            }
        }

        if let Some(array) = current {
            self.raid_arrays.push(array);
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn read_trimmed(path: &std::path::Path) -> String {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    #[cfg(target_os = "linux")]
    fn read_pci_address(base: &std::path::Path) -> String {
        // Resolve the symlink to get PCI BDF
        let device_link = base.join("device");
        std::fs::read_link(&device_link)
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default()
    }

    // ── Windows implementation ──

    /// Query WMI directly rather than shelling out to PowerShell.
    ///
    /// This collector used to spawn three separate `powershell.exe` processes. Each
    /// costs roughly 1.3s in interpreter startup alone, so enumeration took about
    /// four seconds and dominated `simon profile` — 4000ms of its 4900ms total, for
    /// twelve settings. The queries themselves are trivial.
    ///
    /// `wmi` is already a dependency and is used this way elsewhere in the crate.
    #[cfg(target_os = "windows")]
    /// One WMI query, re-serialized to the JSON shape the parsers expect.
    ///
    /// This returned `Option<String>` and reached it through three `.ok()?` --
    /// COM initialization, the namespace connection and the query itself all
    /// collapsed into `None`, which the caller read as "this namespace has no
    /// rows". Each now says which step failed.
    fn query_wmi_json(namespace: &str, query: &str) -> Result<String, SimonError> {
        use std::collections::HashMap;
        use wmi::{COMLibrary, Variant, WMIConnection};

        // COM may already be initialized on this thread (the collector thread holds a
        // guard); `COMLibrary::new` handles that case.
        let com = COMLibrary::new()
            .map_err(|e| SimonError::System(format!("COM initialization failed: {e}")))?;
        let conn = WMIConnection::with_namespace_path(namespace, com).map_err(|e| {
            SimonError::System(format!("cannot connect to WMI namespace {namespace}: {e}"))
        })?;
        let rows: Vec<HashMap<String, Variant>> = conn
            .raw_query(query)
            .map_err(|e| SimonError::System(format!("WMI query failed in {namespace}: {e}")))?;

        // Re-serialize to the JSON shape the existing parsers expect, so the parsing
        // logic below is unchanged.
        let json_rows: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|row| {
                let obj: serde_json::Map<String, serde_json::Value> = row
                    .into_iter()
                    .map(|(k, v)| {
                        let value = match v {
                            Variant::String(s) => serde_json::Value::String(s),
                            Variant::I2(i) => serde_json::Value::from(i),
                            Variant::I4(i) => serde_json::Value::from(i),
                            Variant::UI4(i) => serde_json::Value::from(i),
                            Variant::Bool(b) => serde_json::Value::Bool(b),
                            _ => serde_json::Value::Null,
                        };
                        (k, value)
                    })
                    .collect();
                serde_json::Value::Object(obj)
            })
            .collect();

        serde_json::to_string(&json_rows)
            .map_err(|e| SimonError::Parse(format!("cannot re-serialize WMI rows: {e}")))
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), SimonError> {
        // Three independent queries. A machine with no SCSI controller and a
        // machine whose WMI is unreachable both used to arrive here with an
        // empty list, so the rule is the one in `usb::refresh_windows`: any
        // source succeeding is enough to trust an empty result, and every
        // source failing is not an empty machine.
        let mut failures: Vec<String> = Vec::new();
        let mut any_ok = false;

        // SCSI controllers
        match Self::query_wmi_json(
            "root\\CIMV2",
            "SELECT Name, Manufacturer, DriverName, DeviceID, Status FROM Win32_SCSIController",
        ) {
            Ok(text) => {
                any_ok = true;
                self.parse_windows_controllers(&text, StorageInterface::SCSI);
            }
            Err(e) => failures.push(format!("Win32_SCSIController: {e}")),
        }

        // IDE controllers
        match Self::query_wmi_json(
            "root\\CIMV2",
            "SELECT Name, Manufacturer, DriverName, DeviceID, Status FROM Win32_IDEController",
        ) {
            Ok(text) => {
                any_ok = true;
                self.parse_windows_controllers(&text, StorageInterface::AHCI);
            }
            Err(e) => failures.push(format!("Win32_IDEController: {e}")),
        }

        // There is deliberately no `MSFT_PhysicalDisk` query here.
        //
        // One used to run, `WHERE BusType = 17`, and push each row as a
        // controller. `MSFT_PhysicalDisk` is what `Get-PhysicalDisk` wraps: it
        // enumerates **disks**. So every NVMe device appeared twice -- once as
        // its actual controller from `Win32_SCSIController`, and once as the
        // drive itself -- and this host reported nine controllers where six
        // exist, three of them named `Samsung SSD 990 PRO 4TB` and carrying no
        // PCI address, because a disk does not have a controller's address.
        //
        // Everything that branch published about those drives -- model, serial,
        // firmware -- is disk identity, and `disk.{n}` already reports it from
        // a reader that knows it is describing a disk. The `nvme_info` it
        // attached was never resolved into the ontology at all, and its
        // `namespace_count: 1` was a constant.
        if any_ok {
            Ok(())
        } else {
            Err(SimonError::System(format!(
                "no storage controller enumeration succeeded: {}",
                failures.join("; ")
            )))
        }
    }

    #[cfg(target_os = "windows")]
    fn parse_windows_controllers(&mut self, json_text: &str, default_iface: StorageInterface) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_text) {
            let items = match &val {
                serde_json::Value::Array(arr) => arr.clone(),
                obj @ serde_json::Value::Object(_) => vec![obj.clone()],
                _ => return,
            };
            for item in &items {
                let name = item["Name"].as_str().unwrap_or("").to_string();
                let vendor = item["Manufacturer"].as_str().unwrap_or("").to_string();
                let driver = item["DriverName"].as_str().unwrap_or("").to_string();

                // The bound driver names the transport, and the row already
                // carries it. `default_iface` is the class of the WMI query
                // that returned the row, which is not a property of the
                // device: Windows exposes NVMe controllers through
                // `Win32_SCSIController`, so all three on this host were
                // published as `SCSI`.
                //
                // The name check below it did not save them either. It looked
                // for `nvme`, and Windows calls them `Standard NVM Express
                // Controller` -- the substring the reader wanted is the one
                // string Windows does not use.
                let lower_name = name.to_lowercase();
                let interface = match driver.to_lowercase().as_str() {
                    "stornvme" => StorageInterface::NVMe,
                    "storahci" => StorageInterface::AHCI,
                    _ if lower_name.contains("nvme") || lower_name.contains("nvm express") => {
                        StorageInterface::NVMe
                    }
                    _ if lower_name.contains("sas") => StorageInterface::SAS,
                    _ if lower_name.contains("usb") => StorageInterface::USB,
                    _ if lower_name.contains("ahci") || lower_name.contains("sata") => {
                        StorageInterface::AHCI
                    }
                    _ => default_iface.clone(),
                };

                // `DeviceID` is a device instance path, not an address. For a
                // controller on a PCI bus Windows records the address in the
                // same registry value the PCI enumeration reads, so ask for it:
                // all three NVMe controllers here resolve to `0000:02:00.0`,
                // `0000:06:00.0` and `0000:0d:00.0`, each of which is a node in
                // `pci.*` in the same snapshot. The ontology was refusing the
                // instance path -- correctly -- under a reason claiming the
                // address could not be joined against `pci.*`.
                //
                // The path is kept when there is no address to be had: a
                // `ROOT\SPACEPORT\0000` or `SWD\XVDDENUM\...` controller is not
                // on a PCI bus, the ontology declines to publish it, and the
                // absence reason quotes the path, which says exactly that.
                let device_id = item["DeviceID"].as_str().unwrap_or("").to_string();
                let pci_address = crate::pci_devices::pci_address_of(&device_id)
                    .unwrap_or_else(|| device_id.clone());

                self.controllers.push(StorageControllerInfo {
                    name: name.clone(),
                    vendor,
                    model: name,
                    driver,
                    interface,
                    pci_address,
                    ports: 0,
                    nvme_info: None,
                });
            }
        }
    }

    // ── macOS implementation ──

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), SimonError> {
        // NVMe controllers
        let nvme =
            crate::core::command::capture_json("system_profiler", &["SPNVMeDataType", "-json"])?;
        if let Some(val) = nvme {
            if let Some(items) = val["SPNVMeDataType"].as_array() {
                for (i, item) in items.iter().enumerate() {
                    let name = item["_name"].as_str().unwrap_or("NVMe").to_string();
                    let model = item["device_model"].as_str().unwrap_or("").to_string();
                    let serial = item["device_serial"].as_str().unwrap_or("").to_string();
                    let firmware = item["device_revision"].as_str().unwrap_or("").to_string();

                    let nvme_info = NvmeControllerInfo {
                        name: format!("nvme{}", i),
                        model: model.clone(),
                        serial,
                        firmware,
                        pcie_speed: item["spnvme_linkspeed"].as_str().unwrap_or("").to_string(),
                        pcie_width: item["spnvme_linkwidth"].as_str().unwrap_or("").to_string(),
                        namespace_count: 1,
                        transport: "pcie".to_string(),
                    };

                    self.controllers.push(StorageControllerInfo {
                        name,
                        vendor: String::new(),
                        model,
                        driver: "nvme".to_string(),
                        interface: StorageInterface::NVMe,
                        pci_address: String::new(),
                        // Not read here either; see above.
                        ports: 0,
                        nvme_info: Some(nvme_info),
                    });
                }
            }
        }

        // SATA controllers
        let sata = crate::core::command::capture_json(
            "system_profiler",
            &["SPSerialATADataType", "-json"],
        )?;
        if let Some(val) = sata {
            if let Some(items) = val["SPSerialATADataType"].as_array() {
                for item in items {
                    let name = item["_name"].as_str().unwrap_or("SATA").to_string();
                    let vendor = item["spsata_vendor"].as_str().unwrap_or("").to_string();
                    let port_count = item["_items"]
                        .as_array()
                        .map(|a| a.len() as u32)
                        .unwrap_or(0);

                    self.controllers.push(StorageControllerInfo {
                        name,
                        vendor,
                        model: String::new(),
                        driver: "ahci".to_string(),
                        interface: StorageInterface::AHCI,
                        pci_address: String::new(),
                        ports: port_count,
                        nvme_info: None,
                    });
                }
            }
        }
        Ok(())
    }
}

impl Default for StorageControllerMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            controllers: Vec::new(),
            raid_arrays: Vec::new(),
        })
    }
}

impl std::fmt::Display for StorageInterface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NVMe => write!(f, "NVMe"),
            Self::AHCI => write!(f, "AHCI/SATA"),
            Self::SAS => write!(f, "SAS"),
            Self::SCSI => write!(f, "SCSI"),
            Self::IDE => write!(f, "IDE"),
            Self::USB => write!(f, "USB"),
            Self::FibreChannel => write!(f, "Fibre Channel"),
            Self::ISCSI => write!(f, "iSCSI"),
            Self::Virtio => write!(f, "Virtio"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl std::fmt::Display for RaidLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Raid0 => write!(f, "RAID 0"),
            Self::Raid1 => write!(f, "RAID 1"),
            Self::Raid5 => write!(f, "RAID 5"),
            Self::Raid6 => write!(f, "RAID 6"),
            Self::Raid10 => write!(f, "RAID 10"),
            Self::Raid50 => write!(f, "RAID 50"),
            Self::Raid60 => write!(f, "RAID 60"),
            Self::JBOD => write!(f, "JBOD"),
            Self::Linear => write!(f, "Linear"),
            Self::Unknown(s) => write!(f, "{}", s),
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
    fn test_storage_controller_creation() {
        match StorageControllerMonitor::new() {
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
    fn test_storage_controller_default() {
        let monitor = StorageControllerMonitor::default();
        let _ = monitor.controllers();
        let _ = monitor.raid_arrays();
        let _ = monitor.nvme_controllers();
        let _ = monitor.has_degraded_raids();
    }

    #[test]
    fn test_nvme_info_serialization() {
        let info = NvmeControllerInfo {
            name: "nvme0".into(),
            model: "Samsung 980 PRO".into(),
            serial: "ABC123".into(),
            firmware: "5B2QGXA7".into(),
            pcie_speed: "8 GT/s".into(),
            pcie_width: "x4".into(),
            namespace_count: 1,
            transport: "pcie".into(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("Samsung"));
        let _: NvmeControllerInfo = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_raid_serialization() {
        let array = RaidArrayInfo {
            name: "md0".into(),
            level: RaidLevel::Raid1,
            status: RaidStatus::Active,
            active_devices: 2,
            total_devices: 2,
            failed_devices: 0,
            spare_devices: 0,
            size_bytes: 1024 * 1024 * 1024,
            members: vec!["sda1".into(), "sdb1".into()],
            resync_progress: None,
        };
        let json = serde_json::to_string(&array).unwrap();
        assert!(json.contains("md0"));
        let _: RaidArrayInfo = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_interface_display() {
        assert_eq!(StorageInterface::NVMe.to_string(), "NVMe");
        assert_eq!(StorageInterface::AHCI.to_string(), "AHCI/SATA");
        assert_eq!(RaidLevel::Raid1.to_string(), "RAID 1");
    }
}
