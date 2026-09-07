//! Input device monitoring — keyboards, mice, touchpads, game controllers, tablets.
//!
//! # Platform Support
//!
//! - **Linux**: Reads `/sys/class/input/`, `/proc/bus/input/devices`
//! - **Windows**: Uses WMI (`Win32_Keyboard`, `Win32_PointingDevice`)
//! - **macOS**: Uses `system_profiler SPUSBDataType` and IOKit inference
//!
//! # Examples
//!
//! ```no_run
//! use simonlib::input::InputMonitor;
//!
//! let monitor = InputMonitor::new().unwrap();
//! for device in monitor.devices() {
//!     println!("{}: {:?} ({})", device.name, device.device_type, device.interface);
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::error::SimonError;

/// Type of input device
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputDeviceType {
    /// Standard keyboard
    Keyboard,
    /// Mouse
    Mouse,
    /// Touchpad / trackpad
    Touchpad,
    /// Trackball
    Trackball,
    /// Trackpoint / pointing stick
    Trackpoint,
    /// Game controller / gamepad / joystick
    GameController,
    /// Drawing tablet / digitizer
    Tablet,
    /// Touchscreen
    Touchscreen,
    /// Stylus / pen input
    Stylus,
    /// Remote control / media keys
    Remote,
    /// Biometric reader (fingerprint, etc.)
    Biometric,
    /// Other or unrecognized input
    Other,
}

/// Connection interface for the input device
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputInterface {
    USB,
    Bluetooth,
    PS2,
    I2C,
    SPI,
    Serial,
    Internal,
    Wireless,
    Virtual,
    Unknown,
}

/// Information about a single input device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputDevice {
    /// Device name
    pub name: String,
    /// Device type (keyboard, mouse, etc.)
    pub device_type: InputDeviceType,
    /// Connection interface
    pub interface: InputInterface,
    /// Vendor name or ID
    pub vendor: String,
    /// Product name or ID
    pub product: String,
    /// Physical path (sysfs path on Linux, device instance on Windows)
    pub physical_path: String,
    /// Whether the device appears active / connected
    /// Whether the platform reports the device as connected and usable.
    ///
    /// `None` where it was not established. Linux reads it from the device's
    /// `Handlers` line and Windows from `Status`, both of which answer the
    /// question the entity asks. macOS set the literal `true` at every
    /// construction site -- including the Bluetooth branch, and the entity's own
    /// description names that exact case: "a Bluetooth keyboard out of range is
    /// still enumerated".
    pub is_active: Option<bool>,
    /// Device-specific capabilities (e.g., "buttons:5", "axes:2")
    pub capabilities: Vec<String>,
}

/// Monitor for input devices (keyboards, mice, game controllers, etc.)
pub struct InputMonitor {
    devices: Vec<InputDevice>,
}

/// Match a device instance id's enumerator prefix to a transport.
///
/// The enumerator is the segment before the first backslash: `USB`, `BTHENUM`,
/// `ACPI`, `HID`. **`HID` is deliberately absent from this table** — it names a
/// device class and says nothing about how the device is attached, which is the
/// whole reason [`interface_from_device_tree`] has to walk the parent chain.
#[cfg(target_os = "windows")]
fn classify_enumerator(id: &str) -> Option<InputInterface> {
    let upper = id.to_uppercase();
    match upper.split('\\').next().unwrap_or("") {
        "USB" | "USBSTOR" => Some(InputInterface::USB),
        // `BTHENUM` is classic Bluetooth, `BTHLEDEVICE` is Low Energy.
        "BTHENUM" | "BTHLEDEVICE" | "BTH" => Some(InputInterface::Bluetooth),
        "ACPI" => Some(InputInterface::PS2),
        "HIDI2C" => Some(InputInterface::I2C),
        "SWD" | "ROOT" => Some(InputInterface::Virtual),
        _ => None,
    }
}

