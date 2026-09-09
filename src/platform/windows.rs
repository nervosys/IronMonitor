//! Windows platform implementation
//!
//! Provides CPU, memory, and system monitoring for Windows using Windows APIs.

use crate::core::{
    cpu::{CpuCore, CpuFrequency, CpuStats, CpuTotal},
    gpu::GpuStats,
    memory::{MemoryStats, RamInfo, SwapInfo},
    platform_info::{BoardInfo, HardwareInfo, LibraryVersions, PlatformInfo},
    power::PowerStats,
    temperature::TemperatureStats,
};
use crate::error::{IronError, Result};
use std::collections::HashMap;
use std::mem;
use std::sync::atomic::{AtomicU64, Ordering};
use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
use windows::Win32::System::SystemInformation::{
    GetSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
};

/// Previous CPU times for utilization calculation
static PREV_IDLE_TIME: AtomicU64 = AtomicU64::new(0);
static PREV_KERNEL_TIME: AtomicU64 = AtomicU64::new(0);
static PREV_USER_TIME: AtomicU64 = AtomicU64::new(0);

/// Read CPU statistics on Windows
pub fn read_cpu_stats() -> Result<CpuStats> {
    let mut stats = CpuStats::empty();

    // Get number of processors
    let mut sys_info: SYSTEM_INFO = unsafe { mem::zeroed() };
    unsafe { GetSystemInfo(&mut sys_info) };
    let cpu_count = sys_info.dwNumberOfProcessors as usize;

    // Get system times for overall CPU utilization
    let (user_percent, system_percent, idle_percent) = get_system_cpu_utilization()?;

    // Model name is fixed for the life of the process and cached behind a OnceLock.
    // Clocks are not: each core boosts and parks independently, so they are sampled
    // per call and assigned per core rather than broadcast from core 0.
    let model = get_cpu_model_name();
    let power = query_processor_power(cpu_count);
    // Delivered performance per core, for the machines where `CurrentMhz` is
    // the nominal clock -- which is all of them on Windows 10 and later. `None`
    // on the first call of the process: a rate needs two collections.
    let delivered = crate::platform::windows_pdh::processor_performance_percent();

    // Real per-processor times. Falls back to `None` if the query fails, which
    // consumers render as "unavailable" — deliberately not the system average.
    //
    // Until this existed, every core was assigned the *system-wide* figure from
    // GetSystemTimes, so a 24-core machine showed 24 identical bars that looked like
    // per-core measurements but were one number repeated.
    let per_core = read_per_core_utilization(cpu_count);

    for cpu_id in 0..cpu_count {
        let (core_user, core_system, core_idle) = match per_core {
            Some(ref cores) => match cores.get(cpu_id) {
                Some(&(u, s, i)) => (Some(u), Some(s), Some(i)),
                None => (None, None, None),
            },
            None => (None, None, None),
        };

        let core = CpuCore {
            id: cpu_id,
            online: true,
            governor: "windows".to_string(),
            frequency: get_cpu_frequency(power.as_ref(), cpu_id, delivered.as_ref()),
            user: core_user,
            nice: Some(0.0), // Windows doesn't have nice
            system: core_system,
            idle: core_idle,
            model: model.clone(),
        };
        stats.cores.push(core);
    }

    // Set totals
    stats.total = CpuTotal {
        user: user_percent,
        nice: 0.0,
        system: system_percent,
        idle: idle_percent,
    };

    Ok(stats)
}

// ============================================================================
// Per-processor CPU times
// ============================================================================

/// Per-processor timing record returned by `NtQuerySystemInformation`.
///
/// Mirrors `SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION`. Times are in 100ns units, and
/// `kernel_time` *includes* `idle_time`, matching `GetSystemTimes`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SystemProcessorPerformanceInformation {
    idle_time: i64,
    kernel_time: i64,
    user_time: i64,
    dpc_time: i64,
    interrupt_time: i64,
    interrupt_count: u32,
    // Explicit tail padding so the struct stride matches what the kernel writes;
    // the array is indexed by stride, so getting this wrong misaligns every core
    // after the first.
    _reserved: u32,
}

/// `SystemProcessorPerformanceInformation` information class.
const SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION_CLASS: u32 = 8;

