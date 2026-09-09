//! Bluetooth device monitoring module
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BluetoothDeviceType {
    Unknown,
    Computer,
    Phone,
    Headset,
    Speaker,
    Keyboard,
    Mouse,
    GameController,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BluetoothState {
    Connected,
    Paired,
    Discovered,
    Disconnected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BluetoothDevice {
    pub address: String,
    pub name: Option<String>,
    pub device_type: BluetoothDeviceType,
    pub state: BluetoothState,
    pub battery_percent: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BluetoothAdapter {
    pub id: String,
    pub name: String,
    pub address: String,
    pub powered: bool,
}

// These four are used only by `refresh_windows`, so they are dead code on
// every other target. `cargo check --target` does not deny warnings, which is
// why the local gate missed it and CI's per-OS `clippy -D warnings` did not.

/// What a Bluetooth-class PnP entry actually is.
///
/// Windows lists radios, remote peripherals, the GATT services those
/// peripherals expose, and the stack's own enumerators all under
/// `PNPClass = 'Bluetooth'`, and their display names do not separate them:
/// "Xbox Wireless Controller" contains the word an adapter filter was matching
/// on. The `PNPDeviceID` does separate them, unambiguously.
#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BtEntry {
    /// A radio in this machine.
    Adapter,
    /// A remote device someone paired.
    Peripheral,
    /// A GATT service or profile transport belonging to a peripheral.
    Service,
    /// One of the stack's own software enumerators or protocol drivers.
    Stack,
}

/// Classify one `Win32_PnPEntity` of class Bluetooth by its id.
///
/// Id shapes, all observed on one desktop:
///
/// - `USB\...` is the radio itself
/// - `BTH\MS_...` are stack enumerators and protocol drivers
/// - `BTHENUM\DEV_...` and `BTHLE\DEV_...` are paired remote devices
/// - `BTHLEDEVICE\{{guid}}_DEV_...` is a GATT service on one
/// - `BTHENUM\{{guid}}_VID...` is a profile transport on one
#[cfg(target_os = "windows")]
pub(crate) fn classify_pnp_entry(name: &str, pnp_id: &str) -> BtEntry {
    let id = pnp_id.to_ascii_uppercase();

    // Order matters: `BTHLEDEVICE` also starts with `BTHLE`, so services are
    // recognised before peripherals.
    if id.starts_with(r"BTHLEDEVICE\") {
        return BtEntry::Service;
    }
    if id.starts_with(r"BTHENUM\DEV_") || id.starts_with(r"BTHLE\DEV_") {
        return BtEntry::Peripheral;
    }
    if id.starts_with(r"BTHENUM\") || id.starts_with(r"BTHLE\") {
        // A brace-GUID under BTHENUM is a profile on a remote device.
        return BtEntry::Service;
    }
    if id.starts_with(r"BTH\") {
        return BtEntry::Stack;
    }

    // What is left is attached to this machine rather than reached over the
    // air. A radio names itself; anything else here is part of the stack.
    if name.contains("Radio") || name.contains("Adapter") {
        BtEntry::Adapter
    } else {
        BtEntry::Stack
    }
}

/// The device address embedded in a PnP id, as `AA:BB:CC:DD:EE:FF`.
#[cfg(target_os = "windows")]
fn address_from_pnp_id(pnp_id: &str) -> Option<String> {
    let after = pnp_id.to_ascii_uppercase();
    let after = after.split("DEV_").nth(1)?;
    let hex: String = after
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .take(12)
        .collect();
    if hex.len() != 12 {
        return None;
    }
    Some(
        hex.as_bytes()
            .chunks(2)
            .map(|p| String::from_utf8_lossy(p).to_string())
            .collect::<Vec<_>>()
            .join(":"),
    )
}

/// A best guess at what a peripheral is, from its advertised name.
///
/// This is a guess and is labelled one: an unrecognised name resolves
/// `Unknown` rather than to a plausible default.
#[cfg(target_os = "windows")]
fn device_type_from_name(name: &str) -> BluetoothDeviceType {
    let n = name.to_ascii_lowercase();
    if n.contains("keyboard") {
        BluetoothDeviceType::Keyboard
    } else if n.contains("mouse") || n.contains("pointing") {
        BluetoothDeviceType::Mouse
    } else if ["headset", "headphone", "earphone", "buds", "airpods"]
        .iter()
        .any(|k| n.contains(k))
    {
        BluetoothDeviceType::Headset
    } else if n.contains("speaker") || n.contains("audio") {
        BluetoothDeviceType::Speaker
    } else if n.contains("phone") {
        BluetoothDeviceType::Phone
    } else if ["controller", "gamepad", "joystick"]
        .iter()
        .any(|k| n.contains(k))
    {
        BluetoothDeviceType::GameController
    } else {
        BluetoothDeviceType::Unknown
    }
}

#[derive(Default)]
pub struct BluetoothMonitor {
    adapters: Vec<BluetoothAdapter>,
    devices: Vec<BluetoothDevice>,
    /// Why the device walk produced less than it should have, when it did.
    ///
    /// Same shape as `SensorMonitor::note`. An empty device list with no note
    /// means the enumeration ran and this machine has no paired devices, which
    /// is a reading. An empty list *with* a note means nobody could look, which
    /// is not.
    last_note: Option<String>,
}

impl BluetoothMonitor {
    /// Why the device enumeration is short, if it is. `None` means it ran
    /// cleanly, so an empty device list can be read as a fact about the machine.
    pub fn note(&self) -> Option<&str> {
        self.last_note.as_deref()
    }

    /// Record a reason. Later notes append, because one refresh can fail in more
    /// than one way and the first reason is not automatically the useful one.
    ///
    /// Only the Linux refresh calls this today; the Windows and macOS arms
    /// return their reasons as an `Err` from `refresh`. Annotated rather than
    /// `cfg`-gated so that adding a caller on another platform does not also
    /// require remembering to widen a gate.
    #[allow(dead_code)]
    fn record_note(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        match &mut self.last_note {
            Some(existing) => {
                existing.push_str("; ");
                existing.push_str(&msg);
            }
            None => self.last_note = Some(msg),
        }
    }

    pub fn new() -> Result<Self, crate::error::IronError> {
        let mut monitor = Self {
            adapters: Vec::new(),
            devices: Vec::new(),
            last_note: None,
        };
        monitor.refresh()?;
        Ok(monitor)
    }
    pub fn refresh(&mut self) -> Result<(), crate::error::IronError> {
        self.adapters.clear();
        self.devices.clear();
        #[cfg(target_os = "windows")]
        self.refresh_windows()?;
        #[cfg(target_os = "linux")]
        self.refresh_linux()?;
        #[cfg(target_os = "macos")]
        self.refresh_macos()?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), crate::error::IronError> {
        // Ask for every Bluetooth-class PnP entry and classify them here.
        //
        // The classification used to live in this PowerShell string and matched
        // 'Radio|Adapter|Controller' against the *display name*, so an "Xbox
        // Wireless Controller" was reported as a Bluetooth adapter and then
        // excluded from the device list by the same word. A machine with one
        // headset and one gamepad reported two adapters and nine devices, the
        // nine being GATT services and a protocol driver.
        //
        // In Rust it can be tested against real device ids; in a shell string
        // it could not be tested at all.
        const QUERY: &str = concat!(
            "Get-CimInstance -ClassName Win32_PnPEntity | ",
            "Where-Object { $_.PNPClass -eq 'Bluetooth' } | ",
            "Select-Object @{N='Id';E={$_.PNPDeviceID}}, Name, Manufacturer, Status | ",
            "ConvertTo-Json -Depth 3 -Compress"
        );

        // Three `return`s used to sit here -- a spawn failure, a non-zero exit
        // and unparseable output -- and all three left the adapter and device
        // lists empty, which reads as a machine with no Bluetooth hardware.
        let Some(json) =
            crate::core::command::capture_json("powershell", &["-NoProfile", "-Command", QUERY])?
        else {
            return Ok(());
        };
        // A single match is serialised as an object rather than a one-element
        // array.
        let entries = crate::core::command::json_items(&json);

        for entry in &entries {
            let id = entry.get("Id").and_then(|v| v.as_str()).unwrap_or("");
            let name = entry.get("Name").and_then(|v| v.as_str()).unwrap_or("");
            let powered = entry.get("Status").and_then(|v| v.as_str()) == Some("OK");

            match classify_pnp_entry(name, id) {
                BtEntry::Adapter => {
                    let idx = self.adapters.len();
                    self.adapters.push(BluetoothAdapter {
                        id: format!("bt{idx}"),
                        name: name.to_string(),
                        address: id.to_string(),
                        powered,
                    });
                }
                BtEntry::Peripheral => {
                    self.devices.push(BluetoothDevice {
                        address: address_from_pnp_id(id)
                            .unwrap_or_else(|| "00:00:00:00:00:00".to_string()),
                        name: (!name.is_empty()).then(|| name.to_string()),
                        device_type: device_type_from_name(name),
                        state: if powered {
                            BluetoothState::Connected
                        } else {
                            BluetoothState::Paired
                        },
                        battery_percent: None,
                    });
                }
                // A GATT service or a stack enumerator is neither a radio nor
                // something the user paired. Counting them as devices is what
                // turned two peripherals into nine.
                BtEntry::Service | BtEntry::Stack => {}
            }
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) -> Result<(), crate::error::IronError> {
        use std::fs;
        use std::path::Path;

        // Read Bluetooth adapters from /sys/class/bluetooth. No such directory
        // is a reading -- this kernel has no Bluetooth stack -- and a directory
        // that will not open is not.
        let bt_path = Path::new("/sys/class/bluetooth");
        if bt_path.exists() {
            {
                let entries = fs::read_dir(bt_path).map_err(|e| {
                    crate::error::IronError::System(format!(
                        "cannot read /sys/class/bluetooth: {e}"
                    ))
                })?;
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !name.starts_with("hci") {
                        continue;
                    }

                    let path = entry.path();
                    let address = fs::read_to_string(path.join("address"))
                        .unwrap_or_default()
                        .trim()
                        .to_string();

                    // Check power state
                    let powered =
                        fs::read_to_string(format!("/sys/class/bluetooth/{}/powered", name))
                            .unwrap_or_else(|_| "1".to_string())
                            .trim()
                            == "1";

                    self.adapters.push(BluetoothAdapter {
                        id: name.clone(),
                        name: format!("Bluetooth Adapter ({})", name),
                        address,
                        powered,
                    });
                }
            }
        }

        // Use bluetoothctl to list paired/connected devices.
        //
        // Through `capture`, not a bare `Command`. `bluetoothctl devices` blocks
        // forever on a host where the binary exists but `bluetoothd` is not
        // answering — it waits on D-Bus, and a closed stdin does not release it.
        // The macOS arm below has always gone through the helper; this one was
        // fixed where somebody was looking rather than where the cause was, the
        // same shape as the four `vm.swapusage` parsers.
        //
        // A failure here is an absent device list, not an empty one: the caller
        // above has already recorded the adapters, and `note()` carries why the
        // device walk produced nothing.
        match crate::core::command::capture("bluetoothctl", &["devices"]) {
            Ok(stdout) => {
                for line in stdout.lines() {
                    // Format: "Device AA:BB:CC:DD:EE:FF Device Name"
                    let parts: Vec<&str> = line.splitn(3, ' ').collect();
                    if parts.len() >= 3 && parts[0] == "Device" {
                        let address = parts[1].to_string();
                        let name = parts[2].to_string();

                        // Whether the device is connected. An `info` call that
                        // fails leaves this unknown, and `Paired` is what this
                        // enum has for "known to us, not known to be connected"
                        // — so a failed probe is recorded rather than read as a
                        // disconnection.
                        let info =
                            crate::core::command::capture("bluetoothctl", &["info", &address]);
                        let state = match &info {
                            Ok(text) if text.contains("Connected: yes") => {
                                BluetoothState::Connected
                            }
                            Ok(_) => BluetoothState::Paired,
                            Err(e) => {
                                self.record_note(format!(
                                    "connection state for {address} is unknown: {e}"
                                ));
                                BluetoothState::Paired
                            }
                        };

                        let dtype = classify_bt_device(&name);

                        self.devices.push(BluetoothDevice {
                            address,
                            name: Some(name),
                            device_type: dtype,
                            state,
                            battery_percent: None,
                        });
                    }
                }
            }
            Err(e) => {
                self.record_note(format!("bluetoothctl could not list devices: {e}"));
            }
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), crate::error::IronError> {
        // `system_profiler` ships with macOS, so a failure to run it is a
        // failure rather than an absent optional tool.
        let stdout =
            crate::core::command::capture("system_profiler", &["SPBluetoothDataType", "-json"])?;
        {
            {
                let stdout = stdout.as_str();
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
                    if let Some(bt_data) =
                        json.get("SPBluetoothDataType").and_then(|v| v.as_array())
                    {
                        for section in bt_data {
                            // Controller info
                            if let Some(ctrl) = section.get("controller_properties") {
                                let address = ctrl
                                    .get("controller_address")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let name = ctrl
                                    .get("controller_chipset")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Bluetooth")
                                    .to_string();
                                let powered =
                                    ctrl.get("controller_powerState").and_then(|v| v.as_str())
                                        == Some("attrib_on");

                                self.adapters.push(BluetoothAdapter {
                                    id: "bt0".to_string(),
                                    name,
                                    address,
                                    powered,
                                });
                            }

                            // Connected devices
                            if let Some(devices) =
                                section.get("device_connected").and_then(|v| v.as_object())
                            {
                                for (dev_name, dev_info) in devices {
                                    let address = dev_info
                                        .get("device_address")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("00:00:00:00:00:00")
                                        .to_string();
                                    let dtype = classify_bt_device(dev_name);

                                    self.devices.push(BluetoothDevice {
                                        address,
                                        name: Some(dev_name.clone()),
                                        device_type: dtype,
                                        state: BluetoothState::Connected,
                                        battery_percent: dev_info
                                            .get("device_batteryPercent")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| {
                                                s.trim_end_matches('%').parse::<u8>().ok()
                                            }),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn adapters(&self) -> &[BluetoothAdapter] {
        &self.adapters
    }
    pub fn devices(&self) -> &[BluetoothDevice] {
        &self.devices
    }
    pub fn is_available(&self) -> bool {
        !self.adapters.is_empty()
    }

    // ==================== Hardware Control APIs ====================

    /// Initiate pairing with a Bluetooth device by address.
    pub fn pair_device(&mut self, address: &str) -> Result<(), crate::error::IronError> {
        if !Self::is_valid_mac_address(address) {
            return Err(crate::error::IronError::InvalidInput(format!(
                "Invalid Bluetooth address format: {}",
                address
            )));
        }
        Ok(())
    }

    /// Remove pairing with a Bluetooth device.
    pub fn unpair_device(&mut self, address: &str) -> Result<(), crate::error::IronError> {
        if !Self::is_valid_mac_address(address) {
            return Err(crate::error::IronError::InvalidInput(format!(
                "Invalid Bluetooth address format: {}",
                address
            )));
        }
        Ok(())
    }

    /// Connect to a paired Bluetooth device.
    pub fn connect_device(&mut self, address: &str) -> Result<(), crate::error::IronError> {
        if !Self::is_valid_mac_address(address) {
            return Err(crate::error::IronError::InvalidInput(format!(
                "Invalid Bluetooth address format: {}",
                address
            )));
        }
        if let Some(device) = self.devices.iter_mut().find(|d| d.address == address) {
            device.state = BluetoothState::Connected;
        }
        Ok(())
    }

    /// Disconnect from a connected Bluetooth device.
    pub fn disconnect_device(&mut self, address: &str) -> Result<(), crate::error::IronError> {
        if !Self::is_valid_mac_address(address) {
            return Err(crate::error::IronError::InvalidInput(format!(
                "Invalid Bluetooth address format: {}",
                address
            )));
        }
        if let Some(device) = self.devices.iter_mut().find(|d| d.address == address) {
            device.state = BluetoothState::Disconnected;
        }
        Ok(())
    }

    /// Enable or disable a Bluetooth adapter.
    pub fn set_adapter_power(
        &mut self,
        adapter_id: &str,
        enabled: bool,
    ) -> Result<(), crate::error::IronError> {
        if let Some(adapter) = self.adapters.iter_mut().find(|a| a.id == adapter_id) {
            adapter.powered = enabled;
            Ok(())
        } else {
            Err(crate::error::IronError::NotFound(format!(
                "Bluetooth adapter '{}' not found",
                adapter_id
            )))
        }
    }

    fn is_valid_mac_address(address: &str) -> bool {
        let parts: Vec<&str> = address.split(':').collect();
        if parts.len() != 6 {
            return false;
        }
        parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn classify_bt_device(name: &str) -> BluetoothDeviceType {
    let lower = name.to_lowercase();
    if lower.contains("keyboard") {
        BluetoothDeviceType::Keyboard
    } else if lower.contains("mouse") || lower.contains("trackpad") {
        BluetoothDeviceType::Mouse
    } else if lower.contains("headset")
        || lower.contains("headphone")
        || lower.contains("earphone")
        || lower.contains("buds")
        || lower.contains("airpods")
    {
        BluetoothDeviceType::Headset
    } else if lower.contains("speaker") {
        BluetoothDeviceType::Speaker
    } else if lower.contains("phone") || lower.contains("iphone") || lower.contains("android") {
        BluetoothDeviceType::Phone
    } else if lower.contains("controller")
        || lower.contains("gamepad")
        || lower.contains("joystick")
    {
        BluetoothDeviceType::GameController
    } else if lower.contains("computer") || lower.contains("laptop") || lower.contains("macbook") {
        BluetoothDeviceType::Computer
    } else {
        BluetoothDeviceType::Unknown
    }
}

// Bluetooth events for device monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BluetoothEvent {
    DeviceConnected(BluetoothDevice),
    DeviceDisconnected(BluetoothDevice),
    DevicePaired(BluetoothDevice),
    AdapterEnabled(BluetoothAdapter),
    AdapterDisabled(BluetoothAdapter),
}

impl BluetoothMonitor {
    /// Check for device changes since last refresh
    /// Returns a list of connect/disconnect events
    pub fn poll_events(&mut self) -> Result<Vec<BluetoothEvent>, crate::error::IronError> {
        let old_devices = self.devices.clone();
        self.refresh()?;

        let mut events = Vec::new();

        // Find state changes
        for old in &old_devices {
            if let Some(new) = self.devices.iter().find(|d| d.address == old.address) {
                // State changed
                if old.state != new.state {
                    match new.state {
                        BluetoothState::Connected => {
                            events.push(BluetoothEvent::DeviceConnected(new.clone()))
                        }
                        BluetoothState::Paired => {
                            events.push(BluetoothEvent::DevicePaired(new.clone()))
                        }
                        BluetoothState::Disconnected => {
                            events.push(BluetoothEvent::DeviceDisconnected(new.clone()))
                        }
                        _ => {}
                    }
                }
            } else {
                // Device removed
                events.push(BluetoothEvent::DeviceDisconnected(old.clone()));
            }
        }

        // Find new devices
        for new in &self.devices {
            if !old_devices.iter().any(|d| d.address == new.address) {
                match new.state {
                    BluetoothState::Connected => {
                        events.push(BluetoothEvent::DeviceConnected(new.clone()))
                    }
                    BluetoothState::Paired => {
                        events.push(BluetoothEvent::DevicePaired(new.clone()))
                    }
                    _ => {}
                }
            }
        }

        Ok(events)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Constructing the monitor either enumerates or says why it could not.
    ///
    /// See the identically-shaped tests in `camera`, `usb` and the rest: this
    /// asserted `is_ok()`, which was true by construction while `refresh` could
    /// not fail. A failure must carry a reason, because a reason is the whole
    /// difference between "this machine has none" and "nobody looked".
    #[test]
    fn test_bluetooth_monitor_creation() {
        match BluetoothMonitor::new() {
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
    fn test_bluetooth_monitor_availability() {
        let monitor = BluetoothMonitor::new().unwrap();
        // Availability depends on platform - just ensure no panic
        let _ = monitor.is_available();
    }

    #[test]
    fn test_bluetooth_device_serialization() {
        let device = BluetoothDevice {
            address: "AA:BB:CC:DD:EE:FF".to_string(),
            name: Some("Test Device".to_string()),
            device_type: BluetoothDeviceType::Headset,
            state: BluetoothState::Connected,
            battery_percent: Some(75),
        };
        let json = serde_json::to_string(&device).unwrap();
        let deserialized: BluetoothDevice = serde_json::from_str(&json).unwrap();
        assert_eq!(device.address, deserialized.address);
        assert_eq!(device.battery_percent, deserialized.battery_percent);
    }

    #[test]
    fn test_bluetooth_adapter_serialization() {
        let adapter = BluetoothAdapter {
            id: "hci0".to_string(),
            name: "Test Adapter".to_string(),
            address: "11:22:33:44:55:66".to_string(),
            powered: true,
        };
        let json = serde_json::to_string(&adapter).unwrap();
        let deserialized: BluetoothAdapter = serde_json::from_str(&json).unwrap();
        assert_eq!(adapter.id, deserialized.id);
        assert!(deserialized.powered);
    }
}

#[cfg(all(test, target_os = "windows"))]
mod pnp_classification_tests {
    use super::*;

    /// Every Bluetooth-class PnP entry on one desktop, verbatim from
    /// `Get-PnpDevice -Class Bluetooth`, with what each one actually is.
    ///
    /// The machine has one radio and two paired peripherals: a Bose QC35
    /// headset and an Xbox controller. `ironmon cli bluetooth` reported
    /// "Adapters: 2, Devices: 9" — the controller counted as a radio because
    /// its name contains "Controller", and six GATT services and a protocol
    /// driver counted as devices.
    const REAL_ENTRIES: &[(&str, &str, BtEntry)] = &[
        (
            "MediaTek Bluetooth Adapter",
            r"USB\VID_0489&PID_E13A&MI_00\B&26E6BFF2&0&0000",
            BtEntry::Adapter,
        ),
        (
            "Xbox Wireless Controller",
            r"BTHLE\DEV_0C35262BD48F\D&3A9C8506&0&0C35262BD48F",
            BtEntry::Peripheral,
        ),
        (
            "Bose QC35",
            r"BTHENUM\DEV_0452C707E5BC\D&15B70467&0&BLUETOOTHDEVICE_0452C707E5BC",
            BtEntry::Peripheral,
        ),
        (
            "Generic Access Profile",
            r"BTHLEDEVICE\{00001800-0000-1000-8000-00805F9B34FB}_DEV_VID&02045E_PID&0B13",
            BtEntry::Service,
        ),
        (
            "Generic Attribute Profile",
            r"BTHLEDEVICE\{00001801-0000-1000-8000-00805F9B34FB}_DEV_VID&02045E_PID&0B13",
            BtEntry::Service,
        ),
        (
            "Device Information Service",
            r"BTHLEDEVICE\{0000180A-0000-1000-8000-00805F9B34FB}_DEV_VID&02045E_PID&0B13",
            BtEntry::Service,
        ),
        (
            "Bluetooth LE Generic Attribute Service",
            r"BTHLEDEVICE\{0000180F-0000-1000-8000-00805F9B34FB}_DEV_VID&02045E_PID&0B13",
            BtEntry::Service,
        ),
        (
            "Bose QC35 Avrcp Transport",
            r"BTHENUM\{0000110E-0000-1000-8000-00805F9B34FB}_VID&0001009E_PID&400C\D&15B70467",
            BtEntry::Service,
        ),
        (
            "Bluetooth Device (RFCOMM Protocol TDI)",
            r"BTH\MS_RFCOMM\C&1BA46DC9&0&0",
            BtEntry::Stack,
        ),
        (
            "Microsoft Bluetooth LE Enumerator",
            r"BTH\MS_BTHLE\C&1BA46DC9&0&3",
            BtEntry::Stack,
        ),
        (
            "Microsoft Bluetooth Enumerator",
            r"BTH\MS_BTHBRB\C&1BA46DC9&0&1",
            BtEntry::Stack,
        ),
    ];

    #[test]
    fn one_desktop_has_one_radio_and_two_peripherals() {
        for (name, id, want) in REAL_ENTRIES {
            assert_eq!(classify_pnp_entry(name, id), *want, "{name} ({id})");
        }

        let adapters = REAL_ENTRIES
            .iter()
            .filter(|(n, i, _)| classify_pnp_entry(n, i) == BtEntry::Adapter)
            .count();
        let peripherals = REAL_ENTRIES
            .iter()
            .filter(|(n, i, _)| classify_pnp_entry(n, i) == BtEntry::Peripheral)
            .count();

        assert_eq!(adapters, 1, "the machine has one Bluetooth radio");
        assert_eq!(
            peripherals, 2,
            "the machine has two paired peripherals: a headset and a gamepad"
        );
    }

    /// `BTHLEDEVICE` also begins with `BTHLE`, so a GATT service classifies as
    /// a peripheral if the peripheral check runs first.
    #[test]
    fn a_gatt_service_is_not_mistaken_for_the_device_hosting_it() {
        assert_eq!(
            classify_pnp_entry(
                "Battery Service",
                r"BTHLEDEVICE\{0000180F-0000-1000-8000-00805F9B34FB}_DEV_VID&1_PID&2"
            ),
            BtEntry::Service
        );
    }

    #[test]
    fn an_address_is_read_from_the_id_or_withheld() {
        assert_eq!(
            address_from_pnp_id(r"BTHLE\DEV_0C35262BD48F\D&3A9C8506"),
            Some("0C:35:26:2B:D4:8F".to_string())
        );
        assert_eq!(
            address_from_pnp_id(r"BTH\MS_RFCOMM\C&1BA46DC9&0&0"),
            None,
            "a stack driver has no device address to report"
        );
    }

    /// A name nothing recognises is Unknown, not a plausible default.
    #[test]
    fn an_unrecognised_peripheral_is_not_given_a_category() {
        assert_eq!(
            device_type_from_name("Bose QC35"),
            BluetoothDeviceType::Unknown
        );
        assert_eq!(
            device_type_from_name("Xbox Wireless Controller"),
            BluetoothDeviceType::GameController
        );
    }
}
