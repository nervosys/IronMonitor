//! Hardware Profile Inspector — read-only inspection of vendor driver settings,
//! application profiles, and tunable hardware parameters.
//!
//! Inspired by [NVIDIA Profile Inspector](https://github.com/Orbmu2k/nvidiaProfileInspector),
//! Intel XTU, AMD Ryzen Master, Radeon Adrenalin, and `nvme-cli`, this module
//! exposes a unified, vendor-neutral view of "settings you can usually tweak"
//! across five subsystems:
//!
//! | Subsystem | Inspected sources |
//! |-----------|-------------------|
//! | `Gpu`     | NVIDIA NVAPI driver profile registry, AMD AMDKMDAG/UMD registry, Intel IGCL registry, Linux sysfs |
//! | `Cpu`     | MSR (Linux `/dev/cpu/N/msr`), RAPL, cpufreq governor/EPP, voltage offset state |
//! | `Nvme`    | Linux `/sys/class/nvme/*` feature attrs, Windows `IOCTL_STORAGE_PROTOCOL_COMMAND` admin pass-through |
//! | `Display` | Refresh rate options, HDR mode, color profile path, scale factor |
//! | `Memory`  | DIMM SPD XMP/EXPO profiles, active timings vs JEDEC |
//!
//! ## Design
//!
//! All providers are **read-only** in this initial pass. The API is built to
//! be extended with `apply()` semantics later (gated by elevation + an audit
//! log + per-setting safety classification), but no provider mutates state
//! today. This mirrors the safe foundation pattern used by the existing
//! [`crate::fan_control`] and [`crate::cpufreq`] modules.
//!
//! ## Example
//!
//! ```no_run
//! use ironmonlib::profile::{ProfileInspector, Subsystem};
//!
//! let mut inspector = ProfileInspector::new();
//! let snapshot = inspector.snapshot_all();
//!
//! for (subsystem, providers) in &snapshot.providers {
//!     println!("== {} ==", subsystem);
//!     for provider in providers {
//!         println!("  {} ({})", provider.display_name, provider.source);
//!         for setting in &provider.settings {
//!             println!("    {} = {}", setting.id, setting.value);
//!         }
//!     }
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

pub mod active;
pub mod apply;
pub mod bench;
pub mod cache;
pub mod cpu;
pub mod deviation;
pub mod diff;
pub mod display;
pub mod edid;
pub mod explain;
pub mod gpu;
pub mod memory;
#[cfg(windows)]
pub mod nvidia_drs;
pub mod nvme;
pub mod nvme_features;
pub mod spd_xmp;

/// Top-level hardware subsystems exposed by the profile inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Subsystem {
    /// GPU driver profiles (NVAPI / AMD / Intel)
    Gpu,
    /// CPU tuning (XTU / Ryzen Master / cpufreq)
    Cpu,
    /// NVMe controller parameters
    Nvme,
    /// Display modes and color profiles
    Display,
    /// Memory (DIMM XMP/EXPO)
    Memory,
}

impl Subsystem {
    pub const ALL: &'static [Subsystem] = &[
        Subsystem::Gpu,
        Subsystem::Cpu,
        Subsystem::Nvme,
        Subsystem::Display,
        Subsystem::Memory,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Subsystem::Gpu => "gpu",
            Subsystem::Cpu => "cpu",
            Subsystem::Nvme => "nvme",
            Subsystem::Display => "display",
            Subsystem::Memory => "memory",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "gpu" => Some(Subsystem::Gpu),
            "cpu" => Some(Subsystem::Cpu),
            "nvme" | "storage" => Some(Subsystem::Nvme),
            "display" => Some(Subsystem::Display),
            "memory" | "dimm" | "ram" => Some(Subsystem::Memory),
            _ => None,
        }
    }
}

impl fmt::Display for Subsystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Classification of how risky it would be to *write* this setting. Today the
/// inspector is read-only; this hint exists so that a future `apply()` layer
/// can require elevation and explicit user confirmation for anything beyond
/// `Safe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingRisk {
    /// Read-only metadata (no write semantics defined).
    Informational,
    /// Reversible toggle with no stability impact (e.g. per-app V-Sync).
    Safe,
    /// Affects driver/runtime behavior; reversible on reboot.
    Moderate,
    /// Touches power/thermal/voltage/MSR; can destabilize or damage hardware.
    Dangerous,
}

/// Storage type for a setting's value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SettingValue {
    /// Boolean toggle (on/off).
    Bool(bool),
    /// Signed integer (e.g. clock offset in MHz, voltage in mV).
    Int(i64),
    /// Unsigned integer (e.g. NVAPI DWORD setting).
    Uint(u64),
    /// Floating-point (e.g. fractional scale).
    Float(f64),
    /// String / enum-as-string (e.g. "performance", "DLSS Quality").
    Text(String),
    /// Hex blob (e.g. NVAPI binary settings, raw SPD bytes).
    Hex(String),
    /// Setting exists but the value is unreadable on this platform/permission.
    Unreadable(String),
}

