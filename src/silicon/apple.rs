//! Apple Silicon monitoring via powermetrics and IOKit
//!
//! This module implements comprehensive monitoring for Apple Silicon (M1/M2/M3/M4 series)
//! using macOS's powermetrics utility and IOKit framework.
//!
//! Based on asitop: <https://github.com/tlkh/asitop>

use super::*;
use crate::error::{Error, Result};
use std::collections::HashMap;
use std::process::Command;

/// Apple Silicon monitor
pub struct AppleSiliconMonitor {
    soc_info: SocInfo,
}

/// SOC information
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SocInfo {
    name: String,
    e_core_count: Option<u32>,
    p_core_count: Option<u32>,
}

/// Powermetrics data
#[derive(Debug, Default)]
pub struct PowermetricsData {
    // CPU metrics
    pub e_cluster_freq_mhz: u32,
    pub e_cluster_active: u8,
    pub p_cluster_freq_mhz: u32,
    pub p_cluster_active: u8,

    // Per-core metrics
    pub e_cores: HashMap<u32, (u32, u8)>, // (freq, utilization)
    pub p_cores: HashMap<u32, (u32, u8)>,

    // GPU metrics
    pub gpu_freq_mhz: u32,
    pub gpu_active: u8,

    // Power metrics
    pub cpu_power_mw: u32,
    pub gpu_power_mw: u32,
    pub ane_power_mw: u32,
    pub package_power_mw: u32,

    // Thermal
    pub thermal_pressure: String,
}

impl AppleSiliconMonitor {
    /// Create new Apple Silicon monitor
    pub fn new() -> Result<Self> {
        let soc_info = Self::detect_soc_info()?;

        Ok(Self { soc_info })
    }

    /// Detect SOC information using sysctl and system_profiler
    fn detect_soc_info() -> Result<SocInfo> {
        // Get CPU brand string
        let output = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .map_err(|e| Error::CommandExecutionFailed(format!("sysctl: {}", e)))?;

        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Core counts, or `None` where the sysctl did not answer.
        //
        // These fell back to `4`, `4` and `8` -- **a specific and entirely
        // plausible machine**, which is what makes the fabrication worse than
        // the usual one. An M1 is 4+4 with 8 GPU cores, so a failed read
        // described the hardware as a base M1 whatever it actually was, and
        // nothing downstream could tell the difference: `core_ids` came out as
        // `0..4` and looked like a topology somebody had measured.
        //
        // A `1` is implausible enough to be noticed. A `4` is not, and that is
        // the argument against "sensible defaults" in a reader: the more
        // reasonable the invented value, the longer it survives.
        //
        // `None` means the count was not read. The cluster's `core_ids` is then
        // `None` too, while the frequency and utilisation `powermetrics`
        // actually measured stay -- an unknown composition, not an unknown cluster.
        let e_cores = Self::get_sysctl_value("hw.perflevel1.logicalcpu");
        let p_cores = Self::get_sysctl_value("hw.perflevel0.logicalcpu");

        // There was a per-chip power table here -- CPU, GPU and ANE ceilings
        // by brand-string match, with `(20.0, 20.0, 8.0)` for any chip it did
        // not name. Its only reader divided the ANE's measured draw by that
        // ceiling and reported the result as utilisation; see `npu_info`. A GPU
        // core count read from `system_profiler` sat beside it, stored and
        // never read. Both are gone, and with them a `system_profiler` spawn on
        // every construction.
        Ok(SocInfo {
            name,
            e_core_count: e_cores,
            p_core_count: p_cores,
        })
    }

    /// Get sysctl value as u32
    fn get_sysctl_value(key: &str) -> Option<u32> {
        let output = Command::new("sysctl").args(["-n", key]).output().ok()?;

        String::from_utf8_lossy(&output.stdout).trim().parse().ok()
    }

    /// The live CPU, GPU and ANE figures `powermetrics` reports.
    ///
    /// Always an error, and the error says why. `powermetrics` must run as
    /// root, and IronMonitor does not elevate itself — AGENTS.md promises
    /// that, and an interactive `sudo` would sit waiting for a password inside
    /// a TUI or an agent's tool call.
    ///
    /// What was here could never succeed and could mislead if it did. It read
    /// a plist from `/tmp/ironmon_powermetrics_<pid>`, a file written only by
    /// a `sudo powermetrics` launcher that nothing called; on macOS it failed
    /// with a bare "file not found", and elsewhere it returned a default — every
    /// clock, utilisation and power figure zero — as a successful reading. Its
    /// parser filled each missing key with 0 MHz, 0% busy or core 0, and its
    /// key names were never checked against a real `powermetrics` sample.
    ///
    /// A reader that runs `powermetrics` directly when the process is already
    /// root belongs here, written against output from a real Mac.
    pub fn parse_powermetrics(&self) -> Result<PowermetricsData> {
        Err(crate::error::IronError::PermissionDenied(
            "live Apple Silicon CPU, GPU and ANE figures come from powermetrics, \
             which requires root; IronMonitor does not elevate itself"
                .into(),
        ))
    }
}

