//! Audio device monitoring module
//!
//! Provides cross-platform audio device enumeration.
//! - Windows: `Win32_SoundDevice` via PowerShell/CIM
//! - Linux: ALSA via /proc/asound
//! - macOS: `system_profiler SPAudioDataType`
//!
//! Note that `system_profiler` reports which device is *default* for playback and
//! capture, not what each device is capable of, so macOS device types are inferred
//! rather than read directly.

use crate::error::SimonError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioDeviceType {
    Output,
    Input,
    Duplex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioState {
    Active,
    Idle,
    Suspended,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub device_type: AudioDeviceType,
    /// Endpoint state, where the platform reports one.
    ///
    /// `None` where it was not established, which was not previously
    /// expressible. Linux and macOS set the literal `AudioState::Active` at
    /// every construction site. Windows really does read something -- but it
    /// reads `Win32_SoundDevice.Status`, which is Device-Manager health, while
    /// this entity is described as "endpoint state as the platform reports
    /// it: active, disabled, unplugged, not present". Those are different
    /// properties, and the match had a `_ => Active` arm that turned the eight
    /// WMI status values it did not handle -- `Unknown` and `Pred Fail` among
    /// them -- into the healthiest one.
    ///
    /// The real endpoint state is a `DeviceState` DWORD under
    /// `HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio`
    /// (1 active, 2 disabled, 4 not present, 8 unplugged), readable without
    /// elevation; this reader does not go through that enumeration.
    pub state: Option<AudioState>,
    /// Whether this is the endpoint the system routes to by default.
    ///
    /// `None` where it was not established. On Windows this was "whichever row
    /// came first", which named an audio *controller* as the default output.
    pub is_default: Option<bool>,
    pub is_output: bool,
    pub is_enabled: bool,
    /// Device volume, if it has been read. Nothing reads it: the default
    /// device was given `Some(100)` and every other device `None`, so the one
    /// device a user looks at was the one carrying an invented figure.
    pub volume: Option<u8>,
    /// Device mute state, if it has been read. Nothing reads it either.
    pub muted: Option<bool>,
}

/// One endpoint as the Windows audio service records it.
#[cfg(target_os = "windows")]
struct WindowsEndpoint {
    is_output: bool,
    state: Option<AudioState>,
}

/// Per-endpoint volume, mute and default flag, from the audio service.
///
/// Keyed by endpoint GUID so it joins the registry enumeration this module
/// already does. `IMMDevice::GetId` returns `{0.0.0.00000000}.{guid}`; the
/// trailing brace-wrapped GUID is the same key the `MMDevices\Audio\Render`
/// and `...\Capture` trees use.
///
/// **Why COM and not WMI or the registry.** Neither carries a live master
/// volume: `Win32_SoundDevice` has no volume property at all, and the
/// `MMDevices` tree stores endpoint *configuration*, not the current scalar the
/// mixer shows. The absence reason these readings replace said as much -- "simon
/// has no mixer binding on this platform" and "which endpoint the system routes
/// to by default is a COM call this crate does not make" -- and both were
/// accurate descriptions of an unimplemented reader rather than of the platform.
///
/// Volume is reported as the scalar percentage the mixer displays, which is
/// what `board.audio.{n}.volume` asks for, and not the dB level: the two differ
/// by the taper Windows applies, and a consumer comparing simon's figure against
/// the slider would find the dB one wrong.
#[cfg(target_os = "windows")]
fn windows_endpoint_mixer() -> std::collections::HashMap<String, EndpointMixer> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    let mut out = std::collections::HashMap::new();

    // SAFETY: every call below is on interfaces obtained from the call above
    // it; every interface is created and dropped inside the `'com` block, and
    // `CoUninitialize` runs after that block on every path out.
    //
    // **The block is the fix for a crash, not a tidy-up.** `enumerator` is a
    // reference-counted COM interface whose `Drop` calls `Release`, and Rust
    // drops it at the end of the scope it was declared in -- which was the same
    // scope as `CoUninitialize()`, and *after* it. Releasing an interface on a
    // thread whose COM apartment has already been torn down is undefined, and
    // on Windows it presents as an access violation: the process dies with no
    // panic and the test harness reports only "test exited abnormally".
    //
    // Which is what every Windows CI run has done since 2026-09-03, the day
    // this reader landed. It survives on the development machine because
    // something else -- the `wmi` crate, usually -- has already initialised COM
    // on that thread, so `initialised` is false and `CoUninitialize` never runs.
    // A first-in wins, and on a fresh runner thread this reader is first in.
    unsafe {
        // The audio service is apartment-threaded. A failure here usually means
        // COM is already initialised on this thread with a different model,
        // which is not fatal -- the interfaces still work -- so the result is
        // deliberately not propagated, only the uninitialise is skipped.
        let initialised = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();

        'com: {
            let enumerator: Result<IMMDeviceEnumerator, _> =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL);
            let Ok(enumerator) = enumerator else {
                break 'com;
            };

            // Which endpoint each direction routes to. A machine with no output at
            // all has no default, which is why this is an `Option` rather than a
            // sentinel.
            let mut defaults: std::collections::HashSet<String> = std::collections::HashSet::new();
            for flow in [eRender, eCapture] {
                if let Ok(device) = enumerator.GetDefaultAudioEndpoint(flow, eConsole) {
                    if let Ok(id) = device.GetId() {
                        if let Some(guid) = endpoint_guid(&id.to_string().unwrap_or_default()) {
                            defaults.insert(guid);
                        }
                    }
                }
            }

            for flow in [eRender, eCapture] {
                let Ok(collection) = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
                else {
                    continue;
                };
                let count = collection.GetCount().unwrap_or(0);
                for i in 0..count {
                    let Ok(device) = collection.Item(i) else {
                        continue;
                    };
                    let Ok(id) = device.GetId() else { continue };
                    let Some(guid) = endpoint_guid(&id.to_string().unwrap_or_default()) else {
                        continue;
                    };

                    let volume: Result<IAudioEndpointVolume, _> = device.Activate(CLSCTX_ALL, None);
                    let (level, muted) = match volume {
                        Ok(v) => (
                            // The scalar is 0.0-1.0; the entity is a percentage.
                            v.GetMasterVolumeLevelScalar()
                                .ok()
                                .map(|s| (s * 100.0).round().clamp(0.0, 100.0) as u8),
                            v.GetMute().ok().map(|m| m.as_bool()),
                        ),
                        // An endpoint whose volume interface will not activate
                        // reports no level, rather than a default one.
                        Err(_) => (None, None),
                    };

                    out.insert(
                        guid.clone(),
                        EndpointMixer {
                            volume: level,
                            muted,
                            is_default: defaults.contains(&guid),
                            is_render: flow == eRender,
                        },
                    );
                }
            }
        }
        // Every interface above is out of scope and released by here.

        if initialised {
            CoUninitialize();
        }
    }

    out
}

