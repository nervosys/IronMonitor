//! Main statistics collection and management

use crate::core::{
    cpu::CpuStats, engine::EngineStats, gpu::GpuStats, memory::MemoryStats,
    platform_info::BoardInfo, power::PowerStats, process::ProcessStats,
    temperature::TemperatureStats,
};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Complete system snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// CPU statistics
    pub cpu: CpuStats,
    /// GPU statistics
    pub gpus: std::collections::HashMap<String, crate::core::gpu::GpuInfo>,
    /// Memory statistics
    pub memory: MemoryStats,
    /// Power statistics
    pub power: PowerStats,
    /// Temperature statistics
    pub temperature: TemperatureStats,
    /// Fan statistics
    pub fans: std::collections::HashMap<String, crate::core::fan::FanInfo>,
    /// Board information
    pub board: BoardInfo,
    /// Process statistics
    pub processes: ProcessStats,
    /// Engine statistics
    pub engines: EngineStats,
    /// Uptime in seconds
    pub uptime: Duration,
}

/// Main IronMonitor interface
pub struct IronMonitor {
    /// Update interval
    interval: Duration,
    /// Last snapshot
    last_snapshot: Option<Snapshot>,
    /// Platform information (cached)
    board_info: BoardInfo,
}

impl IronMonitor {
    /// Create a new IronMonitor instance
    ///
    /// # Arguments
    ///
    /// * `interval` - Update interval in seconds (default: 1.0)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use ironmonlib::IronMonitor;
    ///
    /// let stats = IronMonitor::new().unwrap();
    /// ```
    pub fn new() -> Result<Self> {
        Self::with_interval(1.0)
    }

    /// Create a new IronMonitor instance with custom interval
    ///
    /// # Arguments
    ///
    /// * `interval` - Update interval in seconds
    ///
    /// # Example
    ///
    /// ```no_run
    /// use ironmonlib::IronMonitor;
    ///
    /// let stats = IronMonitor::with_interval(0.5).unwrap();
    /// ```
    pub fn with_interval(interval: f64) -> Result<Self> {
        let interval = Duration::from_secs_f64(interval);

        // Detect platform information once
        let board_info = detect_platform_info()?;

        Ok(Self {
            interval,
            last_snapshot: None,
            board_info,
        })
    }

    /// Get current snapshot of all statistics
    ///
    /// # Example
    ///
    /// ```no_run
    /// use ironmonlib::IronMonitor;
    ///
    /// let mut stats = IronMonitor::new().unwrap();
    /// let snapshot = stats.snapshot().unwrap();
    /// println!("CPU cores: {}", snapshot.cpu.cores.len());
    /// ```
    pub fn snapshot(&mut self) -> Result<Snapshot> {
        // Read all stats
        let cpu = read_cpu_stats()?;
        let gpu_stats = read_gpu_stats()?;
        let memory = read_memory_stats()?;
        let power = read_power_stats()?;
        let temperature = read_temperature_stats()?;

        // Read fan stats
        let fans = read_fan_stats();

        // Read process stats
        let processes = read_process_stats()?;

        // Read engine stats
        let engines = read_engine_stats()?;

        // Read uptime
        let uptime = read_uptime()?;

        let snapshot = Snapshot {
            cpu,
            gpus: gpu_stats.gpus().clone(),
            memory,
            power,
            temperature,
            fans,
            board: self.board_info.clone(),
            processes,
            engines,
            uptime,
        };

        self.last_snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }

    /// Read CPU statistics alone.
    ///
    /// [`Self::snapshot`] needs every reader to succeed, so on a platform where any
    /// one of them is unimplemented it returns an error and the rest are
    /// unreachable. macOS is that platform today: CPU, memory, uptime and board
    /// info are implemented while GPU, power and temperature are not. These
    /// accessors expose what does work without the others having to pretend.
    pub fn cpu(&self) -> Result<CpuStats> {
        read_cpu_stats()
    }