#[link(name = "ntdll")]
extern "system" {
    /// Queries kernel performance data.
    ///
    /// Used because Win32 exposes no per-processor time API: `GetSystemTimes` is
    /// system-wide, and the documented alternative (PDH `\Processor(N)\% Processor
    /// Time`) requires a persistent query handle and two sampling passes. This call
    /// is what Task Manager and every mainstream monitor rely on.
    fn NtQuerySystemInformation(
        system_information_class: u32,
        system_information: *mut core::ffi::c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

/// Previous per-processor sample, for delta calculation.
static PREV_PER_CORE: std::sync::OnceLock<
    std::sync::Mutex<Vec<SystemProcessorPerformanceInformation>>,
> = std::sync::OnceLock::new();

/// Read raw per-processor times.
fn query_per_core_times(cpu_count: usize) -> Option<Vec<SystemProcessorPerformanceInformation>> {
    if cpu_count == 0 {
        return None;
    }

    let mut buffer: Vec<SystemProcessorPerformanceInformation> =
        vec![SystemProcessorPerformanceInformation::default(); cpu_count];
    let byte_len = std::mem::size_of_val(buffer.as_slice()) as u32;
    let mut returned: u32 = 0;

    // SAFETY: the buffer is sized for exactly `cpu_count` records and its length in
    // bytes is passed alongside it, so the kernel cannot write past the end. Both
    // out-pointers reference live local storage for the duration of the call.
    let status = unsafe {
        NtQuerySystemInformation(
            SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION_CLASS,
            buffer.as_mut_ptr() as *mut core::ffi::c_void,
            byte_len,
            &mut returned,
        )
    };

    // NTSTATUS: negative values are failures.
    if status < 0 {
        return None;
    }

    // Trust the kernel's reported count over our assumption: a machine with more than
    // 64 logical processors splits into processor groups, and GetSystemInfo reports
    // only the calling group.
    let record_size = std::mem::size_of::<SystemProcessorPerformanceInformation>();
    let returned_records = (returned as usize) / record_size;
    if returned_records == 0 {
        return None;
    }
    buffer.truncate(returned_records.min(cpu_count));

    Some(buffer)
}

/// Per-core `(user, system, idle)` percentages.
///
/// The first call has no previous sample to difference against, so it reports each
/// core's average since boot — genuinely per-core, just averaged over uptime. Every
/// later call reports the delta since the previous call, which is the instantaneous
/// figure callers expect.
fn read_per_core_utilization(cpu_count: usize) -> Option<Vec<(f32, f32, f32)>> {
    let current = query_per_core_times(cpu_count)?;

    let lock = PREV_PER_CORE.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    let mut previous = lock.lock().ok()?;

    let use_deltas = previous.len() == current.len();

    let result = current
        .iter()
        .enumerate()
        .map(|(i, now)| {
            let (idle, kernel, user) = if use_deltas {
                let before = &previous[i];
                (
                    now.idle_time.saturating_sub(before.idle_time),
                    now.kernel_time.saturating_sub(before.kernel_time),
                    now.user_time.saturating_sub(before.user_time),
                )
            } else {
                (now.idle_time, now.kernel_time, now.user_time)
            };

            // kernel_time includes idle_time, so system time is the remainder.
            let system = kernel.saturating_sub(idle);
            let total = idle + system + user;

            if total <= 0 {
                // No elapsed time between samples — report fully idle rather than
                // dividing by zero.
                return (0.0, 0.0, 100.0);
            }

            let total = total as f32;
            (
                (user as f32 / total) * 100.0,
                (system as f32 / total) * 100.0,
                (idle as f32 / total) * 100.0,
            )
        })
        .collect();

    *previous = current;
    Some(result)
}

/// Get CPU utilization using kernel32 GetSystemTimes via FFI
fn get_system_cpu_utilization() -> Result<(f32, f32, f32)> {
    use windows::Win32::Foundation::FILETIME;

    // GetSystemTimes is in kernel32.dll - use raw FFI call
    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemTimes(
            lpIdleTime: *mut FILETIME,
            lpKernelTime: *mut FILETIME,
            lpUserTime: *mut FILETIME,
        ) -> i32;
    }

    let mut idle_time: FILETIME = unsafe { mem::zeroed() };
    let mut kernel_time: FILETIME = unsafe { mem::zeroed() };
    let mut user_time: FILETIME = unsafe { mem::zeroed() };

    let result = unsafe { GetSystemTimes(&mut idle_time, &mut kernel_time, &mut user_time) };

    if result == 0 {
        return Err(IronError::System("GetSystemTimes failed".to_string()));
    }

    // Convert FILETIME to u64
    let idle = filetime_to_u64(&idle_time);
    let kernel = filetime_to_u64(&kernel_time);
    let user = filetime_to_u64(&user_time);

    // Get previous values
    let prev_idle = PREV_IDLE_TIME.load(Ordering::Relaxed);
    let prev_kernel = PREV_KERNEL_TIME.load(Ordering::Relaxed);
    let prev_user = PREV_USER_TIME.load(Ordering::Relaxed);

    // Store current values for next calculation
    PREV_IDLE_TIME.store(idle, Ordering::Relaxed);
    PREV_KERNEL_TIME.store(kernel, Ordering::Relaxed);
    PREV_USER_TIME.store(user, Ordering::Relaxed);

    // Calculate deltas
    let idle_delta = idle.saturating_sub(prev_idle);
    let kernel_delta = kernel.saturating_sub(prev_kernel);
    let user_delta = user.saturating_sub(prev_user);

    // Kernel time includes idle time
    let system_delta = kernel_delta.saturating_sub(idle_delta);
    let total = idle_delta + system_delta + user_delta;

    // On the first call there is no previous sample to difference against, so fall
    // back to the totals accumulated since boot. That is a real measurement — the
    // machine's average over its uptime — rather than a guess.
    //
    // This previously returned a hardcoded (0.0, 0.0, 100.0), asserting the system
    // was completely idle. Every one-shot invocation is a first call, so
    // `ironmon cli cpu` always reported 0% usage regardless of actual load, printed
    // directly above per-core figures that contradicted it.
    let (idle_basis, system_basis, user_basis, total_basis) = if prev_idle == 0 || total == 0 {
        let system_since_boot = kernel.saturating_sub(idle);
        let total_since_boot = idle + system_since_boot + user;
        (idle, system_since_boot, user, total_since_boot)
    } else {
        (idle_delta, system_delta, user_delta, total)
    };

    if total_basis == 0 {
        // Genuinely no elapsed CPU time to apportion. Only reachable if the kernel
        // reports all-zero counters.
        return Ok((0.0, 0.0, 100.0));
    }

    let basis = total_basis as f64;
    let idle_percent = (idle_basis as f64 / basis * 100.0) as f32;
    let system_percent = (system_basis as f64 / basis * 100.0) as f32;
    let user_percent = (user_basis as f64 / basis * 100.0) as f32;

    Ok((user_percent, system_percent, idle_percent))
}

fn filetime_to_u64(ft: &windows::Win32::Foundation::FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
}

/// One entry of the array `CallNtPowerInformation(ProcessorInformation)` writes.
///
/// Mirrors `PROCESSOR_POWER_INFORMATION`. `CurrentMhz` is the live clock, which is
/// the only reason this call is worth making — the registry's `~MHz` is the boot-time
/// nominal and never moves.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ProcessorPowerInformation {
    number: u32,
    max_mhz: u32,
    current_mhz: u32,
    mhz_limit: u32,
    max_idle_state: u32,
    current_idle_state: u32,
}

/// `ProcessorInformation` power information level.
const PROCESSOR_INFORMATION_LEVEL: i32 = 11;

/// Read per-processor clocks.
///
/// Returns one entry per logical processor, or `None` if the query fails — callers
/// render that as "unavailable" rather than substituting a nominal figure.
fn query_processor_power(cpu_count: usize) -> Option<Vec<ProcessorPowerInformation>> {
    use windows::Win32::System::Power::CallNtPowerInformation;

    if cpu_count == 0 {
        return None;
    }

    let mut buffer: Vec<ProcessorPowerInformation> =
        vec![ProcessorPowerInformation::default(); cpu_count];
    let bytes = std::mem::size_of_val(buffer.as_slice()) as u32;

    let status = unsafe {
        CallNtPowerInformation(
            windows::Win32::System::Power::POWER_INFORMATION_LEVEL(PROCESSOR_INFORMATION_LEVEL),
            None,
            0,
            Some(buffer.as_mut_ptr() as *mut core::ffi::c_void),
            bytes,
        )
    };

    status.ok().ok().map(|_| buffer)
}

