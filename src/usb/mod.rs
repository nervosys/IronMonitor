//! USB device monitoring module
use serde::{Deserialize, Serialize};

#[cfg(target_os = "windows")]
mod windows_speed;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsbSpeed {
    Low,
    Full,
    High,
    Super,
    SuperPlus,
    SuperPlusx2,
    Usb4,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsbDeviceClass {
    Audio,
    Communication,
    Hid,
    Printer,
    MassStorage,
    Hub,
    Video,
    Wireless,
    Vendor,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbDevice {
    /// A key for this device that is stable across re-enumeration.
    ///
    /// The ontology builds `usb.{addr}` from this, and the property it needs is
    /// narrow: **unplugging an unrelated device must not change it.** Before
    /// 6.0.0 the id was `{bus_number}_{port_number}`, and Windows filled those
    /// with `0` and the enumeration index -- so every id after a removed device
    /// shifted by one, silently repointing the history in `tsdb`.
    ///
    /// Bus and port are not enough on their own. Of this machine's 41 USB
    /// nodes, only 14 have a hub and port Windows will report; 18 are
    /// interfaces of composite devices, which *share* their parent's location
    /// and are told apart only by the `MI_xx` in their instance path; and 9 are
    /// root hubs with no location at all. An id built from hub and port would
    /// have been stable and **collided** for more than half of them, which is
    /// worse than unstable-and-unique.
    ///
    /// So this is each platform's own device path, normalised into one id
    /// segment: the sysfs name on Linux (`1-4.2`), the PnP instance path on
    /// Windows, the `Location ID` on macOS. Each is what its operating system
    /// uses to identify the device, each is unique by construction, and each
    /// changes only when *this* device's position or identity changes -- which
    /// is the point.
    pub address: String,
    /// The bus this device is attached to, where the platform reports one.
    ///
    /// `None` on Windows: `Win32_PnPEntity` gives no bus number, and this was
    /// filled with `0` for every device -- the sentinel this crate spends its
    /// time removing, sitting in a public field. Linux parses it from the sysfs
    /// name and macOS from the `Location ID`, so both report a real one.
    pub bus_number: Option<u8>,
    /// The port on that bus, where the platform reports one.
    ///
    /// No longer part of the id -- see [`Self::address`]. `None` on Windows:
    /// `Win32_PnPEntity` gives no port, and the hub and port in the registry's
    /// `LocationInformation` are absent for two thirds of the nodes it
    /// enumerates.
    ///
    /// A `0` here used to be indistinguishable from port zero, and
    /// `get_usb_device_details` looked devices up by this field -- so when the
    /// id scheme moved and Windows stopped filling it, 39 devices collapsed to
    /// one addressable pair. The type says it now.
    pub port_number: Option<u8>,
    /// USB vendor id, or `None` for an entry that has none — a root hub's
    /// PnP id is `USB\ROOT_HUB30\...` with no `VID_` at all. This was `u16`
    /// and a missing id parsed to zero, so `simon cli usb` printed
    /// `[0000:0000]` and the agent surface published `"vendor_id": "0000"`,
    /// neither distinguishable from a device that reports those ids.
    pub vendor_id: Option<u16>,
    /// USB product id. See [`Self::vendor_id`].
    pub product_id: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub description: Option<String>,
    pub serial_number: Option<String>,
    pub class: UsbDeviceClass,
    pub speed: UsbSpeed,
}

/// Map a USB base-class byte to the class this crate reports.
///
/// `0x00` is not "unknown": the specification says the class is declared per
/// interface rather than for the device, which is what every composite device
/// reports. There is no variant for that distinction, so it stays `Unknown` and
/// the resolver's absence reason says which of the two it is.
fn class_from_code(code: u8) -> UsbDeviceClass {
    match code {
        0x01 => UsbDeviceClass::Audio,
        0x02 => UsbDeviceClass::Communication,
        0x03 => UsbDeviceClass::Hid,
        0x07 => UsbDeviceClass::Printer,
        0x08 => UsbDeviceClass::MassStorage,
        0x09 => UsbDeviceClass::Hub,
        0x0e => UsbDeviceClass::Video,
        0xe0 => UsbDeviceClass::Wireless,
        0xff => UsbDeviceClass::Vendor,
        _ => UsbDeviceClass::Unknown,
    }
}

/// The USB class Windows records in a device's hardware and compatible ids.
///
/// Two forms appear in the compatible ids and they answer different questions:
///
/// * `USB\DevClass_08&SubClass_06&Prot_50` is `bDeviceClass` from the *device*
///   descriptor. `DevClass_00` is not "unknown" -- the specification uses it to
///   say the class is declared per interface, and on this development machine
///   **every one of the 39 devices reports 00**, which is ordinary for modern
///   hardware.
/// * `USB\COMPAT_VID_046d&Class_03&SubClass_01&Prot_01` is the *interface*
///   class, and it is the one that actually answers "what is this" -- `03` for
///   a keyboard, `0e` for a camera, `02` for a serial adapter. 20 of the 39
///   carry one.
///
/// A non-zero device class wins because the device declared it about itself;
/// otherwise the interface class is used. Note that `DevClass_` ends in the
/// same five characters as `Class_`, so the interface form is matched on
/// `&Class_` rather than on `Class_`.
///
/// This replaced `classify_usb_device`, which matched substrings of the
/// device's *name*: "hub" meant Hub, "disk" meant MassStorage, "camera" meant
/// Video. That is the same shape as the speed heuristic removed in this file --
/// what a device is called standing in for what it declares.
///
/// **Hardware ids are read as well as compatible ids, because a root hub's
/// declaration is only in the former.** Windows leaves `CompatibleID` empty on
/// all six of this machine's root hubs and records `USB\ROOT_HUB30` in
/// `HardwareID`; reading one property and not the other made the hub branch
/// below unreachable for exactly the devices it was written for, and the
/// absence reason that resulted said the platform recorded no hub identifier
/// while the platform was recording one.
#[cfg(target_os = "windows")]
fn class_from_device_ids(hardware: &[String], compatible: &[String]) -> Option<UsbDeviceClass> {
    fn code_after(haystack: &str, marker: &str) -> Option<u8> {
        let rest = haystack.split(marker).nth(1)?;
        let hex: String = rest.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        (!hex.is_empty())
            .then(|| u8::from_str_radix(&hex, 16).ok())
            .flatten()
    }

    // Only the compatible ids carry class codes; the hardware ids carry the
    // vendor, product and revision, and -- for a hub -- the bus driver's own
    // name for what the device is.
    let upper: Vec<String> = compatible.iter().map(|i| i.to_uppercase()).collect();

    // The device's own declaration, when it makes one.
    for id in &upper {
        if let Some(code) = code_after(id, "DEVCLASS_") {
            if code != 0 {
                return Some(class_from_code(code));
            }
        }
    }
    // Otherwise what the interface says it does.
    for id in &upper {
        if let Some(code) = code_after(id, "&CLASS_") {
            if code != 0 {
                return Some(class_from_code(code));
            }
        }
    }
    // Hubs carry neither form. Windows gives them a dedicated identifier
    // instead -- `USB\USB30_HUB` and `USB\USB20_HUB` among the compatible ids
    // of an external hub, and `USB\ROOT_HUB30` among the *hardware* ids of a
    // root hub, which has no compatible ids at all. That is the bus driver
    // declaring what the device is, in a structured identifier, and is not the
    // same thing as finding "hub" in a display name.
    if hardware
        .iter()
        .chain(compatible.iter())
        .map(|id| id.to_uppercase())
        .any(|id| id.contains("ROOT_HUB") || id.ends_with("_HUB"))
    {
        return Some(UsbDeviceClass::Hub);
    }
    None
}

/// Fold a platform device path into something usable as one id segment.
///
/// Lowercased, with every character that is not alphanumeric collapsed to `_`,
/// and runs of `_` squeezed. The dot matters most: ids are dot-separated, so a
/// Linux path of `1-4.2` left alone would make `usb.1-4.2.product` parse as an
/// extra segment and stop matching its template.
fn normalise_address(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

pub struct UsbMonitor {
    devices: Vec<UsbDevice>,
}

impl UsbMonitor {
    pub fn new() -> Result<Self, crate::error::SimonError> {
        let mut monitor = Self {
            devices: Vec::new(),
        };
        monitor.refresh()?;
        Ok(monitor)
    }
    /// Re-enumerate the USB tree.
    ///
    /// Returns `Err` when the enumeration failed, and `Ok` with an empty list
    /// only when it succeeded and found nothing -- which the resolver publishes
    /// as `usb.<none>`, a claim about the machine. See [`crate::core::command`].
    pub fn refresh(&mut self) -> Result<(), crate::error::SimonError> {
        self.devices.clear();
        #[cfg(target_os = "windows")]
        self.refresh_windows()?;
        #[cfg(target_os = "linux")]
        self.refresh_linux()?;
        #[cfg(target_os = "macos")]
        self.refresh_macos()?;
        Ok(())
    }
    pub fn devices(&self) -> &[UsbDevice] {
        &self.devices
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), crate::error::SimonError> {
        // Two independent enumerations. Either one succeeding is enough to
        // trust an empty result; both failing is not an empty machine, and
        // both failing used to be reported as one.
        let wmi = Self::wmi_enumerate_usb();
        if let Ok(devices) = &wmi {
            self.devices.clone_from(devices);
        }
        if !self.devices.is_empty() {
            return Ok(());
        }

        // Fallback to setupapi-based approach
        let registry = Self::registry_enumerate_usb();
        if let Ok(devices) = &registry {
            self.devices.clone_from(devices);
        }

        match (wmi, registry) {
            (Err(wmi_err), Err(registry_err)) => Err(crate::error::SimonError::System(format!(
                "no USB enumeration succeeded: WMI said {wmi_err}; the registry walk said \
                 {registry_err}"
            ))),
            _ => Ok(()),
        }
    }

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) -> Result<(), crate::error::SimonError> {
        use std::fs;
        use std::path::Path;
        // Read from /sys/bus/usb/devices
        let usb_path = Path::new("/sys/bus/usb/devices");
        if usb_path.exists() {
            let entries = fs::read_dir(usb_path).map_err(|e| {
                crate::error::SimonError::System(format!("cannot read {usb_path:?}: {e}"))
            })?;
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if !name.contains('-') || name.contains(':') {
                        continue;
                    }
                    let path = entry.path();
                    let vendor_id = read_usb_attr(&path, "idVendor");
                    let product_id = read_usb_attr(&path, "idProduct");
                    let manufacturer = read_usb_string(&path, "manufacturer");
                    let product = read_usb_string(&path, "product");
                    let serial = read_usb_string(&path, "serial");
                    let speed = match read_usb_string(&path, "speed").as_deref() {
                        Some("1.5") => UsbSpeed::Low,
                        Some("12") => UsbSpeed::Full,
                        Some("480") => UsbSpeed::High,
                        Some("5000") => UsbSpeed::Super,
                        Some("10000") => UsbSpeed::SuperPlus,
                        Some("20000") => UsbSpeed::SuperPlusx2,
                        _ => UsbSpeed::Unknown,
                    };
                    let class_code = read_usb_attr(&path, "bDeviceClass");
                    // `bDeviceClass` is one byte; a sysfs value that does not
                    // fit one is a failed parse, not a class.
                    let class = u8::try_from(class_code)
                        .map(class_from_code)
                        .unwrap_or(UsbDeviceClass::Unknown);
                    let parts: Vec<&str> = name.split('-').collect();
                    let bus_number = parts.first().and_then(|s| s.parse().ok());
                    let port_number = parts
                        .get(1)
                        .and_then(|s| s.split('.').next())
                        .and_then(|s| s.parse().ok());
                    self.devices.push(UsbDevice {
                        // `1-4.2` -- the bus, the root port and the chain of
                        // hub ports below it. Exactly the device's position.
                        address: normalise_address(&name),
                        bus_number,
                        port_number,
                        vendor_id: Some(vendor_id as u16),
                        product_id: Some(product_id as u16),
                        manufacturer,
                        product,
                        description: None,
                        serial_number: serial,
                        class,
                        speed,
                    });
                }
            }
        }
        // There is deliberately no fallback device here.
        //
        // This used to invent one when the sysfs walk found nothing: an Intel
        // root hub, vendor 0x8086, product 0x0001, running at high speed. None
        // of that was read from anything. A machine whose USB tree simon cannot
        // enumerate reported one device that does not exist, and a caller had
        // no way to tell it from a machine with exactly one hub.
        //
        // An empty list is the honest answer, and the callers say so.
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), crate::error::SimonError> {
        // `Err(_) => return` here reported a machine with no USB devices at all
        // whenever `system_profiler` could not be spawned, and a non-zero exit
        // was never looked at.
        let stdout = crate::core::command::capture(
            "system_profiler",
            &["SPUSBDataType", "-detailLevel", "full"],
        )?;
        let mut current_name: Option<String> = None;
        let mut current_vendor_id: Option<u16> = None;
        let mut current_product_id: Option<u16> = None;
        let mut current_manufacturer: Option<String> = None;
        let mut current_serial: Option<String> = None;
        // `Unknown`, not `Full`: a device whose `system_profiler` entry has no
        // `Speed` line has not reported a speed, and full speed is a real value
        // that some devices genuinely negotiate.
        let mut current_speed: UsbSpeed = UsbSpeed::Unknown;
        // The raw `Location ID`, kept for the device address. It encodes the
        // controller and the port chain, which is what makes it stable.
        let mut current_location = String::new();
        let mut bus_number: Option<u8> = None;
        let mut port_number: Option<u8> = None;
        let mut device_idx: u8 = 0;

        for line in stdout.lines() {
            let trimmed = line.trim();

            // Device name lines end with ':'
            if trimmed.ends_with(':') && !trimmed.starts_with("USB") && !trimmed.is_empty() {
                // Save previous device if any
                if let Some(name) = current_name.take() {
                    device_idx += 1;
                    self.devices.push(UsbDevice {
                        address: normalise_address(&current_location),
                        bus_number,
                        // `port_number`, parsed from the `Location ID` above --
                        // not `device_idx`, which is how many devices the
                        // parser has seen. Both were in scope; the real one was
                        // computed and then overwritten by the counter.
                        port_number,
                        vendor_id: current_vendor_id,
                        product_id: current_product_id,
                        manufacturer: current_manufacturer.take(),
                        product: Some(name),
                        description: None,
                        serial_number: current_serial.take(),
                        class: UsbDeviceClass::Unknown,
                        speed: current_speed,
                    });
                    current_vendor_id = None;
                    current_product_id = None;
                    current_speed = UsbSpeed::Unknown;
                    current_location.clear();
                }
                current_name = Some(trimmed.trim_end_matches(':').to_string());
            } else if let Some((key, val)) = trimmed.split_once(':') {
                let key = key.trim();
                let val = val.trim();
                match key {
                    "Vendor ID" => {
                        // Format: "0x05ac (Apple Inc.)"
                        if let Some(hex) = val
                            .strip_prefix("0x")
                            .and_then(|s| s.split_whitespace().next())
                        {
                            current_vendor_id = u16::from_str_radix(hex, 16).ok();
                        }
                    }
                    "Product ID" => {
                        if let Some(hex) = val
                            .strip_prefix("0x")
                            .and_then(|s| s.split_whitespace().next())
                        {
                            current_product_id = u16::from_str_radix(hex, 16).ok();
                        }
                    }
                    "Manufacturer" => {
                        current_manufacturer = Some(val.to_string());
                    }
                    "Serial Number" => {
                        current_serial = Some(val.to_string());
                    }
                    "Speed" => {
                        current_speed = if val.contains("480") {
                            UsbSpeed::High
                        } else if val.contains("5 Gb")
                            || val.contains("10 Gb")
                            || val.contains("20 Gb")
                        {
                            UsbSpeed::Super
                        } else if val.contains("1.5") {
                            UsbSpeed::Low
                        } else if val.contains("12 Mb") {
                            UsbSpeed::Full
                        } else {
                            // A `Speed` line this parser does not recognise is
                            // not full speed; it used to be.
                            UsbSpeed::Unknown
                        };
                    }
                    "Location ID" => {
                        current_location = val.to_string();
                        // Parse bus from location ID hex (e.g., "0x14200000 / 7")
                        if let Some(hex) = val
                            .strip_prefix("0x")
                            .and_then(|s| s.split_whitespace().next())
                        {
                            if let Ok(loc) = u32::from_str_radix(hex, 16) {
                                bus_number = Some(((loc >> 24) & 0xFF) as u8);
                                port_number = Some(((loc >> 20) & 0xF) as u8);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Save last device
        if let Some(name) = current_name.take() {
            device_idx += 1;
            self.devices.push(UsbDevice {
                address: normalise_address(&current_location),
                bus_number,
                // See the flush above: the parsed port, not the counter.
                port_number,
                vendor_id: current_vendor_id,
                product_id: current_product_id,
                manufacturer: current_manufacturer.take(),
                product: Some(name),
                description: None,
                serial_number: current_serial.take(),
                class: UsbDeviceClass::Unknown,
                speed: current_speed,
            });
        }
        Ok(())
    }
}

impl Default for UsbMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            devices: Vec::new(),
        })
    }
}

#[cfg(target_os = "windows")]
impl UsbMonitor {
    /// Enumerate USB devices using WMI Win32_PnPEntity
    fn wmi_enumerate_usb() -> Result<Vec<UsbDevice>, crate::error::SimonError> {
        use std::process::Command;
        let mut devices = Vec::new();

        // Negotiated link speeds, read once for the whole tree. See
        // `windows_speed` for why this cannot come from the WMI query below.
        let speeds = windows_speed::negotiated_speeds();

        // Use PowerShell to query WMI for USB devices
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                concat!(
                    "Get-CimInstance Win32_PnPEntity | ",
                    "Where-Object { $_.PNPDeviceID -like 'USB*' } | ",
                    // HardwareID is selected because a root hub's hub
                    // identifier lives there and nowhere else. Not selecting a
                    // column is how the platform comes to be blamed for an
                    // absence the query created.
                    "Select-Object Name, Manufacturer, PNPDeviceID, Description, ",
                    "Status, HardwareID, CompatibleID | ConvertTo-Json -Compress"
                ),
            ])
            .output()
            .map_err(|e| crate::error::SimonError::Other(format!("WMI query failed: {}", e)))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
                let items = if json.is_array() {
                    json.as_array().cloned().unwrap_or_default()
                } else {
                    vec![json]
                };

                for item in items.iter() {
                    let name = item.get("Name").and_then(|v| v.as_str()).unwrap_or("");
                    let manufacturer = item.get("Manufacturer").and_then(|v| v.as_str());
                    let pnp_id = item
                        .get("PNPDeviceID")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let description = item.get("Description").and_then(|v| v.as_str());

                    // Parse VID/PID from PNPDeviceID like "USB\VID_046D&PID_C52B\..."
                    let (vid, pid) = parse_vid_pid(pnp_id);

                    // The class byte from the device's own descriptor, as
                    // Windows recorded it in the compatible ids.
                    //
                    // This was `classify_usb_device(name, description)`, which
                    // matched substrings: "hub" meant Hub, "disk" meant
                    // MassStorage, "camera" meant Video. That is the same shape
                    // as the speed heuristic removed below -- a guess from what
                    // a device is *called* standing in for what it *declares*
                    // -- and it is wrong in both directions, silently. A
                    // descriptor byte is available and is not a guess.
                    let string_array = |field: &str| -> Vec<String> {
                        item.get(field)
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default()
                    };
                    let hardware = string_array("HardwareID");
                    let compatible = string_array("CompatibleID");
                    let class = class_from_device_ids(&hardware, &compatible)
                        .unwrap_or(UsbDeviceClass::Unknown);

                    // Not read on Windows.
                    //
                    // This was a "class heuristic": `USB3` or `xHCI` in the
                    // device's name or PnP path meant `Super`, `USB 2` or
                    // `EHCI` meant `High`. Those strings describe what the
                    // device *is*, and the entity asks what it *negotiated* --
                    // "a super-speed device on a high-speed port reports high,
                    // which is how a wrong cable shows". A USB 3 device on a
                    // USB 2 cable keeps `USB3` in its PnP path, so the reader
                    // reported `Super` for precisely the case the field exists
                    // to expose. On this host it made all six devices `super`.
                    //
                    // The negotiated speed comes from
                    // `IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX` on the
                    // parent hub, whose `USB_NODE_CONNECTION_INFORMATION_EX`
                    // carries a `Speed` field. That call now happens, in
                    // `windows_speed`, and this is its answer.
                    //
                    // Still `Unknown` for a device the hub did not describe,
                    // which the resolver reports as "the platform did not
                    // report a negotiated bus speed". On this host that is
                    // exactly the six root hubs, and it is the true answer for
                    // them: a root hub sits on no upstream port, so there is no
                    // negotiation above it to describe.
                    let speed = speeds
                        .get(&pnp_id.to_uppercase())
                        .copied()
                        .unwrap_or(UsbSpeed::Unknown);

                    // Extract serial from PNP ID (third segment)
                    let serial = pnp_id
                        .split('\\')
                        .nth(2)
                        .filter(|s| s.len() > 4 && !s.contains('&'))
                        .map(|s| s.to_string());

                    devices.push(UsbDevice {
                        // `USB\\VID_046D&PID_C548&MI_01\\9&24C94812&0&0001` --
                        // vendor, product, interface number and the hub-port
                        // chain Windows derived the instance id from. Unique
                        // for every node this query returns, including the
                        // interfaces that share a parent's location.
                        address: normalise_address(pnp_id),
                        // Not reported by this query. See the field docs.
                        bus_number: None,
                        port_number: None,
                        vendor_id: vid,
                        product_id: pid,
                        manufacturer: manufacturer.map(|s| s.to_string()),
                        product: if name.is_empty() {
                            None
                        } else {
                            Some(name.to_string())
                        },
                        description: description.map(|s| s.to_string()),
                        serial_number: serial,
                        class,
                        speed,
                    });
                }
            }
        }

        Ok(devices)
    }

    /// Fallback: enumerate USB devices from registry
    fn registry_enumerate_usb() -> Result<Vec<UsbDevice>, crate::error::SimonError> {
        use std::process::Command;
        let mut devices = Vec::new();

        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_USBControllerDevice | ForEach-Object { \
                 [PSCustomObject]@{Dependent=$_.Dependent.ToString()} } | ConvertTo-Json -Compress",
            ])
            .output()
            .map_err(|e| {
                crate::error::SimonError::Other(format!("Registry query failed: {}", e))
            })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
                let items = if json.is_array() {
                    json.as_array().cloned().unwrap_or_default()
                } else {
                    vec![json]
                };

                for item in items.iter() {
                    let dep = item.get("Dependent").and_then(|v| v.as_str()).unwrap_or("");
                    let (vid, pid) = parse_vid_pid(dep);
                    if vid.is_some() || pid.is_some() {
                        devices.push(UsbDevice {
                            // The `Dependent` reference is this fallback's
                            // device path -- the same instance path the WMI
                            // enumeration above reads, wrapped in a CIM
                            // reference.
                            address: normalise_address(dep),
                            bus_number: None,
                            port_number: None,
                            vendor_id: vid,
                            product_id: pid,
                            manufacturer: None,
                            product: Some(
                                dep.split('\\')
                                    .next_back()
                                    .unwrap_or("USB Device")
                                    .to_string(),
                            ),
                            description: None,
                            serial_number: None,
                            class: UsbDeviceClass::Unknown,
                            speed: UsbSpeed::Unknown,
                        });
                    }
                }
            }
        }

        Ok(devices)
    }
}