/// Pull the endpoint GUID out of an `IMMDevice` id.
///
/// The id is `{0.0.0.00000000}.{a1b2...}`: a container id, a dot, then the
/// endpoint GUID in braces. The registry trees this module already reads are
/// keyed by that second component alone.
#[cfg(target_os = "windows")]
fn endpoint_guid(device_id: &str) -> Option<String> {
    let guid = device_id.rsplit('.').next()?;
    (guid.starts_with('{') && guid.ends_with('}')).then(|| guid.to_ascii_lowercase())
}

/// What the audio service reports about one endpoint's mixer.
#[cfg(target_os = "windows")]
struct EndpointMixer {
    volume: Option<u8>,
    muted: Option<bool>,
    is_default: bool,
    /// Output rather than input. The two directions each have their own
    /// default, and "master volume" means the output one.
    is_render: bool,
}

pub struct AudioMonitor {
    devices: Vec<AudioDevice>,
    /// System master volume, if it has been read.
    ///
    /// It never has been. This was initialised to `Some(100)` in the
    /// constructor and no `refresh_*` path on any platform assigns it, so
    /// `simon cli audio` reported "Master Volume: 100%" on every machine, the
    /// TUI printed the same, and the agent tool surface published
    /// `"master_volume": 100`. None of those was a reading.
    master_volume: Option<u8>,
    /// System mute state, if it has been read — see [`Self::master_volume`].
    /// It was a constant `false`, reported as "Muted: No".
    master_muted: Option<bool>,
}

