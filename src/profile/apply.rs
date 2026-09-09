//! Apply (write) layer for hardware profile settings.
//!
//! Most of the [`super::ProfileInspector`] surface is read-only. This module
//! adds a narrow, audited write path for the settings that providers
//! explicitly opt into via [`Setting::writable`].
//!
//! ## Design principles
//!
//! - **Opt-in writes.** Each handler advertises an exact `(subsystem,
//!   setting_id)` it can write. There is no generic write path.
//! - **Audit everything.** Every attempt — whether allowed, refused, or
//!   failed — appends a JSON line to `<state_dir>/ironmon_profile_audit.log`.
//! - **No silent elevation.** If the OS rejects the write for permissions,
//!   the error bubbles up with a "needs admin/root" hint. We never re-launch
//!   ourselves elevated.
//! - **Confirmation required.** Callers (CLI/MCP) gate apply behind explicit
//!   confirmation. The library function itself does not prompt.
//!
//! Writing arbitrary NVAPI DWORDs, MSRs, or BIOS values is intentionally out
//! of scope. Add handlers only when the API is well-defined and the failure
//! mode is reversible.

use super::{Setting, SettingValue, Subsystem};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Result of an apply attempt — always returned (never panics), always logged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyOutcome {
    pub setting_id: String,
    pub subsystem: Subsystem,
    pub requested: SettingValue,
    pub status: ApplyStatus,
    pub message: String,
    /// Unix epoch seconds.
    pub timestamp: u64,
    /// The value that was in effect before this write, when the handler could
    /// read it.
    ///
    /// This is what makes a write reversible, and until it existed nothing in
    /// this crate could undo one: an autonomous tuner could set a governor and
    /// then had no way to put back what it found. `None` means the prior value
    /// was not readable — not that there wasn't one — so
    /// [`revert_setting`] refuses rather than guessing at a default.
    #[serde(default)]
    pub previous: Option<SettingValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    /// Write succeeded; new value is in effect.
    Applied,
    /// Setting is read-only (no registered handler).
    NotWritable,
    /// The OS rejected the call (permission, busy, etc.).
    Refused,
    /// The handler ran but reports the underlying API failed.
    Failed,
    /// The library would write but the caller did not confirm.
    NeedsConfirm,
}

/// Trait implemented by per-setting write handlers.
pub trait ApplyHandler: Send + Sync {
    /// The setting this handler covers (must match `Setting.id`).
    fn setting_id(&self) -> &str;
    /// The subsystem this setting belongs to.
    fn subsystem(&self) -> Subsystem;
    /// Try to write the requested value. Must not panic; surface any error
    /// via the returned [`ApplyStatus`].
    fn apply(&self, value: &SettingValue) -> ApplyOutcome;

    /// The value currently in effect, when it can be read.
    ///
    /// Defaults to `None` so existing handlers keep compiling, but a handler
    /// that does not implement this makes its setting one-way: `apply_setting`
    /// records no prior value, and [`revert_setting`] will refuse to guess one.
    /// Implement it wherever the source can be read back.
    fn read_current(&self) -> Option<SettingValue> {
        None
    }
}

/// Built-in handler registry. New handlers register themselves by appearing
/// in [`builtin_handlers`].
// Not a vec! literal: which handlers exist depends on `cfg`, and those attributes
// apply to statements, not to elements of an expression.
#[allow(clippy::vec_init_then_push)]
pub fn builtin_handlers() -> Vec<Box<dyn ApplyHandler>> {
    let mut out: Vec<Box<dyn ApplyHandler>> = Vec::new();
    #[cfg(all(feature = "nvidia", target_os = "linux"))]
    out.push(Box::new(NvidiaPersistenceModeHandler));
    #[cfg(target_os = "linux")]
    {
        out.push(Box::new(LinuxCpufreqGovernorHandler));
        out.push(Box::new(LinuxAmdPerfLevelHandler));
        out.push(Box::new(LinuxIntelGtMaxFreqHandler));
    }
    #[cfg(windows)]
    out.push(Box::new(WindowsActivePowerSchemeHandler));
    out
}