impl SiliconMonitor for AppleSiliconMonitor {
    fn cpu_info(&self) -> Result<(Vec<CpuCore>, Vec<CpuCluster>)> {
        let data = self.parse_powermetrics()?;

        let mut cores = Vec::new();
        let mut clusters = Vec::new();

        // E-cores
        for (id, (freq, util)) in &data.e_cores {
            cores.push(CpuCore {
                id: *id,
                cluster: CpuClusterType::Efficiency,
                frequency_mhz: Some(*freq),
                utilization: Some(*util),
                temperature: None,
            });
        }

        // P-cores
        for (id, (freq, util)) in &data.p_cores {
            cores.push(CpuCore {
                id: *id,
                cluster: CpuClusterType::Performance,
                frequency_mhz: Some(*freq),
                utilization: Some(*util),
                temperature: None,
            });
        }

        // E-cluster. A `None` `core_ids` means the per-level core count was
        // not read, not that the cluster has no cores; the figures beside it
        // are measured either way.
        //
        // Neither cluster reports power. `powermetrics` gives one CPU figure,
        // and this used to split it 40/60 between the clusters -- a fixed
        // ratio presented as two per-cluster readings, the E-cluster's
        // unchanged whether it was idle or saturated.
        clusters.push(CpuCluster {
            cluster_type: CpuClusterType::Efficiency,
            core_ids: self.soc_info.e_core_count.map(|n| (0..n).collect()),
            frequency_mhz: Some(data.e_cluster_freq_mhz),
            utilization: Some(data.e_cluster_active),
            power_watts: None,
        });

        // P-cluster
        clusters.push(CpuCluster {
            cluster_type: CpuClusterType::Performance,
            core_ids: self.soc_info.p_core_count.map(|n| (0..n).collect()),
            frequency_mhz: Some(data.p_cluster_freq_mhz),
            utilization: Some(data.p_cluster_active),
            power_watts: None,
        });

        Ok((cores, clusters))
    }

    fn npu_info(&self) -> Result<Vec<NpuInfo>> {
        // The device comes from IOKit and needs no root. It was read from the
        // `powermetrics` sample, so without root the Neural Engine was not
        // listed at all, though it was there.
        let count = read_neural_engines().map_err(crate::error::IronError::DeviceNotFound)?;
        // `powermetrics` gives one ANE figure, so it belongs to a lone device.
        let power_watts = (count == 1)
            .then(|| self.parse_powermetrics().ok())
            .flatten()
            .map(|data| data.ane_power_mw as f32 / 1000.0);

        Ok((0..count)
            .map(|_| NpuInfo {
                name: "Apple Neural Engine".to_string(),
                vendor: "Apple".to_string(),
                // "Most" is not this machine. The ANE core count is not read.
                cores: None,
                // Not reported. This was the ANE's measured draw divided by a
                // ceiling from a brand-string table -- 8 W for any chip the table
                // did not name -- which is the GPU power-percentage defect over
                // again: a real numerator over an invented denominator, reaching
                // the TUI's accelerator panel as a utilisation. `powermetrics`
                // reports no ANE activity figure, so there is nothing to put here.
                utilization: None,
                power_watts,
                frequency_mhz: None,
            })
            .collect())
    }

    fn io_info(&self) -> Result<Vec<IoController>> {
        let mut controllers = Vec::new();

        // Get disk I/O stats from iostat
        if let Ok(output) = Command::new("iostat").args(["-d", "-c", "1"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            // iostat -d outputs: device, KB/t, tps, MB/s
            for line in text.lines().skip(2) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let name = parts[0].to_string();
                    let mbps: f64 = parts[3].parse().unwrap_or(0.0);
                    controllers.push(IoController {
                        controller_type: if name.starts_with("disk") {
                            "NVMe".to_string()
                        } else {
                            "Storage".to_string()
                        },
                        name,
                        bandwidth_mbps: Some(mbps),
                        // 7000 was the spec figure for Apple Silicon's fastest
                        // NVMe, applied to whatever this disk is. Nothing about
                        // the actual link is read here.
                        max_bandwidth_mbps: None,
                        power_watts: None,
                    });
                }
            }
        }

