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
    /// Known to the system, connection state not established. Used where the
    /// probe that answers it failed. This used to be `Paired`, which says the
    /// device is known and not connected, so a failed probe read as a
    /// disconnection.
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BluetoothDevice {
    /// The device's address, or `None` where it was not read. A peripheral
    /// whose PnP id carries no address used to be given `00:00:00:00:00:00`,
    /// which is a real (if reserved) address and not a missing one.
    pub address: Option<String>,
    pub name: Option<String>,
    /// The device's kind, from its class of device where the platform reports
    /// one, or `None` where it was not read. The advertised name used to decide
    /// this, so a headset called "Bose QC35" was `Unknown` and anything with
    /// "buds" in its name was a headset whatever it was.
    pub device_type: Option<BluetoothDeviceType>,
    pub state: BluetoothState,
    pub battery_percent: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BluetoothAdapter {
    pub id: String,
    /// The adapter's name as the platform reports it, or `None` where it
    /// reports none. macOS used to fall back to the literal "Bluetooth" when
    /// `controller_chipset` was absent, which named a product nobody had read.
    pub name: Option<String>,
    /// The radio's own hardware address as `aa:bb:cc:dd:ee:ff`, or `None`
    /// where the platform did not report one.
    ///
    /// Windows used to store the adapter's PnP device id here, which is a
    /// path and not an address, so every Windows adapter published a value the
    /// ontology had to reject. The radio address is read from the adapter's
    /// `DEVPKEY_Bluetooth_RadioAddress` property.
    pub address: Option<String>,
    /// Whether the radio is on, or `None` where it was not read.
    ///
    /// Linux defaulted an unreadable `powered` file to on; Windows reported
    /// the PnP node's `Status == "OK"`, which says the device is working, not
    /// that its radio is on. Only macOS reads a power state.
    pub powered: Option<bool>,
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

/// Normalise a Bluetooth hardware address to lowercase `aa:bb:cc:dd:ee:ff`.
///
/// Accepts six hex octets separated by `:` or `-`, or twelve bare hex digits.
/// Anything else is `None`, so a PnP path or an empty string is never published
/// as an address.
pub(crate) fn normalise_bd_address(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let hex: String = if raw.contains(':') || raw.contains('-') {
        let octets: Vec<&str> = raw.split([':', '-']).collect();
        if octets.len() != 6 || octets.iter().any(|o| o.len() != 2) {
            return None;
        }
        octets.concat()
    } else {
        raw.to_string()
    };
    if hex.len() != 12 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    Some(
        hex.as_bytes()
            .chunks(2)
            .map(|p| String::from_utf8_lossy(p).to_string())
            .collect::<Vec<_>>()
            .join(":"),
    )
}

/// The radio's hardware address from its `DEVPKEY_Bluetooth_RadioAddress`
/// property, or `None` if the property is absent.
///
/// `Get-PnpDeviceProperty` takes a couple of seconds to start, so this is asked
/// for one radio at a time, and only for an adapter this refresh found. Nothing
/// is derived from the network adapter in its place.
#[cfg(target_os = "windows")]
fn radio_address(instance_id: &str) -> Option<String> {
    let query = format!(
        "(Get-PnpDeviceProperty -InstanceId '{}' -KeyName DEVPKEY_Bluetooth_RadioAddress -ErrorAction SilentlyContinue).Data | ConvertTo-Json -Compress",
        instance_id.replace('\'', "''")
    );
    let value =
        crate::core::command::capture_json("powershell", &["-NoProfile", "-Command", &query])
            .ok()
            .flatten();
    radio_address_from_value(value.as_ref())
}

/// The radio address from a `DEVPKEY_Bluetooth_RadioAddress` value.
///
/// Windows reports it as a 48-bit integer whose hex digits are the address in
/// order: `145815598978622` is `84:9e:56:fa:d6:3e`. A missing or out-of-range
/// value is `None`.
// Windows only, as are the tests that exercise it.
#[cfg(target_os = "windows")]
fn radio_address_from_value(value: Option<&serde_json::Value>) -> Option<String> {
    let n = value?.as_u64()?;
    if n >= 1 << 48 {
        return None;
    }
    normalise_bd_address(&format!("{n:012x}"))
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
    /// Whether the adapter walk ran. See [`Self::adapters_read`].
    adapters_read: bool,
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
            adapters_read: false,
        };
        monitor.refresh()?;
        Ok(monitor)
    }
    pub fn refresh(&mut self) -> Result<(), crate::error::IronError> {
        self.adapters.clear();
        self.devices.clear();
        self.adapters_read = false;
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
            // Not read, so `adapters_read` stays false and the adapter list is
            // reported as unknown rather than as no adapters.
            self.record_note("Bluetooth PnP query returned no data; adapters were not read");
            return Ok(());
        };
        self.adapters_read = true;
        // A single match is serialised as an object rather than a one-element
        // array.
        let entries = crate::core::command::json_items(&json);

        for entry in &entries {
            let id = entry.get("Id").and_then(|v| v.as_str()).unwrap_or("");
            let name = entry.get("Name").and_then(|v| v.as_str()).unwrap_or("");

            match classify_pnp_entry(name, id) {
                BtEntry::Adapter => {
                    let idx = self.adapters.len();
                    self.adapters.push(BluetoothAdapter {
                        id: format!("bt{idx}"),
                        name: (!name.is_empty()).then(|| name.to_string()),
                        address: radio_address(id),
                        // PnP status is not radio state; nothing here reads it.
                        powered: None,
                    });
                }
                BtEntry::Peripheral => {
                    self.devices.push(BluetoothDevice {
                        address: address_from_pnp_id(id),
                        name: (!name.is_empty()).then(|| name.to_string()),
                        // The PnP node carries no class of device, so the type is
                        // not read on Windows. It used to be guessed from the
                        // advertised name, which is not the device's class.
                        device_type: None,
                        // The PnP node carries no connection or pairing state: its
                        // `Status` is node health, and it reads "OK" whether the
                        // peripheral is connected or not. This used to report every
                        // peripheral as `Paired` regardless of its real state. The
                        // state is `Unknown` until a property that says it is read.
                        state: BluetoothState::Unknown,
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
        self.adapters_read = true;
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
                    // An unreadable `address` used to default to an empty
                    // string, which is not an address; it is `None` now.
                    let address = fs::read_to_string(path.join("address"))
                        .ok()
                        .and_then(|s| normalise_bd_address(&s));

                    // Power state, where the kernel publishes one. An
                    // unreadable file defaulted to "1" -- powered on.
                    let powered =
                        fs::read_to_string(format!("/sys/class/bluetooth/{}/powered", name))
                            .ok()
                            .and_then(|s| match s.trim() {
                                "1" => Some(true),
                                "0" => Some(false),
                                _ => None,
                            });

                    // The kernel's sysfs entry names no product, so the name is
                    // `None`. It used to be the constructed label "Bluetooth
                    // Adapter (hciN)", which is the index dressed as a name.
                    self.adapters.push(BluetoothAdapter {
                        id: name.clone(),
                        name: None,
                        address,
                        powered,
                    });
                }
            }
        }

        // No adapter, no paired devices -- and asking `bluetoothctl` anyway
        // cost 30 s on a host with the binary but no `bluetoothd` (WSL2), the
        // helper's full timeout, on every snapshot.
        if self.adapters.is_empty() {
            self.record_note(
                "no Bluetooth adapter is registered with the kernel, so no device list was requested"
                    .to_string(),
            );
            return Ok(());
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
                        if let Err(e) = &info {
                            self.record_note(format!(
                                "connection state for {address} is unknown: {e}"
                            ));
                        }
                        let state =
                            connection_state_from_info(info.as_ref().ok().map(String::as_str));

                        // The type comes from the same `info` block: its class
                        // of device, or BlueZ's icon derived from it. Without
                        // that block there is nothing to read.
                        let device_type = info
                            .as_ref()
                            .ok()
                            .map(String::as_str)
                            .and_then(device_type_from_info);

                        self.devices.push(BluetoothDevice {
                            address: Some(address),
                            name: Some(name),
                            device_type,
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
        self.adapters_read = true;
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
                                    .and_then(normalise_bd_address);
                                // No chipset name is `None`, not a fallback label.
                                let name = ctrl
                                    .get("controller_chipset")
                                    .and_then(|v| v.as_str())
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_string);
                                // A missing or unrecognised state is `None`. It used to
                                // compare equal to "attrib_on" or not, so a missing
                                // field read as the radio being off.
                                let powered = match ctrl
                                    .get("controller_powerState")
                                    .and_then(|v| v.as_str())
                                {
                                    Some("attrib_on") => Some(true),
                                    Some("attrib_off") => Some(false),
                                    _ => None,
                                };

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
                                        .and_then(normalise_bd_address);
                                    self.devices.push(BluetoothDevice {
                                        address,
                                        name: Some(dev_name.clone()),
                                        // system_profiler's entry is not parsed for
                                        // its device class here, so the type is not
                                        // read. It used to be guessed from the name.
                                        device_type: None,
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
    /// Whether the adapter walk ran on this platform. `false` where no reader
    /// exists or the platform query returned nothing, in which case an empty
    /// adapter list is not a claim that there are no adapters.
    pub fn adapters_read(&self) -> bool {
        self.adapters_read
    }
    /// Whether an adapter is present, or `None` when the adapter walk did not
    /// run and so nothing can be said.
    pub fn is_available(&self) -> Option<bool> {
        self.adapters_read.then_some(!self.adapters.is_empty())
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
        if let Some(device) = self
            .devices
            .iter_mut()
            .find(|d| d.address.as_deref() == Some(address))
        {
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
        if let Some(device) = self
            .devices
            .iter_mut()
            .find(|d| d.address.as_deref() == Some(address))
        {
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
        // This set the struct's field and returned `Ok` -- reporting a radio
        // switched that nothing had touched. No platform power control is
        // implemented, so it says so.
        if self.adapters.iter().any(|a| a.id == adapter_id) {
            let _ = enabled;
            Err(crate::error::IronError::NotImplemented(
                "Bluetooth radio power control is not implemented on any platform".into(),
            ))
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

/// Connection state from the text of a `bluetoothctl info` call, or `Unknown`
/// when the call failed and there is no text to read.
#[cfg(any(target_os = "linux", test))]
fn connection_state_from_info(info: Option<&str>) -> BluetoothState {
    match info {
        Some(text) if text.contains("Connected: yes") => BluetoothState::Connected,
        Some(_) => BluetoothState::Paired,
        None => BluetoothState::Unknown,
    }
}

/// The device type from a `bluetoothctl info` block, or `None` when the block
/// names neither a class of device nor an icon.
///
/// The class of device (`Class: 0x...`) is the device's own declaration and is
/// preferred. BlueZ's `Icon:` is derived from the same class, or from the BLE
/// appearance for devices that have no class, so it is used where the class
/// does not decide the answer. A device whose class is read but is not one of
/// this enum's kinds is `Unknown`, not `None`: the class was read.
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn device_type_from_info(info: &str) -> Option<BluetoothDeviceType> {
    let field = |key: &str| {
        info.lines()
            .find_map(|line| line.trim().strip_prefix(key).map(str::trim))
    };
    let from_class = field("Class:")
        .and_then(parse_class_of_device)
        .map(device_type_from_class);
    let from_icon = field("Icon:").map(device_type_from_icon);
    // A known answer from either field wins over `Unknown` from the other.
    match (from_class, from_icon) {
        (Some(BluetoothDeviceType::Unknown) | None, Some(icon))
            if icon != BluetoothDeviceType::Unknown =>
        {
            Some(icon)
        }
        (class, _) => class.or(from_icon),
    }
}

/// Parse a `Class:` value such as `0x00240404` into the 24-bit class of device.
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_class_of_device(text: &str) -> Option<u32> {
    let hex = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))?;
    u32::from_str_radix(hex, 16)
        .ok()
        .filter(|class| *class <= 0xFF_FFFF)
}

/// Decode the major and minor device class bits of a class of device, as the
/// Bluetooth Assigned Numbers lay them out: minor bits 2-7, major bits 8-12.
///
/// The major classes the enum has a kind for are computer (1), phone (2),
/// audio/video (4) and peripheral (5). A peripheral's minor class carries
/// "keyboard" in bit 6 and "pointing device" in bit 7; with neither set its low
/// four bits name a joystick (1) or gamepad (2). Any other class decodes to
/// `Unknown`.
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn device_type_from_class(class: u32) -> BluetoothDeviceType {
    let major = (class >> 8) & 0x1F;
    let minor = (class >> 2) & 0x3F;
    match major {
        1 => BluetoothDeviceType::Computer,
        2 => BluetoothDeviceType::Phone,
        // Wearable headset device and headphones.
        4 if minor == 0x01 || minor == 0x06 => BluetoothDeviceType::Headset,
        // Loudspeaker, portable audio, car audio and HiFi audio device.
        4 if matches!(minor, 0x05 | 0x07 | 0x08 | 0x0A) => BluetoothDeviceType::Speaker,
        5 => match ((class >> 6) & 0x3, minor & 0x0F) {
            (0b01, _) => BluetoothDeviceType::Keyboard,
            (0b10, _) => BluetoothDeviceType::Mouse,
            (0b00, 0x01 | 0x02) => BluetoothDeviceType::GameController,
            _ => BluetoothDeviceType::Unknown,
        },
        _ => BluetoothDeviceType::Unknown,
    }
}

/// Map a BlueZ icon name to a device type. An icon BlueZ names that this enum
/// has no kind for is `Unknown`.
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn device_type_from_icon(icon: &str) -> BluetoothDeviceType {
    match icon {
        "computer" => BluetoothDeviceType::Computer,
        "phone" => BluetoothDeviceType::Phone,
        "audio-headset" | "audio-headphones" => BluetoothDeviceType::Headset,
        "audio-speakers" => BluetoothDeviceType::Speaker,
        "input-keyboard" => BluetoothDeviceType::Keyboard,
        "input-mouse" => BluetoothDeviceType::Mouse,
        "input-gaming" => BluetoothDeviceType::GameController,
        _ => BluetoothDeviceType::Unknown,
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

    /// Setting a field is not switching a radio. This returned `Ok` having
    /// changed only the struct, so a caller was told the radio was off.
    #[test]
    fn setting_adapter_power_does_not_claim_success_it_did_not_have() {
        let mut monitor = BluetoothMonitor {
            adapters: vec![BluetoothAdapter {
                id: "bt0".into(),
                name: Some("Test".into()),
                address: None,
                powered: Some(true),
            }],
            devices: Vec::new(),
            last_note: None,
            adapters_read: true,
        };
        assert!(monitor.set_adapter_power("bt0", false).is_err());
        assert_eq!(
            monitor.adapters[0].powered,
            Some(true),
            "nothing was switched"
        );
    }

    #[test]
    fn a_failed_info_probe_is_an_unknown_state_not_a_paired_one() {
        assert_eq!(
            connection_state_from_info(Some("Device AA:BB\n\tConnected: yes\n")),
            BluetoothState::Connected
        );
        assert_eq!(
            connection_state_from_info(Some("Device AA:BB\n\tConnected: no\n")),
            BluetoothState::Paired
        );
        assert_eq!(connection_state_from_info(None), BluetoothState::Unknown);
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
            address: Some("AA:BB:CC:DD:EE:FF".to_string()),
            name: Some("Test Device".to_string()),
            device_type: Some(BluetoothDeviceType::Headset),
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
            name: Some("Test Adapter".to_string()),
            address: Some("11:22:33:44:55:66".to_string()),
            powered: Some(true),
        };
        let json = serde_json::to_string(&adapter).unwrap();
        let deserialized: BluetoothAdapter = serde_json::from_str(&json).unwrap();
        assert_eq!(adapter.id, deserialized.id);
        assert_eq!(deserialized.powered, Some(true));
        assert_eq!(deserialized.address, adapter.address);
    }

    #[test]
    fn a_hardware_address_is_normalised_to_lowercase_colons() {
        assert_eq!(
            normalise_bd_address("84:9E:56:FA:D6:3E").as_deref(),
            Some("84:9e:56:fa:d6:3e")
        );
        assert_eq!(
            normalise_bd_address("84-9e-56-fa-d6-3e").as_deref(),
            Some("84:9e:56:fa:d6:3e")
        );
        assert_eq!(
            normalise_bd_address(" 849E56FAD63E\n").as_deref(),
            Some("84:9e:56:fa:d6:3e")
        );
    }

    #[test]
    fn a_non_address_is_withheld_rather_than_published() {
        // A PnP path, an empty sysfs read and a short MAC are not addresses.
        for raw in [
            r"USB\VID_0489&PID_E13A&MI_00\B&26E6BFF2&0&0000",
            "",
            "   ",
            "84:9e:56:fa:d6",
            "84:9e:56:fa:d6:3g",
            "849e56fad63",
        ] {
            assert_eq!(normalise_bd_address(raw), None, "{raw:?}");
        }
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

    /// The radio address is the adapter's own MAC: on the machine this was
    /// captured on, `DEVPKEY_Bluetooth_RadioAddress` was 145815598978622 and
    /// `Get-NetAdapter` reports the Bluetooth Network Connection as
    /// 84-9E-56-FA-D6-3E.
    #[test]
    fn a_radio_address_value_is_its_mac() {
        let value = serde_json::json!(145815598978622u64);
        assert_eq!(
            radio_address_from_value(Some(&value)).as_deref(),
            Some("84:9e:56:fa:d6:3e")
        );
    }

    /// A radio that reports no address, or a value wider than 48 bits, is
    /// withheld. Nothing is derived from the network adapter in its place.
    #[test]
    fn a_missing_or_oversized_radio_address_is_withheld() {
        assert_eq!(radio_address_from_value(None), None);
        assert_eq!(
            radio_address_from_value(Some(&serde_json::Value::Null)),
            None
        );
        let too_wide = serde_json::json!(1u64 << 48);
        assert_eq!(radio_address_from_value(Some(&too_wide)), None);
    }

    /// Class-of-device values as `bluetoothctl info` prints them. Each is the
    /// 24-bit value a device advertises for its own kind.
    #[test]
    fn the_class_of_device_decides_the_kind() {
        // Headset: major audio/video (4), minor wearable headset (1).
        assert_eq!(
            device_type_from_class(0x0024_0404),
            BluetoothDeviceType::Headset
        );
        // Loudspeaker: major audio/video, minor loudspeaker (5).
        assert_eq!(
            device_type_from_class(0x0024_0414),
            BluetoothDeviceType::Speaker
        );
        // Keyboard: major peripheral (5), keyboard bit (bit 6) set.
        assert_eq!(
            device_type_from_class(0x0000_2540),
            BluetoothDeviceType::Keyboard
        );
        // Mouse: major peripheral, pointing-device bit (bit 7) set.
        assert_eq!(
            device_type_from_class(0x0000_2580),
            BluetoothDeviceType::Mouse
        );
        // Gamepad: major peripheral, no indicator bit, minor gamepad (2).
        assert_eq!(
            device_type_from_class(0x0000_2508),
            BluetoothDeviceType::GameController
        );
        // Phone (major 2) and laptop (major 1).
        assert_eq!(
            device_type_from_class(0x0000_020C),
            BluetoothDeviceType::Phone
        );
        assert_eq!(
            device_type_from_class(0x0000_0104),
            BluetoothDeviceType::Computer
        );
        // Networking (major 3) is a real class this enum has no kind for.
        assert_eq!(
            device_type_from_class(0x0000_0300),
            BluetoothDeviceType::Unknown
        );
    }

    #[test]
    fn a_class_of_device_is_parsed_from_bluetoothctl_hex() {
        assert_eq!(parse_class_of_device("0x00240404"), Some(0x0024_0404));
        assert_eq!(parse_class_of_device("0X000104"), Some(0x0000_0104));
        assert_eq!(parse_class_of_device("240404"), None);
        assert_eq!(parse_class_of_device("0x01000000"), None);
        assert_eq!(parse_class_of_device("0xzz"), None);
    }

    #[test]
    fn bluez_icons_map_to_kinds_and_unnamed_icons_to_unknown() {
        assert_eq!(
            device_type_from_icon("audio-headset"),
            BluetoothDeviceType::Headset
        );
        assert_eq!(
            device_type_from_icon("input-keyboard"),
            BluetoothDeviceType::Keyboard
        );
        assert_eq!(
            device_type_from_icon("input-mouse"),
            BluetoothDeviceType::Mouse
        );
        assert_eq!(
            device_type_from_icon("input-gaming"),
            BluetoothDeviceType::GameController
        );
        assert_eq!(device_type_from_icon("phone"), BluetoothDeviceType::Phone);
        assert_eq!(
            device_type_from_icon("video-display"),
            BluetoothDeviceType::Unknown
        );
    }

    /// Realistic `bluetoothctl info` blocks. The name is never consulted: a
    /// headset whose name says nothing about audio is still a headset.
    #[test]
    fn the_type_comes_from_the_info_block_not_the_name() {
        let headset = "Device 00:1B:66:AA:BB:CC Bose QC35\n\
            \tName: Bose QC35\n\
            \tAlias: Bose QC35\n\
            \tClass: 0x00240404\n\
            \tIcon: audio-headset\n\
            \tPaired: yes\n\
            \tConnected: yes\n";
        assert_eq!(
            device_type_from_info(headset),
            Some(BluetoothDeviceType::Headset)
        );

        // A keyboard whose advertised name carries no hint.
        let keyboard = "Device 11:22:33:44:55:66 K380\n\
            \tName: K380\n\
            \tClass: 0x00002540\n\
            \tIcon: input-keyboard\n\
            \tConnected: no\n";
        assert_eq!(
            device_type_from_info(keyboard),
            Some(BluetoothDeviceType::Keyboard)
        );

        // No class line: the icon decides.
        let icon_only = "Device 22:33:44:55:66:77 Unnamed\n\
            \tIcon: input-gaming\n\
            \tConnected: no\n";
        assert_eq!(
            device_type_from_info(icon_only),
            Some(BluetoothDeviceType::GameController)
        );

        // A class that decodes to a kind this enum lacks, and an icon that
        // names the kind: the icon's answer is the known one.
        let disagreeing = "Device 33:44:55:66:77:88 Thing\n\
            \tClass: 0x00000300\n\
            \tIcon: phone\n";
        assert_eq!(
            device_type_from_info(disagreeing),
            Some(BluetoothDeviceType::Phone)
        );

        // Neither field: nothing was read, so nothing is claimed. A name that
        // says "headset" does not change that.
        let bare = "Device 44:55:66:77:88:99 Headset Pro\n\tConnected: no\n";
        assert_eq!(device_type_from_info(bare), None);
    }
}