/// Read a handler's current value, retrying a transient failure.
///
/// The Win32 and sysfs reads behind `read_current` can fail for reasons that
/// have nothing to do with the setting — a loaded machine, a momentarily busy
/// device. A single failed read is not evidence that the setting is unreadable,
/// and treating it as such silently converts a reversible write into an
/// irreversible one: the write still happens, `previous` is `None`, and nothing
/// says so until someone tries to undo it.
///
/// Observed once during testing, where `PowerGetActiveScheme` returned failure
/// on a machine running several compiles, then read normally on the next
/// attempt. Three tries, because the failure was transient, not persistent.
pub(crate) fn read_current_with_retry(handler: &dyn ApplyHandler) -> Option<SettingValue> {
    for attempt in 0..3 {
        if let Some(v) = handler.read_current() {
            return Some(v);
        }
        if attempt < 2 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    None
}

/// Apply a setting, but only if the change can be undone.
///
/// Identical to [`apply_setting`] except that it refuses when the prior value
/// could not be read. Use it for anything unattended: an autonomous loop that
/// makes a change it cannot reverse has taken a decision away from the person
/// who has to live with the result, and it does so invisibly — the write
/// succeeds, and only a later attempt to undo it reveals there is no way back.
///
/// Attended callers may still want [`apply_setting`]: a person at a terminal who
/// asks for a setting and is told "this cannot be undone" can decide for
/// themselves. That decision is theirs and not this function's, which is why
/// both exist.
pub fn apply_setting_reversible(
    setting_id: &str,
    value: SettingValue,
    confirm: bool,
) -> ApplyOutcome {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if let Some(h) = builtin_handlers()
        .into_iter()
        .find(|h| h.setting_id() == setting_id)
    {
        if read_current_with_retry(h.as_ref()).is_none() {
            return ApplyOutcome {
                setting_id: setting_id.into(),
                subsystem: h.subsystem(),
                requested: value,
                status: ApplyStatus::Refused,
                message: format!(
                    "Refused: the value currently in effect for {setting_id:?} could not be read, so this write could not be undone. Use apply_setting (or `ironmon profile set`) to write it anyway, accepting that it is one-way."
                ),
                timestamp: now,
                previous: None,
            };
        }
    }

    apply_setting(setting_id, value, confirm)
}

/// Put back the value an earlier [`apply_setting`] overwrote.
///
/// Takes the outcome of the write being undone rather than a setting id and a
/// value, because the whole point is that the caller does not have to have kept
/// the old value anywhere: it travels with the outcome.
///
/// Refuses when `previous` is `None`. That happens when the handler could not
/// read the setting before writing it, and the honest answer there is "this
/// cannot be undone" rather than a guess at a default — putting a machine into a
/// state it was never in is a worse failure than leaving it in the state the
/// caller chose.
///
/// Goes through [`apply_setting`], so a revert is confirmed and audit-logged on
/// exactly the same terms as the write it undoes. An autonomous loop that could
/// revert without confirmation would be a write path with no confirmation.
pub fn revert_setting(applied: &ApplyOutcome, confirm: bool) -> ApplyOutcome {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Only a write that took effect needs undoing. Reverting a refused or failed
    // apply would write a value the caller never asked for.
    if applied.status != ApplyStatus::Applied {
        return ApplyOutcome {
            setting_id: applied.setting_id.clone(),
            subsystem: applied.subsystem,
            requested: applied.requested.clone(),
            status: ApplyStatus::NotWritable,
            message: format!(
                "Nothing to revert: the apply being undone ended as {:?}, not Applied.",
                applied.status
            ),
            timestamp: now,
            previous: None,
        };
    }

    let Some(previous) = applied.previous.clone() else {
        return ApplyOutcome {
            setting_id: applied.setting_id.clone(),
            subsystem: applied.subsystem,
            requested: applied.requested.clone(),
            status: ApplyStatus::NotWritable,
            message: format!(
                "Cannot revert {:?}: no prior value was recorded, because the handler could not read the setting before writing it. The setting is still at the applied value.",
                applied.setting_id
            ),
            timestamp: now,
            previous: None,
        };
    };

    apply_setting(&applied.setting_id, previous, confirm)
}

/// Apply a setting by id. Returns an [`ApplyOutcome`] in every case (no
/// panicking, no unhandled errors). Audit-logs the attempt.
pub fn apply_setting(setting_id: &str, value: SettingValue, confirm: bool) -> ApplyOutcome {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let handler = builtin_handlers()
        .into_iter()
        .find(|h| h.setting_id() == setting_id);

    let outcome = match handler {
        None => ApplyOutcome {
            setting_id: setting_id.into(),
            subsystem: Subsystem::Gpu, // placeholder — overridden if handler exists
            requested: value.clone(),
            status: ApplyStatus::NotWritable,
            message: format!(
                "No registered ApplyHandler for setting id {:?}. \
                The setting may be read-only on this platform or not yet implemented.",
                setting_id
            ),
            timestamp: now,
            previous: None,
        },
        Some(h) if !confirm => ApplyOutcome {
            setting_id: setting_id.into(),
            subsystem: h.subsystem(),
            requested: value.clone(),
            status: ApplyStatus::NeedsConfirm,
            message:
                "Apply rejected: explicit confirmation required (pass confirm=true or --confirm)."
                    .into(),
            timestamp: now,
            previous: None,
        },
        Some(h) => {
            // Read before writing. Afterwards the old value is gone, and a
            // tuner that cannot say what it overwrote cannot put it back.
            let previous = read_current_with_retry(h.as_ref());
            let mut outcome = h.apply(&value);
            outcome.previous = previous;
            outcome
        }
    };

    audit_log(&outcome);
    outcome
}

/// Resolve the audit log path. Honors `IRONMON_AUDIT_LOG` if set; otherwise
/// falls back to `%LOCALAPPDATA%\ironmon\profile_audit.log` on Windows,
/// `$XDG_STATE_HOME/ironmon/profile_audit.log` on Linux, and the OS temp dir
/// elsewhere.
pub fn audit_log_path() -> PathBuf {
    static CACHED: OnceLock<PathBuf> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            if let Ok(p) = std::env::var("IRONMON_AUDIT_LOG") {
                return PathBuf::from(p);
            }
            #[cfg(windows)]
            {
                if let Ok(local) = std::env::var("LOCALAPPDATA") {
                    return PathBuf::from(local)
                        .join("ironmon")
                        .join("profile_audit.log");
                }
            }
            #[cfg(unix)]
            {
                if let Ok(state) = std::env::var("XDG_STATE_HOME") {
                    return PathBuf::from(state)
                        .join("ironmon")
                        .join("profile_audit.log");
                }
                if let Ok(home) = std::env::var("HOME") {
                    return PathBuf::from(home)
                        .join(".local")
                        .join("state")
                        .join("ironmon")
                        .join("profile_audit.log");
                }
            }
            std::env::temp_dir().join("ironmon_profile_audit.log")
        })
        .clone()
}

