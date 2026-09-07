//! PCI device enumeration — all PCI/PCIe devices with class, vendor, driver info.
//!
//! # Platform Support
//!
//! - **Linux**: Reads `/sys/bus/pci/devices/`
//! - **Windows**: Uses WMI (`Win32_PnPEntity` with PCI bus)
//! - **macOS**: Uses `system_profiler SPPCIDataType`
//!
//! # Examples
//!
//! ```no_run
//! use simonlib::pci_devices::{PciDeviceMonitor, PciClass};
//!
//! let monitor = PciDeviceMonitor::new().unwrap();
//! for dev in monitor.devices() {
//!     println!("[{}] {} {} (driver: {})",
//!         dev.address, dev.vendor_name, dev.device_name, dev.driver);
//! }
//! println!("GPU devices: {}", monitor.devices_by_class(PciClass::DisplayController).len());
//! ```

use crate::error::SimonError;
use serde::{Deserialize, Serialize};

/// PCI device class (major categories)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PciClass {
    /// 00: Unclassified
    Unclassified,
    /// 01: Mass storage controller (SATA, NVMe, RAID, etc.)
    MassStorage,
    /// 02: Network controller (Ethernet, WiFi, etc.)
    NetworkController,
    /// 03: Display controller (GPU, VGA)
    DisplayController,
    /// 04: Multimedia controller (audio, video)
    MultimediaController,
    /// 05: Memory controller
    MemoryController,
    /// 06: Bridge (PCI-to-PCI, host bridge, ISA bridge)
    Bridge,
    /// 07: Communication controller (serial, modem)
    CommunicationController,
    /// 08: System peripheral (DMA, timer, PIC)
    SystemPeripheral,
    /// 09: Input device controller
    InputDevice,
    /// 0A: Docking station
    DockingStation,
    /// 0B: Processor
    Processor,
    /// 0C: Serial bus controller (USB, FireWire, SMBus)
    SerialBusController,
    /// 0D: Wireless controller (Bluetooth, WiFi, etc.)
    WirelessController,
    /// 0E: Intelligent controller (I2O)
    IntelligentController,
    /// 0F: Satellite communication
    SatelliteComm,
    /// 10: Encryption controller
    EncryptionController,
    /// 11: Signal processing controller
    SignalProcessing,
    /// 12: Processing accelerator (NPU, FPGA)
    ProcessingAccelerator,
    /// 13: Non-essential instrumentation
    Instrumentation,
    /// 40: Co-processor
    CoProcessor,
    /// Other/unknown class
    Other(u8),
}

/// PCIe link information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PciLinkInfo {
    /// Current link speed (e.g., "8.0 GT/s", "16.0 GT/s")
    pub speed: String,
    /// Current link width (e.g., "x1", "x4", "x16")
    pub width: String,
    /// Maximum supported speed
    pub max_speed: String,
    /// Maximum supported width
    pub max_width: String,
    /// PCIe generation (1-5)
    pub generation: u8,
}

/// Information about a single PCI device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PciDeviceInfo {
    /// BDF address (e.g., "0000:00:1f.3")
    pub address: String,
    /// Vendor ID (hex, e.g., "8086")
    pub vendor_id: String,
    /// Device ID (hex, e.g., "a170")
    pub device_id: String,
    /// Subsystem vendor ID
    pub subsystem_vendor_id: String,
    /// Subsystem device ID
    pub subsystem_device_id: String,
    /// PCI class code (2-digit hex)
    pub class_code: String,
    /// Decoded device class
    pub class: PciClass,
    /// Vendor name (human readable)
    pub vendor_name: String,
    /// Device name (human readable)
    pub device_name: String,
    /// Kernel driver in use
    pub driver: String,
    /// Kernel module loaded for this device
    pub kernel_module: String,
    /// PCI revision
    pub revision: String,
    /// IOMMU group (for passthrough/vfio)
    pub iommu_group: Option<u32>,
    /// PCIe link information (if PCIe device)
    pub link_info: Option<PciLinkInfo>,
    /// Whether the device supports SR-IOV
    pub sriov_capable: bool,
    /// Number of SR-IOV virtual functions
    pub sriov_vfs: u32,
    /// NUMA node affinity (-1 if not applicable)
    pub numa_node: i32,
    /// Power state (e.g., "D0", "D3hot")
    pub power_state: String,
}

/// Monitor for PCI devices
pub struct PciDeviceMonitor {
    devices: Vec<PciDeviceInfo>,
}