impl fmt::Display for SettingValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingValue::Bool(b) => write!(f, "{}", b),
            SettingValue::Int(i) => write!(f, "{}", i),
            SettingValue::Uint(u) => write!(f, "{} (0x{:x})", u, u),
            SettingValue::Float(x) => write!(f, "{}", x),
            SettingValue::Text(s) => write!(f, "{}", s),
            SettingValue::Hex(h) => write!(f, "{}", h),
            SettingValue::Unreadable(reason) => write!(f, "<unreadable: {}>", reason),
        }
    }
}

/// A single inspectable setting. The shape mirrors NVPI's "setting" rows:
/// an ID (vendor-stable), a display name, the current value, and metadata
/// describing the value space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    /// Stable, machine-readable identifier (e.g. "0x10094F1F" for an NVAPI DWORD,
    /// or "power_limit_mw" for an AMD sysfs attr).
    pub id: String,
    /// Human-readable name.
    pub display_name: String,
    /// Current value.
    pub value: SettingValue,
    /// Optional unit suffix (W, MHz, mV, °C, %).
    pub unit: Option<String>,
    /// Optional default value reported by the driver / firmware.
    pub default: Option<SettingValue>,
    /// Optional enumerated choices (display name -> raw value).
    pub choices: Option<Vec<(String, SettingValue)>>,
    /// Optional numeric range (min, max) for Int/Uint/Float settings.
    pub range: Option<(SettingValue, SettingValue)>,
    /// Optional free-form description (driver doc string, manpage line).
    pub description: Option<String>,
    /// Write-risk classification (for future apply() support).
    pub risk: SettingRisk,
    /// Where this setting was read from (e.g. registry path, sysfs file).
    pub source: String,
    /// Whether this setting can be modified via [`apply::apply_setting`].
    /// Defaults to `false`; providers opt individual settings in.
    #[serde(default)]
    pub writable: bool,
}

impl Setting {
    pub fn info(id: impl Into<String>, name: impl Into<String>, value: SettingValue) -> Self {
        Self {
            id: id.into(),
            display_name: name.into(),
            value,
            unit: None,
            default: None,
            choices: None,
            range: None,
            description: None,
            risk: SettingRisk::Informational,
            source: String::new(),
            writable: false,
        }
    }

    pub fn with_writable(mut self, writable: bool) -> Self {
        self.writable = writable;
        self
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    pub fn with_source(mut self, src: impl Into<String>) -> Self {
        self.source = src.into();
        self
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_risk(mut self, risk: SettingRisk) -> Self {
        self.risk = risk;
        self
    }
}

/// A logical group of settings (one device, one application profile, one DIMM).
///
/// Examples:
/// - `("NVIDIA RTX 4090", "Global", [...])`
/// - `("NVIDIA RTX 4090", "Cyberpunk 2077", [...])`
/// - `("CPU0 — Intel Core i9-13900K", "Active state", [...])`
/// - `("NVMe nvme0n1 — Samsung 990 Pro 2TB", "Features", [...])`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileGroup {
    /// Subsystem this group belongs to.
    pub subsystem: Subsystem,
    /// Device or scope name (GPU model, NVMe path, DIMM slot, etc.).
    pub device: String,
    /// Profile / scope name. For NVPI-style data this is the app profile name
    /// ("Global", "Cyberpunk 2077.exe"); for other subsystems often "Default".
    pub display_name: String,
    /// Where the data came from (e.g. "registry: HKLM\\SOFTWARE\\NVIDIA...",
    /// "/sys/class/nvme/nvme0/", "MSR 0x610 (RAPL_POWER_LIMIT)").
    pub source: String,
    /// Settings in this profile.
    pub settings: Vec<Setting>,
    /// Free-form notes (capability hints, permission caveats).
    pub notes: Vec<String>,
}

impl ProfileGroup {
    pub fn new(
        subsystem: Subsystem,
        device: impl Into<String>,
        display_name: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            subsystem,
            device: device.into(),
            display_name: display_name.into(),
            source: source.into(),
            settings: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn push(&mut self, setting: Setting) {
        self.settings.push(setting);
    }

    pub fn note(&mut self, msg: impl Into<String>) {
        self.notes.push(msg.into());
    }
}

/// Trait implemented by each subsystem provider.
pub trait ProfileProvider: Send {
    fn subsystem(&self) -> Subsystem;
    /// Snapshot all profile groups exposed by this provider. Must not panic;
    /// platform/permission failures should be returned as empty results or
    /// surfaced via `ProfileGroup::notes`.
    fn snapshot(&mut self) -> Vec<ProfileGroup>;
}

/// Aggregate snapshot of all subsystems.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSnapshot {
    /// Unix timestamp (seconds) when the snapshot was taken.
    pub timestamp: u64,
    /// Profile groups keyed by subsystem.
    pub providers: BTreeMap<Subsystem, Vec<ProfileGroup>>,
    /// Subsystems that errored out entirely with a short reason.
    pub errors: BTreeMap<Subsystem, String>,
}

impl ProfileSnapshot {
    pub fn total_settings(&self) -> usize {
        self.providers
            .values()
            .flat_map(|groups| groups.iter())
            .map(|g| g.settings.len())
            .sum()
    }