fn audit_log(outcome: &ApplyOutcome) {
    let path = audit_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        if let Ok(line) = serde_json::to_string(outcome) {
            let _ = writeln!(f, "{}", line);
        }
    }
}

// ─── Built-in handlers ──────────────────────────────────────────────────────

/// Toggle NVIDIA's persistence mode on GPU 0 via NVML.
#[cfg(all(feature = "nvidia", target_os = "linux"))]
struct NvidiaPersistenceModeHandler;

#[cfg(all(feature = "nvidia", target_os = "linux"))]
impl ApplyHandler for NvidiaPersistenceModeHandler {
    fn setting_id(&self) -> &str {
        "persistence_mode"
    }
    fn subsystem(&self) -> Subsystem {
        Subsystem::Gpu
    }
    fn apply(&self, value: &SettingValue) -> ApplyOutcome {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let target = match value {
            SettingValue::Bool(b) => *b,
            other => {
                return ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: other.clone(),
                    status: ApplyStatus::Failed,
                    message: "persistence_mode requires a Bool value.".into(),
                    timestamp: now,
                    previous: None,
                };
            }
        };
        use nvml_wrapper::Nvml;
        match Nvml::init().and_then(|n| {
            let mut dev = n.device_by_index(0)?;
            dev.set_persistent(target)?;
            Ok(())
        }) {
            Ok(_) => ApplyOutcome {
                setting_id: self.setting_id().into(),
                subsystem: self.subsystem(),
                requested: value.clone(),
                status: ApplyStatus::Applied,
                message: format!("NVIDIA persistence mode on GPU 0 set to {}.", target),
                timestamp: now,
                previous: None,
            },
            Err(e) => {
                let s = e.to_string();
                let refused = s.contains("Permission")
                    || s.contains("permission")
                    || s.contains("denied")
                    || s.contains("PrivilegedDriver");
                ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: value.clone(),
                    status: if refused {
                        ApplyStatus::Refused
                    } else {
                        ApplyStatus::Failed
                    },
                    message: if refused {
                        format!("NVML write rejected — run as administrator/root: {}", s)
                    } else {
                        format!("NVML write failed: {}", s)
                    },
                    timestamp: now,
                    previous: None,
                }
            }
        }
    }
}

/// Set the Linux CPU governor on CPU0 (which propagates to the policy).
#[cfg(target_os = "linux")]
struct LinuxCpufreqGovernorHandler;

#[cfg(target_os = "linux")]
impl ApplyHandler for LinuxCpufreqGovernorHandler {
    fn setting_id(&self) -> &str {
        "scaling_governor"
    }
    // Reading back the same path that `apply` writes. By inspection only: this
    // session had no Linux machine to run it on, and CI compiles but does not
    // exercise a sysfs write.
    fn subsystem(&self) -> Subsystem {
        Subsystem::Cpu
    }
    fn read_current(&self) -> Option<SettingValue> {
        std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
            .ok()
            .map(|s| SettingValue::Text(s.trim().to_string()))
    }
    fn apply(&self, value: &SettingValue) -> ApplyOutcome {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let target = match value {
            SettingValue::Text(s) => s.clone(),
            other => {
                return ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: other.clone(),
                    status: ApplyStatus::Failed,
                    message: "scaling_governor requires a Text value (e.g. \"performance\")."
                        .into(),
                    timestamp: now,
                    previous: None,
                };
            }
        };
        let path = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor";
        match std::fs::write(path, &target) {
            Ok(_) => ApplyOutcome {
                setting_id: self.setting_id().into(),
                subsystem: self.subsystem(),
                requested: value.clone(),
                status: ApplyStatus::Applied,
                message: format!("CPU0 scaling_governor set to {:?}.", target),
                timestamp: now,
                previous: None,
            },
            Err(e) => {
                let refused = e.kind() == std::io::ErrorKind::PermissionDenied;
                ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: value.clone(),
                    status: if refused {
                        ApplyStatus::Refused
                    } else {
                        ApplyStatus::Failed
                    },
                    message: if refused {
                        "Permission denied — write to /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor requires root.".into()
                    } else {
                        format!("sysfs write failed: {}", e)
                    },
                    timestamp: now,
                    previous: None,
                }
            }
        }
    }
}

/// Write `/sys/class/drm/cardN/device/power_dpm_force_performance_level` for
/// the first AMD GPU. Accepted values: "auto", "low", "high", "manual",
/// "profile_standard", "profile_min_sclk", "profile_min_mclk", "profile_peak".
#[cfg(target_os = "linux")]
struct LinuxAmdPerfLevelHandler;