        // Thunderbolt controllers via system_profiler
        if let Ok(output) = Command::new("system_profiler")
            .args(["-detailLevel", "mini", "SPThunderboltDataType"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut tb_count = 0u32;
            for line in text.lines() {
                if (line.trim().starts_with("Speed:") || line.trim().contains("Thunderbolt"))
                    && line.contains("Up to")
                {
                    tb_count += 1;
                    controllers.push(IoController {
                        controller_type: "Thunderbolt".to_string(),
                        name: format!("Thunderbolt Port {}", tb_count),
                        // The *current* bandwidth was set to the theoretical
                        // maximum, so an idle Thunderbolt port reported 5000
                        // MB/s of traffic — a fabricated saturated bus, not a
                        // sentinel zero. Nothing here reads either figure.
                        bandwidth_mbps: None,
                        max_bandwidth_mbps: None,
                        power_watts: None,
                    });
                }
            }
        }

        Ok(controllers)
    }

    fn network_info(&self) -> Result<Vec<NetworkSilicon>> {
        let mut networks = Vec::new();

        // Get active network interfaces via ifconfig
        if let Ok(output) = Command::new("ifconfig").output() {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut current_iface = String::new();
            let mut is_up = false;

            for line in text.lines() {
                if !line.starts_with('\t') && !line.starts_with(' ') && line.contains(':') {
                    // New interface line like "en0: flags=8863<UP,...>"
                    let iface = line.split(':').next().unwrap_or("").to_string();
                    if !current_iface.is_empty() && is_up && !current_iface.starts_with("lo") {
                        let interface_type = classify_macos_interface(&current_iface);
                        networks.push(NetworkSilicon {
                            interface: interface_type,
                            link_speed_mbps: None,
                            rx_bandwidth_mbps: None,
                            tx_bandwidth_mbps: None,
                            packet_rate: None,
                            power_state: Some("active".to_string()),
                        });
                    }
                    current_iface = iface;
                    is_up = line.contains("UP");
                }
            }
            // Push last
            if !current_iface.is_empty() && is_up && !current_iface.starts_with("lo") {
                let interface_type = classify_macos_interface(&current_iface);
                networks.push(NetworkSilicon {
                    interface: interface_type,
                    link_speed_mbps: None,
                    rx_bandwidth_mbps: None,
                    tx_bandwidth_mbps: None,
                    packet_rate: None,
                    power_state: Some("active".to_string()),
                });
            }
        }

        // netstat -ib byte totals are cumulative since boot, not a bandwidth,
        // so they are not written into the rate fields.

        Ok(networks)
    }
}

/// Classify macOS interface name to type
fn classify_macos_interface(iface: &str) -> String {
    if iface == "en0" {
        "WiFi".to_string()
    } else if iface.starts_with("en") {
        "Ethernet".to_string()
    } else if iface.starts_with("bridge") {
        "Bridge".to_string()
    } else if iface.starts_with("awdl") {
        "AirDrop".to_string()
    } else if iface.starts_with("utun") {
        "VPN Tunnel".to_string()
    } else {
        iface.to_string()
    }
}

/// One CPU performance level, as `hw.perflevelN.*` reports it.
///
/// Apple silicon numbers its levels from 0, the fastest: on every shipping
/// chip level 0 is the performance cores and level 1 the efficiency cores. The
/// name is read rather than assumed from the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerfLevel {
    /// N in `hw.perflevelN`.
    pub index: u32,
    /// `hw.perflevelN.name`, e.g. "Performance" or "Efficiency".
    pub name: Option<String>,
    /// `hw.perflevelN.physicalcpu`.
    pub physical_cores: Option<u32>,
    /// `hw.perflevelN.logicalcpu`.
    pub logical_cores: Option<u32>,
    /// `hw.perflevelN.l2cachesize`, in bytes. Each L2 is shared by
    /// [`Self::cores_per_l2`] cores.
    pub l2_cache_bytes: Option<u64>,
    /// `hw.perflevelN.cpusperl2`.
    pub cores_per_l2: Option<u32>,
}

