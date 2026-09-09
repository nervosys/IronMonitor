//! GPU profile provider.
//!
//! Inspired by NVIDIA Profile Inspector, AMD Radeon Adrenalin's "Graphics" tab,
//! and Intel Graphics Command Center. The full NVPI experience requires
//! NVAPI-level access to the binary DRS profile database
//! (`%ProgramData%\NVIDIA Corporation\Drs\nvdrsdb*.bin`). That's a substantial
//! unsafe-FFI undertaking and is intentionally out of scope for the initial
//! read-only landing.
//!
//! What we *do* read in this pass:
//!
//! - **NVIDIA**: live NVML driver/version info, persistence mode, ECC mode,
//!   power management limit, default/min/max graphics clocks, power state caps,
//!   and the driver's published kernel-module parameters.
//! - **AMD** (Linux): `/sys/class/drm/cardN/device/` knobs (`power_dpm_state`,
//!   `power_dpm_force_performance_level`, `pp_od_clk_voltage`, fan policy).
//! - **Intel** (Linux): `/sys/class/drm/cardN/gt_*` GT-frequency policy values.
//! - **Windows**: enumerate the NVIDIA / AMD / Intel display-class device
//!   registry keys (`HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-...}\NNNN`)
//!   so that callers can see the driver overrides already present on the system.

// `Setting`, `SettingValue` and `SettingRisk` are used only by the reader bodies
// below, every one of which sits behind a vendor feature. A build with none of
// them keeps the trait plumbing and drops the setting constructors, so the import
// is genuinely unused there. Annotated rather than split with a `cfg`, because
// the condition is the disjunction of every vendor gate in this file and getting
// it wrong breaks a platform this machine cannot compile for.
#[allow(unused_imports)]
use super::{ProfileGroup, ProfileProvider, Setting, SettingRisk, SettingValue, Subsystem};

pub struct GpuProfileProvider {
    _private: (),
}

/// NVML's performance state in the notation the domain uses.
///
/// Gated with its caller. This was written at module scope without the gate and
/// broke the build for every feature set without `nvidia`, because that is what
/// links `nvml_wrapper`. `--all-features` cannot catch that, and neither can the
/// two cross-target checks, which both name `nvidia` in their feature lists.
///
/// `nvml_wrapper` names the variants `Zero`..`Fifteen`, so `{:?}` printed
/// "Performance State = Eight" directly above a description reading "P0 =
/// highest performance, P15 = lowest". The value and its own documentation were
/// in different notations, and only one of them is what the driver, the vendor
/// tooling and every overclocking guide call it.
#[cfg(feature = "nvidia")]
fn pstate_name(p: nvml_wrapper::enum_wrappers::device::PerformanceState) -> Option<String> {
    use nvml_wrapper::enum_wrappers::device::PerformanceState as P;
    match p {
        P::Zero => Some("P0".into()),
        P::One => Some("P1".into()),
        P::Two => Some("P2".into()),
        P::Three => Some("P3".into()),
        P::Four => Some("P4".into()),
        P::Five => Some("P5".into()),
        P::Six => Some("P6".into()),
        P::Seven => Some("P7".into()),
        P::Eight => Some("P8".into()),
        P::Nine => Some("P9".into()),
        P::Ten => Some("P10".into()),
        P::Eleven => Some("P11".into()),
        P::Twelve => Some("P12".into()),
        P::Thirteen => Some("P13".into()),
        P::Fourteen => Some("P14".into()),
        P::Fifteen => Some("P15".into()),
        // NVML says it does not know. Publishing the string "unknown" as the
        // value would be the exact thing the absence-word guard exists to stop:
        // a caller cannot tell it from a state named that. The setting is
        // dropped instead, which is what every other reader here does when it
        // has nothing.
        P::Unknown => None,
    }
}