    /// Read memory statistics alone. See [`Self::cpu`].
    pub fn memory(&self) -> Result<MemoryStats> {
        read_memory_stats()
    }

    /// Read system uptime alone. See [`Self::cpu`].
    pub fn uptime(&self) -> Result<Duration> {
        read_uptime()
    }

    /// Get the update interval
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Get board information
    pub fn board_info(&self) -> &BoardInfo {
        &self.board_info
    }

    /// Get last snapshot (if available)
    pub fn last_snapshot(&self) -> Option<&Snapshot> {
        self.last_snapshot.as_ref()
    }
}

/// Seconds since boot, without building a [`IronMonitor`].
///
/// [`IronMonitor::uptime`] needs an instance, and constructing one collects board
/// information a caller who only wants uptime has no use for. The agent tool
/// surface reported `uptime_seconds: 0` with the comment "would need
/// platform-specific impl" while `read_uptime` had one on all three platforms;
/// the missing piece was a way to reach it, not the reader.
pub fn uptime() -> Result<Duration> {
    read_uptime()
}

// Platform-specific implementations
#[cfg(target_os = "linux")]
fn read_cpu_stats() -> Result<CpuStats> {
    crate::platform::linux::read_cpu_stats()
}

#[cfg(target_os = "linux")]
fn read_gpu_stats() -> Result<GpuStats> {
    crate::platform::linux::read_gpu_stats()
}

#[cfg(target_os = "linux")]
fn read_memory_stats() -> Result<MemoryStats> {
    crate::platform::linux::read_memory_stats()
}

#[cfg(target_os = "linux")]
fn read_power_stats() -> Result<PowerStats> {
    crate::platform::linux::read_power_stats()
}

#[cfg(target_os = "linux")]
fn read_temperature_stats() -> Result<TemperatureStats> {
    crate::platform::linux::read_temperature_stats()
}

#[cfg(target_os = "linux")]
fn detect_platform_info() -> Result<BoardInfo> {
    crate::platform::linux::detect_platform()
}

#[cfg(target_os = "linux")]
fn read_uptime() -> Result<Duration> {
    use std::fs;
    let uptime_str = fs::read_to_string("/proc/uptime")?;
    let uptime_secs: f64 = uptime_str
        .split_whitespace()
        .next()
        .ok_or_else(|| crate::error::IronError::Parse("Invalid uptime format".to_string()))?
        .parse()
        .map_err(|e| crate::error::IronError::Parse(format!("Failed to parse uptime: {}", e)))?;
    Ok(Duration::from_secs_f64(uptime_secs))
}

#[cfg(target_os = "linux")]
fn read_process_stats() -> Result<ProcessStats> {
    crate::core::process::linux::read_process_stats()
}

#[cfg(target_os = "linux")]
fn read_engine_stats() -> Result<EngineStats> {
    crate::core::engine::linux::read_engine_stats()
}

#[cfg(windows)]
fn read_cpu_stats() -> Result<CpuStats> {
    crate::platform::windows::read_cpu_stats()
}

#[cfg(windows)]
fn read_gpu_stats() -> Result<GpuStats> {
    crate::platform::windows::read_gpu_stats()
}

#[cfg(windows)]
fn read_memory_stats() -> Result<MemoryStats> {
    crate::platform::windows::read_memory_stats()
}

#[cfg(windows)]
fn read_power_stats() -> Result<PowerStats> {
    crate::platform::windows::read_power_stats()
}

#[cfg(windows)]
fn read_temperature_stats() -> Result<TemperatureStats> {
    crate::platform::windows::read_temperature_stats()
}

#[cfg(windows)]
fn detect_platform_info() -> Result<BoardInfo> {
    crate::platform::windows::detect_platform()
}

#[cfg(windows)]
fn read_uptime() -> Result<Duration> {
    Ok(crate::platform::windows::get_system_uptime())
}

#[cfg(windows)]
fn read_process_stats() -> Result<ProcessStats> {
    crate::platform::windows::read_process_stats()
}