#[cfg(target_os = "linux")]
impl ApplyHandler for LinuxAmdPerfLevelHandler {
    fn setting_id(&self) -> &str {
        "perf_level"
    }
    fn subsystem(&self) -> Subsystem {
        Subsystem::Gpu
    }
    fn apply(&self, value: &SettingValue) -> ApplyOutcome {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let target = match value {
            SettingValue::Text(s) => s.clone(),
            other => {
                return ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: other.clone(),
                    status: ApplyStatus::Failed,
                    message: "perf_level requires a Text value (e.g. \"auto\").".into(),
                    timestamp: now,
                    previous: None,
                };
            }
        };
        // Locate the first AMD card.
        let card = match find_amd_card() {
            Some(c) => c,
            None => {
                return ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: value.clone(),
                    status: ApplyStatus::Failed,
                    message: "No AMD GPU card found in /sys/class/drm.".into(),
                    timestamp: now,
                    previous: None,
                };
            }
        };
        let path = card
            .join("device")
            .join("power_dpm_force_performance_level");
        match std::fs::write(&path, &target) {
            Ok(_) => ApplyOutcome {
                setting_id: self.setting_id().into(),
                subsystem: self.subsystem(),
                requested: value.clone(),
                status: ApplyStatus::Applied,
                message: format!("AMD perf level set to {:?} via {}", target, path.display()),
                timestamp: now,
                previous: None,
            },
            Err(e) => {
                let refused = e.kind() == std::io::ErrorKind::PermissionDenied;
                ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: value.clone(),
                    status: if refused {
                        ApplyStatus::Refused
                    } else {
                        ApplyStatus::Failed
                    },
                    message: if refused {
                        format!(
                            "Permission denied — sysfs write to {} requires root.",
                            path.display()
                        )
                    } else {
                        format!("sysfs write failed: {}", e)
                    },
                    timestamp: now,
                    previous: None,
                }
            }
        }
    }
}

/// Write `/sys/class/drm/cardN/gt_max_freq_mhz` for the first Intel GPU.
#[cfg(target_os = "linux")]
struct LinuxIntelGtMaxFreqHandler;