impl GpuProfileProvider {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for GpuProfileProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileProvider for GpuProfileProvider {
    fn subsystem(&self) -> Subsystem {
        Subsystem::Gpu
    }

    fn snapshot(&mut self) -> Vec<ProfileGroup> {
        let mut groups = Vec::new();

        #[cfg(feature = "nvidia")]
        groups.extend(nvidia_groups());

        #[cfg(all(target_os = "linux", feature = "amd"))]
        groups.extend(amd_linux_groups());

        #[cfg(all(target_os = "linux", feature = "intel"))]
        groups.extend(intel_linux_groups());

        #[cfg(windows)]
        groups.extend(windows_display_class_groups());

        #[cfg(windows)]
        groups.extend(super::nvidia_drs::scan_drs_groups());

        if groups.is_empty() {
            let mut g = ProfileGroup::new(
                Subsystem::Gpu,
                "(no GPU driver readable)",
                "Default",
                "auto-detect",
            );
            g.note("No supported GPU driver surfaces were readable on this platform.");
            g.note("Compile with --features nvidia,amd,intel and run with adequate permissions for full data.");
            groups.push(g);
        }
        groups
    }
}

#[cfg(feature = "nvidia")]
fn nvidia_groups() -> Vec<ProfileGroup> {
    use nvml_wrapper::enum_wrappers::device::{Clock, ClockId};
    use nvml_wrapper::Nvml;

    let Ok(nvml) = Nvml::init() else {
        return Vec::new();
    };
    let count = nvml.device_count().unwrap_or(0);
    let mut groups = Vec::new();

    let driver_version = nvml.sys_driver_version().unwrap_or_else(|_| "?".into());
    let nvml_version = nvml.sys_nvml_version().unwrap_or_else(|_| "?".into());

    for i in 0..count {
        let Ok(dev) = nvml.device_by_index(i) else {
            continue;
        };
        let name = dev.name().unwrap_or_else(|_| format!("NVIDIA GPU {}", i));
        let mut g = ProfileGroup::new(
            Subsystem::Gpu,
            &name,
            "Global driver profile",
            format!("NVML index {}", i),
        );

        g.push(
            Setting::info(
                "driver_version",
                "Driver Version",
                SettingValue::Text(driver_version.clone()),
            )
            .with_source("NVML"),
        );
        g.push(
            Setting::info(
                "nvml_version",
                "NVML Version",
                SettingValue::Text(nvml_version.clone()),
            )
            .with_source("NVML"),
        );

        if let Ok(mode) = dev.is_ecc_enabled() {
            g.push(
                Setting::info(
                    "ecc_mode",
                    "ECC Memory",
                    SettingValue::Bool(mode.currently_enabled),
                )
                .with_description(
                    "Whether ECC error correction is currently active on this GPU's VRAM.",
                )
                .with_risk(SettingRisk::Moderate)
                .with_source("nvmlDeviceIsEccEnabled"),
            );
        }
        #[cfg(target_os = "linux")]
        if let Ok(persistent) = dev.is_in_persistent_mode() {
            g.push(
                Setting::info(
                    "persistence_mode",
                    "Persistence Mode",
                    SettingValue::Bool(persistent),
                )
                .with_description(
                    "When enabled, the NVIDIA driver remains loaded with no active client. Reduces first-launch latency. Writable on Linux with root (NVML).",
                )
                .with_risk(SettingRisk::Safe)
                .with_source("nvmlDeviceGetPersistenceMode")
                .with_writable(true),
            );
        }
        if let Ok(limit_mw) = dev.enforced_power_limit() {
            let mut s = Setting::info(
                "power_limit_mw",
                "Power Limit",
                SettingValue::Uint(limit_mw as u64),
            )
            .with_unit("mW")
            .with_risk(SettingRisk::Moderate)
            .with_source("nvmlDeviceGetEnforcedPowerLimit");
            if let Ok(constraints) = dev.power_management_limit_constraints() {
                s.range = Some((
                    SettingValue::Uint(constraints.min_limit as u64),
                    SettingValue::Uint(constraints.max_limit as u64),
                ));
            }
            if let Ok(default) = dev.power_management_limit_default() {
                s.default = Some(SettingValue::Uint(default as u64));
            }
            g.push(s);
        }
        for (clock, label, id) in [
            (Clock::Graphics, "Max Graphics Clock", "max_gfx_clock_mhz"),
            (Clock::SM, "Max SM Clock", "max_sm_clock_mhz"),
            (Clock::Memory, "Max Memory Clock", "max_mem_clock_mhz"),
        ] {
            if let Ok(v) = dev.max_clock_info(clock) {
                g.push(
                    Setting::info(id, label, SettingValue::Uint(v as u64))
                        .with_unit("MHz")
                        .with_source("nvmlDeviceGetMaxClockInfo"),
                );
            }
        }
        if let Ok(v) = dev.clock(Clock::Graphics, ClockId::Current) {
            g.push(
                Setting::info(
                    "current_gfx_clock_mhz",
                    "Current Graphics Clock",
                    SettingValue::Uint(v as u64),
                )
                .with_unit("MHz")
                .with_source("nvmlDeviceGetClock"),
            );
        }
        if let Some(pstate) = dev.performance_state().ok().and_then(pstate_name) {
            g.push(
                Setting::info(
                    "performance_state",
                    "Performance State",
                    SettingValue::Text(pstate),
                )
                .with_description(
                    "P0 = highest performance, P15 = lowest. The driver picks this automatically.",
                )
                .with_source("nvmlDeviceGetPerformanceState"),
            );
        }
        if let Ok(brand) = dev.brand() {
            g.push(
                Setting::info("brand", "Brand", SettingValue::Text(format!("{:?}", brand)))
                    .with_source("NVML"),
            );
        }
        if let Ok(vbios) = dev.vbios_version() {
            g.push(
                Setting::info("vbios_version", "VBIOS Version", SettingValue::Text(vbios))
                    .with_source("nvmlDeviceGetVbiosVersion"),
            );
        }

        g.note(
            "Per-application profiles (NVPI-style DRS database) require NVAPI access \
            and are not yet implemented. The values shown here are NVML-readable \
            global driver state.",
        );
        groups.push(g);
    }

    if let Some(extra) = nvidia_kmod_params_group() {
        groups.push(extra);
    }
    groups
}

#[cfg(all(feature = "nvidia", target_os = "linux"))]
fn nvidia_kmod_params_group() -> Option<ProfileGroup> {
    let path = std::path::Path::new("/sys/module/nvidia/parameters");
    if !path.exists() {
        return None;
    }
    let mut g = ProfileGroup::new(
        Subsystem::Gpu,
        "NVIDIA kernel module",
        "nvidia.ko parameters",
        path.display().to_string(),
    );
    if let Ok(rd) = std::fs::read_dir(path) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let val = std::fs::read_to_string(entry.path())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|e| format!("<err: {}>", e));
            g.push(
                Setting::info(&name, &name, SettingValue::Text(val))
                    .with_source("/sys/module/nvidia/parameters"),
            );
        }
    }
    Some(g)
}