/// What IOKit reports for one Apple GPU, without root.
///
/// `ioreg` lists each GPU as an `IOAccelerator` whose `PerformanceStatistics`
/// dictionary the driver keeps current. It is the GPU's own activity counter,
/// so it is a reading where `powermetrics` cannot be run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppleGpuCounters {
    /// `model`, e.g. "Apple M2 Pro".
    pub model: Option<String>,
    /// `gpu-core-count`.
    pub core_count: Option<u32>,
    /// `Device Utilization %`: the share of time the GPU was busy.
    pub device_utilization: Option<u8>,
    /// `Renderer Utilization %`.
    pub renderer_utilization: Option<u8>,
    /// `Tiler Utilization %`.
    pub tiler_utilization: Option<u8>,
    /// `In use system memory`, bytes: unified memory the GPU is using now.
    pub in_use_system_memory: Option<u64>,
    /// `Alloc system memory`, bytes: unified memory allocated to the GPU.
    pub allocated_system_memory: Option<u64>,
}

/// The SoC's unified memory, shared by the CPU, GPU and Neural Engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnifiedMemory {
    /// `hw.memsize`, bytes.
    pub total_bytes: Option<u64>,
    /// The memory type `system_profiler` names, e.g. "LPDDR5".
    pub memory_type: Option<String>,
    /// The memory maker `system_profiler` names.
    pub manufacturer: Option<String>,
}

/// Everything about an Apple silicon SoC that can be read without root.
///
/// Each part carries its own failure, so one unreadable part does not hide
/// the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleSocInventory {
    /// `machdep.cpu.brand_string`, e.g. "Apple M3 Max".
    pub chip: Option<String>,
    /// The CPU's performance levels, fastest first.
    pub perf_levels: std::result::Result<Vec<PerfLevel>, String>,
    /// The GPUs IOKit lists.
    pub gpus: std::result::Result<Vec<AppleGpuCounters>, String>,
    /// How many Neural Engine devices IOKit lists. An error, not zero, where
    /// none was found: every Apple silicon chip has one, so an empty lookup
    /// says the lookup missed, not that the chip lacks it.
    pub neural_engines: std::result::Result<u32, String>,
    /// The unified memory.
    pub unified_memory: UnifiedMemory,
}

/// The IOKit class the Neural Engine's driver registers.
const ANE_IOKIT_CLASS: &str = "H11ANEIn";

/// Run a command and return its stdout, or why it could not be read.
fn command_text(program: &str, args: &[&str]) -> std::result::Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("{program} could not be run: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The value of an `ioreg` property line `"key" = value`, unquoted.
fn ioreg_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.trim().strip_prefix('"')?.strip_prefix(key)?;
    let value = rest
        .strip_prefix('"')?
        .trim_start()
        .strip_prefix('=')?
        .trim();
    // Strings print as "x"; device-tree data as <"x">.
    let value = value.trim_start_matches('<').trim_end_matches('>');
    Some(value.trim_matches('"'))
}

/// The integer entries of an `ioreg` inline dictionary `{"a"=1,"b"=2}`.
///
/// Entries whose value is not an integer are skipped rather than read as 0.
fn ioreg_int_dict(dict: &str) -> HashMap<String, u64> {
    let mut entries = HashMap::new();
    let inner = dict.trim().trim_start_matches('{').trim_end_matches('}');
    let mut rest = inner;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else { break };
        let key = &after[..end];
        let tail = &after[end + 1..];
        let Some(tail) = tail.trim_start().strip_prefix('=') else {
            rest = tail;
            continue;
        };
        let stop = tail.find([',', '}']).unwrap_or(tail.len());
        if let Ok(value) = tail[..stop].trim().parse::<u64>() {
            entries.insert(key.to_string(), value);
        }
        rest = &tail[stop..];
    }
    entries
}

/// A percentage counter, withheld rather than clamped when it exceeds 100.
fn percent(stats: &HashMap<String, u64>, key: &str) -> Option<u8> {
    stats
        .get(key)
        .copied()
        .filter(|v| *v <= 100)
        .map(|v| v as u8)
}