#[cfg(windows)]
fn read_engine_stats() -> Result<EngineStats> {
    // Windows doesn't have the same engine concept as Jetson
    // Return empty stats
    Ok(EngineStats::default())
}

/// Read fan stats - returns empty map on failure (non-critical)
#[cfg(target_os = "linux")]
fn read_fan_stats() -> std::collections::HashMap<String, crate::core::fan::FanInfo> {
    use crate::core::fan::FanInfo;
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    let mut fans = HashMap::new();

    // Look for fans in hwmon
    let hwmon_path = Path::new("/sys/class/hwmon");
    if let Ok(entries) = fs::read_dir(hwmon_path) {
        for entry in entries.flatten() {
            let hwmon_dir = entry.path();

            // Get device name
            let name = fs::read_to_string(hwmon_dir.join("name"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string());

            // Look for fan inputs (fan1_input, fan2_input, etc.)
            for i in 1..=10 {
                let fan_input = hwmon_dir.join(format!("fan{}_input", i));
                if !fan_input.exists() {
                    continue;
                }

                // Read RPM
                let rpm = fs::read_to_string(&fan_input)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .unwrap_or(0);

                // Try to read PWM (speed control)
                let pwm_path = hwmon_dir.join(format!("pwm{}", i));
                let speed_percent = if pwm_path.exists() {
                    fs::read_to_string(&pwm_path)
                        .ok()
                        .and_then(|s| s.trim().parse::<u32>().ok())
                        .map(|pwm| pwm * 100 / 255) // PWM is 0-255
                        .unwrap_or(0)
                } else {
                    // If no PWM, estimate from RPM (assume max 5000 RPM)
                    std::cmp::min(rpm / 50, 100)
                };

                fans.insert(
                    format!("{}-fan{}", name, i),
                    FanInfo {
                        speed: vec![speed_percent],
                        rpm: Some(vec![rpm]),
                        profile: "auto".to_string(),
                        governor: None,
                        control: None,
                    },
                );
            }
        }
    }

    // Also check for Jetson-style thermal cooling devices
    let cooling_path = Path::new("/sys/class/thermal");
    if let Ok(entries) = fs::read_dir(cooling_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !name.starts_with("cooling_device") {
                continue;
            }

            // Read current state and max state
            let cur_state = fs::read_to_string(path.join("cur_state"))
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(0);
            let max_state = fs::read_to_string(path.join("max_state"))
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(1);

            let speed_percent = (cur_state * 100).checked_div(max_state).unwrap_or(0);

            // Get device type
            let device_type = fs::read_to_string(path.join("type"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "fan".to_string());

            if device_type.contains("fan") || device_type.contains("pwm") {
                fans.insert(
                    format!("{}", name),
                    FanInfo {
                        speed: vec![speed_percent],
                        rpm: None,
                        profile: "thermal".to_string(),
                        governor: None,
                        control: Some(device_type),
                    },
                );
            }
        }
    }

    fans
}

/// Read fan stats on Windows using WMI
#[cfg(windows)]
fn read_fan_stats() -> std::collections::HashMap<String, crate::core::fan::FanInfo> {
    use crate::core::fan::FanInfo;
    use serde::Deserialize;
    use std::collections::HashMap;
    use wmi::{COMLibrary, WMIConnection};

    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct Win32Fan {
        name: Option<String>,
        active_cooling: Option<bool>,
    }

    let mut fans = HashMap::new();

    // Try WMI fan class
    if let Ok(com_con) = COMLibrary::new() {
        if let Ok(wmi_con) = WMIConnection::with_namespace_path("root\\CIMV2", com_con) {
            // Try Win32_Fan class (may not be populated on all systems)
            let wmi_fans: Vec<Win32Fan> = wmi_con
                .raw_query("SELECT Name, ActiveCooling FROM Win32_Fan")
                .unwrap_or_default();

            for (idx, fan) in wmi_fans.iter().enumerate() {
                let name = fan.name.clone().unwrap_or_else(|| format!("Fan{}", idx));
                let active = fan.active_cooling.unwrap_or(false);

                fans.insert(
                    name.clone(),
                    FanInfo {
                        speed: vec![if active { 100 } else { 0 }],
                        rpm: None,
                        profile: if active { "active" } else { "idle" }.to_string(),
                        governor: None,
                        control: None,
                    },
                );
            }
        }

        // Try OpenHardwareMonitor/LibreHardwareMonitor for detailed fan info
        if let Ok(lhm_con) =
            WMIConnection::with_namespace_path("root\\LibreHardwareMonitor", com_con)
        {
            #[derive(Deserialize, Debug)]
            #[serde(rename_all = "PascalCase")]
            #[allow(dead_code)]
            struct LhmSensor {
                name: String,
                sensor_type: String,
                value: f32,
                parent: String,
            }

            let lhm_fans: Vec<LhmSensor> = lhm_con
                .raw_query(
                    "SELECT Name, SensorType, Value, Parent FROM Sensor WHERE SensorType = 'Fan'",
                )
                .unwrap_or_default();

            for fan in lhm_fans {
                let name = format!(
                    "{}-{}",
                    fan.parent.split('/').next_back().unwrap_or("MB"),
                    fan.name
                );
                fans.insert(
                    name,
                    FanInfo {
                        speed: vec![(fan.value / 50.0).min(100.0) as u32], // Rough estimate from RPM
                        rpm: Some(vec![fan.value as u32]),
                        profile: "auto".to_string(),
                        governor: None,
                        control: None,
                    },
                );
            }
        }
    }

    fans
}

/// Read fan stats on macOS
#[cfg(target_os = "macos")]
fn read_fan_stats() -> std::collections::HashMap<String, crate::core::fan::FanInfo> {
    use crate::core::fan::FanInfo;
    use std::collections::HashMap;
    use std::process::Command;

    let mut fans = HashMap::new();

    // Try using smcFanControl or similar if available
    // For now, try iStats gem if available
    if let Ok(output) = Command::new("istats").args(["fan", "speed"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(rpm_str) = line.split_whitespace().last() {
                if let Ok(rpm) = rpm_str.replace("RPM", "").trim().parse::<u32>() {
                    fans.insert(
                        "CPU Fan".to_string(),
                        FanInfo {
                            speed: vec![(rpm / 50).min(100)], // Rough estimate
                            rpm: Some(vec![rpm]),
                            profile: "auto".to_string(),
                            governor: None,
                            control: None,
                        },
                    );
                    break;
                }
            }
        }
    }

    fans
}

// Platform implementations for macOS.
//
// `stats.rs` has carried Linux and Windows implementations of these since the
// beginning and never had macOS ones, so `IronMonitor` could not be built there at all —
// the manifest bug meant nothing ever tried, and the gap went unnoticed.
//
// These report `UnsupportedPlatform` rather than guessing. Writing real ones means
// sysctl and IOKit work that has to be verified on actual hardware; a plausible
// implementation that returned made-up numbers would be worse than an honest
// refusal, because callers cannot tell fabricated telemetry from measured
// telemetry. `UnifiedMonitor` (see `lib.rs`) has working macOS paths and is the
// type to use there today.
#[cfg(target_os = "macos")]
mod macos_stats {
    use super::*;
    use crate::core::cpu::{CpuCore, CpuTotal};
    use crate::core::memory::{RamInfo, SwapInfo};
    use crate::core::platform_info::{HardwareInfo, LibraryVersions, PlatformInfo};
    use crate::error::IronError;

    fn unsupported(what: &str) -> IronError {
        IronError::UnsupportedPlatform(format!(
            "{what} is not implemented for macOS in `stats::IronMonitor`; use `UnifiedMonitor`"
        ))
    }

    pub(super) fn read_cpu_stats() -> Result<CpuStats> {
        use crate::platform::macos;

        let model = macos::sysctl_string("machdep.cpu.brand_string")
            // Apple Silicon does not publish machdep.cpu.brand_string; the machine
            // identifier is the closest thing it does publish.
            .or_else(|| macos::sysctl_string("hw.model"))
            .unwrap_or_default();

        // `host_processor_info` is preferred over parsing `top`: it is the only
        // source of per-core figures and of nice time, and it reports cumulative
        // ticks, which is the same thing the Linux reader turns into percentages
        // from /proc/stat. Both therefore mean an average since boot.
        if let Some(ticks) = macos::per_core_ticks() {
            if let Some(total) = macos::aggregate_ticks(&ticks).percentages() {
                let cores = ticks
                    .iter()
                    .enumerate()
                    .map(|(id, core)| {
                        let split = core.percentages();
                        CpuCore {
                            id,
                            online: true,
                            governor: String::new(),
                            frequency: None,
                            user: split.map(|s| s.user),
                            nice: split.map(|s| s.nice),
                            system: split.map(|s| s.system),
                            idle: split.map(|s| s.idle),
                            model: model.clone(),
                        }
                    })
                    .collect();

                return Ok(CpuStats {
                    cores,
                    total: CpuTotal {
                        user: total.user,
                        nice: total.nice,
                        system: total.system,
                        idle: total.idle,
                    },
                });
            }
        }

        // Fallback: `top`, which reports one aggregate and no nice time. Reached
        // only if the Mach call fails, and it keeps a failure there from turning
        // into no CPU data at all.
        let usage = macos::cpu_usage().ok_or_else(|| {
            unsupported("CPU statistics: host_processor_info failed and `top` was not understood")
        })?;

        let count = macos::sysctl_u64("hw.logicalcpu").unwrap_or(0) as usize;
        let cores = (0..count)
            .map(|id| CpuCore {
                id,
                online: true,
                governor: String::new(),
                frequency: None,
                // One aggregate copied into every core would present an average as
                // a per-core measurement.
                user: None,
                nice: None,
                system: None,
                idle: None,
                model: model.clone(),
            })
            .collect();

        Ok(CpuStats {
            cores,
            total: CpuTotal {
                user: usage.user,
                // `top` does not separate nice time. Only reachable on the
                // fallback path; the primary one reports it properly.
                nice: 0.0,
                system: usage.system,
                idle: usage.idle,
            },
        })
    }

    pub(super) fn read_gpu_stats() -> Result<GpuStats> {
        Err(unsupported("GPU statistics"))
    }

    pub(super) fn read_memory_stats() -> Result<MemoryStats> {
        use crate::platform::macos;

        let vm = macos::vm_stat()
            .ok_or_else(|| unsupported("memory statistics: `vm_stat` was not understood"))?;

        // hw.memsize is the installed RAM. Summing vm_stat's pages gives a slightly
        // different figure — the kernel's own footprint is not in the page counts —
        // so the installed size is the one to report as total.
        let total = macos::sysctl_u64("hw.memsize").unwrap_or_else(|| {
            (vm.free + vm.active + vm.inactive + vm.speculative + vm.wired + vm.compressed)
                * vm.page_size
        });

        // Swap being unreadable is different from swap being off: the first is
        // unknown, the second is a real zero. `SwapInfo` carries that
        // distinction as of 6.0.0, so the value passes through as `Option`
        // rather than collapsing to a zero with a comment apologising for it.
        // This was the third place in the crate working around the same missing
        // type.
        let swap = macos::swap_usage();

        Ok(MemoryStats {
            // KB throughout, which every field of `RamInfo` and `SwapInfo`
            // documents. This reader assigned bytes to all of them — a factor of
            // 1024 — while `platform::macos::read_memory_stats` divided
            // correctly, so the same machine reported two different sizes
            // depending on which entry point a caller used.
            ram: RamInfo {
                total: total / 1024,
                used: vm.used_bytes() / 1024,
                free: vm.free_bytes() / 1024,
                // macOS has no buffer cache distinct from the file cache, so
                // there is no figure to report rather than a figure of zero.
                buffers: None,
                cached: Some(vm.cached_bytes() / 1024),
                // Not read here.
                shared: None,
                lfb: None,
            },
            swap: SwapInfo {
                total: swap.map(|s| s.total / 1024),
                used: swap.map(|s| s.used / 1024),
                cached: None,
            },
            emc: None,
            iram: None,
        })
    }

    pub(super) fn read_power_stats() -> Result<PowerStats> {
        Err(unsupported("power statistics"))
    }

    pub(super) fn read_temperature_stats() -> Result<TemperatureStats> {
        Err(unsupported("temperature statistics"))
    }

    pub(super) fn detect_platform_info() -> Result<BoardInfo> {
        use crate::platform::macos;

        // `IronMonitor::with_interval` calls this in its constructor, so a failure here
        // makes the whole type unconstructible — which is what kept CPU and memory
        // out of reach on macOS even after they were implemented.
        let machine = macos::sysctl_string("hw.machine").unwrap_or_default();
        let model = macos::sysctl_string("hw.model").unwrap_or_default();
        let release = macos::sysctl_string("kern.osrelease").unwrap_or_default();
        let product = macos::sysctl_string("kern.osproductversion");

        Ok(BoardInfo {
            platform: PlatformInfo {
                machine,
                system: "Darwin".to_string(),
                // The marketing version (26.x) rather than the Darwin kernel
                // version, which `release` already carries.
                distribution: product.map(|v| format!("macOS {v}")),
                release,
            },
            hardware: HardwareInfo {
                model,
                p_number: None,
                module: None,
                soc: macos::sysctl_string("machdep.cpu.brand_string"),
                cuda_arch: None,
                codename: None,
                serial_number: macos::sysctl_string("hw.serialnumber"),
                l4t: None,
                jetpack: None,
            },
            libraries: LibraryVersions {
                cuda: None,
                cudnn: None,
                tensorrt: None,
                other: std::collections::HashMap::new(),
            },
        })
    }

    pub(super) fn read_uptime() -> Result<Duration> {
        use crate::platform::macos;

        // kern.boottime prints `{ sec = 1754500000, usec = 123456 } ...`
        let raw = macos::sysctl_string("kern.boottime").ok_or_else(|| unsupported("uptime"))?;
        let boot_secs: u64 = raw
            .split("sec = ")
            .nth(1)
            .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| unsupported("uptime: kern.boottime was not understood"))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // A boot time in the future means the two clocks disagree; reporting a
        // saturated zero is better than a wrapped duration of 584 million years.
        Ok(Duration::from_secs(now.saturating_sub(boot_secs)))
    }

    pub(super) fn read_process_stats() -> Result<ProcessStats> {
        Err(unsupported("process statistics"))
    }

    pub(super) fn read_engine_stats() -> Result<EngineStats> {
        Err(unsupported("engine statistics"))
    }
}

#[cfg(target_os = "macos")]
use macos_stats::{
    detect_platform_info, read_cpu_stats, read_engine_stats, read_gpu_stats, read_memory_stats,
    read_power_stats, read_process_stats, read_temperature_stats, read_uptime,
};

/// The platform CPU reader, for the rest of the crate.
///
/// One wrapper rather than making each platform's reader visible: the macOS
/// implementations are `pub(super)` inside a nested module and cannot be
/// re-exported, and three callers outside this module need a reader that works
/// everywhere. Until 6.0.0 they used `CpuStats::new()` instead, which returns
/// 100% idle — `UnifiedMonitor::snapshot_cpu`, the health checks and the
/// Prometheus exporter all published that as a measurement.
pub(crate) fn platform_cpu_stats() -> Result<CpuStats> {
    read_cpu_stats()
}

/// The platform memory reader. See [`platform_cpu_stats`].
pub(crate) fn platform_memory_stats() -> Result<MemoryStats> {
    read_memory_stats()
}