/// The transport a device instance sits on, following the parent chain.
///
/// A `HID\...` instance id names a device *class*, not a transport: the same
/// prefix covers a USB mouse, a Bluetooth keyboard and an I2C touchpad. The
/// reader used to look only at the device's own id and answer `Unknown` for
/// every HID node, which was the honest answer to the question it was asking.
///
/// It is not the only question available. Windows records
/// `DEVPKEY_Device_Parent`, and the HID node's parent carries the transport in
/// its own enumerator prefix -- on this machine, the HID mouse
/// `HID\VID_046D&PID_C548&MI_01\...` has the parent
/// `USB\VID_046D&PID_C548&MI_01\...`. Walking up until an enumerator is
/// recognised turns a refusal to guess into a reading.
///
/// Still `Unknown` when the walk finds nothing it recognises, which is the same
/// answer as before and for the same reason: naming a transport this crate has
/// not established would be the guess the old comment declined to make.
#[cfg(target_os = "windows")]
fn interface_from_device_tree(instance_id: &str) -> InputInterface {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Device_IDW, CM_Get_Parent, CM_Locate_DevNodeW, CM_LOCATE_DEVNODE_NORMAL, CR_SUCCESS,
    };

    if let Some(direct) = classify_enumerator(instance_id) {
        return direct;
    }

    let wide: Vec<u16> = std::ffi::OsStr::new(instance_id)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut devinst = 0u32;
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    if unsafe {
        CM_Locate_DevNodeW(
            &mut devinst,
            PCWSTR(wide.as_ptr()),
            CM_LOCATE_DEVNODE_NORMAL,
        )
    } != CR_SUCCESS
    {
        return InputInterface::Unknown;
    }

    // Bounded: the device tree is a tree and terminates on its own, but a
    // corrupted one should not hang a monitoring process.
    let mut node = devinst;
    for _ in 0..8 {
        let mut parent = 0u32;
        // SAFETY: `parent` is a live local; the call writes one `u32`.
        if unsafe { CM_Get_Parent(&mut parent, node, 0) } != CR_SUCCESS {
            return InputInterface::Unknown;
        }

        let mut buf = [0u16; 512];
        // SAFETY: `buf` is the buffer the call fills, sized by its own length.
        if unsafe { CM_Get_Device_IDW(parent, &mut buf, 0) } != CR_SUCCESS {
            return InputInterface::Unknown;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let id = String::from_utf16_lossy(&buf[..len]);

        if let Some(found) = classify_enumerator(&id) {
            return found;
        }
        node = parent;
    }

    InputInterface::Unknown
}

/// The manufacturer WMI reports for an input device, if it named one.
///
/// Windows fills `Manufacturer` on many device nodes with the *driver package
/// provider* rather than the hardware vendor, and marks those by convention
/// with parentheses: "(Standard system devices)", "(Standard keyboards)",
/// "(Standard mouse types)". Those name who wrote the inbox driver, not who
/// built the device, and publishing one as a vendor would be the same class of
/// error as publishing "Unknown" -- a real-looking string standing in for an
/// absence. The resolver's own guard rejects the word "unknown"; it has no way
/// to know about this convention, so the reader applies it here.
///
/// Verified on this machine: `Win32_PointingDevice` reports "Microsoft" for one
/// device and "(Standard system devices)" for another, and `Win32_Keyboard`
/// reports an empty string for all three keyboards.
///
/// Where the column says nothing usable the *numeric* vendor id in the device's
/// instance path does, and the entity asks for "vendor name or numeric id,
/// whichever the platform gave" -- so the fallback below is the entity being
/// answered rather than a second-best guess. Before it, five of this machine's
/// six input devices published no vendor at all while `USB\VID_0557` sat in
/// their instance paths.
#[cfg(target_os = "windows")]
fn device_vendor(item: &serde_json::Value, instance_id: &str) -> String {
    let raw = item["Manufacturer"].as_str().unwrap_or("").trim();
    if !raw.is_empty() && !raw.starts_with('(') {
        return raw.to_string();
    }
    // "Vendor name **or numeric id**, whichever the platform gave" -- and the
    // platform gave one, in the instance path: `USB\VID_0557&PID_220E&MI_00`.
    // The Linux reader has always published `0x046d` from
    // `/proc/bus/input/devices`; this branch is what makes Windows answer the
    // same entity the same way instead of reporting an empty reader.
    match crate::usb::parse_vid_pid(instance_id).0 {
        Some(vid) => format!("0x{vid:04x}"),
        None => String::new(),
    }
}

