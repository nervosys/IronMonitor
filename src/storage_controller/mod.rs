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
//! use ironmonitor::storage_controller::StorageControllerMonitor;
//!
//! let monitor = StorageControllerMonitor::new().unwrap();
//! for ctrl in monitor.controllers() {
//!     println!(
//!         "{}: {} ({:?})",
//!         ctrl.name,
//!         ctrl.driver.as_deref().unwrap_or("unknown driver"),
//!         ctrl.interface
//!     );
//! }
//! for array in monitor.raid_arrays() {
//!     println!("RAID {:?}: {} - {:?}", array.level, array.name, array.status);
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::error::IronError;

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
    /// Failed
    Failed,
    /// Inactive: the kernel lists the array but has not started it. Not the
    /// same as `Failed`, which is an array that is running and broken. This
    /// used to be mapped to `Failed`.
    Inactive,
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
    /// Number of namespaces, or `None` where it was not read.
    pub namespace_count: Option<u32>,
    /// Transport type (e.g., "pcie", "tcp"), or `None` where it was not read.
    pub transport: Option<String>,
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
    /// Driver in use, or `None` where it was not read.
    pub driver: Option<String>,
    /// Controller interface type
    pub interface: StorageInterface,
    /// PCI address (if applicable)
    pub pci_address: String,
    /// Number of ports, or `None` where it was not read. Not zero: zero is a
    /// controller with no ports.
    pub ports: Option<u32>,
    /// NVMe-specific info (if NVMe controller)
    pub nvme_info: Option<NvmeControllerInfo>,
}