impl AudioMonitor {
    pub fn new() -> Result<Self, SimonError> {
        let mut monitor = Self {
            devices: Vec::new(),
            // Nothing reads these yet. Reading the system mixer needs
            // IAudioEndpointVolume on Windows, PulseAudio or ALSA on Linux and
            // CoreAudio on macOS; until one of those is wired up the honest
            // value is "not read", not "100%".
            master_volume: None,
            master_muted: None,
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    pub fn refresh(&mut self) -> Result<(), SimonError> {
        self.devices.clear();
        #[cfg(target_os = "windows")]
        self.refresh_windows()?;
        #[cfg(target_os = "linux")]
        self.refresh_linux()?;
        #[cfg(target_os = "macos")]
        self.refresh_macos()?;
        Ok(())
    }

    pub fn devices(&self) -> &[AudioDevice] {
        &self.devices
    }
    pub fn master_volume(&self) -> Option<u8> {
        self.master_volume
    }
    /// Whether the system is muted, or `None` because nothing reads it.
    pub fn is_muted(&self) -> Option<bool> {
        self.master_muted
    }
    pub fn default_output(&self) -> Option<&AudioDevice> {
        self.devices
            .iter()
            .find(|d| d.is_default == Some(true) && d.is_output)
    }
    pub fn default_input(&self) -> Option<&AudioDevice> {
        self.devices
            .iter()
            .find(|d| d.is_default == Some(true) && !d.is_output)
    }

    // ==================== Hardware Control APIs ====================

    /// Set the master volume level (0-100).
    ///
    /// # Not implemented
    ///
    /// This assigned the field and returned `Ok(())`, touching no audio API on
    /// any platform. A caller set the volume to 20, was told it had worked, and
    /// the machine did not change — while `master_volume()` then returned the
    /// 20 it had just stored, so even reading it back appeared to confirm it.
    ///
    /// It returns an error until a real mixer call is behind it. A control that
    /// reports success without acting is worse than one that is absent.
    pub fn set_master_volume(&mut self, volume: u8) -> Result<(), crate::error::SimonError> {
        if volume > 100 {
            return Err(crate::error::SimonError::InvalidInput(format!(
                "Volume must be 0-100, got {volume}"
            )));
        }
        Err(crate::error::SimonError::NotImplemented(
            concat!(
                "setting the master volume: simon has no mixer binding on this ",
                "platform, and this call previously changed only its own copy of ",
                "the value"
            )
            .to_string(),
        ))
    }

    /// Set the master mute state.
    ///
    /// # Not implemented
    ///
    /// See [`Self::set_master_volume`]. This changed only the field.
    pub fn set_mute(&mut self, _muted: bool) -> Result<(), crate::error::SimonError> {
        Err(crate::error::SimonError::NotImplemented(
            "setting the mute state: simon has no mixer binding on this platform".to_string(),
        ))
    }

    /// Set volume for a specific device by ID.
    pub fn set_device_volume(
        &mut self,
        device_id: &str,
        volume: u8,
    ) -> Result<(), crate::error::SimonError> {
        if volume > 100 {
            return Err(crate::error::SimonError::InvalidInput(format!(
                "Volume must be 0-100, got {}",
                volume
            )));
        }
        if let Some(device) = self.devices.iter_mut().find(|d| d.id == device_id) {
            device.volume = Some(volume);
            Ok(())
        } else {
            Err(crate::error::SimonError::NotFound(format!(
                "Audio device '{}' not found",
                device_id
            )))
        }
    }

    /// Set mute state for a specific device by ID.
    ///
    /// # Not implemented
    ///
    /// See [`Self::set_master_volume`]. This changed only the field.
    pub fn set_device_mute(
        &mut self,
        device_id: &str,
        _muted: bool,
    ) -> Result<(), crate::error::SimonError> {
        if self.devices.iter().any(|d| d.id == device_id) {
            Err(crate::error::SimonError::NotImplemented(
                concat!(
                    "setting a device's mute state: simon has no mixer binding ",
                    "on this platform"
                )
                .to_string(),
            ))
        } else {
            Err(crate::error::SimonError::NotFound(format!(
                "Audio device '{}' not found",
                device_id
            )))
        }
    }

    /// Enumerate this machine's audio endpoints.
    ///
    /// Three separate things were wrong here, and all three came from the same
    /// root cause as the display reader: **the list was of the wrong kind of
    /// object**, so everything hung off it had to be guessed.
    ///
    /// The PowerShell it ran concatenated `Win32_SoundDevice` — audio
    /// *adapters* — with the PnP `AudioEndpoint` class. On the development
    /// machine that produced twelve "endpoints", eight of which are codecs and
    /// controllers: `Realtek High Definition Audio`, `AMD Streaming Audio
    /// Device`, `NVIDIA High Definition Audio` twice. The entity reads
    /// "endpoint name **as the platform presents it to a user**", and a user is
    /// never shown any of those.
    ///
    /// Direction came from a regular expression over the name:
    ///
    /// ```ignore
    /// $isInput = $dev.Name -match 'Microphone|Input|Capture|Line In'
    /// ```
    ///
    /// which was **inverted on half the real endpoints here**. A virtual audio
    /// interface names its endpoints from the application's point of view, so
    /// `MOTIV Mix Virtual Input` is what you play *into* — a render endpoint —
    /// and `MOTIV Mix Virtual Output` is what you record *from*. The registry
    /// says Render and Capture respectively; the regex said the opposite of
    /// both.
    ///
    /// And the default endpoint was whichever row came first:
    ///
    /// ```ignore
    /// let is_default = if is_output && !has_default_output { .. true } else { false };
    /// ```
    ///
    /// The endpoint list itself is kept — it is Windows' own presentation, and
    /// it carries no duplicates — and joined to
    /// `MMDevices\Audio\{Render,Capture}` on the GUID that ends the PnP device
    /// id, which is exactly the registry subkey name. That join is an equality
    /// on a unique key rather than a name match: the friendly names do not join
    /// reliably, because Windows disambiguates a second instance of an adapter
    /// by prefixing `2- `.
    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), SimonError> {
        const QUERY: &str = concat!(
            "Get-CimInstance Win32_PnPEntity | Where-Object { $_.PNPClass -eq 'AudioEndpoint' } ",
            "| Select-Object Name, PNPDeviceID | ConvertTo-Json -Compress"
        );

        let endpoints = Self::windows_endpoint_registry();
        // One COM session for the whole enumeration rather than one per device.
        let mixer = windows_endpoint_mixer();

        // "Master volume" is the level of the endpoint the system routes output
        // to -- the figure the volume slider shows. It is not a separate system
        // property, and Windows has none: there is only ever a per-endpoint
        // level, so taking the default output's is what the name means.
        //
        // `None` where no default output was found, which is a machine with no
        // active render endpoint at all. It used to be a hardcoded `Some(100)`.
        if let Some(default_out) = mixer.values().find(|m| m.is_default && m.is_render) {
            self.master_volume = default_out.volume;
            self.master_muted = default_out.muted;
        }

        let Some(value) =
            crate::core::command::capture_json("powershell", &["-NoProfile", "-Command", QUERY])?
        else {
            return Ok(());
        };

        for (idx, item) in crate::core::command::json_items(&value).iter().enumerate() {
            let Some(name) = item["Name"].as_str().map(str::to_string) else {
                continue;
            };
            let pnp_id = item["PNPDeviceID"].as_str().unwrap_or("");

            // `SWD\MMDEVAPI\{0.0.0.00000000}.{116ba9f0-...}` — the endpoint's
            // own GUID is the last brace group.
            let Some(guid) = pnp_id
                .rfind('{')
                .map(|i| pnp_id[i..].to_ascii_lowercase())
                .filter(|g| g.ends_with('}'))
            else {
                continue;
            };

            // An endpoint the audio service does not have a key for is one this
            // reader cannot describe: its direction and state would both have
            // to be invented, which is what the previous version did.
            let Some(endpoint) = endpoints.get(&guid) else {
                continue;
            };

            self.devices.push(AudioDevice {
                id: format!("audio{idx}"),
                name,
                device_type: if endpoint.is_output {
                    AudioDeviceType::Output
                } else {
                    AudioDeviceType::Input
                },
                state: endpoint.state,
                // From the audio service. This was "whichever row came first",
                // which on this machine named an audio *controller* as the
                // default output; then it was `None` with a note that the COM
                // call was not made. It is made now -- see
                // `windows_endpoint_mixer`.
                //
                // An endpoint the mixer enumeration did not return keeps its
                // absence: `DEVICE_STATE_ACTIVE` is what that enumeration asks
                // for, so a disabled or unplugged endpoint has no live volume
                // to report and is not given one.
                is_default: mixer.get(&guid).map(|m| m.is_default),
                is_output: endpoint.is_output,
                is_enabled: endpoint.state == Some(AudioState::Active),
                volume: mixer.get(&guid).and_then(|m| m.volume),
                muted: mixer.get(&guid).and_then(|m| m.muted),
            });
        }
        Ok(())
    }

    /// Every audio endpoint the Windows audio service knows about, by GUID.
    ///
    /// `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio` has a
    /// `Render` and a `Capture` subtree, which is where the direction comes
    /// from, and each endpoint key carries a `DeviceState` DWORD. Both are
    /// readable without elevation.
    #[cfg(target_os = "windows")]
    fn windows_endpoint_registry() -> std::collections::HashMap<String, WindowsEndpoint> {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;

        const BASE: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio";

        let mut out = std::collections::HashMap::new();
        for (flow, is_output) in [("Render", true), ("Capture", false)] {
            let Ok(tree) =
                RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(format!("{BASE}\\{flow}"))
            else {
                continue;
            };
            for guid in tree.enum_keys().flatten() {
                let Ok(key) = tree.open_subkey(&guid) else {
                    continue;
                };
                // The documented states are a four-bit mask -- 1 active,
                // 2 disabled, 4 not present, 8 unplugged -- and Windows sets
                // further undocumented bits above them, so the value is masked
                // rather than compared.
                let state =
                    key.get_value::<u32, _>("DeviceState")
                        .ok()
                        .and_then(|raw| match raw & 0xF {
                            1 => Some(AudioState::Active),
                            2 => Some(AudioState::Suspended),
                            4 | 8 => Some(AudioState::Unavailable),
                            _ => None,
                        });
                out.insert(
                    guid.to_ascii_lowercase(),
                    WindowsEndpoint { is_output, state },
                );
            }
        }
        out
    }
    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) -> Result<(), SimonError> {
        use std::fs;

        // Read from /proc/asound for ALSA card enumeration. A kernel with no
        // ALSA has no /proc/asound, which is a reading; a file that will not
        // open is not, and both used to leave the list silently empty.
        let cards_path = std::path::Path::new("/proc/asound/cards");
        let cards = match fs::read_to_string(cards_path) {
            Ok(c) => Some(c),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                return Err(SimonError::System(format!(
                    "cannot read /proc/asound/cards: {e}"
                )))
            }
        };
        if let Some(cards_content) = cards {
            for line in cards_content.lines() {
                let trimmed = line.trim();
                // Lines like " 0 [PCH            ]: HDA-Intel - HDA Intel PCH"
                if let Some(bracket_start) = trimmed.find('[') {
                    if let Some(bracket_end) = trimmed.find(']') {
                        let card_id_str: String = trimmed
                            .chars()
                            .take_while(|c| c.is_ascii_digit() || c.is_whitespace())
                            .collect();
                        let card_num = card_id_str.trim().parse::<u32>().unwrap_or(0);
                        let short_name = trimmed[bracket_start + 1..bracket_end].trim().to_string();

                        // Get full name from the colon part
                        let full_name = if let Some(colon_idx) = trimmed.find("- ") {
                            trimmed[colon_idx + 2..].trim().to_string()
                        } else {
                            short_name.clone()
                        };

                        // Check for playback/capture devices
                        let pcm_path = format!("/proc/asound/card{}", card_num);
                        let has_playback =
                            std::path::Path::new(&format!("{}/pcm0p", pcm_path)).exists();
                        let has_capture =
                            std::path::Path::new(&format!("{}/pcm0c", pcm_path)).exists();

                        let device_type = match (has_playback, has_capture) {
                            (true, true) => AudioDeviceType::Duplex,
                            (true, false) => AudioDeviceType::Output,
                            (false, true) => AudioDeviceType::Input,
                            _ => AudioDeviceType::Output,
                        };

                        let is_output = has_playback || !has_capture;

                        self.devices.push(AudioDevice {
                            id: format!("hw:{}", card_num),
                            name: full_name,
                            device_type,
                            state: None,
                            is_default: Some(card_num == 0),
                            is_output,
                            is_enabled: true,
                            volume: None,
                            muted: None,
                        });
                    }
                }
            }
        }