/// Get CPU frequency for a specific logical processor.
///
/// Reads `CallNtPowerInformation` rather than shelling out to `wmic`, which Microsoft
/// removed in Windows 11 24H2 — on any current build the subprocess simply failed to
/// spawn, so this returned `None` and the frontends printed "0 MHz" as if measured.
///
/// **`CurrentMhz` is not a current frequency on Windows 10 or later.** It is the
/// nominal clock, and it is returned unchanged whatever the cores are doing. On
/// the 9900X this was found on it read 4400 for all 24 logical processors, idle
/// and under load, across repeated samples — while
/// `\Processor Information(*)\% Processor Performance` showed those same cores
/// at 105–119% of nominal, boosting past 5GHz and differing from each other.
/// WMI agrees with the wrong answer: `CurrentClockSpeed` and `MaxClockSpeed` are
/// both 4400. This is why Task Manager reads the performance counter instead.
///
/// So a `CurrentMhz` equal to `MaxMhz` is reported as no current reading rather
/// than as a measurement. `cpu.core.{n}.frequency` is declared `Measured` and
/// described as "current core clock"; publishing the nominal figure there put a
/// specification behind a measurement's provenance on every core of every
/// Windows machine, and it passed every conformance test because nothing checks
/// whether a value that claims to be current ever changes.
///
/// That suppression is no longer the end of it. `% Processor Performance` is
/// what Task Manager reads, the kernel derives it from APERF and MPERF, and it
/// does vary per core -- 60% to 124% of nominal on this machine, a real
/// 2.6-5.4 GHz spread. [`crate::platform::windows_pdh`] keeps a PDH query open
/// so the two samples a rate needs are consecutive *calls* rather than a sleep
/// inside one, and the nominal maximum times that percentage is reported as the
/// current clock -- flagged `current_is_derived`, because a specification
/// multiplied by a measurement is not a measurement.
fn get_cpu_frequency(
    power: Option<&Vec<ProcessorPowerInformation>>,
    index: usize,
    delivered: Option<&Vec<Option<f64>>>,
) -> Option<CpuFrequency> {
    let entry = power?.get(index)?;
    if entry.current_mhz == 0 && entry.max_mhz == 0 {
        return None;
    }

    // The nominal maximum scaled by delivered performance. Only used when
    // `CurrentMhz` gave nothing, so a platform that does report a real current
    // clock keeps it.
    let derived = delivered
        .and_then(|d| d.get(index).copied().flatten())
        .filter(|pct| *pct > 0.0)
        .map(|pct| (f64::from(entry.max_mhz) * pct / 100.0).round() as u32);

    let measured = (entry.current_mhz != entry.max_mhz).then_some(entry.current_mhz);
    let current_is_derived = measured.is_none() && derived.is_some();

    Some(CpuFrequency {
        current_is_derived,
        // CallNtPowerInformation returns the nominal clock, not the current
        // one: when it equals the maximum for every core, nothing was measured.
        // This used to say `0` and rely on each consumer reading that as an
        // absence. Only the ontology resolver did.
        current: measured.or(derived),
        // No Win32 API reports a minimum operating frequency; leaving it at the
        // measured current would claim a floor that was never read.
        min: None,
        max: (entry.max_mhz > 0).then_some(entry.max_mhz),
    })
}

/// Get CPU model name from the registry.
///
/// `HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0\ProcessorNameString` is what
/// the firmware published at boot — the same string WMI reports, without the COM
/// connection or the (now absent) `wmic` subprocess.
fn get_cpu_model_name() -> String {
    static MODEL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MODEL
        .get_or_init(|| {
            read_registry_string(
                r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
                "ProcessorNameString",
            )
            .unwrap_or_else(|| "Unknown CPU".to_string())
        })
        .clone()
}

/// Read one `REG_SZ` value from `HKEY_LOCAL_MACHINE`, trimmed.
///
/// Returns `None` when the key, the value, or a non-empty string is missing, so
/// callers can distinguish "not published by this firmware" from an empty reading.
pub(crate) fn read_registry_string(subkey: &str, value: &str) -> Option<String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(subkey)
        .ok()?;
    let raw: String = key.get_value(value).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Read one `REG_DWORD` value from `HKEY_LOCAL_MACHINE`.
///
/// `None` means the key or value is absent, which is not the same as a value of 0 —
/// a distinction that matters for flags like `UEFISecureBootEnabled`, where "no such
/// setting" and "the setting is off" have different causes.
pub(crate) fn read_registry_u32(subkey: &str, value: &str) -> Option<u32> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(subkey)
        .ok()?;
    key.get_value(value).ok()
}

/// True VRAM in bytes for each display adapter, from the driver's own registry
/// key, paired with the `DriverDesc` it belongs to.
///
/// `Win32_VideoController.AdapterRAM` is a 32-bit field. An RTX 3090 Ti with 24GB
/// reports 4293918720 through it — just under 4GiB — and every card above 4GB
/// reports something similarly wrong. `hardware_ai` divided that by 2^30, got
/// 4.0, and concluded "Insufficient VRAM for ML training" about a card that has
/// 24GB of it.
///
/// The display-adapter class key carries `HardwareInformation.qwMemorySize`, a
/// `REG_QWORD` with the real figure: 25757220864 on the same machine. It needs no
/// elevation. The 32-bit `HardwareInformation.MemorySize` sits beside it holding
/// the same wrong number as WMI, which is a useful reminder that the overflow is
/// in the field width and not in the query.
///
/// Adapters whose key carries no `qwMemorySize` are omitted rather than reported
/// at whatever the 32-bit value says. A caller that cannot find its adapter here
/// has not learnt that the card has 4GB; it has learnt nothing, which is the
/// truth.
pub(crate) fn adapter_vram_bytes() -> Vec<(String, u64)> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    // The GUID_DEVCLASS_DISPLAY class key. Subkeys are `0000`, `0001`, ... one
    // per adapter the driver stack knows about.
    const DISPLAY_CLASS: &str =
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

    let Ok(class_key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(DISPLAY_CLASS) else {
        return Vec::new();
    };

    let names: Vec<String> = class_key.enum_keys().flatten().collect();

    names
        .into_iter()
        .filter_map(|name| {
            let key = class_key.open_subkey(&name).ok()?;
            let desc: String = key.get_value("DriverDesc").ok()?;
            let bytes: u64 = key.get_value("HardwareInformation.qwMemorySize").ok()?;
            (bytes > 0).then(|| (desc.trim().to_string(), bytes))
        })
        .collect()
}

/// Whether the machine booted via UEFI, legacy BIOS, or something unreported.
///
/// `GetFirmwareType` is the documented Win32 answer. The alternative in use before
/// this — testing `$env:firmware_type` from a spawned PowerShell — reads an
/// environment variable that Windows does not set for a non-interactive child, so it
/// was empty on every call and every machine fell through to "Legacy".
pub(crate) fn firmware_type() -> Option<crate::boot_config::BootType> {
    use windows::Win32::System::SystemInformation::{
        FirmwareTypeBios, FirmwareTypeUefi, GetFirmwareType, FIRMWARE_TYPE,
    };

    let mut kind = FIRMWARE_TYPE::default();
    unsafe { GetFirmwareType(&mut kind) }.ok()?;

    // Compared rather than matched: these are lowercase constants, and a `match` arm
    // naming one that failed to resolve would become an irrefutable binding that
    // silently swallows every other value.
    if kind == FirmwareTypeUefi {
        Some(crate::boot_config::BootType::Uefi)
    } else if kind == FirmwareTypeBios {
        Some(crate::boot_config::BootType::Legacy)
    } else {
        None
    }
}