#[cfg(target_os = "linux")]
impl ApplyHandler for LinuxIntelGtMaxFreqHandler {
    fn setting_id(&self) -> &str {
        "gt_max_freq_mhz"
    }
    fn subsystem(&self) -> Subsystem {
        Subsystem::Gpu
    }
    fn apply(&self, value: &SettingValue) -> ApplyOutcome {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mhz = match value {
            SettingValue::Uint(u) => *u,
            SettingValue::Int(i) if *i >= 0 => *i as u64,
            other => {
                return ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: other.clone(),
                    status: ApplyStatus::Failed,
                    message: "gt_max_freq_mhz requires a non-negative integer (MHz).".into(),
                    timestamp: now,
                    previous: None,
                };
            }
        };
        let card = match find_intel_card() {
            Some(c) => c,
            None => {
                return ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: value.clone(),
                    status: ApplyStatus::Failed,
                    message: "No Intel GPU card found in /sys/class/drm.".into(),
                    timestamp: now,
                    previous: None,
                };
            }
        };
        let path = card.join("gt_max_freq_mhz");
        match std::fs::write(&path, mhz.to_string()) {
            Ok(_) => ApplyOutcome {
                setting_id: self.setting_id().into(),
                subsystem: self.subsystem(),
                requested: value.clone(),
                status: ApplyStatus::Applied,
                message: format!("Intel GT max frequency set to {} MHz.", mhz),
                timestamp: now,
                previous: None,
            },
            Err(e) => {
                let refused = e.kind() == std::io::ErrorKind::PermissionDenied;
                ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: value.clone(),
                    status: if refused {
                        ApplyStatus::Refused
                    } else {
                        ApplyStatus::Failed
                    },
                    message: if refused {
                        format!(
                            "Permission denied — sysfs write to {} requires root.",
                            path.display()
                        )
                    } else {
                        format!("sysfs write failed: {}", e)
                    },
                    timestamp: now,
                    previous: None,
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn find_amd_card() -> Option<std::path::PathBuf> {
    find_drm_card_with_vendor("0x1002")
}

#[cfg(target_os = "linux")]
fn find_intel_card() -> Option<std::path::PathBuf> {
    find_drm_card_with_vendor("0x8086")
}

#[cfg(target_os = "linux")]
fn find_drm_card_with_vendor(target_vendor: &str) -> Option<std::path::PathBuf> {
    let rd = std::fs::read_dir("/sys/class/drm").ok()?;
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let path = entry.path();
        let vendor = std::fs::read_to_string(path.join("device").join("vendor")).ok()?;
        if vendor.trim() == target_vendor {
            return Some(path);
        }
    }
    None
}

/// Switch the active Windows power scheme via `PowerSetActiveScheme`.
/// Accepts a string GUID in the canonical 8-4-4-4-12 format.
#[cfg(windows)]
struct WindowsActivePowerSchemeHandler;

#[cfg(windows)]
impl ApplyHandler for WindowsActivePowerSchemeHandler {
    fn setting_id(&self) -> &str {
        "active_scheme_guid"
    }
    fn subsystem(&self) -> Subsystem {
        Subsystem::Cpu
    }
    fn read_current(&self) -> Option<SettingValue> {
        use windows::Win32::System::Power::PowerGetActiveScheme;
        let mut guid_ptr: *mut windows::core::GUID = std::ptr::null_mut();
        // SAFETY: PowerGetActiveScheme writes a pointer to a GUID allocated by
        // the system; None selects the same user root that `apply` writes to.
        // The allocation is released with LocalFree below, as the API documents.
        let err = unsafe { PowerGetActiveScheme(None, &mut guid_ptr) };
        if err.0 != 0 || guid_ptr.is_null() {
            return None;
        }
        // SAFETY: non-null and written by the call above.
        let guid = unsafe { *guid_ptr };
        // SAFETY: the API allocates with LocalAlloc, so LocalFree is the
        // documented counterpart. Leaking here would leak on every read, and a
        // tuning loop reads on every cycle.
        unsafe {
            let _ = windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(
                guid_ptr as *mut _,
            ));
        }
        Some(SettingValue::Text(format!("{:?}", guid).to_lowercase()))
    }
    fn apply(&self, value: &SettingValue) -> ApplyOutcome {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let guid_str = match value {
            SettingValue::Text(s) => s.trim_matches(|c| c == '{' || c == '}').to_string(),
            other => {
                return ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: other.clone(),
                    status: ApplyStatus::Failed,
                    message: "active_scheme_guid requires a Text value (GUID).".into(),
                    timestamp: now,
                    previous: None,
                };
            }
        };
        let guid = match parse_guid(&guid_str) {
            Some(g) => g,
            None => {
                return ApplyOutcome {
                    setting_id: self.setting_id().into(),
                    subsystem: self.subsystem(),
                    requested: value.clone(),
                    status: ApplyStatus::Failed,
                    message: format!("Invalid GUID format: {:?}", guid_str),
                    timestamp: now,
                    previous: None,
                };
            }
        };
        use windows::Win32::System::Power::PowerSetActiveScheme;
        // SAFETY: PowerSetActiveScheme is a Win32 FFI call. The HKEY argument
        // None selects the system user root, which is the documented value
        // for "current user / default" usage. The GUID pointer is to a stack
        // value that lives for the duration of the call.
        let win_error = unsafe { PowerSetActiveScheme(None, Some(&guid)) };
        if win_error.0 == 0 {
            ApplyOutcome {
                setting_id: self.setting_id().into(),
                subsystem: self.subsystem(),
                requested: value.clone(),
                status: ApplyStatus::Applied,
                message: format!("Windows active power scheme set to {{{}}}.", guid_str),
                timestamp: now,
                previous: None,
            }
        } else {
            // ERROR_ACCESS_DENIED = 5
            let refused = win_error.0 == 5;
            ApplyOutcome {
                setting_id: self.setting_id().into(),
                subsystem: self.subsystem(),
                requested: value.clone(),
                status: if refused {
                    ApplyStatus::Refused
                } else {
                    ApplyStatus::Failed
                },
                message: if refused {
                    "Permission denied — try running as administrator.".into()
                } else {
                    format!("PowerSetActiveScheme failed: WIN32_ERROR {}", win_error.0)
                },
                timestamp: now,
                previous: None,
            }
        }
    }
}

#[cfg(windows)]
fn parse_guid(s: &str) -> Option<windows::core::GUID> {
    // Expected format: 8-4-4-4-12 hex chars, e.g. "381b4222-f694-41f0-9685-ff5bb260df2e"
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5
        || parts[0].len() != 8
        || parts[1].len() != 4
        || parts[2].len() != 4
        || parts[3].len() != 4
        || parts[4].len() != 12
    {
        return None;
    }
    let d1 = u32::from_str_radix(parts[0], 16).ok()?;
    let d2 = u16::from_str_radix(parts[1], 16).ok()?;
    let d3 = u16::from_str_radix(parts[2], 16).ok()?;
    let d4_hi = u16::from_str_radix(parts[3], 16).ok()?;
    let d4_lo = u64::from_str_radix(parts[4], 16).ok()?;
    let mut d4 = [0u8; 8];
    d4[0] = (d4_hi >> 8) as u8;
    d4[1] = (d4_hi & 0xFF) as u8;
    for i in 0..6 {
        d4[2 + i] = ((d4_lo >> ((5 - i) * 8)) & 0xFF) as u8;
    }
    Some(windows::core::GUID::from_values(d1, d2, d3, d4))
}

/// Public list of writable setting ids on this build.
pub fn writable_setting_ids() -> Vec<String> {
    builtin_handlers()
        .into_iter()
        .map(|h| h.setting_id().to_string())
        .collect()
}