        // Try PulseAudio/PipeWire for default device info
        if let Ok(output) = std::process::Command::new("pactl").args(["info"]).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.starts_with("Default Sink:") {
                        if let Some(name) = line.split(':').nth(1) {
                            // Mark matching device as default
                            let sink_name = name.trim();
                            if let Some(dev) = self.devices.iter_mut().find(|d| d.is_output) {
                                dev.is_default = Some(true);
                                if dev.name == format!("hw:{}", 0) {
                                    dev.name = sink_name.to_string();
                                }
                            }
                        }
                    }
                }
            }
        }

        // No fallback device. An empty list means the ALSA cards file told us
        // nothing, which is not the same as one working duplex device.
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), SimonError> {
        // Use system_profiler for audio device info. It ships with macOS, so a
        // failure to run it is a failure rather than an absent optional tool.
        let stdout =
            crate::core::command::capture("system_profiler", &["SPAudioDataType", "-json"])?;
        {
            {
                let stdout = stdout.as_str();
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
                    if let Some(audio_data) = json.get("SPAudioDataType").and_then(|v| v.as_array())
                    {
                        let mut idx = 0u32;
                        for device in audio_data {
                            let name = device
                                .get("_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Audio Device");
                            let has_output = device
                                .get("coreaudio_default_audio_output_device")
                                .and_then(|v| v.as_str())
                                == Some("spaudio_yes");
                            let has_input = device
                                .get("coreaudio_default_audio_input_device")
                                .and_then(|v| v.as_str())
                                == Some("spaudio_yes");

                            // These keys mark which device is *default* for each
                            // direction, not what a device supports, so capability is
                            // inferred.
                            //
                            // The previous expression was `(has_output || true, ...)`,
                            // whose first element is unconditionally true — so
                            // `has_output` was dead and no device was ever classified
                            // as Input, including the default microphone.
                            let device_type = if has_input && has_output {
                                AudioDeviceType::Duplex
                            } else if has_input {
                                AudioDeviceType::Input
                            } else {
                                // Capability is unknown; output is the right default
                                // for the overwhelming majority of audio devices.
                                AudioDeviceType::Output
                            };

                            self.devices.push(AudioDevice {
                                id: format!("audio{}", idx),
                                name: name.to_string(),
                                device_type,
                                state: None,
                                is_default: Some(has_output || has_input),
                                is_output: has_output || (!has_input),
                                is_enabled: true,
                                volume: None,
                                muted: None,
                            });
                            idx += 1;
                        }
                    }
                }
            }
        }

        // No fallback device, and no invented 100% volume with it. See the
        // Windows path above.
        Ok(())
    }
}