/// Whether UEFI Secure Boot is currently enforcing.
///
/// Reads `UEFISecureBootEnabled` rather than testing for the key that contains it:
/// `SecureBoot\State` exists on every UEFI machine, enabled or not, so its presence
/// says only that the firmware is UEFI. Callers that tested existence reported Secure
/// Boot as on for every UEFI system.
pub(crate) fn secure_boot_enabled() -> Option<bool> {
    read_registry_u32(
        r"SYSTEM\CurrentControlSet\Control\SecureBoot\State",
        "UEFISecureBootEnabled",
    )
    .map(|v| v == 1)
}

/// Pagefile capacity and usage, from `Win32_PageFileUsage`.
///
/// `AllocatedBaseSize` and `CurrentUsage` are both in megabytes and describe
/// the pagefile itself -- not the commit limit, which is what every native API
/// on this platform offers and what this reader used to publish.
///
/// A machine can have several pagefiles, so the rows are summed. No pagefile at
/// all is a real reading of zero; a query that fails is `None`.
#[cfg(target_os = "windows")]
fn read_pagefile_usage() -> Option<SwapInfo> {
    use serde::Deserialize;
    use wmi::{COMLibrary, WMIConnection};

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct PageFileUsage {
        allocated_base_size: u32,
        current_usage: u32,
    }

    let com = COMLibrary::new().ok()?;
    let conn = WMIConnection::new(com).ok()?;
    let rows: Vec<PageFileUsage> = conn
        .raw_query("SELECT AllocatedBaseSize, CurrentUsage FROM Win32_PageFileUsage")
        .ok()?;

    let mb_to_kb = |mb: u32| u64::from(mb) * 1024;
    Some(SwapInfo {
        total: Some(rows.iter().map(|r| mb_to_kb(r.allocated_base_size)).sum()),
        used: Some(rows.iter().map(|r| mb_to_kb(r.current_usage)).sum()),
        cached: None,
    })
}

/// Read memory statistics on Windows
pub fn read_memory_stats() -> Result<MemoryStats> {
    let mut mem_status: MEMORYSTATUSEX = unsafe { mem::zeroed() };
    mem_status.dwLength = mem::size_of::<MEMORYSTATUSEX>() as u32;

    unsafe {
        GlobalMemoryStatusEx(&mut mem_status)
            .map_err(|e| IronError::System(format!("GlobalMemoryStatusEx failed: {}", e)))?;
    }

    // Pagefile usage.
    //
    // This used to come from `GetPerformanceInfo`:
    //
    // ```ignore
    // let total_pages = perf_info.CommitLimit as u64;
    // let used_pages  = perf_info.CommitTotal as u64;
    // ```
    //
    // Those are **commit accounting**, not pagefile accounting. The commit
    // limit is RAM plus pagefile, and the commit charge is every byte of
    // private memory the system has promised -- most of which is resident in
    // RAM and has never been written to disk. Renaming them `total_pages` and
    // `used_pages` and storing them in `SwapInfo` made the numbers say
    // something they do not mean. On the development machine:
    //
    // ```text
    // reported:  swap total 154 GB   swap used 97 GB
    // actual:    pagefile   50 GB    in use    3.4 GB
    // ```
    //
    // 97 GB of swap in use on a host with 93.6 GB of RAM, 52.8 GB of it used,
    // is not a plausible reading, and an agent asked whether the machine is
    // thrashing would have said yes. The `MEMORYSTATUSEX` fallback below it
    // had the same defect wearing a more convincing name:
    // `ullTotalPageFile` is documented as the commit limit, not the pagefile.
    //
    // `Win32_PageFileUsage` reports the real figures and needs no elevation.
    // It is a WMI round trip, which is why the physical-memory numbers above
    // still come from the native call.
    let swap_info = read_pagefile_usage().unwrap_or(SwapInfo {
        // Not read. Better nothing than the commit charge wearing this name.
        total: None,
        used: None,
        // Windows exposes no cached-swap figure. `None`, not zero: the
        // quantity is unreported, not measured at nothing.
        cached: None,
    });

    // The system file cache, in KB. `GetPerformanceInfo` reports it in pages.
    //
    // This call used to sit here computing swap from `CommitLimit` and
    // `CommitTotal`, which are commit accounting and not pagefile accounting;
    // that reading was wrong and was removed. `SystemCache` is a different
    // field of the same struct and is exactly what it says, so the call comes
    // back for the one quantity it does answer.
    let system_cache_kb = {
        let mut perf: PERFORMANCE_INFORMATION = unsafe { mem::zeroed() };
        perf.cb = mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
        if unsafe { GetPerformanceInfo(&mut perf, perf.cb) }.is_ok() {
            Some((perf.SystemCache as u64).saturating_mul(perf.PageSize as u64) / 1024)
        } else {
            None
        }
    };

    let total_kb = mem_status.ullTotalPhys / 1024;
    let avail_kb = mem_status.ullAvailPhys / 1024;
    let used_kb = total_kb - avail_kb;

    Ok(MemoryStats {
        ram: RamInfo {
            total: total_kb,
            used: used_kb,
            free: avail_kb,
            // Windows has no "buffers" figure at all -- it is a Linux
            // `/proc/meminfo` line -- so this is an absence, not a zero.
            buffers: None,
            // The system file cache, in pages, which is where Task Manager's
            // "Cached" figure comes from.
            //
            // Do not "correct" this to perfmon's `\Memory\Cache Bytes`. They
            // are different quantities and differ by fifty-fold: on this host
            // `SystemCache` reads 52.1 GB while `Cache Bytes` reads 1.0 GB,
            // because the first counts every cached page (standby 48.5 GB plus
            // modified 0.4 GB plus resident 1.0 GB) and the second counts only
            // the cache's own resident working set. The 52 GB figure is the one
            // that corresponds to Linux's `Cached` line, which is what this
            // field means.
            cached: system_cache_kb,
            // Not read on Windows. `None` rather than a zero that reads like a
            // measurement of no shared memory.
            shared: None,
            lfb: None,
        },
        swap: swap_info,
        emc: None,  // Not applicable to Windows
        iram: None, // Not applicable to Windows
    })
}

/// Get system uptime on Windows
pub fn get_system_uptime() -> std::time::Duration {
    use windows::Win32::System::SystemInformation::GetTickCount64;

    let ticks = unsafe { GetTickCount64() };
    std::time::Duration::from_millis(ticks)
}

/// Detected GPUs, kept alive across calls.
///
/// Enumeration opens vendor libraries (NVML, DXGI/WMI) and costs tens to hundreds
/// of milliseconds; [`read_gpu_stats`] is called once per tick by the interactive
/// monitor, so re-enumerating every time would dominate the tick. `Gpu` is
/// `Send + Sync`, so the collection can live in a static behind a mutex.
static GPU_COLLECTION: std::sync::OnceLock<std::sync::Mutex<crate::gpu::GpuCollection>> =
    std::sync::OnceLock::new();

