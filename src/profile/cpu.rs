//! CPU profile provider — XTU / Ryzen Master-style read-only view.
//!
//! Inspired by Intel Extreme Tuning Utility (XTU) and AMD Ryzen Master. The
//! full XTU/RM experience requires a signed kernel driver on Windows to
//! access MSRs and SMU mailboxes. That capability is intentionally out of
//! scope for this read-only pass.
//!
//! What we *do* read:
//!
//! - **Linux**: per-CPU `cpufreq` policy (min/max/cur, governor, energy
//!   preference), `intel_pstate` toggles, idle drivers, MSR power limits when
//!   `/dev/cpu/0/msr` is readable.
//! - **Windows**: power-policy values via `powercfg`-style WMI, plus
//!   processor performance ratios surfaced through existing
//!   [`crate::cpufreq`].
//! - **All platforms**: live counters from [`crate::rapl`] when available.

#[allow(unused_imports)]
use super::{ProfileGroup, ProfileProvider, Setting, SettingRisk, SettingValue, Subsystem};

pub struct CpuProfileProvider {
    _private: (),
}

impl CpuProfileProvider {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for CpuProfileProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileProvider for CpuProfileProvider {
    fn subsystem(&self) -> Subsystem {
        Subsystem::Cpu
    }

    fn snapshot(&mut self) -> Vec<ProfileGroup> {
        let mut groups = Vec::new();

        #[cfg(target_os = "linux")]
        {
            if let Some(g) = linux_cpufreq_group() {
                groups.push(g);
            }
            if let Some(g) = linux_intel_pstate_group() {
                groups.push(g);
            }
            if let Some(g) = linux_msr_power_group() {
                groups.push(g);
            }
        }

        #[cfg(windows)]
        {
            if let Some(g) = windows_power_policy_group() {
                groups.push(g);
            }
        }

        if let Some(g) = rapl_group() {
            groups.push(g);
        }

        if groups.is_empty() {
            let mut g = ProfileGroup::new(Subsystem::Cpu, "(CPU)", "Default", "auto-detect");
            g.note(
                "No CPU tuning surfaces were readable. On Linux, run as root for \
                MSR access; on Windows, signed-driver MSR access is required for \
                XTU/Ryzen Master-equivalent data.",
            );
            groups.push(g);
        }
        groups
    }
}

#[cfg(target_os = "linux")]
fn linux_cpufreq_group() -> Option<ProfileGroup> {
    let base = std::path::Path::new("/sys/devices/system/cpu");
    if !base.exists() {
        return None;
    }
    let mut g = ProfileGroup::new(
        Subsystem::Cpu,
        "All CPUs (cpufreq)",
        "Governor and frequency policy",
        base.display().to_string(),
    );

    // Aggregate across CPU0 only for compactness — the governor is typically
    // uniform. Per-core breakdown is available via crate::cpufreq.
    let cpu0 = base.join("cpu0/cpufreq");
    let read = |name: &str| {
        std::fs::read_to_string(cpu0.join(name))
            .ok()
            .map(|s| s.trim().to_string())
    };

    if let Some(v) = read("scaling_governor") {
        let mut s = Setting::info("scaling_governor", "CPU Governor", SettingValue::Text(v))
            .with_risk(SettingRisk::Moderate)
            .with_source(cpu0.join("scaling_governor").display().to_string())
            .with_writable(true);
        if let Some(avail) = read("scaling_available_governors") {
            s.choices = Some(
                avail
                    .split_whitespace()
                    .map(|c| (c.to_string(), SettingValue::Text(c.to_string())))
                    .collect(),
            );
        }
        g.push(s);
    }
    for (file, id, label, unit) in [
        (
            "scaling_min_freq",
            "scaling_min_khz",
            "Min Frequency",
            "kHz",
        ),
        (
            "scaling_max_freq",
            "scaling_max_khz",
            "Max Frequency",
            "kHz",
        ),
        (
            "scaling_cur_freq",
            "scaling_cur_khz",
            "Current Frequency",
            "kHz",
        ),
        ("cpuinfo_min_freq", "hw_min_khz", "Hardware Min", "kHz"),
        ("cpuinfo_max_freq", "hw_max_khz", "Hardware Max", "kHz"),
    ] {
        if let Some(v) = read(file) {
            if let Ok(u) = v.parse::<u64>() {
                g.push(
                    Setting::info(id, label, SettingValue::Uint(u))
                        .with_unit(unit)
                        .with_risk(SettingRisk::Moderate)
                        .with_source(cpu0.join(file).display().to_string()),
                );
            }
        }
    }
    if let Some(v) = read("energy_performance_preference") {
        g.push(
            Setting::info(
                "epp",
                "Energy Performance Preference",
                SettingValue::Text(v),
            )
            .with_risk(SettingRisk::Moderate)
            .with_source("EPP"),
        );
    }
    Some(g)
}

#[cfg(target_os = "linux")]
fn linux_intel_pstate_group() -> Option<ProfileGroup> {
    let dir = std::path::Path::new("/sys/devices/system/cpu/intel_pstate");
    if !dir.exists() {
        return None;
    }
    let mut g = ProfileGroup::new(
        Subsystem::Cpu,
        "Intel P-State driver",
        "intel_pstate parameters",
        dir.display().to_string(),
    );
    let read = |name: &str| {
        std::fs::read_to_string(dir.join(name))
            .ok()
            .map(|s| s.trim().to_string())
    };
    for (file, label, risk) in [
        ("status", "Driver Status", SettingRisk::Informational),
        (
            "no_turbo",
            "No-Turbo (Turbo Disabled)",
            SettingRisk::Moderate,
        ),
        (
            "hwp_dynamic_boost",
            "HWP Dynamic Boost",
            SettingRisk::Moderate,
        ),
        ("max_perf_pct", "Max Performance %", SettingRisk::Moderate),
        ("min_perf_pct", "Min Performance %", SettingRisk::Moderate),
        (
            "num_pstates",
            "Number of P-States",
            SettingRisk::Informational,
        ),
        ("turbo_pct", "Turbo Range %", SettingRisk::Informational),
    ] {
        if let Some(v) = read(file) {
            let value = v
                .parse::<i64>()
                .map(SettingValue::Int)
                .unwrap_or_else(|_| SettingValue::Text(v));
            g.push(
                Setting::info(file, label, value)
                    .with_risk(risk)
                    .with_source(dir.join(file).display().to_string()),
            );
        }
    }
    Some(g)
}

#[cfg(target_os = "linux")]
fn linux_msr_power_group() -> Option<ProfileGroup> {
    // MSR_PKG_POWER_LIMIT = 0x610 on Intel, lives at offset 0x610 in
    // /dev/cpu/0/msr. Reading is non-destructive but requires root + the
    // 'msr' module to be loaded.
    use std::io::{Read, Seek, SeekFrom};

    let mut f = std::fs::File::open("/dev/cpu/0/msr").ok()?;
    let mut g = ProfileGroup::new(
        Subsystem::Cpu,
        "CPU0 — MSR power",
        "MSR_PKG_POWER_LIMIT (0x610) / MSR_RAPL_POWER_UNIT (0x606)",
        "/dev/cpu/0/msr",
    );

    let mut read_msr = |addr: u64| -> Option<u64> {
        f.seek(SeekFrom::Start(addr)).ok()?;
        let mut buf = [0u8; 8];
        f.read_exact(&mut buf).ok()?;
        Some(u64::from_le_bytes(buf))
    };
    if let Some(units) = read_msr(0x606) {
        // power: bits[3:0] in units of (1/2^P) W
        let power_unit = (units & 0xF) as u32;
        let power_w = 1.0_f64 / (1u64 << power_unit) as f64;
        g.push(
            Setting::info(
                "rapl_power_unit_w",
                "RAPL Power Unit",
                SettingValue::Float(power_w),
            )
            .with_unit("W")
            .with_source("MSR 0x606"),
        );
        if let Some(limit) = read_msr(0x610) {
            // bits[14:0] = PL1 in power units, bit15 = enable, bits[23:17] = time window
            let pl1_raw = limit & 0x7FFF;
            let pl1_w = pl1_raw as f64 * power_w;
            let pl1_enabled = (limit >> 15) & 1 == 1;
            let pl2_raw = (limit >> 32) & 0x7FFF;
            let pl2_w = pl2_raw as f64 * power_w;
            let pl2_enabled = ((limit >> 47) & 1) == 1;
            g.push(
                Setting::info("pl1_w", "Package Power Limit 1 (PL1)", SettingValue::Float(pl1_w))
                    .with_unit("W")
                    .with_description("Long-duration sustained power limit. CPU must keep package power below this on long average.")
                    .with_risk(SettingRisk::Dangerous)
                    .with_source("MSR 0x610 [14:0]"),
            );
            g.push(
                Setting::info("pl1_enable", "PL1 Enabled", SettingValue::Bool(pl1_enabled))
                    .with_source("MSR 0x610 [15]"),
            );
            g.push(
                Setting::info("pl2_w", "Package Power Limit 2 (PL2)", SettingValue::Float(pl2_w))
                    .with_unit("W")
                    .with_description("Short-duration burst power limit. CPU may spike up to this for the configured time window.")
                    .with_risk(SettingRisk::Dangerous)
                    .with_source("MSR 0x610 [46:32]"),
            );
            g.push(
                Setting::info("pl2_enable", "PL2 Enabled", SettingValue::Bool(pl2_enabled))
                    .with_source("MSR 0x610 [47]"),
            );
        }
    } else {
        g.note(
            "MSR 0x606 (RAPL_POWER_UNIT) not readable. The `msr` kernel module \
            may be unloaded — try `modprobe msr` and run as root.",
        );
    }
    Some(g)
}

#[cfg(windows)]
fn windows_power_policy_group() -> Option<ProfileGroup> {
    // Read the active power-scheme GUID and processor-power-management
    // sub-keys from HKLM. Avoids spawning powercfg.exe.
    use winreg::enums::*;
    use winreg::RegKey;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let policies = hklm
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes")
        .ok()?;
    let active: String = policies.get_value("ActivePowerScheme").unwrap_or_default();
    let mut g = ProfileGroup::new(
        Subsystem::Cpu,
        "Windows Processor Power Policy",
        "Active power scheme",
        r"HKLM\SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes",
    );

    // Enumerate all available schemes via PowerEnumerate so the user can pick
    // a GUID without an external powercfg /list invocation.
    let schemes = enumerate_power_schemes();
    let mut s = Setting::info(
        "active_scheme_guid",
        "Active Power Scheme",
        SettingValue::Text(active),
    )
    .with_description(
        "GUID of the active Windows power scheme. Set via PowerSetActiveScheme — writable without admin elevation.",
    )
    .with_risk(SettingRisk::Safe)
    .with_writable(true);
    if !schemes.is_empty() {
        s.choices = Some(
            schemes
                .iter()
                .map(|(guid, name)| (name.clone(), SettingValue::Text(guid.clone())))
                .collect(),
        );
    }
    g.push(s);

    // Also surface each scheme as its own informational setting.
    for (guid, name) in &schemes {
        g.push(
            Setting::info(
                format!("scheme.{}", guid),
                format!("Scheme: {}", name),
                SettingValue::Text(guid.clone()),
            )
            .with_description("Available Windows power scheme (use its GUID with `ironmon profile set active_scheme_guid <guid> --confirm`).")
            .with_source("PowerEnumerate(ACCESS_SCHEME)"),
        );
    }
    g.note(
        "Per-state processor power management (PROCTHROTTLEMAX, PROCFREQMAX, \
        boost mode, etc.) lives under the active scheme GUID. Full XTU-style \
        MSR access requires a signed kernel driver and is not yet implemented.",
    );
    Some(g)
}

/// Enumerate all available Windows power schemes via `PowerEnumerate`.
/// Returns `(guid_string, friendly_name)` pairs.
#[cfg(windows)]
pub fn enumerate_power_schemes() -> Vec<(String, String)> {
    use windows::Win32::System::Power::{PowerEnumerate, ACCESS_SCHEME};
    let mut out = Vec::new();
    let mut index = 0u32;
    loop {
        let mut guid = windows::core::GUID::default();
        let mut size: u32 = std::mem::size_of::<windows::core::GUID>() as u32;
        let rc = unsafe {
            PowerEnumerate(
                None,
                None,
                None,
                ACCESS_SCHEME,
                index,
                Some(&mut guid as *mut _ as *mut u8),
                &mut size,
            )
        };
        // ERROR_NO_MORE_ITEMS = 259
        if rc.0 != 0 {
            break;
        }
        let guid_str = format_guid(&guid);
        let name = read_friendly_name(&guid).unwrap_or_else(|| "<unnamed>".into());
        out.push((guid_str, name));
        index += 1;
        if index > 64 {
            break; // safety cap
        }
    }
    out
}

#[cfg(windows)]
fn format_guid(g: &windows::core::GUID) -> String {
    let d4 = g.data4;
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        g.data1, g.data2, g.data3, d4[0], d4[1], d4[2], d4[3], d4[4], d4[5], d4[6], d4[7]
    )
}