/// Parse `ioreg -r -d 1 -w 0 -c IOAccelerator` into one entry per GPU.
pub fn parse_ioreg_accelerators(text: &str) -> Vec<AppleGpuCounters> {
    let mut gpus: Vec<AppleGpuCounters> = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("+-o ") {
            gpus.push(AppleGpuCounters::default());
            continue;
        }
        let Some(gpu) = gpus.last_mut() else {
            continue;
        };
        if let Some(model) = ioreg_value(line, "model") {
            gpu.model = Some(model.to_string()).filter(|m| !m.is_empty());
        } else if let Some(count) = ioreg_value(line, "gpu-core-count") {
            gpu.core_count = count.parse().ok().filter(|n| *n > 0);
        } else if let Some(dict) = ioreg_value(line, "PerformanceStatistics") {
            let stats = ioreg_int_dict(dict);
            gpu.device_utilization = percent(&stats, "Device Utilization %");
            gpu.renderer_utilization = percent(&stats, "Renderer Utilization %");
            gpu.tiler_utilization = percent(&stats, "Tiler Utilization %");
            gpu.in_use_system_memory = stats.get("In use system memory").copied();
            gpu.allocated_system_memory = stats.get("Alloc system memory").copied();
        }
    }
    gpus
}

/// How many objects an `ioreg -r -d 1 -c <class>` listing holds.
pub fn count_ioreg_objects(text: &str) -> u32 {
    text.lines()
        .filter(|line| line.trim_start().starts_with("+-o "))
        .count() as u32
}

/// Read every GPU IOKit lists, with its live counters.
pub fn read_gpu_counters() -> std::result::Result<Vec<AppleGpuCounters>, String> {
    command_text(
        "ioreg",
        &["-r", "-d", "1", "-w", "0", "-c", "IOAccelerator"],
    )
    .map(|text| parse_ioreg_accelerators(&text))
}

/// Count the Neural Engine devices IOKit lists. See
/// [`AppleSocInventory::neural_engines`].
pub fn read_neural_engines() -> std::result::Result<u32, String> {
    let text = command_text(
        "ioreg",
        &["-r", "-d", "1", "-w", "0", "-c", ANE_IOKIT_CLASS],
    )?;
    match count_ioreg_objects(&text) {
        0 => Err(format!(
            "IOKit lists no {ANE_IOKIT_CLASS} object, the Neural Engine driver's class"
        )),
        n => Ok(n),
    }
}

/// Read the CPU's performance levels from `hw.nperflevels` and
/// `hw.perflevelN.*`.
pub fn read_perf_levels() -> std::result::Result<Vec<PerfLevel>, String> {
    use crate::platform::macos::{sysctl_string, sysctl_u64};
    let count = sysctl_u64("hw.nperflevels")
        .ok_or_else(|| "sysctl hw.nperflevels could not be read".to_string())?;
    Ok((0..count as u32)
        .map(|index| {
            let key = |field: &str| format!("hw.perflevel{index}.{field}");
            let small = |field: &str| sysctl_u64(&key(field)).and_then(|v| u32::try_from(v).ok());
            PerfLevel {
                index,
                name: sysctl_string(&key("name")).filter(|n| !n.is_empty()),
                physical_cores: small("physicalcpu"),
                logical_cores: small("logicalcpu"),
                l2_cache_bytes: sysctl_u64(&key("l2cachesize")),
                cores_per_l2: small("cpusperl2"),
            }
        })
        .collect())
}

/// Read the unified memory's size, and its type and maker where
/// `system_profiler` names them.
pub fn read_unified_memory() -> UnifiedMemory {
    use crate::memory_topology::{FormFactor, MemoryTopologyMonitor, MemoryType};
    let package = MemoryTopologyMonitor::new().ok().and_then(|monitor| {
        monitor
            .topology()
            .dimms
            .iter()
            .find(|d| d.form_factor == FormFactor::OnPackage)
            .cloned()
    });
    UnifiedMemory {
        total_bytes: crate::platform::macos::sysctl_u64("hw.memsize").filter(|b| *b > 0),
        memory_type: package
            .as_ref()
            .filter(|d| d.memory_type != MemoryType::Unknown)
            .map(|d| d.memory_type.to_string()),
        manufacturer: package.map(|d| d.manufacturer).filter(|m| !m.is_empty()),
    }
}

/// Read everything [`AppleSocInventory`] describes.
pub fn read_soc_inventory() -> AppleSocInventory {
    AppleSocInventory {
        chip: crate::platform::macos::sysctl_string("machdep.cpu.brand_string")
            .filter(|c| !c.is_empty()),
        perf_levels: read_perf_levels(),
        gpus: read_gpu_counters(),
        neural_engines: read_neural_engines(),
        unified_memory: read_unified_memory(),
    }
}