/// Read GPU stats via the `gpu` module's vendor adapters.
///
/// This used to return `GpuStats::new()` unconditionally, so every consumer of
/// `IronMonitor::snapshot()` on Windows — including `ironmon cli monitor` — reported
/// "No GPUs detected" on machines where `ironmon cli gpu`, in the same binary,
/// listed every device.
///
/// A device whose per-tick query fails is omitted rather than emitted with zeroed
/// fields: a GPU reported at 0% load and 0°C is worse than a GPU not reported.
pub fn read_gpu_stats() -> Result<GpuStats> {
    use crate::core::gpu::{GpuFrequency, GpuInfo, GpuStatus, GpuType};

    let mut stats = GpuStats::new();

    let collection = GPU_COLLECTION.get_or_init(|| {
        std::sync::Mutex::new(crate::gpu::GpuCollection::auto_detect().unwrap_or_default())
    });
    let collection = collection
        .lock()
        .map_err(|_| IronError::HardwareError("GPU collection mutex poisoned".to_string()))?;

    let snapshots = collection.snapshot_all_partial();
    let gpus = stats.gpus_mut();

    for info in snapshots.into_iter().flatten() {
        let si = &info.static_info;
        let di = &info.dynamic_info;

        let status = GpuStatus {
            load: di.utilization.map(f32::from),
            railgate: None,
            tpc_pg_mask: None,
            scaling_3d: None,
            // This already treated a zero total as an absence and wrapped the
            // three figures in `Some` only when it was non-zero. The type says
            // it now, so the guard is gone rather than duplicated.
            memory_used: di.memory.used,
            memory_total: di.memory.total,
            memory_free: di.memory.free,
            // `GpuThermal::temperature` is already degrees Celsius.
            temperature: di.thermal.temperature.map(|t| t as f32),
            // `GpuPower` is milliwatts; `GpuStatus` is watts.
            power_draw: di.power.draw.map(|mw| mw as f32 / 1000.0),
            power_limit: di.power.limit.map(|mw| mw as f32 / 1000.0),
        };

        let frequency = GpuFrequency {
            current: di.clocks.graphics.or(di.clocks.sm).unwrap_or(0),
            min: 0, // No vendor adapter reports a minimum clock.
            max: di.clocks.graphics_max.unwrap_or(0),
            // `governor` is a Linux DVFS concept; Windows has no equivalent, and
            // naming the vendor here would present it as one.
            governor: String::new(),
            gpc: None,
        };

        let gpu = GpuInfo {
            gpu_type: if si.integrated {
                GpuType::Integrated
            } else {
                GpuType::Discrete
            },
            status,
            frequency,
            power_control: si.vendor.to_string(),
        };

        // `GpuStats` is keyed by name, and identical cards are common (two
        // RTX 3090s report the same string). Disambiguate by index so a
        // second card cannot silently overwrite the first.
        let key = if gpus.contains_key(&si.name) {
            format!("{} #{}", si.name, si.index)
        } else {
            si.name.clone()
        };
        gpus.insert(key, gpu);
    }

    Ok(stats)
}

/// Read power rails on Windows.
///
/// Windows exposes no general system power telemetry, so the only rail available is
/// a battery or UPS, and only when its driver publishes a real charge or discharge
/// rate through `root\WMI\BatteryStatus`.
///
/// This previously derived "power" from `Win32_Battery` as
/// `full_charge_capacity * design_voltage / 1000 / 3` — the pack's total energy
/// spread over an assumed three-hour discharge. That is not a measurement of
/// anything: it is a constant for a given battery, unchanged by what the machine is
/// doing, and it was reported as a live power rail. When those fields were absent (a
/// USB-attached UPS reports neither) it collapsed to 0, so `ironmon cli monitor`
/// displayed "Total Power: 0.00W" on a desktop drawing hundreds of watts.
///
/// A rail is now emitted only when a rate was actually read. A machine with no
/// battery, or a UPS whose driver publishes no rate, reports no rails at all, and
/// callers should say "not measured" rather than "0 W".
pub fn read_power_stats() -> Result<PowerStats> {
    use serde::Deserialize;
    use wmi::{COMLibrary, WMIConnection};

    /// `root\WMI` battery telemetry. Rates are milliwatts, voltage millivolts.
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct BatteryStatus {
        instance_name: Option<String>,
        voltage: Option<u32>,
        discharge_rate: Option<u32>,
        charge_rate: Option<u32>,
        discharging: Option<bool>,
    }

    let mut power_stats = PowerStats::default();

    let Ok(com_con) = COMLibrary::new() else {
        return Ok(power_stats);
    };
    // `BatteryStatus` lives in root\WMI, not root\CIMV2, and is absent entirely on
    // desktops and on UPS units attached over USB HID.
    let Ok(wmi_con) = WMIConnection::with_namespace_path("root\\WMI", com_con) else {
        return Ok(power_stats);
    };

    let batteries: Vec<BatteryStatus> = wmi_con
        .raw_query(
            "SELECT InstanceName, Voltage, DischargeRate, ChargeRate, Discharging \
             FROM BatteryStatus",
        )
        .unwrap_or_default();

    for (idx, battery) in batteries.iter().enumerate() {
        // At most one rate is non-zero at a time; whichever it is, it is measured.
        // Both present and zero means idle on AC, which is also a real reading.
        let (rate_mw, sensor_type) = match (battery.discharge_rate, battery.charge_rate) {
            (Some(d), _) if d > 0 => (d, "Battery (discharging)"),
            (_, Some(c)) if c > 0 => (c, "Battery (charging)"),
            (Some(_), _) | (_, Some(_)) => (
                0,
                if battery.discharging.unwrap_or(false) {
                    "Battery (discharging)"
                } else {
                    "Battery (idle)"
                },
            ),
            // Neither rate was reported: nothing was measured, so emit nothing.
            (None, None) => continue,
        };

        let name = battery
            .instance_name
            .clone()
            .unwrap_or_else(|| format!("Battery{}", idx));

        power_stats.rails.insert(
            name,
            crate::core::power::PowerRail {
                online: true,
                sensor_type: sensor_type.to_string(),
                voltage: battery.voltage.unwrap_or(0),
                current: 0, // Not exposed by this class.
                power: rate_mw,
                average: rate_mw,
                warn: None,
                crit: None,
            },
        );

        power_stats.total.power += rate_mw;
        power_stats.total.average += rate_mw;
    }

    Ok(power_stats)
}