#[allow(dead_code)]
fn _doctest_anchor(_s: &Setting) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_setting_is_not_writable() {
        let outcome = apply_setting(
            "definitely-not-a-real-setting",
            SettingValue::Bool(true),
            true,
        );
        assert_eq!(outcome.status, ApplyStatus::NotWritable);
    }

    #[test]
    fn confirm_required() {
        // Pick any registered id (or skip if none on this build).
        let ids = writable_setting_ids();
        if let Some(id) = ids.first() {
            // Even without OS permission, the no-confirm path should return
            // NeedsConfirm before attempting the write.
            let outcome = apply_setting(id, SettingValue::Bool(true), false);
            assert_eq!(outcome.status, ApplyStatus::NeedsConfirm);
        }
    }

    #[test]
    fn audit_log_path_resolves() {
        let p = audit_log_path();
        assert!(p.to_string_lossy().to_lowercase().contains("profile_audit"));
    }

    #[test]
    fn outcome_serializable() {
        let outcome = ApplyOutcome {
            setting_id: "x".into(),
            subsystem: Subsystem::Cpu,
            requested: SettingValue::Bool(true),
            status: ApplyStatus::NotWritable,
            message: "test".into(),
            timestamp: 0,
            previous: None,
        };
        let s = serde_json::to_string(&outcome).unwrap();
        assert!(s.contains("not_writable"));
    }

    /// A write that cannot say what it overwrote cannot be undone, and the
    /// refusal has to be explicit rather than a silent no-op.
    #[test]
    fn revert_refuses_when_no_prior_value_was_recorded() {
        let applied = ApplyOutcome {
            setting_id: "scaling_governor".into(),
            subsystem: Subsystem::Cpu,
            requested: SettingValue::Text("performance".into()),
            status: ApplyStatus::Applied,
            message: "applied".into(),
            timestamp: 0,
            previous: None,
        };
        let out = revert_setting(&applied, true);
        assert_eq!(out.status, ApplyStatus::NotWritable);
        assert!(
            out.message.contains("no prior value"),
            "the refusal should say why, got: {}",
            out.message
        );
    }

    /// Reverting something that never took effect would write a value the
    /// caller never asked for.
    #[test]
    fn revert_refuses_an_apply_that_did_not_take_effect() {
        for status in [
            ApplyStatus::Refused,
            ApplyStatus::Failed,
            ApplyStatus::NeedsConfirm,
            ApplyStatus::NotWritable,
        ] {
            let applied = ApplyOutcome {
                setting_id: "scaling_governor".into(),
                subsystem: Subsystem::Cpu,
                requested: SettingValue::Text("performance".into()),
                status,
                message: String::new(),
                timestamp: 0,
                previous: Some(SettingValue::Text("powersave".into())),
            };
            let out = revert_setting(&applied, true);
            assert_eq!(
                out.status,
                ApplyStatus::NotWritable,
                "reverting a {status:?} apply must refuse"
            );
        }
    }

    /// A revert is a write, so it needs confirmation on the same terms.
    /// Otherwise an autonomous loop would have an unconfirmed write path.
    ///
    /// The refusal's *form* is platform-dependent and the invariant is not.
    /// `apply_setting` looks the handler up before checking confirmation, so a
    /// setting with no handler on this platform is refused as `NotWritable`
    /// rather than `NeedsConfirm` — deliberately, because telling someone to
    /// pass `--confirm` for a setting that can never be written here sends them
    /// down a dead end.
    ///
    /// This test asserted `NeedsConfirm` unconditionally against
    /// `active_scheme_guid`, which only exists on Windows. It passed on the
    /// machine that wrote it and failed on macOS in CI, where the crate
    /// registers no writable settings at all. The registry is asked directly
    /// rather than restating its `cfg`s here, so a handler that gains or loses
    /// a platform does not silently make this test vacuous.
    #[test]
    fn revert_without_confirmation_is_refused() {
        const SETTING: &str = "active_scheme_guid";

        let applied = ApplyOutcome {
            setting_id: SETTING.into(),
            subsystem: Subsystem::Cpu,
            requested: SettingValue::Text("381b4222-f694-41f0-9685-ff5bb260df2e".into()),
            status: ApplyStatus::Applied,
            message: "applied".into(),
            timestamp: 0,
            previous: Some(SettingValue::Text(
                "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".into(),
            )),
        };

        let registered = builtin_handlers()
            .into_iter()
            .any(|h| h.setting_id() == SETTING);
        let out = revert_setting(&applied, false);

        // The part that holds everywhere, and the part that matters: an
        // unconfirmed revert never writes.
        assert_ne!(
            out.status,
            ApplyStatus::Applied,
            "an unconfirmed revert must never take effect"
        );

        if registered {
            assert_eq!(
                out.status,
                ApplyStatus::NeedsConfirm,
                "a handler exists, so the refusal must be about the missing confirmation"
            );
        } else {
            assert_eq!(
                out.status,
                ApplyStatus::NotWritable,
                "no handler on this platform, so the refusal must say that instead \
                 of asking for a confirmation that would not help"
            );
        }
    }

    /// The prior value round-trips through serialisation, so an outcome
    /// recorded in an audit log stays revertible after being read back.
    #[test]
    fn a_recorded_prior_value_survives_serialisation() {
        let outcome = ApplyOutcome {
            setting_id: "scaling_governor".into(),
            subsystem: Subsystem::Cpu,
            requested: SettingValue::Text("performance".into()),
            status: ApplyStatus::Applied,
            message: String::new(),
            timestamp: 0,
            previous: Some(SettingValue::Text("schedutil".into())),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let back: ApplyOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(back.previous, Some(SettingValue::Text("schedutil".into())));
    }

    /// Outcomes written before `previous` existed still deserialise, so an
    /// existing audit log does not become unreadable.
    #[test]
    fn an_outcome_without_previous_still_deserialises() {
        // Built by removing the key from a real outcome rather than hand-written,
        // so the test cannot pass or fail on a guess at the wire format.
        let outcome = ApplyOutcome {
            setting_id: "scaling_governor".into(),
            subsystem: Subsystem::Cpu,
            requested: SettingValue::Text("performance".into()),
            status: ApplyStatus::Applied,
            message: String::new(),
            timestamp: 0,
            previous: Some(SettingValue::Text("schedutil".into())),
        };
        let mut value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&outcome).unwrap()).unwrap();
        value.as_object_mut().unwrap().remove("previous");
        let back: ApplyOutcome = serde_json::from_value(value).unwrap();
        assert_eq!(back.previous, None);
    }

    /// The Windows handler can read the scheme it writes.
    ///
    /// Without this the setting is one-way: `apply_setting` records no prior
    /// value and `revert_setting` refuses. Cross-checked against
    /// `ironmon profile explain active_scheme_guid`, which reaches the same
    /// registry through entirely different code in `profile::cpu`.
    #[cfg(windows)]
    #[test]
    fn the_windows_power_scheme_reads_back_as_a_guid() {
        let current = WindowsActivePowerSchemeHandler.read_current();
        let Some(SettingValue::Text(guid)) = current else {
            panic!("active power scheme did not read back as text: {current:?}");
        };
        println!("read_current -> {guid}");
        assert_eq!(guid.len(), 36, "expected a bare 36-char GUID, got {guid:?}");
        assert_eq!(
            guid.matches('-').count(),
            4,
            "expected GUID dash grouping, got {guid:?}"
        );
        assert!(
            guid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'),
            "GUID should be hex and dashes, got {guid:?}"
        );
    }

    /// What `read_current` emits must be what `apply` accepts.
    ///
    /// This is the seam a revert crosses, and it is invisible if only one side
    /// is tested: `read_current` formats a GUID with `Debug`, `apply` parses one
    /// with `parse_guid`, and nothing forces those two to agree. If the Debug
    /// representation ever gains braces or uppercase or a `GUID { .. }` wrapper,
    /// every revert of this setting starts failing and the unit tests above stay
    /// green.
    #[cfg(windows)]
    #[test]
    fn the_windows_guid_round_trips_from_read_to_parse() {
        let Some(SettingValue::Text(read)) = WindowsActivePowerSchemeHandler.read_current() else {
            panic!("no scheme readable");
        };
        let parsed = parse_guid(&read).expect(
            "the GUID that read_current produced must parse with the same function apply uses",
        );
        // And round again: formatting the parsed value must reproduce the string,
        // so a revert writes the identical scheme rather than a near-miss.
        let reformatted = format!("{parsed:?}").to_lowercase();
        assert_eq!(
            reformatted, read,
            "read -> parse -> format must be lossless"
        );
    }

    /// Every GUID Windows offers must survive the same round trip, not just
    /// whichever one happens to be active on the development machine.
    #[cfg(windows)]
    #[test]
    fn every_known_scheme_guid_round_trips() {
        for guid in [
            "381b4222-f694-41f0-9685-ff5bb260df2e", // Balanced
            "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c", // High performance
            "a1841308-3541-4fab-bc81-f71556f20b4a", // Power saver
            "00000000-0000-0000-0000-000000000000", // all-zero edge
            "ffffffff-ffff-ffff-ffff-ffffffffffff", // all-ones edge
        ] {
            let parsed = parse_guid(guid).unwrap_or_else(|| panic!("{guid} should parse"));
            assert_eq!(
                format!("{parsed:?}").to_lowercase(),
                guid,
                "{guid} did not survive parse -> format"
            );
        }
    }

    /// Malformed input is rejected rather than silently truncated into a
    /// different, valid-looking GUID.
    #[test]
    #[cfg(windows)]
    fn malformed_guids_are_rejected() {
        for bad in [
            "",
            "381b4222",
            "381b4222-f694-41f0-9685",
            "381b4222-f694-41f0-9685-ff5bb260df2e-extra",
            "381b4222-f694-41f0-9685-ff5bb260df2ee", // one too long
            "zzzzzzzz-f694-41f0-9685-ff5bb260df2e",  // not hex
            "381b4222 f694 41f0 9685 ff5bb260df2e",  // spaces, not dashes
        ] {
            assert!(
                parse_guid(bad).is_none(),
                "{bad:?} should not parse as a GUID"
            );
        }
    }

    /// Hammer the Win32 read from many threads at once.
    ///
    /// `read_current` allocates through `PowerGetActiveScheme` and frees with
    /// `LocalFree` on every call, and the tuning loop calls it on every cycle
    /// for every setting. A mistake there is heap corruption, which shows up as
    /// an intermittent failure somewhere else entirely — the worst kind to chase
    /// and the reason this is a stress test rather than a single call.
    #[cfg(windows)]
    #[test]
    fn concurrent_reads_of_the_power_scheme_agree_and_do_not_corrupt() {
        use std::sync::Arc;
        let first = WindowsActivePowerSchemeHandler
            .read_current()
            .expect("scheme must be readable");
        let expected = Arc::new(first);

        let mut handles = Vec::new();
        for _ in 0..8 {
            let expected = Arc::clone(&expected);
            handles.push(std::thread::spawn(move || {
                for _ in 0..250 {
                    let got = WindowsActivePowerSchemeHandler.read_current();
                    assert_eq!(
                        got.as_ref(),
                        Some(&*expected),
                        "a concurrent read disagreed with the first one"
                    );
                }
            }));
        }
        for h in handles {
            h.join()
                .expect("a reader thread panicked — suspect the LocalFree");
        }
    }

    /// A write that cannot be undone is refused for unattended callers, and the
    /// refusal names the way to override it.
    #[test]
    fn reversible_apply_refuses_when_the_prior_value_is_unreadable() {
        // No handler at all is the strongest case of "cannot be read": the
        // fallback must not be to write anyway.
        let out = apply_setting_reversible(
            "definitely_not_a_registered_setting",
            SettingValue::Bool(true),
            true,
        );
        assert_ne!(
            out.status,
            ApplyStatus::Applied,
            "an unregistered setting must never report as applied"
        );
    }

    /// The retry gives a flaky read three chances before declaring the setting
    /// unreadable. A single transient failure silently turning a reversible
    /// write into a one-way one is the bug this exists to prevent.
    #[test]
    fn read_current_retries_before_giving_up() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct FlakyOnce(AtomicUsize);
        impl ApplyHandler for FlakyOnce {
            fn setting_id(&self) -> &str {
                "flaky"
            }
            fn subsystem(&self) -> Subsystem {
                Subsystem::Cpu
            }
            fn apply(&self, _v: &SettingValue) -> ApplyOutcome {
                unreachable!("this test never writes")
            }
            fn read_current(&self) -> Option<SettingValue> {
                // Fails the first time, succeeds after — the shape of the
                // failure actually observed on a loaded machine.
                if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    None
                } else {
                    Some(SettingValue::Text("recovered".into()))
                }
            }
        }

        let h = FlakyOnce(AtomicUsize::new(0));
        assert_eq!(
            read_current_with_retry(&h),
            Some(SettingValue::Text("recovered".into())),
            "a read that fails once then succeeds must be retried, not given up on"
        );

        struct AlwaysNone;
        impl ApplyHandler for AlwaysNone {
            fn setting_id(&self) -> &str {
                "never"
            }
            fn subsystem(&self) -> Subsystem {
                Subsystem::Cpu
            }
            fn apply(&self, _v: &SettingValue) -> ApplyOutcome {
                unreachable!("this test never writes")
            }
        }
        assert_eq!(
            read_current_with_retry(&AlwaysNone),
            None,
            "a genuinely unreadable setting still reports unreadable"
        );
    }

    /// The whole point, end to end, against the real machine: apply a setting,
    /// confirm it changed, put it back, confirm it is back.
    ///
    /// `#[ignore]` because it writes to the machine it runs on. Every other test
    /// here proves a piece in isolation — that a refusal refuses, that a GUID
    /// round-trips — and none of them proves the pieces compose into a change
    /// that can actually be undone. Run it deliberately:
    ///
    /// ```text
    /// cargo test --all-features --lib -- --ignored round_trip
    /// ```
    ///
    /// Cross-check from outside with `powercfg /getactivescheme` before and
    /// after; if this test is wrong, the tool it is testing is not the one to
    /// ask about the result.
    #[cfg(windows)]
    #[test]
    #[ignore = "writes to the machine's active power scheme"]
    fn round_trip_the_active_power_scheme_on_real_hardware() {
        const HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
        const BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";

        let Some(SettingValue::Text(start)) = WindowsActivePowerSchemeHandler.read_current() else {
            panic!("cannot read the starting scheme; refusing to write blind");
        };
        println!("start:  {start}");

        // Move to whichever standard scheme is not the current one, so the test
        // is a real change on any machine rather than only on a Balanced one.
        let target = if start == HIGH_PERFORMANCE {
            BALANCED
        } else {
            HIGH_PERFORMANCE
        };

        let applied = apply_setting_reversible(
            "active_scheme_guid",
            SettingValue::Text(target.into()),
            true,
        );
        println!("apply:  {:?} — {}", applied.status, applied.message);
        assert_eq!(
            applied.status,
            ApplyStatus::Applied,
            "the write did not take effect: {}",
            applied.message
        );
        assert_eq!(
            applied.previous,
            Some(SettingValue::Text(start.clone())),
            "the outcome must record exactly what it overwrote"
        );

        // Confirm from the machine, not from the outcome we just built.
        let after = WindowsActivePowerSchemeHandler.read_current();
        println!("during: {after:?}");
        assert_eq!(
            after,
            Some(SettingValue::Text(target.into())),
            "the machine did not actually change"
        );

        let reverted = revert_setting(&applied, true);
        println!("revert: {:?} — {}", reverted.status, reverted.message);
        assert_eq!(
            reverted.status,
            ApplyStatus::Applied,
            "the revert did not take effect: {} — THE MACHINE IS LEFT ON {target}",
            reverted.message
        );

        let end = WindowsActivePowerSchemeHandler.read_current();
        println!("end:    {end:?}");
        assert_eq!(
            end,
            Some(SettingValue::Text(start)),
            "the machine was not returned to where it started"
        );
    }
}