/// The numeric product id in an input device's instance path.
///
/// This was `String::new()` for every Windows input device -- hardcoded, with
/// the ontology reporting "reader returned an empty string" as though the
/// platform had been asked. It had not been: `HID\VID_046D&PID_C548&MI_01&COL01`
/// carries the product id, the crate already has a parser for that shape in
/// `usb`, and Linux publishes the same field from the same kind of id.
///
/// Empty for a device whose path carries no `PID_` -- a PS/2 keyboard on
/// `ACPI\PNP0303`, a virtual device on `ROOT\` -- which the resolver then
/// reports absent, honestly this time.
#[cfg(target_os = "windows")]
fn device_product(instance_id: &str) -> String {
    match crate::usb::parse_vid_pid(instance_id).1 {
        Some(pid) => format!("0x{pid:04x}"),
        None => String::new(),
    }
}

impl InputMonitor {
    /// Create a new InputMonitor and enumerate all input devices.
    pub fn new() -> Result<Self, SimonError> {
        let mut monitor = Self {
            devices: Vec::new(),
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    /// Refresh the list of input devices from the system.
    /// Re-enumerate keyboards, pointing devices and other input hardware.
    ///
    /// Returns `Err` when the enumeration failed, and `Ok` with an empty list
    /// only when it succeeded and found nothing -- which the resolver publishes
    /// as `board.input.<none>`. See [`crate::core::command`].
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

    /// Get all detected input devices.
    pub fn devices(&self) -> &[InputDevice] {
        &self.devices
    }

    /// Get devices filtered by type.
    pub fn devices_by_type(&self, device_type: InputDeviceType) -> Vec<&InputDevice> {
        self.devices
            .iter()
            .filter(|d| d.device_type == device_type)
            .collect()
    }

    /// Get all keyboards.
    pub fn keyboards(&self) -> Vec<&InputDevice> {
        self.devices_by_type(InputDeviceType::Keyboard)
    }

    /// Get all mice and pointing devices.
    pub fn pointing_devices(&self) -> Vec<&InputDevice> {
        self.devices
            .iter()
            .filter(|d| {
                matches!(
                    d.device_type,
                    InputDeviceType::Mouse
                        | InputDeviceType::Touchpad
                        | InputDeviceType::Trackball
                        | InputDeviceType::Trackpoint
                )
            })
            .collect()
    }

    /// Get all game controllers.
    pub fn game_controllers(&self) -> Vec<&InputDevice> {
        self.devices_by_type(InputDeviceType::GameController)
    }

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) -> Result<(), SimonError> {
        // Parse /proc/bus/input/devices for comprehensive device info
        // A machine with no /proc/bus/input/devices has no input layer at
        // all, which is a real answer; an unreadable one is not.
        let content = match std::fs::read_to_string("/proc/bus/input/devices") {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(SimonError::System(format!(
                    "cannot read /proc/bus/input/devices: {e}"
                )))
            }
        };
        let mut name = String::new();
        let mut phys = String::new();
        let mut sysfs = String::new();
        let mut handlers = String::new();
        let mut bitmap_ev = String::new();
        let mut bitmap_key = String::new();
        let mut bitmap_rel = String::new();
        let mut bitmap_abs = String::new();
        let mut vendor_id = String::new();
        let mut product_id = String::new();

        let flush = |devices: &mut Vec<InputDevice>,
                     name: &str,
                     phys: &str,
                     sysfs: &str,
                     handlers: &str,
                     bitmap_ev: &str,
                     bitmap_key: &str,
                     bitmap_rel: &str,
                     bitmap_abs: &str,
                     vendor_id: &str,
                     product_id: &str| {
            if name.is_empty() {
                return;
            }
            let device_type = Self::classify_linux(
                name, handlers, bitmap_ev, bitmap_key, bitmap_rel, bitmap_abs,
            );
            let interface = Self::infer_interface_linux(phys, sysfs);
            let mut caps = Vec::new();
            if !bitmap_key.is_empty() && bitmap_key != "0" {
                caps.push("keys".into());
            }
            if !bitmap_rel.is_empty() && bitmap_rel != "0" {
                caps.push("relative-axes".into());
            }
            if !bitmap_abs.is_empty() && bitmap_abs != "0" {
                caps.push("absolute-axes".into());
            }
            if handlers.contains("js") {
                caps.push("joystick".into());
            }

            devices.push(InputDevice {
                name: name.to_string(),
                device_type,
                interface,
                // `format!("0x{}", ...)` on an id the `I:` line did not carry
                // produced the string `"0x"` -- a non-empty value naming
                // nothing, which the resolver would publish as a reading. An
                // absent id stays absent.
                vendor: if vendor_id.is_empty() {
                    String::new()
                } else {
                    format!("0x{vendor_id}")
                },
                product: if product_id.is_empty() {
                    String::new()
                } else {
                    format!("0x{product_id}")
                },
                physical_path: sysfs.to_string(),
                is_active: Some(handlers.contains("event")),
                capabilities: caps,
            });
        };

        for line in content.lines() {
            if line.is_empty() {
                flush(
                    &mut self.devices,
                    &name,
                    &phys,
                    &sysfs,
                    &handlers,
                    &bitmap_ev,
                    &bitmap_key,
                    &bitmap_rel,
                    &bitmap_abs,
                    &vendor_id,
                    &product_id,
                );
                name.clear();
                phys.clear();
                sysfs.clear();
                handlers.clear();
                bitmap_ev.clear();
                bitmap_key.clear();
                bitmap_rel.clear();
                bitmap_abs.clear();
                vendor_id.clear();
                product_id.clear();
            } else if let Some(rest) = line.strip_prefix("N: Name=\"") {
                name = rest.trim_end_matches('"').to_string();
            } else if let Some(rest) = line.strip_prefix("P: Phys=") {
                phys = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("S: Sysfs=") {
                sysfs = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("H: Handlers=") {
                handlers = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("I: Bus=") {
                // Format: Bus=XXXX Vendor=XXXX Product=XXXX Version=XXXX
                for part in rest.split_whitespace() {
                    if let Some(v) = part.strip_prefix("Vendor=") {
                        vendor_id = v.to_string();
                    } else if let Some(p) = part.strip_prefix("Product=") {
                        product_id = p.to_string();
                    }
                }
            } else if let Some(rest) = line.strip_prefix("B: EV=") {
                bitmap_ev = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("B: KEY=") {
                bitmap_key = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("B: REL=") {
                bitmap_rel = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("B: ABS=") {
                bitmap_abs = rest.to_string();
            }
        }
        // Flush last device
        flush(
            &mut self.devices,
            &name,
            &phys,
            &sysfs,
            &handlers,
            &bitmap_ev,
            &bitmap_key,
            &bitmap_rel,
            &bitmap_abs,
            &vendor_id,
            &product_id,
        );
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn classify_linux(
        name: &str,
        handlers: &str,
        _ev: &str,
        _key: &str,
        rel: &str,
        abs: &str,
    ) -> InputDeviceType {
        let lower = name.to_lowercase();
        if lower.contains("keyboard") || lower.contains("kbd") {
            return InputDeviceType::Keyboard;
        }
        if lower.contains("touchpad") || lower.contains("trackpad") || lower.contains("clickpad") {
            return InputDeviceType::Touchpad;
        }
        if lower.contains("touchscreen") || lower.contains("touch screen") {
            return InputDeviceType::Touchscreen;
        }
        if lower.contains("trackpoint") || lower.contains("pointing stick") {
            return InputDeviceType::Trackpoint;
        }
        if lower.contains("trackball") {
            return InputDeviceType::Trackball;
        }
        if lower.contains("tablet") || lower.contains("wacom") || lower.contains("digitizer") {
            return InputDeviceType::Tablet;
        }
        if lower.contains("stylus") || lower.contains("pen") {
            return InputDeviceType::Stylus;
        }
        if lower.contains("gamepad")
            || lower.contains("joystick")
            || lower.contains("controller")
            || lower.contains("xbox")
            || lower.contains("playstation")
            || lower.contains("dualshock")
            || lower.contains("dualsense")
            || lower.contains("nintendo")
            || handlers.contains("js")
        {
            return InputDeviceType::GameController;
        }
        if lower.contains("fingerprint") || lower.contains("biometric") {
            return InputDeviceType::Biometric;
        }
        if lower.contains("remote") || lower.contains("consumer control") || lower.contains("media")
        {
            return InputDeviceType::Remote;
        }
        if lower.contains("mouse") || (!rel.is_empty() && rel != "0") {
            return InputDeviceType::Mouse;
        }
        if !abs.is_empty() && abs != "0" {
            return InputDeviceType::Touchscreen;
        }
        InputDeviceType::Other
    }

    #[cfg(target_os = "linux")]
    fn infer_interface_linux(phys: &str, _sysfs: &str) -> InputInterface {
        let p = phys.to_lowercase();
        if p.contains("usb") {
            InputInterface::USB
        } else if p.contains("bluetooth") || p.contains("bt") {
            InputInterface::Bluetooth
        } else if p.contains("i8042") || p.contains("ps/2") || p.contains("isa") {
            InputInterface::PS2
        } else if p.contains("i2c") {
            InputInterface::I2C
        } else if p.contains("spi") {
            InputInterface::SPI
        } else if p.contains("serial") || p.contains("tty") {
            InputInterface::Serial
        } else if p.is_empty() || p.contains("virtual") || p.contains("input") {
            InputInterface::Virtual
        } else {
            InputInterface::Unknown
        }
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), SimonError> {
        // Keyboards via WMI
        let keyboards = crate::core::command::capture_json(
            "powershell",
            &["-NoProfile", "-Command", "Get-CimInstance Win32_Keyboard | Select-Object Name, Manufacturer, Description, DeviceID, Status, Layout | ConvertTo-Json -Compress"],
        )?;
        if let Some(val) = keyboards {
            let items = crate::core::command::json_items(&val);
            for item in &items {
                let name = item["Name"]
                    .as_str()
                    .unwrap_or("Unknown Keyboard")
                    .to_string();
                let device_id = item["DeviceID"].as_str().unwrap_or("").to_string();
                // A bare `HID\` instance names the device class, not the
                // transport -- it may be Bluetooth or I2C rather than USB. An
                // earlier note said exactly that while the code reported USB
                // anyway; then the code stopped guessing and answered `Unknown`
                // for every HID node. Neither was necessary: the parent chain
                // records the transport. See `interface_from_device_tree`.
                let iface = interface_from_device_tree(&device_id);
                self.devices.push(InputDevice {
                    name,
                    device_type: InputDeviceType::Keyboard,
                    interface: iface,
                    // Both queries now select `Manufacturer`, which they did
                    // not before -- the field was hardcoded empty and the
                    // ontology reported "reader returned an empty string" for
                    // it. On this machine `Win32_Keyboard.Manufacturer` really
                    // is blank on all three keyboards, so the numeric vendor id
                    // in the instance path answers instead; see
                    // `device_vendor`.
                    vendor: device_vendor(item, &device_id),
                    product: device_product(&device_id),
                    physical_path: device_id,
                    is_active: Some(item["Status"].as_str() == Some("OK")),
                    capabilities: vec!["keys".into()],
                });
            }
        }

        // Pointing devices via WMI
        let pointers = crate::core::command::capture_json(
            "powershell",
            &["-NoProfile", "-Command", "Get-CimInstance Win32_PointingDevice | Select-Object Name, Manufacturer, Description, DeviceID, Status, PointingType, NumberOfButtons | ConvertTo-Json -Compress"],
        )?;
        if let Some(val) = pointers {
            let items = crate::core::command::json_items(&val);
            for item in &items {
                let name = item["Name"]
                    .as_str()
                    .unwrap_or("Unknown Pointing Device")
                    .to_string();
                let lower = name.to_lowercase();
                let device_type = if lower.contains("touchpad") || lower.contains("trackpad") {
                    InputDeviceType::Touchpad
                } else if lower.contains("trackball") {
                    InputDeviceType::Trackball
                } else if lower.contains("touchscreen") || lower.contains("touch screen") {
                    InputDeviceType::Touchscreen
                } else if lower.contains("tablet") || lower.contains("wacom") {
                    InputDeviceType::Tablet
                } else {
                    InputDeviceType::Mouse
                };
                let device_id = item["DeviceID"].as_str().unwrap_or("").to_string();
                // See the keyboard branch above.
                let iface = interface_from_device_tree(&device_id);
                let buttons = item["NumberOfButtons"].as_u64().unwrap_or(0);
                let mut caps = vec!["relative-axes".into()];
                if buttons > 0 {
                    caps.push(format!("buttons:{}", buttons));
                }
                self.devices.push(InputDevice {
                    name,
                    device_type,
                    interface: iface,
                    vendor: device_vendor(item, &device_id),
                    product: device_product(&device_id),
                    physical_path: device_id,
                    is_active: Some(item["Status"].as_str() == Some("OK")),
                    capabilities: caps,
                });
            }
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), SimonError> {
        // Use system_profiler for HID devices
        let text = crate::core::command::capture(
            "system_profiler",
            &["SPUSBDataType", "-detailLevel", "mini"],
        )?;
        // Parse USB tree for HID/input devices
        let mut current_name = String::new();
        let mut current_vendor = String::new();
        let mut current_product = String::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.ends_with(':') && !trimmed.starts_with("USB") && !trimmed.is_empty() {
                // This is a device name
                if !current_name.is_empty() {
                    // Check if previous device was an input device
                    self.maybe_add_macos_device(&current_name, &current_vendor, &current_product);
                }
                current_name = trimmed.trim_end_matches(':').to_string();
                current_vendor.clear();
                current_product.clear();
            } else if let Some(v) = trimmed.strip_prefix("Vendor ID:") {
                current_vendor = v.trim().to_string();
            } else if let Some(p) = trimmed.strip_prefix("Product ID:") {
                current_product = p.trim().to_string();
            }
        }
        if !current_name.is_empty() {
            self.maybe_add_macos_device(&current_name, &current_vendor, &current_product);
        }

        // Also check for Bluetooth input devices
        let text = crate::core::command::capture(
            "system_profiler",
            &["SPBluetoothDataType", "-detailLevel", "mini"],
        )?;
        for line in text.lines() {
            let trimmed = line.trim();
            let lower = trimmed.to_lowercase();
            if (lower.contains("keyboard")
                || lower.contains("mouse")
                || lower.contains("trackpad")
                || lower.contains("magic"))
                && trimmed.ends_with(':')
            {
                let name = trimmed.trim_end_matches(':').to_string();
                let device_type = if lower.contains("keyboard") {
                    InputDeviceType::Keyboard
                } else if lower.contains("trackpad") {
                    InputDeviceType::Touchpad
                } else {
                    InputDeviceType::Mouse
                };
                self.devices.push(InputDevice {
                    name,
                    device_type,
                    interface: InputInterface::Bluetooth,
                    vendor: "Apple".into(),
                    product: String::new(),
                    physical_path: String::new(),
                    is_active: None,
                    capabilities: Vec::new(),
                });
            }
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn maybe_add_macos_device(&mut self, name: &str, vendor: &str, product: &str) {
        let lower = name.to_lowercase();
        let device_type = if lower.contains("keyboard") {
            InputDeviceType::Keyboard
        } else if lower.contains("mouse") {
            InputDeviceType::Mouse
        } else if lower.contains("trackpad") || lower.contains("touchpad") {
            InputDeviceType::Touchpad
        } else if lower.contains("gamepad")
            || lower.contains("controller")
            || lower.contains("joystick")
        {
            InputDeviceType::GameController
        } else if lower.contains("tablet") || lower.contains("wacom") || lower.contains("intuos") {
            InputDeviceType::Tablet
        } else if lower.contains("touchscreen") {
            InputDeviceType::Touchscreen
        } else {
            return; // Skip non-input USB devices
        };
        self.devices.push(InputDevice {
            name: name.to_string(),
            device_type,
            interface: InputInterface::USB,
            vendor: vendor.to_string(),
            product: product.to_string(),
            physical_path: String::new(),
            is_active: None,
            capabilities: Vec::new(),
        });
    }
}

impl Default for InputMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            devices: Vec::new(),
        })
    }
}

impl std::fmt::Display for InputDeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Keyboard => write!(f, "Keyboard"),
            Self::Mouse => write!(f, "Mouse"),
            Self::Touchpad => write!(f, "Touchpad"),
            Self::Trackball => write!(f, "Trackball"),
            Self::Trackpoint => write!(f, "Trackpoint"),
            Self::GameController => write!(f, "Game Controller"),
            Self::Tablet => write!(f, "Tablet"),
            Self::Touchscreen => write!(f, "Touchscreen"),
            Self::Stylus => write!(f, "Stylus"),
            Self::Remote => write!(f, "Remote"),
            Self::Biometric => write!(f, "Biometric"),
            Self::Other => write!(f, "Other"),
        }
    }
}

impl std::fmt::Display for InputInterface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::USB => write!(f, "USB"),
            Self::Bluetooth => write!(f, "Bluetooth"),
            Self::PS2 => write!(f, "PS/2"),
            Self::I2C => write!(f, "I2C"),
            Self::SPI => write!(f, "SPI"),
            Self::Serial => write!(f, "Serial"),
            Self::Internal => write!(f, "Internal"),
            Self::Wireless => write!(f, "Wireless"),
            Self::Virtual => write!(f, "Virtual"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod enumerator_tests {
    use super::{classify_enumerator, InputInterface};

    /// The reason the parent walk exists at all.
    #[test]
    fn hid_is_a_device_class_and_names_no_transport() {
        // A HID instance id is compatible with USB, Bluetooth and I2C alike,
        // so it must not resolve to any of them on its own. This is the real
        // id of this machine's mouse.
        assert_eq!(
            classify_enumerator(r"HID\VID_046D&PID_C548&MI_01&COL01\A&27230CC3&0&0000"),
            None
        );
    }

    #[test]
    fn an_enumerator_prefix_names_its_transport() {
        // The parent of that HID node, and the shapes of the other buses.
        assert_eq!(
            classify_enumerator(r"USB\VID_046D&PID_C548&MI_01\9&24c94812&0&0001"),
            Some(InputInterface::USB)
        );
        assert_eq!(
            classify_enumerator(r"BTHENUM\Dev_0000\7&2&0"),
            Some(InputInterface::Bluetooth)
        );
        assert_eq!(
            classify_enumerator(r"ACPI\PNP0303\4&1"),
            Some(InputInterface::PS2)
        );
        assert_eq!(
            classify_enumerator(r"ROOT\SYSTEM\0000"),
            Some(InputInterface::Virtual)
        );

        // Instance ids arrive in both cases.
        assert_eq!(
            classify_enumerator(r"usb\vid_046d"),
            Some(InputInterface::USB)
        );
    }

    #[test]
    fn an_unrecognised_enumerator_is_not_forced_into_a_transport() {
        // Absence, not a nearest match. Naming a transport this crate has not
        // established is the guess the reader exists to avoid.
        assert_eq!(classify_enumerator(r"PCI\VEN_1022&DEV_43F7"), None);
        assert_eq!(classify_enumerator(""), None);
        assert_eq!(classify_enumerator(r"SOMETHINGNEW\X"), None);
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_vendor_tests {
    use super::{device_product, device_vendor};
    use serde_json::json;

    /// A PS/2 keyboard's instance path: no `VID_`, no `PID_`, so there is
    /// nothing to fall back to and the absence stands.
    const NO_IDS: &str = r"ACPI\PNP0303\4&1";
    /// This machine's KVM keyboard, whose manufacturer Windows leaves blank.
    const ATEN_KEYBOARD: &str = r"USB\VID_0557&PID_220E&MI_00\B&2F3F2B&0&0000";

    /// Windows' parenthesised driver-provider strings are not vendors.
    #[test]
    fn a_driver_package_provider_is_not_a_device_vendor() {
        // Real values from `Win32_PointingDevice` on the development host.
        assert_eq!(
            device_vendor(&json!({"Manufacturer": "Microsoft"}), NO_IDS),
            "Microsoft"
        );
        assert_eq!(
            device_vendor(&json!({"Manufacturer": "(Standard system devices)"}), NO_IDS),
            ""
        );
        assert_eq!(
            device_vendor(&json!({"Manufacturer": "(Standard keyboards)"}), NO_IDS),
            ""
        );

        // An absent or blank column stays absent rather than becoming a name.
        assert_eq!(device_vendor(&json!({"Manufacturer": ""}), NO_IDS), "");
        assert_eq!(device_vendor(&json!({}), NO_IDS), "");

        // A real vendor that merely contains a bracket keeps it; only a
        // leading one marks the convention.
        assert_eq!(
            device_vendor(&json!({"Manufacturer": "Logitech (Suisse) SA"}), NO_IDS),
            "Logitech (Suisse) SA"
        );
    }

    /// The entity is "vendor name **or numeric id**, whichever the platform
    /// gave", and the instance path gives one where the column does not.
    #[test]
    fn a_blank_or_driver_provided_manufacturer_falls_back_to_the_numeric_id() {
        assert_eq!(
            device_vendor(&json!({"Manufacturer": ""}), ATEN_KEYBOARD),
            "0x0557"
        );
        assert_eq!(
            device_vendor(
                &json!({"Manufacturer": "(Standard system devices)"}),
                ATEN_KEYBOARD
            ),
            "0x0557"
        );
        // A real name still outranks the number.
        assert_eq!(
            device_vendor(&json!({"Manufacturer": "Microsoft"}), ATEN_KEYBOARD),
            "Microsoft"
        );
    }

    /// `product` was hardcoded empty on Windows while the id sat in the path.
    #[test]
    fn the_product_id_comes_from_the_instance_path() {
        assert_eq!(
            device_product(r"HID\VID_046D&PID_C548&MI_01&COL01\A&27230CC3&0&0000"),
            "0xc548"
        );
        assert_eq!(device_product(ATEN_KEYBOARD), "0x220e");
        assert_eq!(device_product(NO_IDS), "");
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
    fn test_input_monitor_creation() {
        match InputMonitor::new() {
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
    fn test_input_monitor_default() {
        let monitor = InputMonitor::default();
        let _ = monitor.devices();
    }

    #[test]
    fn test_input_device_type_display() {
        assert_eq!(InputDeviceType::Keyboard.to_string(), "Keyboard");
        assert_eq!(
            InputDeviceType::GameController.to_string(),
            "Game Controller"
        );
    }

    #[test]
    fn test_input_device_serialization() {
        let device = InputDevice {
            name: "Test Keyboard".into(),
            device_type: InputDeviceType::Keyboard,
            interface: InputInterface::USB,
            vendor: "TestVendor".into(),
            product: "TestProduct".into(),
            physical_path: "/sys/test".into(),
            is_active: Some(true),
            capabilities: vec!["keys".into()],
        };
        let json = serde_json::to_string(&device).unwrap();
        assert!(json.contains("Test Keyboard"));
        let _: InputDevice = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_filter_by_type() {
        let monitor = InputMonitor::default();
        let _keyboards = monitor.keyboards();
        let _pointing = monitor.pointing_devices();
        let _controllers = monitor.game_controllers();
    }
}