// `estimate_link_speed` was here, and its own doc comment said what was wrong
// with it: "These are guesses keyed off the interface name, not measurements:
// `en0` is assumed to be Wi-Fi 6 and anything else `en*`/`bridge*` gigabit
// Ethernet. Callers must not present the result as a read link rate."
//
// The caller two lines away assigned it straight to `link_speed_mbps`. A
// warning in a doc comment does not bind the caller; a type does. The field is
// `Option<u32>` and reads `None` here until something calls `IOKit` for the
// interface's active media type.

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    /// Without root there is no `powermetrics` sample, and the readers built
    /// on it say so rather than reporting a machine at 0 MHz and 0 mW.
    #[test]
    fn live_figures_are_refused_with_the_reason_not_zeroed() {
        let monitor = AppleSiliconMonitor {
            soc_info: SocInfo {
                name: "Apple M2".into(),
                e_core_count: Some(4),
                p_core_count: Some(4),
            },
        };
        let err = monitor.parse_powermetrics().expect_err("no sample exists");
        assert!(
            matches!(err, crate::error::IronError::PermissionDenied(ref why) if why.contains("root")),
            "{err:?}"
        );
        assert!(monitor.cpu_info().is_err());
        // The Neural Engine is listed from IOKit, without root; its power
        // figure is the part that needs `powermetrics`.
        if let Ok(npus) = monitor.npu_info() {
            assert!(npus.iter().all(|npu| npu.power_watts.is_none()), "{npus:?}");
        }
    }

    /// A trimmed capture of `ioreg -r -d 1 -w 0 -c IOAccelerator` on an M1.
    const IOREG_AGX: &str = r#"+-o AGXAcceleratorG13G  <class AGXAcceleratorG13G, id 0x1000004b8, registered, matched, active, busy 0 (0 ms), retain 49>
    {
      "IOClass" = "AGXAcceleratorG13G"
      "gpu-core-count" = 8
      "model" = "Apple M1"
      "PerformanceStatistics" = {"In use system memory (driver)"=0,"Alloc system memory"=1012334592,"Tiler Utilization %"=7,"recoveryCount"=0,"lastRecoveryTime"=0,"Renderer Utilization %"=11,"TiledSceneBytes"=1638400,"Device Utilization %"=12,"SplitSceneCount"=0,"Allocated PB Size"=168820736,"In use system memory"=423624704}
      "IOMatchedAtBoot" = Yes
    }
"#;

    #[test]
    fn ioreg_accelerator_counters_are_read_by_name() {
        let gpus = parse_ioreg_accelerators(IOREG_AGX);
        assert_eq!(gpus.len(), 1);
        let gpu = &gpus[0];
        assert_eq!(gpu.model.as_deref(), Some("Apple M1"));
        assert_eq!(gpu.core_count, Some(8));
        assert_eq!(gpu.device_utilization, Some(12));
        assert_eq!(gpu.renderer_utilization, Some(11));
        assert_eq!(gpu.tiler_utilization, Some(7));
        assert_eq!(gpu.in_use_system_memory, Some(423_624_704));
        assert_eq!(gpu.allocated_system_memory, Some(1_012_334_592));
    }

    /// A missing or impossible counter is unread, not 0 and not clamped.
    #[test]
    fn missing_or_impossible_gpu_counters_are_unread() {
        let gpus = parse_ioreg_accelerators(concat!(
            "+-o AGXAcceleratorG14X  <class AGXAcceleratorG14X>\n",
            "      \"PerformanceStatistics\" = {\"Device Utilization %\"=140,",
            "\"Renderer Utilization %\"=bad}\n",
            "+-o AppleParavirtGPU  <class AppleParavirtGPU>\n",
        ));
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].device_utilization, None);
        assert_eq!(gpus[0].renderer_utilization, None);
        assert_eq!(gpus[0].core_count, None);
        assert_eq!(gpus[1], AppleGpuCounters::default());
        assert!(parse_ioreg_accelerators("").is_empty());
    }

    #[test]
    fn ioreg_objects_are_counted_by_their_tree_lines() {
        let ane = concat!(
            "+-o H11ANEIn  <class H11ANEIn, id 0x100000254, registered>\n",
            "    {\n",
            "      \"IOClass\" = \"H11ANEIn\"\n",
            "    }\n",
        );
        assert_eq!(count_ioreg_objects(ane), 1);
        assert_eq!(count_ioreg_objects(""), 0);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_detect_soc() {
        let soc = AppleSiliconMonitor::detect_soc_info();
        if let Ok(soc_info) = soc {
            println!("Detected SOC: {}", soc_info.name);
            println!(
                "E-cores: {:?}, P-cores: {:?}",
                soc_info.e_core_count, soc_info.p_core_count
            );
        }
    }
}