/// The `domain:bus:device.function` address Windows records for a PnP device.
///
/// The same registry lookup the PCI enumeration uses, exposed for readers that
/// hold a device instance path and want the address it corresponds to. The
/// storage-controller reader is the first: `Win32_SCSIController.DeviceID` is
/// an instance path like
/// `PCI\VEN_144D&DEV_A810&SUBSYS_A801144D&REV_00\4&2EC11BF&0&000A`, which was
/// published as this crate's `pci_address` field and then correctly refused by
/// the ontology, under a reason saying it "cannot be joined against `pci.*`".
/// It can: that device's `LocationInformation` says PCI bus 2, device 0,
/// function 0, and `pci.0000_02_00_0` is in the same snapshot.
///
/// `None` for a device Windows records no location for -- a `ROOT\` or `SWD\`
/// instance path is not on a PCI bus at all, and there is nothing to find.
#[cfg(target_os = "windows")]
pub(crate) fn pci_address_of(pnp_id: &str) -> Option<String> {
    PciDeviceMonitor::registry_location_and_service(pnp_id).0
}

impl PciDeviceMonitor {
    pub fn new() -> Result<Self, SimonError> {
        let mut monitor = Self {
            devices: Vec::new(),
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    /// Re-enumerate the PCI bus.
    ///
    /// Returns `Err` when the enumeration itself failed, and `Ok` with an empty
    /// device list only when the enumeration succeeded and found nothing. The
    /// three platform readers used to swallow every failure -- a spawn error, a
    /// non-zero exit, empty output, unparseable JSON -- and this returned
    /// `Ok(())` regardless. A failed enumeration was therefore indistinguishable
    /// from a machine with no PCI devices, and the resolver above published the
    /// second: "no PCI devices enumerated on this machine". It was found by
    /// `resolution_is_stable_across_calls` going red once under a fully loaded
    /// test run, where one PowerShell spawn lost the race.
    pub fn refresh(&mut self) -> Result<(), SimonError> {
        self.devices.clear();

        #[cfg(target_os = "linux")]
        self.refresh_linux()?;

        #[cfg(target_os = "windows")]
        self.refresh_windows()?;

        #[cfg(target_os = "macos")]
        self.refresh_macos()?;

        Ok(())
    }

    pub fn devices(&self) -> &[PciDeviceInfo] {
        &self.devices
    }

    /// Get devices by class.
    pub fn devices_by_class(&self, class: PciClass) -> Vec<&PciDeviceInfo> {
        self.devices.iter().filter(|d| d.class == class).collect()
    }

    /// Get devices by vendor ID.
    pub fn devices_by_vendor(&self, vendor_id: &str) -> Vec<&PciDeviceInfo> {
        self.devices
            .iter()
            .filter(|d| d.vendor_id == vendor_id)
            .collect()
    }

    /// Get all GPU devices.
    pub fn gpus(&self) -> Vec<&PciDeviceInfo> {
        self.devices_by_class(PciClass::DisplayController)
    }

    /// Get all network devices.
    pub fn network_devices(&self) -> Vec<&PciDeviceInfo> {
        let mut devs = self.devices_by_class(PciClass::NetworkController);
        devs.extend(self.devices_by_class(PciClass::WirelessController));
        devs
    }

    /// Get all storage controllers.
    pub fn storage_devices(&self) -> Vec<&PciDeviceInfo> {
        self.devices_by_class(PciClass::MassStorage)
    }

    /// Get SR-IOV capable devices.
    pub fn sriov_devices(&self) -> Vec<&PciDeviceInfo> {
        self.devices.iter().filter(|d| d.sriov_capable).collect()
    }

    /// Classify a PCI device by its class byte.
    pub fn classify_pci(class_byte: u8) -> PciClass {
        match class_byte {
            0x00 => PciClass::Unclassified,
            0x01 => PciClass::MassStorage,
            0x02 => PciClass::NetworkController,
            0x03 => PciClass::DisplayController,
            0x04 => PciClass::MultimediaController,
            0x05 => PciClass::MemoryController,
            0x06 => PciClass::Bridge,
            0x07 => PciClass::CommunicationController,
            0x08 => PciClass::SystemPeripheral,
            0x09 => PciClass::InputDevice,
            0x0A => PciClass::DockingStation,
            0x0B => PciClass::Processor,
            0x0C => PciClass::SerialBusController,
            0x0D => PciClass::WirelessController,
            0x0E => PciClass::IntelligentController,
            0x0F => PciClass::SatelliteComm,
            0x10 => PciClass::EncryptionController,
            0x11 => PciClass::SignalProcessing,
            0x12 => PciClass::ProcessingAccelerator,
            0x13 => PciClass::Instrumentation,
            0x40 => PciClass::CoProcessor,
            other => PciClass::Other(other),
        }
    }

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) -> Result<(), SimonError> {
        let pci_base = std::path::Path::new("/sys/bus/pci/devices");
        if !pci_base.exists() {
            // A real answer: this kernel exposes no PCI bus at all.
            return Ok(());
        }

        let entries = std::fs::read_dir(pci_base)
            .map_err(|e| SimonError::System(format!("cannot read /sys/bus/pci/devices: {e}")))?;
        for entry in entries.flatten() {
            let address = entry.file_name().to_string_lossy().to_string();
            let base = entry.path();

            let vendor_id = Self::read_hex_id(&base.join("vendor"));
            let device_id = Self::read_hex_id(&base.join("device"));
            let subsystem_vendor_id = Self::read_hex_id(&base.join("subsystem_vendor"));
            let subsystem_device_id = Self::read_hex_id(&base.join("subsystem_device"));
            let revision = Self::read_hex_id(&base.join("revision"));

            let class_hex = Self::read_trimmed(&base.join("class"));
            let class_byte = u8::from_str_radix(
                class_hex.trim_start_matches("0x").get(..2).unwrap_or("00"),
                16,
            )
            .unwrap_or(0);
            let class = Self::classify_pci(class_byte);
            let class_code = format!("{:02x}", class_byte);

            // Driver
            let driver = std::fs::read_link(base.join("driver"))
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_default();

            // IOMMU group
            let iommu_group = std::fs::read_link(base.join("iommu_group"))
                .ok()
                .and_then(|p| p.file_name().and_then(|n| n.to_string_lossy().parse().ok()));

            // NUMA node
            let numa_node: i32 = Self::read_trimmed(&base.join("numa_node"))
                .parse()
                .unwrap_or(-1);

            // Power state
            let power_state = Self::read_trimmed(&base.join("power_state"));

            // SR-IOV
            let sriov_capable = base.join("sriov_totalvfs").exists();
            let sriov_vfs: u32 = if sriov_capable {
                Self::read_trimmed(&base.join("sriov_numvfs"))
                    .parse()
                    .unwrap_or(0)
            } else {
                0
            };

            // PCIe link info
            let link_info = {
                let speed = Self::read_trimmed(&base.join("current_link_speed"));
                let width = Self::read_trimmed(&base.join("current_link_width"));
                let max_speed = Self::read_trimmed(&base.join("max_link_speed"));
                let max_width = Self::read_trimmed(&base.join("max_link_width"));
                if !speed.is_empty() || !max_speed.is_empty() {
                    let gen = if max_speed.contains("32") {
                        5
                    } else if max_speed.contains("16") {
                        4
                    } else if max_speed.contains("8") {
                        3
                    } else if max_speed.contains("5") {
                        2
                    } else if max_speed.contains("2.5") {
                        1
                    } else {
                        0
                    };
                    Some(PciLinkInfo {
                        speed,
                        width,
                        max_speed,
                        max_width,
                        generation: gen,
                    })
                } else {
                    None
                }
            };

            // Vendor/device name from lspci or /usr/share/hwdata
            let (vendor_name, device_name) = Self::lookup_pci_names(&vendor_id, &device_id);

            self.devices.push(PciDeviceInfo {
                address,
                vendor_id,
                device_id,
                subsystem_vendor_id,
                subsystem_device_id,
                class_code,
                class,
                vendor_name,
                device_name,
                driver,
                kernel_module: String::new(),
                revision,
                iommu_group,
                link_info,
                sriov_capable,
                sriov_vfs,
                numa_node,
                power_state,
            });
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn lookup_pci_names(vendor_id: &str, _device_id: &str) -> (String, String) {
        // Well-known vendor IDs
        let vendor = match vendor_id {
            "10de" => "NVIDIA Corporation",
            "1002" => "Advanced Micro Devices [AMD/ATI]",
            "8086" => "Intel Corporation",
            "14e4" => "Broadcom Inc.",
            "1b36" => "Red Hat (Virtio)",
            "15b3" => "Mellanox Technologies",
            "1af4" => "Virtio",
            "144d" => "Samsung Electronics",
            "1987" => "Phison Electronics",
            "126f" => "Silicon Motion",
            "1179" => "Toshiba/Kioxia",
            "1c5c" => "SK Hynix",
            "c0a9" => "Micron Technology",
            "106b" => "Apple Inc.",
            "1022" => "Advanced Micro Devices [AMD]",
            _ => "",
        };
        (vendor.to_string(), String::new())
    }

    #[cfg(target_os = "linux")]
    fn read_trimmed(path: &std::path::Path) -> String {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    #[cfg(target_os = "linux")]
    fn read_hex_id(path: &std::path::Path) -> String {
        Self::read_trimmed(path)
            .trim_start_matches("0x")
            .to_lowercase()
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), SimonError> {
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command",
                r#"Get-CimInstance Win32_PnPEntity | Where-Object { $_.PNPDeviceID -like 'PCI\*' } | Select-Object Name, Manufacturer, PNPDeviceID, Status, ConfigManagerErrorCode -First 500 | ConvertTo-Json -Compress"#])
            .output()
            .map_err(|e| SimonError::CommandFailed(format!("powershell: {e}")))?;
        if !output.status.success() {
            return Err(SimonError::CommandFailed(format!(
                "Win32_PnPEntity query exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let text = String::from_utf8(output.stdout)
            .map_err(|e| SimonError::Parse(format!("Win32_PnPEntity output not UTF-8: {e}")))?;
        if text.trim().is_empty() {
            // ConvertTo-Json of an empty result set prints nothing, so this is
            // the one shape that really does mean "no PCI devices".
            return Ok(());
        }
        let val: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| SimonError::Parse(format!("Win32_PnPEntity JSON: {e}")))?;
        let items = match &val {
            serde_json::Value::Array(arr) => arr.clone(),
            obj @ serde_json::Value::Object(_) => vec![obj.clone()],
            _ => vec![],
        };
        for item in &items {
            let pnp_id = item["PNPDeviceID"].as_str().unwrap_or("");
            let name = item["Name"].as_str().unwrap_or("").to_string();
            let vendor_name = item["Manufacturer"].as_str().unwrap_or("").to_string();

            // Parse VEN_XXXX&DEV_XXXX from PNP ID
            let vendor_id = Self::extract_pnp_field(pnp_id, "VEN_");
            let device_id = Self::extract_pnp_field(pnp_id, "DEV_");
            let subsys_id = Self::extract_pnp_field(pnp_id, "SUBSYS_");

            // Infer class from name
            let class = Self::infer_class_from_name(&name);

            // The PnP id is a device-instance path, not an address, and
            // its tail is a volatile instance id — unusable as a stable
            // key. The registry holds the real bus/device/function and
            // the bound driver, both readable unelevated.
            let (bdf, service) = Self::registry_location_and_service(pnp_id);

            self.devices.push(PciDeviceInfo {
                address: bdf.unwrap_or_else(|| pnp_id.to_string()),
                vendor_id,
                device_id,
                subsystem_vendor_id: subsys_id.clone(),
                subsystem_device_id: subsys_id,
                class_code: String::new(),
                class,
                vendor_name,
                device_name: name,
                driver: service.unwrap_or_default(),
                kernel_module: String::new(),
                revision: String::new(),
                iommu_group: None,
                link_info: Self::devnode_link_info(pnp_id),
                sriov_capable: false,
                sriov_vfs: 0,
                numa_node: -1,
                power_state: item["Status"].as_str().unwrap_or("").to_string(),
            });
        }
        Ok(())
    }

    /// Bus/device/function and bound driver for one PnP device, from the registry.
    ///
    /// `HKLM\SYSTEM\CurrentControlSet\Enum\<pnp id>` holds both, and is readable
    /// without elevation. `LocationInformation` is a resource string whose tail is
    /// the decimal triple:
    ///
    /// ```text
    /// @System32\drivers\pci.sys,#65536;PCI bus %1, device %2, function %3;(122,0,0)
    /// ```
    ///
    /// The leading text is a format template — parsing the `%1` placeholders would
    /// yield nothing. Only the parenthesised tail carries values.
    ///
    /// Returns the address in the conventional `domain:bus:device.function` form so
    /// it matches what `lspci` prints and what the Linux reader produces. Windows
    /// exposes no segment/domain number here; it is 0 on all but a handful of very
    /// large systems, and stating 0000 is better than inventing a fourth format.
    #[cfg(target_os = "windows")]
    fn registry_location_and_service(pnp_id: &str) -> (Option<String>, Option<String>) {
        use winreg::enums::*;
        use winreg::RegKey;

        if pnp_id.is_empty() {
            return (None, None);
        }

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let path = format!(r"SYSTEM\CurrentControlSet\Enum\{pnp_id}");
        let Ok(key) = hklm.open_subkey_with_flags(&path, KEY_READ) else {
            return (None, None);
        };

        let bdf = key
            .get_value::<String, _>("LocationInformation")
            .ok()
            .and_then(|loc| Self::parse_location_bdf(&loc));

        // A device with no service has no driver bound — a real and interesting
        // state. An empty string is stored as absent so callers cannot confuse
        // "unbound" with "not looked up".
        let service = key
            .get_value::<String, _>("Service")
            .ok()
            .filter(|s| !s.trim().is_empty());

        (bdf, service)
    }

    /// PCIe link state for one device, from the device node property store.
    ///
    /// The Windows PCI driver publishes negotiated and maximum link speed and
    /// width as `DEVPKEY_PciDevice_*` properties. `Get-PnpDeviceProperty` reads the
    /// same values but costs about 0.4 s per device — 25 s across the 64 devices of
    /// the development machine, which is not a price a snapshot can pay. These are
    /// two `cfgmgr32` calls per device instead.
    ///
    /// Returns `None` for a device with no PCIe capability — a plain PCI or
    /// integrated device — which is a fact about the device, not a failure.
    #[cfg(target_os = "windows")]
    fn devnode_link_info(pnp_id: &str) -> Option<PciLinkInfo> {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::GUID;
        use windows::core::PCWSTR;
        use windows::Win32::Devices::DeviceAndDriverInstallation::{
            CM_Get_DevNode_PropertyW, CM_Locate_DevNodeW, CM_LOCATE_DEVNODE_NORMAL, CR_SUCCESS,
        };
        use windows::Win32::Devices::Properties::{DEVPROPKEY, DEVPROPTYPE};

        // {3AB22E31-8264-4B4E-9AF5-A8D2D8E33E62}, the PCI device property class.
        // The property ids below were read off a live device rather than taken
        // from memory: `Get-PnpDeviceProperty` reports the raw key alongside the
        // friendly name, and a wrong id here would return a real number from the
        // wrong property — plausible, and wrong, which is the failure this project
        // spends its comments on.
        const PCI_DEVICE_FMTID: GUID = GUID::from_u128(0x3ab22e31_8264_4b4e_9af5_a8d2d8e33e62);
        const PID_CURRENT_LINK_SPEED: u32 = 9;
        const PID_CURRENT_LINK_WIDTH: u32 = 10;
        const PID_MAX_LINK_SPEED: u32 = 11;
        const PID_MAX_LINK_WIDTH: u32 = 12;

        let wide: Vec<u16> = std::ffi::OsStr::new(pnp_id)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut devinst = 0u32;
        // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call.
        let located = unsafe {
            CM_Locate_DevNodeW(
                &mut devinst,
                PCWSTR(wide.as_ptr()),
                CM_LOCATE_DEVNODE_NORMAL,
            )
        };
        if located != CR_SUCCESS {
            return None;
        }

        let read_u32 = |pid: u32| -> Option<u32> {
            let key = DEVPROPKEY {
                fmtid: PCI_DEVICE_FMTID,
                pid,
            };
            let mut value = 0u32;
            let mut ty = DEVPROPTYPE::default();
            let mut len = std::mem::size_of::<u32>() as u32;
            // SAFETY: the buffer is exactly the size reported in `len`, and the
            // call writes at most that many bytes.
            let ret = unsafe {
                CM_Get_DevNode_PropertyW(
                    devinst,
                    &key,
                    &mut ty,
                    Some(&mut value as *mut u32 as *mut u8),
                    &mut len,
                    0,
                )
            };
            (ret == CR_SUCCESS).then_some(value)
        };

        // Width is the reliable signal that this is a PCIe device at all: a device
        // without the capability has neither property, and reporting a speed with
        // no width would describe half a link.
        let current_width = read_u32(PID_CURRENT_LINK_WIDTH)?;
        let max_width = read_u32(PID_MAX_LINK_WIDTH).unwrap_or(current_width);
        let current_speed = read_u32(PID_CURRENT_LINK_SPEED).unwrap_or(0);
        let max_speed = read_u32(PID_MAX_LINK_SPEED).unwrap_or(current_speed);

        Some(PciLinkInfo {
            speed: Self::link_speed_label(current_speed),
            width: format!("x{current_width}"),
            max_speed: Self::link_speed_label(max_speed),
            max_width: format!("x{max_width}"),
            generation: current_speed.min(u8::MAX as u32) as u8,
        })
    }

    /// Render a PCIe link speed encoding as the transfer rate it denotes.
    ///
    /// The property holds a generation number, not a rate: 4 means Gen 4, which is
    /// 16 GT/s. Reporting the 4 as though it were a rate — the obvious mistake —
    /// would describe a Gen 4 link as slower than Gen 1.
    #[cfg(target_os = "windows")]
    fn link_speed_label(encoding: u32) -> String {
        match encoding {
            1 => "2.5 GT/s".to_string(),
            2 => "5.0 GT/s".to_string(),
            3 => "8.0 GT/s".to_string(),
            4 => "16.0 GT/s".to_string(),
            5 => "32.0 GT/s".to_string(),
            6 => "64.0 GT/s".to_string(),
            // A generation this code predates. The number is still the truth the
            // device reported, so it is passed through labelled rather than
            // dropped or guessed at.
            other if other > 0 => format!("PCIe gen {other}"),
            _ => String::new(),
        }
    }

    /// Extract `0000:7a:00.0` from a `LocationInformation` resource string.
    #[cfg(target_os = "windows")]
    fn parse_location_bdf(location: &str) -> Option<String> {
        let open = location.rfind('(')?;
        let close = location.rfind(')')?;
        if close <= open + 1 {
            return None;
        }
        let mut parts = location[open + 1..close].split(',');
        let bus: u32 = parts.next()?.trim().parse().ok()?;
        let device: u32 = parts.next()?.trim().parse().ok()?;
        let function: u32 = parts.next()?.trim().parse().ok()?;
        // A fourth field would mean this is not the triple we think it is.
        if parts.next().is_some() {
            return None;
        }
        Some(format!("0000:{bus:02x}:{device:02x}.{function}"))
    }

    #[cfg(target_os = "windows")]
    fn extract_pnp_field(pnp_id: &str, prefix: &str) -> String {
        pnp_id
            .find(prefix)
            .map(|pos| {
                let start = pos + prefix.len();
                pnp_id[start..]
                    .chars()
                    .take_while(|c| c.is_ascii_hexdigit())
                    .collect::<String>()
                    .to_lowercase()
            })
            .unwrap_or_default()
    }

    #[cfg(target_os = "windows")]
    fn infer_class_from_name(name: &str) -> PciClass {
        let lower = name.to_lowercase();
        if lower.contains("display")
            || lower.contains("video")
            || lower.contains("graphics")
            || lower.contains("gpu")
            || lower.contains("vga")
        {
            PciClass::DisplayController
        } else if lower.contains("ethernet")
            || lower.contains("network")
            || lower.contains("wi-fi")
            || lower.contains("wifi")
        {
            PciClass::NetworkController
        } else if lower.contains("storage")
            || lower.contains("sata")
            || lower.contains("ahci")
            || lower.contains("nvme")
            || lower.contains("raid")
        {
            PciClass::MassStorage
        } else if lower.contains("audio") || lower.contains("sound") || lower.contains("multimedia")
        {
            PciClass::MultimediaController
        } else if lower.contains("usb") || lower.contains("xhci") || lower.contains("smbus") {
            PciClass::SerialBusController
        } else if lower.contains("bridge") || lower.contains("pci-to-pci") || lower.contains("host")
        {
            PciClass::Bridge
        } else if lower.contains("bluetooth") || lower.contains("wireless") {
            PciClass::WirelessController
        } else if lower.contains("encryption") || lower.contains("tpm") {
            PciClass::EncryptionController
        } else if lower.contains("signal") || lower.contains("sensor") {
            PciClass::SignalProcessing
        } else {
            PciClass::Other(0xff)
        }
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), SimonError> {
        let output = std::process::Command::new("system_profiler")
            .args(["SPPCIDataType", "-json"])
            .output()
            .map_err(|e| SimonError::CommandFailed(format!("system_profiler: {e}")))?;
        if !output.status.success() {
            return Err(SimonError::CommandFailed(format!(
                "system_profiler SPPCIDataType exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let text = String::from_utf8(output.stdout)
            .map_err(|e| SimonError::Parse(format!("system_profiler output not UTF-8: {e}")))?;
        let val: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| SimonError::Parse(format!("SPPCIDataType JSON: {e}")))?;
        if let Some(items) = val["SPPCIDataType"].as_array() {
            for item in items {
                let name = item["_name"].as_str().unwrap_or("").to_string();
                let vendor_id = item["sppci_vendor-id"]
                    .as_str()
                    .unwrap_or("")
                    .trim_start_matches("0x")
                    .to_lowercase();
                let device_id = item["sppci_device-id"]
                    .as_str()
                    .unwrap_or("")
                    .trim_start_matches("0x")
                    .to_lowercase();
                let slot = item["sppci_slot_name"].as_str().unwrap_or("").to_string();
                let driver = item["sppci_driver_installed"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                let link_speed = item["sppci_link-speed"].as_str().unwrap_or("").to_string();
                let link_width = item["sppci_link-width"].as_str().unwrap_or("").to_string();

                let link_info = if !link_speed.is_empty() {
                    Some(PciLinkInfo {
                        speed: link_speed.clone(),
                        width: link_width.clone(),
                        max_speed: link_speed,
                        max_width: link_width,
                        generation: 0,
                    })
                } else {
                    None
                };

                let class = Self::infer_class_from_name_mac(&name);

                self.devices.push(PciDeviceInfo {
                    address: slot,
                    vendor_id,
                    device_id,
                    subsystem_vendor_id: String::new(),
                    subsystem_device_id: String::new(),
                    class_code: String::new(),
                    class,
                    vendor_name: String::new(),
                    device_name: name,
                    driver,
                    kernel_module: String::new(),
                    revision: String::new(),
                    iommu_group: None,
                    link_info,
                    sriov_capable: false,
                    sriov_vfs: 0,
                    numa_node: -1,
                    power_state: String::new(),
                });
            }
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn infer_class_from_name_mac(name: &str) -> PciClass {
        let lower = name.to_lowercase();
        if lower.contains("gpu") || lower.contains("display") || lower.contains("graphics") {
            PciClass::DisplayController
        } else if lower.contains("ethernet") || lower.contains("wifi") || lower.contains("network")
        {
            PciClass::NetworkController
        } else if lower.contains("nvme") || lower.contains("ahci") || lower.contains("storage") {
            PciClass::MassStorage
        } else if lower.contains("audio") || lower.contains("thunderbolt") {
            PciClass::MultimediaController
        } else if lower.contains("usb") || lower.contains("xhci") {
            PciClass::SerialBusController
        } else if lower.contains("bridge") {
            PciClass::Bridge
        } else {
            PciClass::Other(0xff)
        }
    }
}

impl Default for PciDeviceMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            devices: Vec::new(),
        })
    }
}

impl std::fmt::Display for PciClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unclassified => write!(f, "Unclassified"),
            Self::MassStorage => write!(f, "Mass Storage"),
            Self::NetworkController => write!(f, "Network Controller"),
            Self::DisplayController => write!(f, "Display Controller"),
            Self::MultimediaController => write!(f, "Multimedia"),
            Self::MemoryController => write!(f, "Memory Controller"),
            Self::Bridge => write!(f, "Bridge"),
            Self::CommunicationController => write!(f, "Communication"),
            Self::SystemPeripheral => write!(f, "System Peripheral"),
            Self::InputDevice => write!(f, "Input Device"),
            Self::DockingStation => write!(f, "Docking Station"),
            Self::Processor => write!(f, "Processor"),
            Self::SerialBusController => write!(f, "Serial Bus"),
            Self::WirelessController => write!(f, "Wireless"),
            Self::IntelligentController => write!(f, "Intelligent Controller"),
            Self::SatelliteComm => write!(f, "Satellite"),
            Self::EncryptionController => write!(f, "Encryption"),
            Self::SignalProcessing => write!(f, "Signal Processing"),
            Self::ProcessingAccelerator => write!(f, "Processing Accelerator"),
            Self::Instrumentation => write!(f, "Instrumentation"),
            Self::CoProcessor => write!(f, "Co-Processor"),
            Self::Other(c) => write!(f, "Other(0x{:02x})", c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property holds a PCIe *generation*, not a transfer rate. Reporting the
    /// raw 4 would describe a Gen 4 link as slower than Gen 1 — a number that is
    /// wrong while looking entirely reasonable.
    #[cfg(target_os = "windows")]
    #[test]
    fn link_speed_encodings_render_as_transfer_rates() {
        assert_eq!(PciDeviceMonitor::link_speed_label(1), "2.5 GT/s");
        assert_eq!(PciDeviceMonitor::link_speed_label(3), "8.0 GT/s");
        assert_eq!(PciDeviceMonitor::link_speed_label(4), "16.0 GT/s");
        assert_eq!(PciDeviceMonitor::link_speed_label(6), "64.0 GT/s");
    }

    /// A generation newer than this table is still a real reading. Dropping it
    /// would lose information; guessing a rate for it would invent one.
    #[cfg(target_os = "windows")]
    #[test]
    fn an_unknown_generation_is_labelled_rather_than_guessed() {
        assert_eq!(PciDeviceMonitor::link_speed_label(7), "PCIe gen 7");
        // Zero is the absence of a reading, not a speed.
        assert_eq!(PciDeviceMonitor::link_speed_label(0), "");
    }

    /// The address must come out in the same form `lspci` and the Linux reader
    /// use, or the id space differs by platform for no reason.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_location_string_yields_a_conventional_bdf() {
        let loc = r"@System32\drivers\pci.sys,#65536;PCI bus %1, device %2, function %3;(122,0,0)";
        assert_eq!(
            PciDeviceMonitor::parse_location_bdf(loc),
            Some("0000:7a:00.0".to_string())
        );
        // Single-digit values still pad, so ids sort and compare as text.
        let loc2 = r"@System32\drivers\pci.sys,#65536;PCI bus %1, device %2, function %3;(0,2,1)";
        assert_eq!(
            PciDeviceMonitor::parse_location_bdf(loc2),
            Some("0000:00:02.1".to_string())
        );
    }

    /// The template text contains `%1` placeholders and commas of its own. Reading
    /// those instead of the parenthesised tail would produce a confident wrong
    /// address, which is worse than none.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_location_string_without_a_value_triple_is_refused() {
        assert_eq!(
            PciDeviceMonitor::parse_location_bdf("PCI bus %1, device %2, function %3"),
            None
        );
        assert_eq!(PciDeviceMonitor::parse_location_bdf(""), None);
        assert_eq!(PciDeviceMonitor::parse_location_bdf("()"), None);
        assert_eq!(PciDeviceMonitor::parse_location_bdf("(1,2)"), None);
        // A four-field tail is not the triple this parser is reading.
        assert_eq!(PciDeviceMonitor::parse_location_bdf("(1,2,3,4)"), None);
        assert_eq!(PciDeviceMonitor::parse_location_bdf("(a,b,c)"), None);
    }

    /// Constructing the monitor either enumerates or says why it could not.
    ///
    /// This asserted `is_ok()`, which was true by construction until 6.0.0
    /// because `refresh` could not fail. Now that it can, the contract is
    /// weaker and more useful: **whatever happens, the caller can tell which
    /// happened.** A failure has to carry a reason, because a reason is the
    /// whole difference between "this machine has none" and "nobody looked".
    #[test]
    fn test_monitor_creation() {
        match PciDeviceMonitor::new() {
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
    fn test_monitor_default() {
        let monitor = PciDeviceMonitor::default();
        let _ = monitor.devices();
        let _ = monitor.gpus();
        let _ = monitor.network_devices();
        let _ = monitor.storage_devices();
    }

    #[test]
    fn test_class_display() {
        assert_eq!(
            PciClass::DisplayController.to_string(),
            "Display Controller"
        );
        assert_eq!(PciClass::MassStorage.to_string(), "Mass Storage");
    }

    #[test]
    fn test_classify_pci() {
        assert_eq!(
            PciDeviceMonitor::classify_pci(0x03),
            PciClass::DisplayController
        );
        assert_eq!(PciDeviceMonitor::classify_pci(0x01), PciClass::MassStorage);
        assert_eq!(
            PciDeviceMonitor::classify_pci(0x02),
            PciClass::NetworkController
        );
    }

    #[test]
    fn test_serialization() {
        let dev = PciDeviceInfo {
            address: "0000:00:02.0".into(),
            vendor_id: "8086".into(),
            device_id: "a780".into(),
            subsystem_vendor_id: String::new(),
            subsystem_device_id: String::new(),
            class_code: "03".into(),
            class: PciClass::DisplayController,
            vendor_name: "Intel Corporation".into(),
            device_name: "UHD Graphics".into(),
            driver: "i915".into(),
            kernel_module: String::new(),
            revision: "00".into(),
            iommu_group: Some(1),
            link_info: None,
            sriov_capable: false,
            sriov_vfs: 0,
            numa_node: 0,
            power_state: "D0".into(),
        };
        let json = serde_json::to_string(&dev).unwrap();
        assert!(json.contains("Intel"));
        let _: PciDeviceInfo = serde_json::from_str(&json).unwrap();
    }
}