/// Read temperature stats from WMI thermal zones and optionally Open Hardware Monitor
pub fn read_temperature_stats() -> Result<TemperatureStats> {
    use serde::Deserialize;
    use std::collections::HashMap;
    use wmi::{COMLibrary, WMIConnection};

    // WMI thermal zone structure (in tenths of Kelvin)
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct ThermalZone {
        instance_name: String,
        current_temperature: u32, // In tenths of Kelvin
    }

    // Performance counter thermal zone (Kelvin)
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct PerfThermalZone {
        name: String,
        temperature: u32, // In Kelvin (not tenths!)
    }

    // Open Hardware Monitor sensor structure (if OHM is installed)
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    #[allow(dead_code)]
    struct OhmSensor {
        name: String,
        sensor_type: String,
        value: f32,
        parent: String,
    }

    let mut sensors = HashMap::new();

    // Initialize COM library
    let com_con = COMLibrary::new()
        .map_err(|e| IronError::System(format!("Failed to initialize COM: {}", e)))?;

    // Try to get CPU temperature from Open Hardware Monitor if available
    // OHM exposes sensors via WMI in root\OpenHardwareMonitor namespace
    if let Ok(ohm_con) = WMIConnection::with_namespace_path("root\\OpenHardwareMonitor", com_con) {
        let ohm_sensors: Vec<OhmSensor> = ohm_con
            .raw_query("SELECT Name, SensorType, Value, Parent FROM Sensor WHERE SensorType = 'Temperature'")
            .unwrap_or_default();

        for sensor in ohm_sensors {
            // Filter to CPU and motherboard temperatures
            let sensor_name = if sensor.parent.contains("CPU") || sensor.name.contains("CPU") {
                format!("CPU-{}", sensor.name.replace(' ', "_"))
            } else if sensor.parent.contains("Motherboard") {
                format!("MB-{}", sensor.name.replace(' ', "_"))
            } else {
                continue; // Skip other sensors (GPU temps come from NVML)
            };

            sensors.insert(
                sensor_name,
                crate::core::temperature::TemperatureSensor {
                    online: true,
                    temp: sensor.value,
                    max: Some(95.0),
                    crit: Some(105.0),
                },
            );
        }
    }

    // Try LibreHardwareMonitor as well (fork of OHM with better modern hardware support)
    if let Ok(lhm_con) = WMIConnection::with_namespace_path("root\\LibreHardwareMonitor", com_con) {
        let lhm_sensors: Vec<OhmSensor> = lhm_con
            .raw_query("SELECT Name, SensorType, Value, Parent FROM Sensor WHERE SensorType = 'Temperature'")
            .unwrap_or_default();

        for sensor in lhm_sensors {
            // Filter to CPU and motherboard temperatures
            let sensor_name = if sensor.parent.contains("CPU") || sensor.name.contains("CPU") {
                format!("CPU-{}", sensor.name.replace(' ', "_"))
            } else if sensor.parent.contains("Motherboard") {
                format!("MB-{}", sensor.name.replace(' ', "_"))
            } else {
                continue;
            };

            // Only add if not already present from OHM
            if let std::collections::hash_map::Entry::Vacant(e) = sensors.entry(sensor_name) {
                e.insert(crate::core::temperature::TemperatureSensor {
                    online: true,
                    temp: sensor.value,
                    max: Some(95.0),
                    crit: Some(105.0),
                });
            }
        }
    }

    // Try CIMV2 performance counters for thermal zone info (more widely available)
    if let Ok(cimv2_con) = WMIConnection::with_namespace_path("root\\CIMV2", com_con) {
        let perf_zones: Vec<PerfThermalZone> = cimv2_con
            .raw_query("SELECT Name, Temperature FROM Win32_PerfFormattedData_Counters_ThermalZoneInformation")
            .unwrap_or_default();

        for zone in perf_zones {
            // Temperature is in Kelvin, convert to Celsius
            let temp_celsius = zone.temperature as f32 - 273.15;

            // Only add valid temperatures
            if temp_celsius > 0.0 && temp_celsius < 150.0 {
                let sensor_name = format!("TZ-{}", zone.name.replace("\\_TZ.", ""));
                sensors
                    .entry(sensor_name)
                    .or_insert(crate::core::temperature::TemperatureSensor {
                        online: true,
                        temp: temp_celsius,
                        max: Some(100.0),
                        crit: Some(105.0),
                    });
            }
        }
    }

    // Try ACPI thermal zones in root\WMI (requires admin, but try anyway)
    if let Ok(wmi_con) = WMIConnection::with_namespace_path("root\\WMI", com_con) {
        let thermal_zones: Vec<ThermalZone> = wmi_con
            .raw_query("SELECT InstanceName, CurrentTemperature FROM MSAcpi_ThermalZoneTemperature")
            .unwrap_or_default();

        for zone in thermal_zones {
            // Convert from tenths of Kelvin to Celsius
            let temp_celsius = (zone.current_temperature as f32 / 10.0) - 273.15;

            // Only add valid temperatures (ignore invalid readings)
            if temp_celsius > 0.0 && temp_celsius < 150.0 {
                let sensor_name = zone
                    .instance_name
                    .replace("ACPI\\ThermalZone\\", "")
                    .replace("_0", "");
                if !sensors.contains_key(&format!("TZ-{}", sensor_name)) {
                    sensors.insert(
                        format!("ACPI-{}", sensor_name),
                        crate::core::temperature::TemperatureSensor {
                            online: true,
                            temp: temp_celsius,
                            max: Some(100.0),
                            crit: Some(105.0),
                        },
                    );
                }
            }
        }
    }

    Ok(TemperatureStats { sensors })
}

pub fn detect_platform() -> Result<BoardInfo> {
    // SMBIOS baseboard fields, as the firmware published them at boot. Formerly read
    // by shelling out to `wmic baseboard`, which no longer exists on Windows 11 24H2
    // and later — every board there reported itself as "Unknown".
    const BIOS_KEY: &str = r"HARDWARE\DESCRIPTION\System\BIOS";
    let manufacturer = read_registry_string(BIOS_KEY, "BaseBoardManufacturer");
    let model = read_registry_string(BIOS_KEY, "BaseBoardProduct");

    const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

    // `release` is the OS release — the analogue of `uname -r`. It was the constant
    // "NT", which names the kernel lineage and is the same string on every Windows
    // since 1993, so `ironmon cli board` printed "Kernel: NT" as though it had read
    // something. `distribution` was left empty despite the edition being published
    // right next to the build number.
    let build = read_registry_string(CURRENT_VERSION, "CurrentBuildNumber");
    let release = match (&build, read_registry_u32(CURRENT_VERSION, "UBR")) {
        (Some(build), Some(ubr)) => format!("{build}.{ubr}"),
        (Some(build), None) => build.clone(),
        // Nothing was read, so nothing is claimed.
        (None, _) => String::new(),
    };

    let distribution = read_registry_string(CURRENT_VERSION, "ProductName").map(|product| {
        // The registry still says "Windows 10" on Windows 11; build 22000 is the
        // documented dividing line.
        let build: u32 = build.as_deref().and_then(|b| b.parse().ok()).unwrap_or(0);
        if build >= 22000 {
            product.replace("Windows 10", "Windows 11")
        } else {
            product
        }
    });

    Ok(BoardInfo {
        platform: PlatformInfo {
            machine: std::env::consts::ARCH.to_string(),
            system: "Windows".to_string(),
            distribution,
            release,
        },
        hardware: HardwareInfo {
            model: model.unwrap_or_else(|| "Unknown".to_string()),
            p_number: None,
            module: manufacturer,
            soc: None,
            cuda_arch: None,
            codename: None,
            serial_number: None,
            l4t: None,
            jetpack: None,
        },
        libraries: LibraryVersions {
            cuda: None,
            cudnn: None,
            tensorrt: None,
            other: HashMap::new(),
        },
    })
}