#[cfg(windows)]
fn read_friendly_name(scheme: &windows::core::GUID) -> Option<String> {
    use windows::Win32::System::Power::PowerReadFriendlyName;
    // First call: size probe.
    let mut size: u32 = 0;
    let _ = unsafe { PowerReadFriendlyName(None, Some(scheme), None, None, None, &mut size) };
    if size == 0 || size > 4096 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let rc = unsafe {
        PowerReadFriendlyName(
            None,
            Some(scheme),
            None,
            None,
            Some(buf.as_mut_ptr()),
            &mut size,
        )
    };
    if rc.0 != 0 {
        return None;
    }
    // Result is UTF-16LE with null terminator.
    let wide: Vec<u16> = buf
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&w| w != 0)
        .collect();
    Some(String::from_utf16_lossy(&wide))
}

fn rapl_group() -> Option<ProfileGroup> {
    use crate::rapl::RaplMonitor;
    let monitor = RaplMonitor::new().ok()?;
    let readings = monitor.readings();
    if readings.is_empty() {
        return None;
    }
    let mut g = ProfileGroup::new(
        Subsystem::Cpu,
        "RAPL energy domains",
        "Running Average Power Limit",
        "Intel RAPL / AMD energy MSRs",
    );
    for r in readings {
        let domain = format!("{:?}", r.domain);
        g.push(
            Setting::info(
                format!("{}_energy_uj", domain.to_lowercase()),
                format!("{} energy", domain),
                SettingValue::Uint(r.energy_uj),
            )
            .with_unit("μJ")
            .with_source(format!("RAPL {}", domain)),
        );
    }
    Some(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_smoke() {
        let mut p = CpuProfileProvider::new();
        let _ = p.snapshot();
    }
}