#[cfg(all(feature = "nvidia", not(target_os = "linux")))]
fn nvidia_kmod_params_group() -> Option<ProfileGroup> {
    None
}

#[cfg(all(target_os = "linux", feature = "amd"))]
fn amd_linux_groups() -> Vec<ProfileGroup> {
    use std::fs;
    let mut groups = Vec::new();
    let drm = match fs::read_dir("/sys/class/drm") {
        Ok(rd) => rd,
        Err(_) => return groups,
    };
    for entry in drm.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let dev_dir = entry.path().join("device");
        let vendor = fs::read_to_string(dev_dir.join("vendor")).unwrap_or_default();
        if vendor.trim() != "0x1002" {
            continue;
        }
        let model = fs::read_to_string(dev_dir.join("device"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let mut g = ProfileGroup::new(
            Subsystem::Gpu,
            format!("AMD {} ({})", name, model),
            "amdgpu profile",
            dev_dir.display().to_string(),
        );
        for (file, id, label, risk) in [
            (
                "power_dpm_state",
                "power_dpm_state",
                "Power DPM State",
                SettingRisk::Moderate,
            ),
            (
                "power_dpm_force_performance_level",
                "perf_level",
                "Performance Level",
                SettingRisk::Moderate,
            ), // marked writable below via post-processing
            (
                "pp_power_profile_mode",
                "power_profile_mode",
                "Power Profile Mode",
                SettingRisk::Moderate,
            ),
            (
                "pp_od_clk_voltage",
                "od_clk_voltage",
                "Overdrive Clock/Voltage Table",
                SettingRisk::Dangerous,
            ),
            (
                "pp_dpm_sclk",
                "dpm_sclk",
                "SCLK DPM Table",
                SettingRisk::Informational,
            ),
            (
                "pp_dpm_mclk",
                "dpm_mclk",
                "MCLK DPM Table",
                SettingRisk::Informational,
            ),
            (
                "pp_dpm_pcie",
                "dpm_pcie",
                "PCIe DPM Table",
                SettingRisk::Informational,
            ),
        ] {
            let p = dev_dir.join(file);
            if let Ok(v) = fs::read_to_string(&p) {
                let value = if v.contains('\n') {
                    SettingValue::Text(v.trim().to_string())
                } else if let Ok(u) = v.trim().parse::<u64>() {
                    SettingValue::Uint(u)
                } else {
                    SettingValue::Text(v.trim().to_string())
                };
                let writable = id == "perf_level";
                g.push(
                    Setting::info(id, label, value)
                        .with_risk(risk)
                        .with_source(p.display().to_string())
                        .with_writable(writable),
                );
            }
        }
        // hwmon power cap
        if let Ok(hwmon) = fs::read_dir(dev_dir.join("hwmon")) {
            for hw in hwmon.flatten() {
                let cap = hw.path().join("power1_cap");
                if let Ok(v) = fs::read_to_string(&cap) {
                    if let Ok(uw) = v.trim().parse::<u64>() {
                        g.push(
                            Setting::info("power_cap_uw", "Power Cap", SettingValue::Uint(uw))
                                .with_unit("μW")
                                .with_risk(SettingRisk::Moderate)
                                .with_source(cap.display().to_string()),
                        );
                    }
                }
            }
        }
        groups.push(g);
    }
    groups
}

#[cfg(all(target_os = "linux", feature = "intel"))]
fn intel_linux_groups() -> Vec<ProfileGroup> {
    use std::fs;
    let mut groups = Vec::new();
    let drm = match fs::read_dir("/sys/class/drm") {
        Ok(rd) => rd,
        Err(_) => return groups,
    };
    for entry in drm.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let dev_dir = entry.path().join("device");
        let vendor = fs::read_to_string(dev_dir.join("vendor")).unwrap_or_default();
        if vendor.trim() != "0x8086" {
            continue;
        }
        let model = fs::read_to_string(dev_dir.join("device"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let card_dir = entry.path();
        let mut g = ProfileGroup::new(
            Subsystem::Gpu,
            format!("Intel {} ({})", name, model),
            "i915/xe profile",
            card_dir.display().to_string(),
        );
        for (file, id, label) in [
            ("gt_min_freq_mhz", "gt_min_freq_mhz", "GT Min Frequency"),
            ("gt_max_freq_mhz", "gt_max_freq_mhz", "GT Max Frequency"),
            (
                "gt_boost_freq_mhz",
                "gt_boost_freq_mhz",
                "GT Boost Frequency",
            ),
            ("gt_cur_freq_mhz", "gt_cur_freq_mhz", "GT Current Frequency"),
            ("rc6_enable", "rc6_enable", "RC6 Power Saving"),
        ] {
            let p = card_dir.join(file);
            if let Ok(v) = fs::read_to_string(&p) {
                let value = v
                    .trim()
                    .parse::<u64>()
                    .map(SettingValue::Uint)
                    .unwrap_or_else(|_| SettingValue::Text(v.trim().to_string()));
                let writable = id == "gt_max_freq_mhz";
                g.push(
                    Setting::info(id, label, value)
                        .with_unit(if id.ends_with("_mhz") { "MHz" } else { "" })
                        .with_risk(SettingRisk::Moderate)
                        .with_source(p.display().to_string())
                        .with_writable(writable),
                );
            }
        }
        groups.push(g);
    }
    groups
}

#[cfg(windows)]
fn windows_display_class_groups() -> Vec<ProfileGroup> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let class_path =
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let Ok(class_key) = hklm.open_subkey(class_path) else {
        return Vec::new();
    };

    let mut groups = Vec::new();
    let subkeys: Vec<String> = class_key.enum_keys().filter_map(|k| k.ok()).collect();
    for sub in subkeys {
        // Only 4-digit instance subkeys.
        if sub.len() != 4 || !sub.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(inst) = class_key.open_subkey(&sub) else {
            continue;
        };
        let desc: String = inst.get_value("DriverDesc").unwrap_or_default();
        if desc.is_empty() {
            continue;
        }
        let provider: String = inst.get_value("ProviderName").unwrap_or_default();
        let driver_ver: String = inst.get_value("DriverVersion").unwrap_or_default();
        let driver_date: String = inst.get_value("DriverDate").unwrap_or_default();

        let mut g = ProfileGroup::new(
            Subsystem::Gpu,
            &desc,
            "Driver class registry",
            format!(r"HKLM\{}\{}", class_path, sub),
        );
        g.push(Setting::info(
            "provider",
            "Provider",
            SettingValue::Text(provider),
        ));
        g.push(Setting::info(
            "driver_version",
            "Driver Version",
            SettingValue::Text(driver_ver),
        ));
        g.push(Setting::info(
            "driver_date",
            "Driver Date",
            SettingValue::Text(driver_date),
        ));

        // Enumerate string/dword values under the instance key — these are
        // the vendor-specific UMD/KMD overrides that vendor control panels
        // and tools like NVPI / AMD Adrenalin write.
        for name in inst.enum_values().filter_map(|v| v.ok()).map(|(n, _)| n) {
            if matches!(
                name.as_str(),
                "DriverDesc"
                    | "ProviderName"
                    | "DriverVersion"
                    | "DriverDate"
                    | "DriverDateData"
                    | "InfPath"
                    | "InfSection"
                    | "MatchingDeviceId"
            ) {
                continue;
            }
            // Try DWORD first, fall back to string.
            if let Ok(dw) = inst.get_value::<u32, _>(&name) {
                g.push(
                    Setting::info(&name, &name, SettingValue::Uint(dw as u64))
                        .with_risk(SettingRisk::Moderate)
                        .with_source("registry"),
                );
            } else if let Ok(s) = inst.get_value::<String, _>(&name) {
                if s.len() < 256 {
                    g.push(
                        Setting::info(&name, &name, SettingValue::Text(s))
                            .with_risk(SettingRisk::Moderate)
                            .with_source("registry"),
                    );
                }
            }
        }
        g.note(
            "Vendor control panels (NVIDIA Control Panel, AMD Adrenalin, Intel Arc \
            Control) and tweakers like NVIDIA Profile Inspector write per-driver \
            overrides into this registry instance. Per-application profiles \
            require NVAPI/ADL access (not yet implemented).",
        );
        groups.push(g);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_smoke() {
        let mut p = GpuProfileProvider::new();
        let _ = p.snapshot();
    }
}