impl Default for AudioMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            devices: Vec::new(),
            master_volume: None,
            master_muted: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing in simon reads the system mixer, so nothing may report one.
    ///
    /// `master_volume` was initialised to `Some(100)` in the constructor and
    /// assigned by no `refresh_*` path on any platform, so `simon cli audio`
    /// printed "Master Volume: 100%" on every machine, the TUI printed the
    /// same, and the agent tool surface published `"master_volume": 100`. This
    /// fails the moment a real reader lands, which is when the wording on
    /// those three surfaces needs revisiting.
    #[test]
    fn the_master_volume_is_the_default_output_or_nothing() {
        let monitor = AudioMonitor::new().expect("monitor");

        // This test was written when no platform read a mixer at all, and it
        // asserted `None` everywhere. Windows reads one now, so asserting
        // absence would pin the defect rather than the principle.
        //
        // The principle is what the original constructor got wrong: it set
        // `Some(100)` from nowhere, and every surface published "Master Volume:
        // 100%" on every machine. A master volume is not a system property --
        // Windows has only per-endpoint levels -- so the only honest value is
        // the default *output* endpoint's, and any other number is invented.
        let default_output = monitor
            .devices()
            .iter()
            .find(|d| d.is_output && d.is_default == Some(true));

        match default_output {
            Some(device) => {
                assert_eq!(
                    monitor.master_volume(),
                    device.volume,
                    "master volume must be the default output's level, not a figure of its own"
                );
                assert_eq!(monitor.is_muted(), device.muted);
            }
            // No default output found: on a platform with no mixer binding at
            // all, and on a machine with no active render endpoint.
            None => {
                assert_eq!(
                    monitor.master_volume(),
                    None,
                    "with no default output there is nothing for a master volume to be"
                );
                assert_eq!(monitor.is_muted(), None);
            }
        }

        // Whatever is reported is a percentage. The old value was 100, which is
        // in range -- so this alone would not have caught it, and it is here to
        // catch a scalar published without its 0-1 to 0-100 conversion.
        for device in monitor.devices() {
            if let Some(volume) = device.volume {
                assert!(
                    volume <= 100,
                    "{}: volume {volume} is not a percentage",
                    device.name
                );
            }
        }
    }

    /// A control that reports success without acting is worse than one that is
    /// absent, because the caller has no way to find out.
    #[test]
    fn the_mixer_setters_decline_rather_than_pretend() {
        let mut monitor = AudioMonitor::new().expect("monitor");
        let before_volume = monitor.master_volume();
        let before_muted = monitor.is_muted();

        assert!(
            monitor.set_master_volume(75).is_err(),
            "setting the volume touches no audio API and must not return Ok"
        );
        assert!(monitor.set_mute(true).is_err());

        // The failed call must not have left its argument behind as though it
        // were a reading. Compared against what was read before the call rather
        // than against `None`, since a mixer that *is* read has a real value
        // here and pinning `None` would only pin the platform this runs on.
        assert_eq!(monitor.master_volume(), before_volume);
        assert_eq!(monitor.is_muted(), before_muted);

        // Argument validation still comes first.
        assert!(monitor.set_master_volume(101).is_err());
    }

    /// Constructing the monitor either enumerates or says why it could not.
    ///
    /// See the identically-shaped tests in `camera`, `usb` and the rest: this
    /// asserted `is_ok()`, which was true by construction while `refresh` could
    /// not fail. A failure must carry a reason, because a reason is the whole
    /// difference between "this machine has none" and "nobody looked".
    #[test]
    fn test_audio_monitor_creation() {
        match AudioMonitor::new() {
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

    /// Whatever is enumerated must be identifiable. How many there are is the
    /// machine's business.
    ///
    /// This asserted `!devices().is_empty()`, under a comment reading
    /// "Should have at least one device (placeholder on all platforms)" — it
    /// existed to assert that the invented fallback device was present, and it
    /// failed on the Linux and Windows CI runners the moment that device was
    /// removed, because a headless runner genuinely has no audio endpoint.
    ///
    /// A test that pins a placeholder makes the placeholder the contract. The
    /// honest assertion is about the shape of what is reported, not about a
    /// count the hardware decides.
    #[test]
    fn test_audio_monitor_devices() {
        let monitor = AudioMonitor::new().unwrap();
        for device in monitor.devices() {
            assert!(
                !device.id.is_empty(),
                "an audio device was reported with no id"
            );
            assert!(
                !device.name.is_empty(),
                "audio device {} was reported with no name",
                device.id
            );
        }
    }

    #[test]
    fn test_audio_monitor_master_volume() {
        let monitor = AudioMonitor::new().unwrap();
        if let Some(vol) = monitor.master_volume() {
            assert!(vol <= 100);
        }
    }

    #[test]
    fn test_audio_device_serialization() {
        let device = AudioDevice {
            id: "test".to_string(),
            name: "Test Device".to_string(),
            device_type: AudioDeviceType::Output,
            state: Some(AudioState::Active),
            is_default: Some(true),
            is_output: true,
            is_enabled: true,
            volume: Some(50),
            muted: Some(false),
        };
        let json = serde_json::to_string(&device).unwrap();
        let deserialized: AudioDevice = serde_json::from_str(&json).unwrap();
        assert_eq!(device.id, deserialized.id);
        assert_eq!(device.name, deserialized.name);
    }

    #[test]
    fn test_audio_monitor_default() {
        let monitor = AudioMonitor::default();
        // Default should work without panic
        let _ = monitor.devices();
    }
}
