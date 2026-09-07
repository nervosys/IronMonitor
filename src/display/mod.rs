//! Display/Monitor monitoring module
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayConnection {
    Hdmi,
    DisplayPort,
    Dvi,
    Vga,
    Internal,
    Edp,
    UsbC,
    Usb,
    Wireless,
    Virtual,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HdrMode {
    Off,
    Hdr10,
    Hdr10Plus,
    DolbyVision,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: String,
    pub name: Option<String>,
    pub manufacturer: Option<String>,
    pub connection: DisplayConnection,
    pub is_primary: bool,
    /// Horizontal pixels of the current mode, or `None` when the display is
    /// attached and its mode was not readable. Zero is not a resolution — the
    /// ontology already refused to publish one and said so in as many words,
    /// while `simon cli display` printed "RX-A740 0x0 @ 0Hz" and the agent tool
    /// surface published `"resolution": "0x0"`.
    pub width: Option<u32>,
    /// Vertical pixels of the current mode. See [`Self::width`].
    pub height: Option<u32>,
    /// Refresh rate in hertz, or `None` where it was not read. The Linux and
    /// macOS readers do not read it at all and used to write `0.0`, so every
    /// display on those platforms reported 0 Hz as though measured.
    pub refresh_rate: Option<f32>,
    pub brightness: Option<f32>,
    pub hdr: HdrMode,
    pub scale_factor: Option<f64>,
    pub physical_width_mm: Option<u32>,
    pub physical_height_mm: Option<u32>,
    pub bits_per_pixel: Option<u8>,
}

/// EDID metadata for one monitor, joined to the GDI enumeration by hardware id.
#[cfg(target_os = "windows")]
struct WindowsMonitorMeta {
    hardware_id: Option<String>,
    name: Option<String>,
    manufacturer: Option<String>,
    connection: DisplayConnection,
    physical_width_mm: Option<u32>,
    physical_height_mm: Option<u32>,
}

impl DisplayInfo {
    /// The reduced aspect ratio, or `None` when there is no mode to reduce.
    ///
    /// This returned the string "unknown" for a display whose mode was not read,
    /// which reached the agent tool surface as
    /// `"aspect_ratio": "unknown"` and the profile surface as
    /// `Aspect Ratio = unknown`. It is the word `push_str_as` has refused in
    /// readings since the DMI "n/a" finding, arriving by a different road: a
    /// return type with no way to say nothing.
    pub fn aspect_ratio(&self) -> Option<String> {
        let (width, height) = (self.width?, self.height?);
        if width == 0 || height == 0 {
            return None;
        }
        fn gcd(a: u32, b: u32) -> u32 {
            if b == 0 {
                a
            } else {
                gcd(b, a % b)
            }
        }
        let g = gcd(width, height);
        if g == 0 {
            return None;
        }
        Some(format!("{}:{}", width / g, height / g))
    }
}

pub struct DisplayMonitor {
    displays: Vec<DisplayInfo>,
}

impl DisplayMonitor {
    pub fn new() -> Result<Self, crate::error::SimonError> {
        let mut monitor = Self {
            displays: Vec::new(),
        };
        monitor.refresh()?;
        Ok(monitor)
    }
    pub fn refresh(&mut self) -> Result<(), crate::error::SimonError> {
        self.displays.clear();
        #[cfg(target_os = "windows")]
        self.refresh_windows()?;
        #[cfg(target_os = "linux")]
        self.refresh_linux()?;
        #[cfg(target_os = "macos")]
        self.refresh_macos()?;
        Ok(())
    }
    pub fn displays(&self) -> &[DisplayInfo] {
        &self.displays
    }
    pub fn primary(&self) -> Option<&DisplayInfo> {
        self.displays.iter().find(|d| d.is_primary)
    }
    pub fn count(&self) -> usize {
        self.displays.len()
    }

    /// Enumerate the displays attached to this desktop.
    ///
    /// This used to loop over `Win32_VideoController` — graphics *adapters* —
    /// and call each one a display, then paste monitor details onto them **by
    /// array index**:
    ///
    /// ```ignore
    /// foreach ($ctrl in $controllers) {
    ///     $mon = [PSCustomObject]@{ Name = $ctrl.Name; ... }
    ///     if ($monitorDetails -and $idx -lt @($monitorDetails).Count) {
    ///         $mdet = @($monitorDetails)[$idx]      # positional, not a join
    /// ```
    ///
    /// On the development machine that reported three displays where one
    /// exists, two of them named `AMD Radeon(TM) Graphics` and
    /// `NVIDIA GeForce RTX 3090 Ti`, and the one correct name was correct by
    /// coincidence of ordering. Name, brightness and connection type were all
    /// attributed by the same positional guess.
    ///
    /// `EnumDisplayDevices` answers the actual question. It enumerates display
    /// devices, flags the ones `ATTACHED_TO_DESKTOP`, and gives each one's
    /// attached monitor; `EnumDisplaySettings` gives that device's current
    /// mode. The same machine enumerates thirteen devices with exactly one
    /// attached, at 3440x1440.
    ///
    /// The monitor's friendly name, connection type and physical size still
    /// come from `root\wmi`, joined on the **hardware id** — `GSM76F6` in both
    /// `MONITOR\GSM76F6\{guid}\0001` and
    /// `DISPLAY\GSM76F6\5&2a745970&0&UID4352_0`. Two monitors of the same model
    /// share that id, and they also share their EDID name, connection family
    /// and panel size, so the ambiguity does not reach any of the three values
    /// taken from it. Per-instance state would need the UID, which is why
    /// brightness is not taken from it.
    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), crate::error::SimonError> {
        use windows::core::PCWSTR;
        use windows::Win32::Graphics::Gdi::{
            EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW,
            ENUM_CURRENT_SETTINGS,
        };

        /// `DISPLAY_DEVICE_ATTACHED_TO_DESKTOP`
        const ATTACHED_TO_DESKTOP: u32 = 0x0000_0001;
        /// `DISPLAY_DEVICE_PRIMARY_DEVICE`
        const PRIMARY_DEVICE: u32 = 0x0000_0004;

        fn wide_to_string(buf: &[u16]) -> String {
            let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
            String::from_utf16_lossy(&buf[..end])
        }

        /// The hardware id segment of a device path, used to join the GDI
        /// enumeration to the WMI monitor tables.
        fn hardware_id(path: &str) -> Option<String> {
            path.split('\\').nth(1).map(str::to_ascii_uppercase)
        }

        let wmi = Self::windows_monitor_metadata();

        let mut idx = 0u32;
        loop {
            let mut adapter = DISPLAY_DEVICEW {
                cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                ..Default::default()
            };
            if !unsafe { EnumDisplayDevicesW(None, idx, &mut adapter, 0) }.as_bool() {
                break;
            }
            idx += 1;

            // A device that is not attached to the desktop is not a display.
            // Thirteen of the fourteen on the development machine are outputs
            // the driver exposes and nothing is plugged into.
            if adapter.StateFlags & ATTACHED_TO_DESKTOP == 0 {
                continue;
            }

            let device_name: Vec<u16> = adapter.DeviceName.to_vec();
            let mut mode = DEVMODEW {
                dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                ..Default::default()
            };
            let has_mode = unsafe {
                EnumDisplaySettingsW(
                    PCWSTR(device_name.as_ptr()),
                    ENUM_CURRENT_SETTINGS,
                    &mut mode,
                )
            }
            .as_bool();

            // The monitor on this output, for its hardware id.
            let mut monitor = DISPLAY_DEVICEW {
                cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                ..Default::default()
            };
            let has_monitor =
                unsafe { EnumDisplayDevicesW(PCWSTR(device_name.as_ptr()), 0, &mut monitor, 0) }
                    .as_bool();
            let monitor_id = has_monitor
                .then(|| wide_to_string(&monitor.DeviceID))
                .and_then(|id| hardware_id(&id));

            // Where several WMI nodes carry the same hardware id, prefer one
            // that states a connection.
            //
            // The join used to take the first match and the doc comment above
            // argued that the ambiguity could not reach any value taken from
            // it, because two panels of a model share their EDID name,
            // connection family and size. On this machine it reached one: the
            // single LG enumerates twice in `root\wmi`, once per adapter path,
            // and the first node reports `D3DKMDT_VOT_UNINITIALIZED` while the
            // second reports HDMI. The display's connection was published as an
            // absence with a stated value sitting in the row beside it.
            //
            // This is still a tie-break within one hardware id, not a guess
            // across panels: every candidate describes a monitor of this model,
            // and the choice is only between one that answered and one that did
            // not.
            let meta = monitor_id.as_ref().and_then(|id| {
                let matching =
                    || wmi.iter().filter(|m| m.hardware_id.as_deref() == Some(id.as_str()));
                matching()
                    .find(|m| m.connection != DisplayConnection::Unknown)
                    .or_else(|| matching().next())
            });

            // `DeviceString` for the monitor is the driver's name for it --
            // "Generic PnP Monitor" -- so the EDID name from WMI is preferred
            // and the driver name is not used as a stand-in for it.
            let name = meta.and_then(|m| m.name.clone());

            self.displays.push(DisplayInfo {
                id: wide_to_string(&adapter.DeviceName),
                name,
                manufacturer: meta.and_then(|m| m.manufacturer.clone()),
                connection: meta
                    .map(|m| m.connection)
                    .unwrap_or(DisplayConnection::Unknown),
                is_primary: adapter.StateFlags & PRIMARY_DEVICE != 0,
                // Zero is not a resolution, and `EnumDisplaySettings` fills the
                // struct with zeroes when it fails.
                width: has_mode.then_some(mode.dmPelsWidth).filter(|v| *v > 0),
                height: has_mode.then_some(mode.dmPelsHeight).filter(|v| *v > 0),
                refresh_rate: has_mode
                    .then_some(mode.dmDisplayFrequency)
                    .filter(|v| *v > 1)
                    .map(|v| v as f32),
                // Not read. `WmiMonitorBrightness` is keyed by the monitor's
                // full instance name, and the hardware id this reader joins on
                // does not distinguish two panels of the same model.
                brightness: None,
                hdr: HdrMode::Off,
                scale_factor: None,
                physical_width_mm: meta.and_then(|m| m.physical_width_mm),
                physical_height_mm: meta.and_then(|m| m.physical_height_mm),
                bits_per_pixel: has_mode
                    .then_some(mode.dmBitsPerPel)
                    .filter(|v| *v > 0)
                    .map(|v| v as u8),
            });
        }

        // No synthetic fallback: if nothing is attached to the desktop, no
        // display was detected and the list stays empty.
        //
        // This previously invented a "Primary Display" with zero dimensions,
        // `is_primary: true` and `hdr: Off` -- definite claims about hardware
        // nothing had observed. `count()` then reported 1 display on a machine
        // where detection had failed entirely, which is worse than reporting
        // none.
        Ok(())
    }
    /// EDID-derived monitor metadata, keyed by hardware id.
    ///
    /// `WmiMonitorID`, `WmiMonitorConnectionParams` and
    /// `WmiMonitorBasicDisplayParams` all carry `InstanceName`, so they join to
    /// each other exactly. All three are readable without elevation, and a
    /// machine that publishes none of them simply has no metadata to attach.
    #[cfg(target_os = "windows")]
    fn windows_monitor_metadata() -> Vec<WindowsMonitorMeta> {
        const QUERY: &str = concat!(
            "$id = @(Get-CimInstance -Namespace root/wmi -ClassName WmiMonitorID ",
            "-ErrorAction SilentlyContinue); ",
            "$conn = @(Get-CimInstance -Namespace root/wmi -ClassName ",
            "WmiMonitorConnectionParams -ErrorAction SilentlyContinue); ",
            "$dim = @(Get-CimInstance -Namespace root/wmi -ClassName ",
            "WmiMonitorBasicDisplayParams -ErrorAction SilentlyContinue); ",
            "$id | ForEach-Object { $i = $_; [PSCustomObject]@{ ",
            "InstanceName = $i.InstanceName; ",
            "Name = (($i.UserFriendlyName | Where-Object { $_ -ne 0 } | ",
            "ForEach-Object { [char]$_ }) -join ''); ",
            "Manufacturer = (($i.ManufacturerName | Where-Object { $_ -ne 0 } | ",
            "ForEach-Object { [char]$_ }) -join ''); ",
            "Technology = ($conn | Where-Object { $_.InstanceName -eq $i.InstanceName } | ",
            "Select-Object -First 1 -ExpandProperty VideoOutputTechnology); ",
            "WidthCm = ($dim | Where-Object { $_.InstanceName -eq $i.InstanceName } | ",
            "Select-Object -First 1 -ExpandProperty MaxHorizontalImageSize); ",
            "HeightCm = ($dim | Where-Object { $_.InstanceName -eq $i.InstanceName } | ",
            "Select-Object -First 1 -ExpandProperty MaxVerticalImageSize) } } | ",
            "ConvertTo-Json -Compress"
        );

        let Ok(Some(value)) =
            crate::core::command::capture_json("powershell", &["-NoProfile", "-Command", QUERY])
        else {
            return Vec::new();
        };

        crate::core::command::json_items(&value)
            .iter()
            .map(|item| {
                let text = |k: &str| {
                    item[k]
                        .as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                };
                // EDID reports the panel size in whole centimetres; the field
                // is millimetres, and a zero is EDID's "not stated".
                let mm = |k: &str| item[k].as_u64().filter(|v| *v > 0).map(|v| (v * 10) as u32);
                WindowsMonitorMeta {
                    hardware_id: item["InstanceName"]
                        .as_str()
                        .and_then(|s| s.split('\\').nth(1))
                        .map(str::to_ascii_uppercase),
                    name: text("Name"),
                    manufacturer: text("Manufacturer"),
                    // `VideoOutputTechnology`, as defined by
                    // `D3DKMDT_VIDEO_OUTPUT_TECHNOLOGY`.
                    connection: match item["Technology"].as_i64() {
                        Some(0) => DisplayConnection::Vga,
                        Some(4) => DisplayConnection::Dvi,
                        Some(5) => DisplayConnection::Hdmi,
                        Some(9) | Some(10) => DisplayConnection::DisplayPort,
                        Some(11) => DisplayConnection::Internal,
                        Some(6) | Some(14) => DisplayConnection::Edp,
                        // `D3DKMDT_VOT_UNINITIALIZED` is `0xFFFFFFFF`, and
                        // arrives from WMI as an unsigned 4294967295 rather
                        // than as -1. It is the driver saying the field was
                        // never filled in, which is an absence and not a
                        // connection type -- and this machine's LG has one WMI
                        // node saying that and another saying HDMI. See the
                        // join in `refresh_windows`.
                        Some(4_294_967_295) | Some(-1) => DisplayConnection::Unknown,
                        _ => DisplayConnection::Unknown,
                    },
                    physical_width_mm: mm("WidthCm"),
                    physical_height_mm: mm("HeightCm"),
                }
            })
            .collect()
    }

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) -> Result<(), crate::error::SimonError> {
        use std::fs;
        use std::process::Command;

        // Try DRM/sysfs first (works without X11). No /sys/class/drm is a
        // reading -- this kernel exposes no DRM -- and a directory that will
        // not open is not.
        let drm_path = std::path::Path::new("/sys/class/drm");
        if drm_path.exists() {
            {
                let entries = fs::read_dir(drm_path).map_err(|e| {
                    crate::error::SimonError::System(format!("cannot read /sys/class/drm: {e}"))
                })?;
                let mut idx = 0u32;
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    // Look for card*-* entries (e.g. card0-HDMI-A-1, card0-eDP-1)
                    if !name.starts_with("card") || !name.contains('-') {
                        continue;
                    }
                    let path = entry.path();

                    // Check if connected
                    let status = fs::read_to_string(path.join("status")).unwrap_or_default();
                    if status.trim() != "connected" {
                        continue;
                    }

                    // Parse connection type from name
                    let connector = name.split('-').skip(1).collect::<Vec<_>>().join("-");
                    let connection = if connector.starts_with("HDMI") {
                        DisplayConnection::Hdmi
                    } else if connector.starts_with("DP") || connector.starts_with("DisplayPort") {
                        DisplayConnection::DisplayPort
                    } else if connector.starts_with("DVI") {
                        DisplayConnection::Dvi
                    } else if connector.starts_with("VGA") {
                        DisplayConnection::Vga
                    } else if connector.starts_with("eDP") {
                        DisplayConnection::Edp
                    } else {
                        DisplayConnection::Unknown
                    };

                    // Read EDID for monitor name/manufacturer
                    let (mon_name, manufacturer, phys_w, phys_h) =
                        if let Ok(edid) = fs::read(path.join("edid")) {
                            parse_edid_basic(&edid)
                        } else {
                            (None, None, None, None)
                        };

                    // Try to get current mode from "modes" file. A connector
                    // with no modes file, an empty one, or a line that does not
                    // parse yields no mode — not a mode of zero by zero. Every
                    // branch here used to produce `(0, 0)`.
                    let (width, height) = fs::read_to_string(path.join("modes"))
                        .ok()
                        .and_then(|modes| {
                            let first_mode = modes.lines().next()?;
                            let (w, h) = first_mode.split_once('x')?;
                            let w = w.trim().parse::<u32>().ok()?;
                            let h = h.trim().parse::<u32>().ok()?;
                            (w > 0 && h > 0).then_some((w, h))
                        })
                        .map_or((None, None), |(w, h)| (Some(w), Some(h)));

                    self.displays.push(DisplayInfo {
                        id: format!("display{}", idx),
                        name: mon_name.or(Some(connector.clone())),
                        manufacturer,
                        connection,
                        is_primary: idx == 0,
                        width,
                        height,
                        // The DRM modes file does not always carry a rate, and this
                        // reader never parsed one. None, not zero hertz.
                        refresh_rate: None,
                        brightness: read_backlight_brightness(),
                        hdr: HdrMode::Off,
                        scale_factor: None,
                        physical_width_mm: phys_w,
                        physical_height_mm: phys_h,
                        bits_per_pixel: None,
                    });
                    idx += 1;
                }
            }
        }

        // Fallback: try xrandr
        if self.displays.is_empty() {
            if let Ok(output) = Command::new("xrandr").args(["--current"]).output() {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let mut idx = 0u32;
                    for line in stdout.lines() {
                        if line.contains(" connected") {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            let conn_name = parts.first().unwrap_or(&"Unknown");
                            let is_primary = line.contains("primary");

                            let connection = if conn_name.starts_with("HDMI") {
                                DisplayConnection::Hdmi
                            } else if conn_name.starts_with("DP") {
                                DisplayConnection::DisplayPort
                            } else if conn_name.starts_with("DVI") {
                                DisplayConnection::Dvi
                            } else if conn_name.starts_with("VGA") {
                                DisplayConnection::Vga
                            } else if conn_name.starts_with("eDP") {
                                DisplayConnection::Edp
                            } else {
                                DisplayConnection::Unknown
                            };

                            // Parse resolution from something like "1920x1080+0+0"
                            let (w, h) = parts
                                .iter()
                                .find(|p| p.contains('x') && p.contains('+'))
                                .and_then(|mode| {
                                    let res_part = mode.split('+').next()?;
                                    let dims: Vec<&str> = res_part.split('x').collect();
                                    if dims.len() == 2 {
                                        // A mode that does not parse is not a
                                        // mode of zero by zero.
                                        // A mode that does not parse is not a
                                        // mode of zero by zero.
                                        let (w, h) = (
                                            dims[0].parse::<u32>().ok()?,
                                            dims[1].parse::<u32>().ok()?,
                                        );
                                        (w > 0 && h > 0).then_some((w, h))
                                    } else {
                                        None
                                    }
                                })
                                .map_or((None, None), |(w, h)| (Some(w), Some(h)));

                            // Parse physical dimensions from "520mm x 290mm"
                            let (phys_w, phys_h) = if let Some(mm_idx) = line.find("mm x ") {
                                let before = &line[..mm_idx];
                                let pw = before
                                    .rsplit_once(' ')
                                    .and_then(|(_, n)| n.parse::<u32>().ok());
                                let after = &line[mm_idx + 5..];
                                let ph = after
                                    .split("mm")
                                    .next()
                                    .and_then(|n| n.trim().parse::<u32>().ok());
                                (pw, ph)
                            } else {
                                (None, None)
                            };

                            self.displays.push(DisplayInfo {
                                id: format!("display{}", idx),
                                name: Some(conn_name.to_string()),
                                manufacturer: None,
                                connection,
                                is_primary,
                                width: w,
                                height: h,
                                // xrandr's rate is not parsed here.
                                refresh_rate: None,
                                brightness: read_backlight_brightness(),
                                hdr: HdrMode::Off,
                                scale_factor: None,
                                physical_width_mm: phys_w,
                                physical_height_mm: phys_h,
                                bits_per_pixel: None,
                            });
                            idx += 1;
                        }
                    }
                }
            }
        }

        // No synthetic fallback — see the Windows collector. DRM and xrandr have both
        // been tried at this point; an empty list means no display was detected.
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), crate::error::SimonError> {
        // `system_profiler` ships with macOS, so a failure to run it is a
        // failure rather than an absent optional tool.
        let stdout =
            crate::core::command::capture("system_profiler", &["SPDisplaysDataType", "-json"])?;
        {
            {
                let stdout = stdout.as_str();
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
                    if let Some(displays_data) =
                        json.get("SPDisplaysDataType").and_then(|v| v.as_array())
                    {
                        let mut idx = 0u32;
                        for gpu in displays_data {
                            if let Some(ndrvs) =
                                gpu.get("spdisplays_ndrvs").and_then(|v| v.as_array())
                            {
                                for display in ndrvs {
                                    let name = display
                                        .get("_name")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                    let res = display
                                        .get("_spdisplays_resolution")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    // `_spdisplays_resolution` is absent on
                                    // some displays and formatted differently
                                    // on others; neither case is a mode of zero
                                    // by zero.
                                    let (w, h) = res
                                        .find(" x ")
                                        .and_then(|x_idx| {
                                            let w = res[..x_idx]
                                                .trim()
                                                .replace(" ", "")
                                                .parse::<u32>()
                                                .ok()?;
                                            let h: String = res[x_idx + 3..]
                                                .chars()
                                                .take_while(|c| c.is_ascii_digit())
                                                .collect();
                                            let h = h.parse::<u32>().ok()?;
                                            (w > 0 && h > 0).then_some((w, h))
                                        })
                                        .map_or((None, None), |(w, h)| (Some(w), Some(h)));

                                    let connection = if display
                                        .get("spdisplays_connection_type")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .contains("Internal")
                                    {
                                        DisplayConnection::Internal
                                    } else {
                                        DisplayConnection::Unknown
                                    };

                                    self.displays.push(DisplayInfo {
                                        id: format!("display{}", idx),
                                        name,
                                        manufacturer: display
                                            .get("_spdisplays_display-vendor-id")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string()),
                                        connection,
                                        is_primary: display
                                            .get("spdisplays_main")
                                            .and_then(|v| v.as_str())
                                            == Some("spdisplays_yes"),
                                        width: w,
                                        height: h,
                                        // system_profiler's rate is not parsed here.
                                        refresh_rate: None,
                                        brightness: None,
                                        hdr: HdrMode::Off,
                                        scale_factor: None,
                                        physical_width_mm: None,
                                        physical_height_mm: None,
                                        bits_per_pixel: display
                                            .get("spdisplays_pixelresolution")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| {
                                                if s.contains("32") {
                                                    Some(32u8)
                                                } else if s.contains("30") {
                                                    Some(30)
                                                } else {
                                                    None
                                                }
                                            }),
                                    });
                                    idx += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        // No synthetic fallback — see the Windows collector. This one additionally
        // asserted `DisplayConnection::Internal`, claiming a built-in panel on
        // hardware that may well be a Mac mini or Mac Pro.
        Ok(())
    }
}

/// Parse basic EDID data for monitor name and manufacturer
#[cfg(target_os = "linux")]
fn parse_edid_basic(edid: &[u8]) -> (Option<String>, Option<String>, Option<u32>, Option<u32>) {
    if edid.len() < 128 {
        return (None, None, None, None);
    }

    // Manufacturer ID from bytes 8-9 (3 letters encoded)
    let mfr = if edid.len() > 9 {
        let mfr_code = ((edid[8] as u16) << 8) | edid[9] as u16;
        let c1 = ((mfr_code >> 10) & 0x1F) as u8 + b'A' - 1;
        let c2 = ((mfr_code >> 5) & 0x1F) as u8 + b'A' - 1;
        let c3 = (mfr_code & 0x1F) as u8 + b'A' - 1;
        if c1.is_ascii_alphabetic() && c2.is_ascii_alphabetic() && c3.is_ascii_alphabetic() {
            Some(format!("{}{}{}", c1 as char, c2 as char, c3 as char))
        } else {
            None
        }
    } else {
        None
    };

    // Physical dimensions from bytes 21-22 (cm)
    let phys_w = if edid[21] > 0 {
        Some(edid[21] as u32 * 10)
    } else {
        None
    };
    let phys_h = if edid[22] > 0 {
        Some(edid[22] as u32 * 10)
    } else {
        None
    };

    // Monitor name from descriptor blocks (bytes 54-125, 18 bytes each)
    let mut name = None;
    for block_start in (54..=108).step_by(18) {
        if block_start + 17 < edid.len()
            && edid[block_start] == 0
            && edid[block_start + 1] == 0
            && edid[block_start + 3] == 0xFC
        {
            // Monitor name descriptor
            let name_bytes: Vec<u8> = edid[block_start + 5..block_start + 18]
                .iter()
                .copied()
                .take_while(|&b| b != 0x0A && b != 0x00)
                .collect();
            if let Ok(n) = String::from_utf8(name_bytes) {
                let trimmed = n.trim().to_string();
                if !trimmed.is_empty() {
                    name = Some(trimmed);
                }
            }
            break;
        }
    }

    (name, mfr, phys_w, phys_h)
}

/// Read backlight brightness from sysfs (0.0 - 1.0)
#[cfg(target_os = "linux")]
fn read_backlight_brightness() -> Option<f32> {
    use std::fs;
    let bl_path = std::path::Path::new("/sys/class/backlight");
    if let Ok(entries) = fs::read_dir(bl_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let brightness = fs::read_to_string(path.join("brightness"))
                .ok()
                .and_then(|s| s.trim().parse::<f32>().ok())?;
            let max = fs::read_to_string(path.join("max_brightness"))
                .ok()
                .and_then(|s| s.trim().parse::<f32>().ok())?;
            if max > 0.0 {
                return Some(brightness / max);
            }
        }
    }
    None
}

impl Default for DisplayMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            displays: Vec::new(),
        })
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
    fn test_display_monitor_creation() {
        match DisplayMonitor::new() {
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

    /// Whatever displays are reported must be real ones.
    ///
    /// This previously asserted `count() >= 1`, which only held because every
    /// platform collector pushed a synthetic "Primary Display" when detection found
    /// nothing — so the test was codifying the fabrication rather than checking
    /// anything. A headless machine legitimately has zero displays.
    #[test]
    fn test_display_monitor_count() {
        let monitor = DisplayMonitor::new().unwrap();
        assert_eq!(
            monitor.count(),
            monitor.displays().len(),
            "count() must agree with the reported list"
        );
        for display in monitor.displays() {
            assert!(!display.id.is_empty(), "a reported display has no id");
        }
    }

    #[test]
    fn test_display_aspect_ratio() {
        let display = DisplayInfo {
            id: "test".to_string(),
            name: Some("Test".to_string()),
            manufacturer: None,
            connection: DisplayConnection::Hdmi,
            is_primary: true,
            width: Some(1920),
            height: Some(1080),
            refresh_rate: Some(60.0),
            brightness: None,
            hdr: HdrMode::Off,
            scale_factor: Some(1.0),
            physical_width_mm: None,
            physical_height_mm: None,
            bits_per_pixel: Some(32),
        };
        assert_eq!(display.aspect_ratio().as_deref(), Some("16:9"));
    }

    #[test]
    fn test_display_4k_aspect_ratio() {
        let display = DisplayInfo {
            id: "test".to_string(),
            name: None,
            manufacturer: None,
            connection: DisplayConnection::DisplayPort,
            is_primary: false,
            width: Some(3840),
            height: Some(2160),
            refresh_rate: Some(144.0),
            brightness: Some(0.8),
            hdr: HdrMode::Hdr10,
            scale_factor: Some(1.5),
            physical_width_mm: Some(600),
            physical_height_mm: Some(340),
            bits_per_pixel: Some(30),
        };
        assert_eq!(display.aspect_ratio().as_deref(), Some("16:9"));
    }

    #[test]
    fn test_display_info_serialization() {
        let display = DisplayInfo {
            id: "test".to_string(),
            name: Some("Test Display".to_string()),
            manufacturer: Some("Acme".to_string()),
            connection: DisplayConnection::Hdmi,
            is_primary: true,
            width: Some(1920),
            height: Some(1080),
            refresh_rate: Some(60.0),
            brightness: Some(0.5),
            hdr: HdrMode::Off,
            scale_factor: Some(1.0),
            physical_width_mm: Some(530),
            physical_height_mm: Some(300),
            bits_per_pixel: Some(32),
        };
        let json = serde_json::to_string(&display).unwrap();
        let deserialized: DisplayInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(display.id, deserialized.id);
        assert_eq!(display.width, deserialized.width);
    }
}