#[cfg(target_os = "windows")]
/// The vendor and product ids in a PnP id, each `None` when absent.
///
/// This returned `(0, 0)` for an id with no `VID_`, which is every root hub and
/// every virtual device. One of the two callers already guarded on
/// `vid != 0 || pid != 0`, so the sentinel was known to be meaningless there;
/// the other did not, which is how `[0000:0000]` reached the screen.
///
/// `pub(crate)` for `input`, whose Windows reader needs the same two numbers
/// out of the same shape of instance id. A second copy of this parser is how
/// the two readers would come to disagree about the same device.
pub(crate) fn parse_vid_pid(pnp_id: &str) -> (Option<u16>, Option<u16>) {
    let upper = pnp_id.to_uppercase();
    let vid = upper
        .find("VID_")
        .and_then(|i| u16::from_str_radix(&upper[i + 4..][..4.min(upper.len() - i - 4)], 16).ok());
    let pid = upper
        .find("PID_")
        .and_then(|i| u16::from_str_radix(&upper[i + 4..][..4.min(upper.len() - i - 4)], 16).ok());
    (vid, pid)
}

#[cfg(target_os = "linux")]
fn read_usb_attr(path: &std::path::Path, attr: &str) -> u32 {
    std::fs::read_to_string(path.join(attr))
        .ok()
        .and_then(|s| u32::from_str_radix(s.trim(), 16).ok())
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn read_usb_string(path: &std::path::Path, attr: &str) -> Option<String> {
    std::fs::read_to_string(path.join(attr))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// USB events for device monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsbEvent {
    Connected(UsbDevice),
    Disconnected(UsbDevice),
}

impl UsbMonitor {
    /// Check for device changes since last refresh
    /// Returns a list of connect/disconnect events
    pub fn poll_events(&mut self) -> Result<Vec<UsbEvent>, crate::error::SimonError> {
        let old_devices = self.devices.clone();
        self.refresh()?;

        let mut events = Vec::new();

        // Find disconnected devices (in old but not in new)
        for old in &old_devices {
            if !self.devices.iter().any(|d| device_matches(d, old)) {
                events.push(UsbEvent::Disconnected(old.clone()));
            }
        }

        // Find connected devices (in new but not in old)
        for new in &self.devices {
            if !old_devices.iter().any(|d| device_matches(d, new)) {
                events.push(UsbEvent::Connected(new.clone()));
            }
        }

        Ok(events)
    }
}

fn device_matches(a: &UsbDevice, b: &UsbDevice) -> bool {
    a.vendor_id == b.vendor_id
        && a.product_id == b.product_id
        && a.bus_number == b.bus_number
        && a.port_number == b.port_number
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
    fn test_usb_monitor_creation() {
        match UsbMonitor::new() {
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
    fn test_usb_monitor_devices() {
        let monitor = UsbMonitor::new().unwrap();
        // Real USB detection may find devices or not depending on environment
        // Just verify the method doesn't panic
        let _devices = monitor.devices();
    }

    /// A root hub has no VID or PID, and must not be given one.
    ///
    /// `simon cli usb` printed `[0000:0000]` for every root hub and virtual
    /// device on this desktop — four of forty-one entries — an identifier no
    /// caller could tell from a device that genuinely reports those ids. The
    /// ids below are verbatim from `Get-PnpDevice -Class USB`.
    #[cfg(target_os = "windows")]
    #[test]
    fn an_entry_with_no_ids_is_not_given_zeros() {
        assert_eq!(
            parse_vid_pid(r"USB\ROOT_HUB30\9&EBA7CE&0&0"),
            (None, None),
            "a root hub carries no VID_ or PID_ at all"
        );
        assert_eq!(
            parse_vid_pid(r"USB\VID_046D&PID_C548\5&2E6429FC&0&3"),
            (Some(0x046d), Some(0xc548)),
            "a real device's ids are read"
        );
        // A device reporting 0000:0000 is a different fact from one reporting
        // nothing, and the two must not collapse.
        assert_eq!(
            parse_vid_pid(r"USB\VID_0000&PID_0000\6&1"),
            (Some(0), Some(0)),
            "an id of literally 0000 is a value, not an absence"
        );
    }

    #[test]
    fn test_usb_device_serialization() {
        let device = UsbDevice {
            address: "1_4_2".into(),
            bus_number: Some(1),
            port_number: Some(2),
            vendor_id: Some(0x1234),
            product_id: Some(0x5678),
            manufacturer: Some("Test Manufacturer".to_string()),
            product: Some("Test Product".to_string()),
            description: None,
            serial_number: Some("ABC123".to_string()),
            class: UsbDeviceClass::MassStorage,
            speed: UsbSpeed::High,
        };
        let json = serde_json::to_string(&device).unwrap();
        let deserialized: UsbDevice = serde_json::from_str(&json).unwrap();
        assert_eq!(device.vendor_id, deserialized.vendor_id);
        assert_eq!(device.product_id, deserialized.product_id);
        assert_eq!(device.serial_number, deserialized.serial_number);
    }

    #[test]
    fn test_usb_speed_variants() {
        let speeds = [
            UsbSpeed::Low,
            UsbSpeed::Full,
            UsbSpeed::High,
            UsbSpeed::Super,
            UsbSpeed::SuperPlus,
            UsbSpeed::SuperPlusx2,
            UsbSpeed::Usb4,
            UsbSpeed::Unknown,
        ];
        for speed in speeds {
            let json = serde_json::to_string(&speed).unwrap();
            let deserialized: UsbSpeed = serde_json::from_str(&json).unwrap();
            assert_eq!(speed, deserialized);
        }
    }

    /// The ids in this test were read off one Windows host with
    /// `Get-CimInstance Win32_PnPEntity | Select HardwareID, CompatibleID`,
    /// not composed to match the parser.
    ///
    /// The root-hub case is the reason the function takes both lists: Windows
    /// leaves `CompatibleID` empty on a root hub, so a reader that looked only
    /// there classified all six of this machine's root hubs `Unknown` and
    /// published an absence reason saying no hub identifier was recorded.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_root_hub_is_a_hub_by_the_identifier_windows_puts_in_its_hardware_ids() {
        let hardware = [
            r"USB\ROOT_HUB30&VID1022&PID43FD&REV0001".to_string(),
            r"USB\ROOT_HUB30&VID1022&PID43FD".to_string(),
            r"USB\ROOT_HUB30".to_string(),
        ];
        assert_eq!(
            class_from_device_ids(&hardware, &[]),
            Some(UsbDeviceClass::Hub),
            "a root hub declares itself in its hardware ids and carries no \
             compatible ids at all"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn an_interface_class_still_wins_over_the_hardware_ids_beside_it() {
        // A Logitech receiver interface: the hardware ids name the vendor and
        // product, and the class is in the compatible ids.
        let hardware = [
            r"USB\VID_046D&PID_C548&REV_0503&MI_01".to_string(),
            r"USB\VID_046D&PID_C548&MI_01".to_string(),
        ];
        let compatible = [
            r"USB\COMPAT_VID_046d&Class_03&SubClass_01&Prot_02".to_string(),
            r"USB\Class_03".to_string(),
        ];
        assert_eq!(
            class_from_device_ids(&hardware, &compatible),
            Some(UsbDeviceClass::Hid)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn a_composite_parent_and_a_usbstor_disk_both_decline() {
        let composite_hw = [r"USB\VID_046D&PID_C548&REV_0503".to_string()];
        let composite_co = [
            r"USB\COMPAT_VID_046D&DevClass_00&SubClass_00&Prot00".to_string(),
            r"USB\DevClass_00".to_string(),
            r"USB\COMPOSITE".to_string(),
        ];
        assert_eq!(
            class_from_device_ids(&composite_hw, &composite_co),
            None,
            "class 00 is the device saying its interfaces answer this, not a class"
        );

        let stor_hw = [
            r"USBSTOR\DiskLinux___File-Stor_Gadget0515".to_string(),
            r"USBSTOR\GenDisk".to_string(),
        ];
        let stor_co = [
            r"USBSTOR\Disk".to_string(),
            r"USBSTOR\RAW".to_string(),
            "GenDisk".to_string(),
        ];
        assert_eq!(
            class_from_device_ids(&stor_hw, &stor_co),
            None,
            "a storage-stack child node has no USB descriptor to declare a class"
        );
    }

    #[test]
    fn test_usb_class_variants() {
        let classes = [
            UsbDeviceClass::Audio,
            UsbDeviceClass::Communication,
            UsbDeviceClass::Hid,
            UsbDeviceClass::Printer,
            UsbDeviceClass::MassStorage,
            UsbDeviceClass::Hub,
            UsbDeviceClass::Video,
            UsbDeviceClass::Wireless,
            UsbDeviceClass::Vendor,
            UsbDeviceClass::Unknown,
        ];
        for class in classes {
            let json = serde_json::to_string(&class).unwrap();
            let deserialized: UsbDeviceClass = serde_json::from_str(&json).unwrap();
            assert_eq!(class, deserialized);
        }
    }
}