/// Read process statistics on Windows using CreateToolhelp32Snapshot
pub fn read_process_stats() -> Result<crate::core::process::ProcessStats> {
    use crate::core::process::{ProcessInfo, ProcessStats};
    use std::mem;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    let mut stats = ProcessStats::empty();

    unsafe {
        // Create snapshot of all processes
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .map_err(|e| IronError::System(format!("Failed to create process snapshot: {}", e)))?;

        let mut entry: PROCESSENTRY32W = mem::zeroed();
        entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;

        // Iterate through processes
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let pid = entry.th32ProcessID;

                // Get process name from entry
                let name_len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..name_len]);

                // Try to open process for more info
                let mut memory_kb = 0u64;
                let mut cpu_percent = 0.0f32;

                if let Ok(process_handle) =
                    OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
                {
                    // Get memory info
                    let mut mem_counters: PROCESS_MEMORY_COUNTERS = mem::zeroed();
                    mem_counters.cb = mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

                    if GetProcessMemoryInfo(
                        process_handle,
                        &mut mem_counters,
                        mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                    )
                    .is_ok()
                    {
                        memory_kb = mem_counters.WorkingSetSize as u64 / 1024;
                    }

                    // Get CPU times for utilization calculation
                    let mut creation_time = mem::zeroed();
                    let mut exit_time = mem::zeroed();
                    let mut kernel_time = mem::zeroed();
                    let mut user_time = mem::zeroed();

                    if GetProcessTimes(
                        process_handle,
                        &mut creation_time,
                        &mut exit_time,
                        &mut kernel_time,
                        &mut user_time,
                    )
                    .is_ok()
                    {
                        let kernel = (kernel_time.dwHighDateTime as u64) << 32
                            | kernel_time.dwLowDateTime as u64;
                        let user = (user_time.dwHighDateTime as u64) << 32
                            | user_time.dwLowDateTime as u64;

                        // Simple estimate: total CPU time / uptime
                        let total_time = kernel + user;
                        let uptime_100ns = get_system_uptime().as_nanos() as u64 / 100;
                        if uptime_100ns > 0 {
                            cpu_percent = (total_time as f64 / uptime_100ns as f64 * 100.0) as f32;
                            cpu_percent = cpu_percent.min(100.0);
                        }
                    }

                    let _ = CloseHandle(process_handle);
                }

                // Determine process state based on thread count
                let state = if entry.cntThreads > 0 { 'R' } else { 'S' };

                // Only include processes with significant memory usage (> 1MB)
                if memory_kb > 1024 {
                    stats.processes.push(ProcessInfo {
                        pid,
                        user: String::new(), // Would need additional API calls
                        gpu: String::new(),
                        process_type: "System".to_string(),
                        priority: entry.pcPriClassBase as i32,
                        state,
                        cpu_percent,
                        memory_kb,
                        gpu_memory_kb: 0, // Filled in by GPU module
                        name,
                    });
                }

                // Move to next process
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }

    // Sort by memory usage (descending)
    stats
        .processes
        .sort_by_key(|p| std::cmp::Reverse(p.memory_kb));

    // Keep top 50 processes
    stats.processes.truncate(50);

    Ok(stats)
}

// ============================================================================
// Logical drive enumeration
// ============================================================================

/// A mounted logical drive with capacity and filesystem information.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogicalDrive {
    /// Drive root, e.g. `C:\`.
    pub name: String,
    /// Total capacity in bytes.
    pub total: u64,
    /// Used bytes (total minus free).
    pub used: u64,
    /// Free bytes available to the caller.
    pub free: u64,
    /// Filesystem name as reported by the volume, e.g. `NTFS`, `exFAT`.
    pub filesystem: String,
}

/// Enumerate mounted logical drives.
///
/// This is deliberately cheaper and safer than probing `A:`..`Z:` with
/// `fs::metadata`:
///
/// - `GetLogicalDrives` returns the present-drive bitmask in a single call rather
///   than 26 filesystem round-trips.
/// - Network (`DRIVE_REMOTE`) and optical (`DRIVE_CDROM`) volumes are skipped.
///   A disconnected mapped network drive can block for seconds on capacity queries,
///   which would stall the collector tick.
/// - The filesystem name comes from `GetVolumeInformationW` instead of being
///   hardcoded to `"NTFS"`.
pub fn logical_drives() -> Result<Vec<LogicalDrive>> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;

    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Err(IronError::Other(
            "GetLogicalDrives returned no drives".to_string(),
        ));
    }

    let mut drives = Vec::new();

    for bit in 0..26u32 {
        if mask & (1 << bit) == 0 {
            continue;
        }

        let letter = b'A' + bit as u8;
        // Drive roots are pure ASCII, so build the wide string directly rather than
        // allocating through OsStr.
        let root_wide: [u16; 4] = [letter as u16, b':' as u16, b'\\' as u16, 0];
        let root = PCWSTR(root_wide.as_ptr());

        // Skip volumes whose capacity queries can block.
        let drive_type = unsafe { GetDriveTypeW(root) };
        if drive_type == DRIVE_REMOTE || drive_type == DRIVE_CDROM {
            continue;
        }

        let mut free_to_caller: u64 = 0;
        let mut total: u64 = 0;
        let mut total_free: u64 = 0;

        let has_space = unsafe {
            GetDiskFreeSpaceExW(
                root,
                Some(&mut free_to_caller),
                Some(&mut total),
                Some(&mut total_free),
            )
            .is_ok()
        };

        // An empty card reader reports as present but has no media.
        if !has_space || total == 0 {
            continue;
        }

        let mut fs_buf = [0u16; 32];
        let filesystem = unsafe {
            if GetVolumeInformationW(root, None, None, None, None, Some(&mut fs_buf)).is_ok() {
                let len = fs_buf.iter().position(|&c| c == 0).unwrap_or(fs_buf.len());
                String::from_utf16_lossy(&fs_buf[..len])
            } else {
                "Unknown".to_string()
            }
        };

        drives.push(LogicalDrive {
            name: format!("{}:\\", letter as char),
            total,
            used: total.saturating_sub(total_free),
            free: free_to_caller,
            filesystem,
        });
    }

    Ok(drives)
}