/// Software RAID array information (mdadm, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidArrayInfo {
    /// Array name (e.g., "md0")
    pub name: String,
    /// RAID level, or `None` when the array's state line names no personality.
    /// An inactive array names none, so this is `None` there rather than a
    /// member device or a guessed level.
    pub level: Option<RaidLevel>,
    /// Current status
    pub status: RaidStatus,
    /// Members neither failed, spare nor journal, counted from the member list.
    /// The kernel's own figure is `working_devices`.
    pub active_devices: u32,
    /// Member devices enumerated on the array line, including failed and spare ones
    pub total_devices: u32,
    /// Devices the array wants (`n` in `[n/m]`). None if no blocks line was read.
    pub wanted_devices: Option<u32>,
    /// Devices working (`m` in `[n/m]`). None if no blocks line was read.
    pub working_devices: Option<u32>,
    /// Members flagged `(F)` on the array line
    pub failed_devices: u32,
    /// Members flagged `(S)` on the array line
    pub spare_devices: u32,
    /// Array size in bytes. The blocks line counts 1 KiB units.
    /// None if no blocks line was read.
    pub size_bytes: Option<u64>,
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
    pub fn new() -> Result<Self, IronError> {
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
    pub fn refresh(&mut self) -> Result<(), IronError> {
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

    /// Check if any RAID arrays are degraded, failed or stopped. An inactive
    /// array is counted here as before, when it was mapped to `Failed`.
    pub fn has_degraded_raids(&self) -> bool {
        self.raid_arrays.iter().any(|a| {
            matches!(
                a.status,
                RaidStatus::Degraded | RaidStatus::Failed | RaidStatus::Inactive
            )
        })
    }

    // ── Linux implementations ──

    #[cfg(target_os = "linux")]
    fn refresh_nvme_linux(&mut self) -> Result<(), IronError> {
        let nvme_base = std::path::Path::new("/sys/class/nvme");
        if !nvme_base.exists() {
            // A real answer: this kernel exposes no such class.
            return Ok(());
        }

        let entries = std::fs::read_dir(nvme_base)
            .map_err(|e| IronError::System(format!("cannot read /sys/class/nvme: {e}")))?;
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
                // None when the controller directory cannot be listed. It used
                // to be `0`, which reads as a controller with no namespaces.
                let ns_count = std::fs::read_dir(&base).ok().map(|rd| {
                    rd.flatten()
                        .filter(|e| {
                            e.file_name()
                                .to_string_lossy()
                                .starts_with(&format!("{}n", name))
                        })
                        .count() as u32
                });

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
                    // No transport file means no transport was read. It used to
                    // fall back to "pcie", which the kernel was never asked for.
                    transport: (!transport.is_empty()).then_some(transport),
                };

                self.controllers.push(StorageControllerInfo {
                    name: name.clone(),
                    vendor: String::new(),
                    model,
                    // The driver the device is bound to, from its sysfs link.
                    // This used to be the literal "nvme" whatever was bound.
                    driver: std::fs::read_link(base.join("device/driver"))
                        .ok()
                        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())),
                    interface: StorageInterface::NVMe,
                    pci_address: Self::read_pci_address(&base),
                    // Not a port count. `ns_count` is the namespaces on the
                    // controller, and used to be reported as its ports.
                    ports: None,
                    nvme_info: Some(nvme_info),
                });
            }
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn refresh_scsi_linux(&mut self) -> Result<(), IronError> {
        let scsi_base = std::path::Path::new("/sys/class/scsi_host");
        if !scsi_base.exists() {
            // A real answer: this kernel exposes no such class.
            return Ok(());
        }

        let entries = std::fs::read_dir(scsi_base)
            .map_err(|e| IronError::System(format!("cannot read /sys/class/scsi_host: {e}")))?;
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
                    _ if proc_name.contains("iscsi") => StorageInterface::ISCSI,
                    // No evidence in the host's name for an interface. It used
                    // to fall through to SCSI, which was a guess for every host
                    // this match does not list, and for a host whose name was
                    // never read.
                    _ => StorageInterface::Unknown,
                };

                let model_name = Self::read_trimmed(&base.join("model_name"));

                self.controllers.push(StorageControllerInfo {
                    name,
                    vendor: String::new(),
                    model: model_name,
                    // `proc_name` is the host's own report; empty when unread.
                    driver: (!proc_name.is_empty()).then_some(proc_name),
                    interface,
                    pci_address: String::new(),
                    // Not read from this host's sysfs. It used to be `0`.
                    ports: None,
                    nvme_info: None,
                });
            }
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn refresh_mdstat_linux(&mut self) -> Result<(), IronError> {
        // A kernel with no md subsystem has no /proc/mdstat, which is a real
        // answer; a file that will not open is not.
        let content = match std::fs::read_to_string("/proc/mdstat") {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(IronError::System(format!("cannot read /proc/mdstat: {e}"))),
        };
        self.raid_arrays.extend(Self::parse_mdstat(&content));
        Ok(())
    }

    /// Parse the text of `/proc/mdstat` into one entry per array. Takes the text
    /// rather than reading the file, so it can be tested on any platform.
    ///
    /// An array line is `mdN : <state> [<personality>] <members...>`. A running
    /// array names its personality (`raid1`, `raid5`, ...). An inactive array
    /// names none, and its first member would read as one if the level were
    /// taken by position, so the level is taken only when the word is a known
    /// personality. Members are every token of the form `name[index]`, with an
    /// optional `(F)`, `(S)` or `(J)` flag.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn parse_mdstat(content: &str) -> Vec<RaidArrayInfo> {
        let mut arrays = Vec::new();
        let mut current: Option<RaidArrayInfo> = None;

        for line in content.lines() {
            let line = line.trim();

            // "md0 : active raid1 sda1[0] sdb1[1]"
            // "md127 : inactive sdb[1](S) sda[0](S)"
            if line.starts_with("md") && line.contains(" : ") {
                // Save previous array
                if let Some(array) = current.take() {
                    arrays.push(array);
                }

                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 3 {
                    continue;
                }

                let name = parts[0].to_string();
                let status = match parts[2] {
                    "active" => RaidStatus::Active,
                    "inactive" => RaidStatus::Inactive,
                    _ => RaidStatus::Unknown,
                };

                // The personality is the word after the state, when one is named.
                let level = parts
                    .get(3)
                    .filter(|word| Self::is_md_personality(word))
                    .map(|word| Self::raid_level_of(word));

                let listed: Vec<(&str, &str)> = parts[3..]
                    .iter()
                    .filter_map(|t| Self::parse_member(t))
                    .collect();
                let members: Vec<String> = listed.iter().map(|(n, _)| n.to_string()).collect();

                // Member flags follow the index: "sdc1[2](F)", "sdd1[3](S)".
                // Counted from what the line lists, so a flag that is absent
                // reads as zero of that flag.
                let failed_devices =
                    listed.iter().filter(|(_, f)| f.contains("(F)")).count() as u32;
                let spare_devices = listed.iter().filter(|(_, f)| f.contains("(S)")).count() as u32;
                let active_devices = listed
                    .iter()
                    .filter(|(_, f)| !f.contains("(F)") && !f.contains("(S)") && !f.contains("(J)"))
                    .count() as u32;

                current = Some(RaidArrayInfo {
                    name,
                    level,
                    status,
                    active_devices,
                    total_devices: members.len() as u32,
                    wanted_devices: None,
                    working_devices: None,
                    failed_devices,
                    spare_devices,
                    size_bytes: None,
                    members,
                    resync_progress: None,
                });
            } else if let Some(ref mut array) = current {
                // "123456 blocks super 1.2 [2/2] [UU]"
                if line.contains("blocks") {
                    if let Some(blocks_str) = line.split_whitespace().next() {
                        if let Ok(blocks) = blocks_str.parse::<u64>() {
                            array.size_bytes = Some(blocks * 1024);
                        }
                    }
                    // "[2/2]": devices the array wants (n) and devices working (m)
                    let counts = line.split_whitespace().find_map(|t| {
                        let (n, m) = t.strip_prefix('[')?.strip_suffix(']')?.split_once('/')?;
                        Some((n.parse::<u32>().ok()?, m.parse::<u32>().ok()?))
                    });
                    if let Some((wanted, working)) = counts {
                        array.wanted_devices = Some(wanted);
                        array.working_devices = Some(working);
                    }
                    // Parse [U_] or [UU] status
                    if let Some(bracket_pos) = line.rfind('[') {
                        let remainder = &line[bracket_pos + 1..];
                        if let Some(end) = remainder.find(']') {
                            let status_chars = &remainder[..end];
                            if status_chars.contains('_') {
                                array.status = RaidStatus::Degraded;
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
            arrays.push(array);
        }
        arrays
    }

    /// Whether a word in an md array line is a personality the kernel can run.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn is_md_personality(word: &str) -> bool {
        matches!(
            word,
            "raid0"
                | "raid1"
                | "raid4"
                | "raid5"
                | "raid6"
                | "raid10"
                | "linear"
                | "multipath"
                | "faulty"
        )
    }

    /// The level a personality word names. Personalities with no `RaidLevel`
    /// variant (`raid4`, `multipath`, `faulty`) keep their own name.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn raid_level_of(word: &str) -> RaidLevel {
        match word {
            "raid0" => RaidLevel::Raid0,
            "raid1" => RaidLevel::Raid1,
            "raid5" => RaidLevel::Raid5,
            "raid6" => RaidLevel::Raid6,
            "raid10" => RaidLevel::Raid10,
            "linear" => RaidLevel::Linear,
            other => RaidLevel::Unknown(other.to_string()),
        }
    }

    /// One member token of an array line: `sdb1[1]`, or `sdc1[2](F)` with a
    /// flag. Returns the device name and the text after the index, which holds
    /// the flag. `None` for any token that is not a member, such as a
    /// personality word or a blocks count.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn parse_member(token: &str) -> Option<(&str, &str)> {
        let (name, after) = token.split_once('[')?;
        if name.is_empty() {
            return None;
        }
        let (index, flags) = after.split_once(']')?;
        if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some((name, flags))
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
    /// four seconds and dominated `ironmon profile` — 4000ms of its 4900ms total, for
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
    fn query_wmi_json(namespace: &str, query: &str) -> Result<String, IronError> {
        use std::collections::HashMap;
        use wmi::{COMLibrary, Variant, WMIConnection};

        // COM may already be initialized on this thread (the collector thread holds a
        // guard); `COMLibrary::new` handles that case.
        let com = COMLibrary::new()
            .map_err(|e| IronError::System(format!("COM initialization failed: {e}")))?;
        let conn = WMIConnection::with_namespace_path(namespace, com).map_err(|e| {
            IronError::System(format!("cannot connect to WMI namespace {namespace}: {e}"))
        })?;
        let rows: Vec<HashMap<String, Variant>> = conn
            .raw_query(query)
            .map_err(|e| IronError::System(format!("WMI query failed in {namespace}: {e}")))?;

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
            .map_err(|e| IronError::Parse(format!("cannot re-serialize WMI rows: {e}")))
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), IronError> {
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
                self.parse_windows_controllers(&text);
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
                self.parse_windows_controllers(&text);
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
            Err(IronError::System(format!(
                "no storage controller enumeration succeeded: {}",
                failures.join("; ")
            )))
        }
    }

    #[cfg(target_os = "windows")]
    fn parse_windows_controllers(&mut self, json_text: &str) {
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
                // carries it. The WMI class that returned the row is not used:
                // it is the query's class, not a property of the device.
                // Windows exposes NVMe controllers through
                // `Win32_SCSIController`, so the class said SCSI for all three
                // on this host. A row whose driver and name match nothing is
                // Unknown.
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
                    _ => StorageInterface::Unknown,
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
                    // `DriverName` is empty when Windows has none to report.
                    driver: (!driver.is_empty()).then_some(driver),
                    interface,
                    pci_address,
                    // Not read from this query. It used to be `0`.
                    ports: None,
                    nvme_info: None,
                });
            }
        }
    }

    // ── macOS implementation ──

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), IronError> {
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
                        // Neither is read from `SPNVMeDataType`. They were the
                        // literals `1` and "pcie" for every drive.
                        namespace_count: None,
                        transport: None,
                    };

                    self.controllers.push(StorageControllerInfo {
                        name,
                        vendor: String::new(),
                        model,
                        // Not read here; the driver name was a literal. It used to be "nvme".
                        driver: None,
                        interface: StorageInterface::NVMe,
                        pci_address: String::new(),
                        // Not read here either; see above. It used to be `0`.
                        ports: None,
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
                    // `_items` lists the devices attached to the controller, not
                    // its ports: a six-port controller with two drives would read
                    // as two. The port count is not read, so it is None.
                    self.controllers.push(StorageControllerInfo {
                        name,
                        vendor,
                        model: String::new(),
                        // Not read; "ahci" was a literal for every SATA controller.
                        driver: None,
                        // The data type says SATA, not AHCI, and nothing read
                        // here names the programming interface. It used to be
                        // AHCI for every controller.
                        interface: StorageInterface::Unknown,
                        pci_address: String::new(),
                        ports: None,
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
            namespace_count: Some(1),
            transport: Some("pcie".into()),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("Samsung"));
        let _: NvmeControllerInfo = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_raid_serialization() {
        let array = RaidArrayInfo {
            name: "md0".into(),
            level: Some(RaidLevel::Raid1),
            status: RaidStatus::Active,
            active_devices: 2,
            total_devices: 2,
            wanted_devices: Some(2),
            working_devices: Some(2),
            failed_devices: 0,
            spare_devices: 0,
            size_bytes: Some(1024 * 1024 * 1024),
            members: vec!["sda1".into(), "sdb1".into()],
            resync_progress: None,
        };
        let json = serde_json::to_string(&array).unwrap();
        assert!(json.contains("md0"));
        let _: RaidArrayInfo = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_parse_mdstat_counts_flags_and_blocks() {
        let text = concat!(
            "Personalities : [raid1] [raid5]\n",
            "md1 : active raid5 sdd1[3](S) sdc1[2](F) sdb1[1] sda1[0]\n",
            "      2093056 blocks super 1.2 level 5, 512k chunk, algorithm 2 [3/2] [UU_]\n",
            "\n",
            "unused devices: <none>\n",
        );
        let arrays = StorageControllerMonitor::parse_mdstat(text);
        assert_eq!(arrays.len(), 1);
        let md1 = &arrays[0];
        assert_eq!(md1.name, "md1");
        assert_eq!(md1.level, Some(RaidLevel::Raid5));
        assert_eq!(md1.status, RaidStatus::Degraded);
        assert_eq!(md1.members, vec!["sdd1", "sdc1", "sdb1", "sda1"]);
        assert_eq!(md1.total_devices, 4);
        assert_eq!(md1.active_devices, 2);
        assert_eq!(md1.failed_devices, 1);
        assert_eq!(md1.spare_devices, 1);
        assert_eq!(md1.wanted_devices, Some(3));
        assert_eq!(md1.working_devices, Some(2));
        assert_eq!(md1.size_bytes, Some(2093056 * 1024));
    }

    #[test]
    fn test_parse_mdstat_active_raid1() {
        let text = concat!(
            "Personalities : [raid1]\n",
            "md0 : active raid1 sdb1[1] sda1[0]\n",
            "      976630464 blocks super 1.2 [2/2] [UU]\n",
            "      bitmap: 0/8 pages [0KB], 65536KB chunk\n",
            "\n",
            "unused devices: <none>\n",
        );
        let arrays = StorageControllerMonitor::parse_mdstat(text);
        assert_eq!(arrays.len(), 1);
        let md0 = &arrays[0];
        assert_eq!(md0.name, "md0");
        assert_eq!(md0.level, Some(RaidLevel::Raid1));
        assert_eq!(md0.status, RaidStatus::Active);
        assert_eq!(md0.members, vec!["sdb1", "sda1"]);
        assert_eq!(md0.total_devices, 2);
        assert_eq!(md0.active_devices, 2);
        assert_eq!(md0.failed_devices, 0);
        assert_eq!(md0.spare_devices, 0);
        assert_eq!(md0.wanted_devices, Some(2));
        assert_eq!(md0.working_devices, Some(2));
        assert_eq!(md0.size_bytes, Some(976630464 * 1024));
        assert_eq!(md0.resync_progress, None);
    }

    /// An inactive array has no personality word, so its first member must not
    /// be read as the level, and no member may be dropped. It is `Inactive`,
    /// not `Failed`. The next array's blocks line must not leak into it.
    #[test]
    fn test_parse_mdstat_inactive_array() {
        let text = concat!(
            "Personalities : [raid1]\n",
            "md127 : inactive sdb[1](S) sda[0](S)\n",
            "      2930135040 blocks super 1.2\n",
            "\n",
            "md0 : active raid1 sdc1[1] sdd1[0]\n",
            "      1046528 blocks super 1.2 [2/2] [UU]\n",
            "\n",
            "unused devices: <none>\n",
        );
        let arrays = StorageControllerMonitor::parse_mdstat(text);
        assert_eq!(arrays.len(), 2);

        let md127 = &arrays[0];
        assert_eq!(md127.name, "md127");
        assert_eq!(md127.level, None);
        assert_eq!(md127.status, RaidStatus::Inactive);
        assert_eq!(md127.members, vec!["sdb", "sda"]);
        assert_eq!(md127.total_devices, 2);
        assert_eq!(md127.spare_devices, 2);
        assert_eq!(md127.failed_devices, 0);
        assert_eq!(md127.active_devices, 0);
        assert_eq!(md127.wanted_devices, None);
        assert_eq!(md127.working_devices, None);
        assert_eq!(md127.size_bytes, Some(2930135040 * 1024));

        let md0 = &arrays[1];
        assert_eq!(md0.name, "md0");
        assert_eq!(md0.level, Some(RaidLevel::Raid1));
        assert_eq!(md0.status, RaidStatus::Active);
        assert_eq!(md0.size_bytes, Some(1046528 * 1024));
    }

    #[test]
    fn test_parse_mdstat_empty_is_no_arrays() {
        let text = "Personalities : \nunused devices: <none>\n";
        assert!(StorageControllerMonitor::parse_mdstat(text).is_empty());
    }

    #[test]
    fn test_interface_display() {
        assert_eq!(StorageInterface::NVMe.to_string(), "NVMe");
        assert_eq!(StorageInterface::AHCI.to_string(), "AHCI/SATA");
        assert_eq!(RaidLevel::Raid1.to_string(), "RAID 1");
    }
}