    pub fn total_groups(&self) -> usize {
        self.providers.values().map(|g| g.len()).sum()
    }

    /// Search settings across all subsystems by case-insensitive substring
    /// match against id, display name, description, or value.
    pub fn search(&self, needle: &str) -> Vec<(Subsystem, &ProfileGroup, &Setting)> {
        let needle = needle.to_ascii_lowercase();
        let mut out = Vec::new();
        for (sub, groups) in &self.providers {
            for group in groups {
                for setting in &group.settings {
                    let hay = format!(
                        "{} {} {} {}",
                        setting.id,
                        setting.display_name,
                        setting.description.as_deref().unwrap_or(""),
                        setting.value,
                    )
                    .to_ascii_lowercase();
                    if hay.contains(&needle) {
                        out.push((*sub, group, setting));
                    }
                }
            }
        }
        out
    }
}

/// Top-level inspector that fans out to each subsystem provider.
pub struct ProfileInspector {
    providers: Vec<Box<dyn ProfileProvider>>,
}

impl ProfileInspector {
    /// Construct an inspector with the default set of providers for the
    /// current platform.
    pub fn new() -> Self {
        let providers: Vec<Box<dyn ProfileProvider>> = vec![
            Box::new(gpu::GpuProfileProvider::new()),
            Box::new(cpu::CpuProfileProvider::new()),
            Box::new(nvme::NvmeProfileProvider::new()),
            Box::new(display::DisplayProfileProvider::new()),
            Box::new(memory::MemoryProfileProvider::new()),
        ];
        Self { providers }
    }

    pub fn with_providers(providers: Vec<Box<dyn ProfileProvider>>) -> Self {
        Self { providers }
    }

    /// Take a snapshot of all configured providers.
    pub fn snapshot_all(&mut self) -> ProfileSnapshot {
        let mut providers = BTreeMap::new();
        let errors = BTreeMap::new();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        for p in self.providers.iter_mut() {
            let sub = p.subsystem();
            let groups = p.snapshot();
            providers.entry(sub).or_insert_with(Vec::new).extend(groups);
        }

        ProfileSnapshot {
            timestamp,
            providers,
            errors,
        }
    }

    /// Snapshot a single subsystem.
    pub fn snapshot(&mut self, subsystem: Subsystem) -> Vec<ProfileGroup> {
        let mut out = Vec::new();
        for p in self.providers.iter_mut() {
            if p.subsystem() == subsystem {
                out.extend(p.snapshot());
            }
        }
        out
    }
}

impl Default for ProfileInspector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsystem_roundtrip() {
        for s in Subsystem::ALL {
            assert_eq!(Subsystem::parse(s.as_str()), Some(*s));
        }
        assert_eq!(Subsystem::parse("RAM"), Some(Subsystem::Memory));
        assert_eq!(Subsystem::parse("storage"), Some(Subsystem::Nvme));
        assert_eq!(Subsystem::parse("nonsense"), None);
    }

    #[test]
    fn snapshot_total_counts() {
        let mut snap = ProfileSnapshot {
            timestamp: 0,
            providers: BTreeMap::new(),
            errors: BTreeMap::new(),
        };
        let mut g = ProfileGroup::new(Subsystem::Gpu, "GPU0", "Global", "test");
        g.push(Setting::info("k1", "Key 1", SettingValue::Bool(true)));
        g.push(Setting::info("k2", "Key 2", SettingValue::Uint(42)));
        snap.providers.insert(Subsystem::Gpu, vec![g]);
        assert_eq!(snap.total_groups(), 1);
        assert_eq!(snap.total_settings(), 2);
        assert_eq!(snap.search("key 2").len(), 1);
        assert_eq!(snap.search("Key").len(), 2);
    }

    #[test]
    fn setting_value_display() {
        assert_eq!(SettingValue::Bool(true).to_string(), "true");
        assert_eq!(SettingValue::Uint(255).to_string(), "255 (0xff)");
        assert_eq!(SettingValue::Text("perf".into()).to_string(), "perf");
        assert!(SettingValue::Unreadable("perm".into())
            .to_string()
            .contains("perm"));
    }

    #[test]
    fn inspector_default_runs() {
        // Smoke test: the inspector must never panic on any platform, even
        // with zero hardware support. Result may legitimately be empty.
        let mut inspector = ProfileInspector::new();
        let _snap = inspector.snapshot_all();
    }
}