#[cfg(test)]
mod tests {

    /// The nominal clock must not be reported as the current one.
    ///
    /// `CallNtPowerInformation` returns `CurrentMhz == MaxMhz` on Windows 10 and
    /// later whatever the cores are doing, so publishing it put a specification
    /// behind `cpu.core.{n}.frequency`, which is declared `Measured` and
    /// described as the current clock. Every conformance test passed, because
    /// none of them ask whether a value claiming to be current ever changes.
    #[test]
    fn a_nominal_clock_is_not_reported_as_the_current_one() {
        let entry = |current_mhz, max_mhz| ProcessorPowerInformation {
            number: 0,
            max_mhz,
            current_mhz,
            mhz_limit: 0,
            max_idle_state: 0,
            current_idle_state: 0,
        };

        // The case this fixes: 4400/4400 on all 24 cores of a 9900X that was
        // actually boosting past 5GHz.
        let power = vec![entry(4400, 4400)];
        let f =
            get_cpu_frequency(Some(&power), 0, None).expect("max is known, so a row is returned");
        assert_eq!(
            f.current, None,
            "the nominal clock reported as current is not a reading"
        );
        assert_eq!(f.max, Some(4400), "the maximum is still known");

        // A genuine current reading below the maximum survives untouched.
        let power = vec![entry(3200, 4400)];
        let f = get_cpu_frequency(Some(&power), 0, None).expect("a real reading");
        assert_eq!(f.current, Some(3200));

        // Nothing known at all stays absent rather than becoming a zero.
        let power = vec![entry(0, 0)];
        assert!(get_cpu_frequency(Some(&power), 0, None).is_none());
    }
    use super::*;

    /// Per-core figures must be genuinely per-core, not one number replicated.
    ///
    /// This is the regression that motivated the NtQuerySystemInformation path:
    /// `GetSystemTimes` is system-wide, so every core previously reported the same
    /// value and a 24-core box drew 24 identical bars that looked measured.
    /// The aggregate figure must not contradict the per-core breakdown.
    ///
    /// `get_system_cpu_utilization` used to return a hardcoded (0, 0, 100) on its
    /// first call, so any one-shot invocation reported the machine as completely idle
    /// while the per-core rows directly beneath it showed real load. Both now fall
    /// back to since-boot totals when there is no previous sample, so they agree.
    #[test]
    fn aggregate_cpu_agrees_with_per_core() {
        let stats = read_cpu_stats().expect("cpu stats");
        assert!(!stats.cores.is_empty(), "no cores reported");

        let aggregate_busy = 100.0 - stats.total.idle;

        let per_core: Vec<f32> = stats
            .cores
            .iter()
            .filter_map(|c| c.idle.map(|idle| 100.0 - idle))
            .collect();

        // Cores may report None if the per-processor query failed; nothing to compare
        // against in that case.
        if per_core.is_empty() {
            eprintln!("skipping: per-core data unavailable");
            return;
        }

        let mean_busy = per_core.iter().sum::<f32>() / per_core.len() as f32;
        eprintln!("aggregate busy {aggregate_busy:.1}%, per-core mean {mean_busy:.1}%");

        // The two use different sampling bases, so they will not match exactly. What
        // must not happen is one claiming idle while the other reports load.
        assert!(
            !(aggregate_busy < 1.0 && mean_busy > 10.0),
            "aggregate reports {aggregate_busy:.1}% busy while cores average \
             {mean_busy:.1}% — the aggregate is a placeholder, not a measurement"
        );
    }

    #[test]
    fn per_core_utilization_is_not_the_system_average() {
        // `available_parallelism` rather than `num_cpus::get`: num_cpus is an
        // optional dependency enabled by `cli`, so naming it here made the library's
        // own test target fail to build on any feature set without the CLI.
        let cpu_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);

        // Assert on the *cumulative* counters rather than instantaneous percentages.
        //
        // An earlier version of this test loaded one thread and asserted the busiest
        // and least-busy cores differed. That is flaky by construction: run under a
        // saturated machine (the full test suite, or CI) every core legitimately
        // reads 100% busy, and the test fails on correct data.
        //
        // Idle time accumulated since boot, by contrast, differs per core on any real
        // system regardless of present load — and is exactly what would be identical
        // if the values were the system aggregate replicated, which was the bug.
        let raw = query_per_core_times(cpu_count)
            .expect("NtQuerySystemInformation returned no per-processor data");
        assert!(!raw.is_empty(), "no per-processor records returned");

        if raw.len() > 1 {
            let distinct = raw
                .iter()
                .map(|r| r.idle_time)
                .collect::<std::collections::HashSet<_>>()
                .len();
            eprintln!(
                "{} distinct cumulative idle times across {} cores",
                distinct,
                raw.len()
            );
            assert!(
                distinct > 1,
                "all {} cores report identical cumulative idle time — the values are \
                 a single system-wide figure replicated, not per-core data",
                raw.len()
            );
        }

        // Percentages must still be well-formed, whatever the load happens to be.
        let cores = read_per_core_utilization(cpu_count).expect("per-core utilization");
        for (i, (user, system, idle)) in cores.iter().enumerate() {
            let total = user + system + idle;
            assert!(
                (total - 100.0).abs() < 1.0,
                "core {i} percentages sum to {total}, expected ~100"
            );
            assert!(
                (0.0..=100.0).contains(idle),
                "core {i} idle {idle} out of range"
            );
        }
    }

    #[test]
    fn test_logical_drives() {
        let drives = logical_drives().expect("enumeration should succeed");
        assert!(
            !drives.is_empty(),
            "a Windows host always has at least one fixed drive"
        );
        for drive in &drives {
            assert!(drive.total > 0, "{} reported zero capacity", drive.name);
            assert!(
                drive.used <= drive.total,
                "{} used {} exceeds total {}",
                drive.name,
                drive.used,
                drive.total
            );
            assert!(
                drive.name.ends_with(":\\"),
                "{} is not a drive root",
                drive.name
            );
            assert!(!drive.filesystem.is_empty());
        }
    }

    #[test]
    fn test_read_cpu_stats() {
        let stats = read_cpu_stats();
        assert!(stats.is_ok());
        let stats = stats.unwrap();
        assert!(!stats.cores.is_empty());
    }

    #[test]
    fn test_read_memory_stats() {
        let stats = read_memory_stats();
        assert!(stats.is_ok());
        let stats = stats.unwrap();
        assert!(stats.ram.total > 0);
    }

    #[test]
    fn test_get_system_uptime() {
        let uptime = get_system_uptime();
        assert!(uptime.as_secs() > 0);
    }
}
