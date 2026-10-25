//! Linux silicon monitoring
//!
//! Comprehensive hardware monitoring for Linux systems using sysfs, /proc, and hwmon

use super::*;
use crate::error::Result;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Energy the RAPL counter gained between two readings, in microjoules.
///
/// The counter wraps at `max_energy_range_uj`. Across a wrap the gain is
/// `(max - prev) + current`. With the range unknown the interval has no energy
/// figure, so this is `None`. The post-wrap reading used to stand in for the
/// gain, which is the energy since the wrap and not since the last sample.
fn rapl_energy_delta(prev: u64, current: u64, max_range: Option<u64>) -> Option<u64> {
    if current >= prev {
        return Some(current - prev);
    }
    let max = max_range?;
    if max <= prev {
        return None;
    }
    Some(max - prev + current)
}

/// Usable bandwidth of a PCIe link in MB/s.
///
/// Raw GT/s times lanes gives Gbit/s. Line encoding removes its overhead: Gen 1
/// and 2 (2.5 and 5 GT/s) use 8b/10b, Gen 3 and later use 128b/130b. Dividing
/// by 8 bits per byte then gives MB/s, so a Gen 4 x4 link is about 7877 MB/s.
fn pcie_link_mb_per_s(gt_per_s: f64, lanes: u32) -> f64 {
    let encoding = if gt_per_s <= 5.0 {
        8.0 / 10.0
    } else {
        128.0 / 130.0
    };
    gt_per_s * lanes as f64 * encoding * 1000.0 / 8.0
}

/// Idle and total jiffies per CPU from one `/proc/stat` reading, by CPU number.
///
/// The aggregate `cpu ` line is skipped. Its name has no number, and reading it
/// as CPU 0 would mix the whole machine into core 0.
fn parse_proc_stat_ticks(stat: &str) -> HashMap<u32, (u64, u64)> {
    let mut ticks = HashMap::new();
    for line in stat.lines() {
        let Some(rest) = line.strip_prefix("cpu") else {
            continue;
        };
        if rest.starts_with(char::is_whitespace) {
            continue;
        }
        let mut fields = rest.split_whitespace();
        let Some(cpu) = fields.next().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        // user, nice, system, idle, ...
        let times: Vec<u64> = fields.filter_map(|s| s.parse().ok()).collect();
        if times.len() >= 4 {
            let total: u64 = times.iter().sum();
            ticks.insert(cpu, (times[3], total));
        }
    }
    ticks
}

/// Utilization percentage per CPU from the ticks gained between two readings.
///
/// A CPU needs a line in both readings, and a counter that went backwards is a
/// reset rather than a measurement. Either leaves that CPU out, so it reads as
/// unmeasured.
fn cpu_utilization_between(
    prev: &HashMap<u32, (u64, u64)>,
    cur: &HashMap<u32, (u64, u64)>,
) -> HashMap<u32, u8> {
    let mut utilization = HashMap::new();
    for (&cpu, &(idle, total)) in cur {
        let Some(&(prev_idle, prev_total)) = prev.get(&cpu) else {
            continue;
        };
        let (Some(d_idle), Some(d_total)) =
            (idle.checked_sub(prev_idle), total.checked_sub(prev_total))
        else {
            continue;
        };
        if d_total == 0 {
            continue;
        }
        let busy = d_total - d_idle.min(d_total);
        utilization.insert(cpu, (busy * 100 / d_total).min(100) as u8);
    }
    utilization
}

/// Linux silicon monitor
pub struct LinuxSiliconMonitor {
    cpu_count: Option<usize>,
    has_hybrid_cpu: bool,
}

impl LinuxSiliconMonitor {
    /// Create a new Linux silicon monitor
    pub fn new() -> Result<Self> {
        let cpu_count = Self::detect_cpu_count();
        let has_hybrid_cpu = Self::detect_hybrid_architecture();

        Ok(Self {
            cpu_count,
            has_hybrid_cpu,
        })
    }

    fn detect_cpu_count() -> Option<usize> {
        if let Ok(online) = fs::read_to_string("/sys/devices/system/cpu/online") {
            if let Some(count) = Self::parse_cpu_range(&online) {
                return Some(count);
            }
        }

        // Fallback: count CPU directories. A failed read is no count, not one CPU.
        fs::read_dir("/sys/devices/system/cpu")
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.starts_with("cpu") && name[3..].chars().all(|c| c.is_ascii_digit())
                    })
                    .count()
            })
            .filter(|&n| n > 0)
    }

    fn parse_cpu_range(range: &str) -> Option<usize> {
        // Parse ranges like "0-7" or "0,2-5,7"
        let parts: Vec<&str> = range.trim().split(',').collect();
        let mut max_cpu = 0;

        for part in parts {
            if let Some(hyphen_pos) = part.find('-') {
                let end = part[hyphen_pos + 1..].parse::<usize>().ok()?;
                max_cpu = max_cpu.max(end);
            } else {
                let cpu = part.parse::<usize>().ok()?;
                max_cpu = max_cpu.max(cpu);
            }
        }

        Some(max_cpu + 1)
    }

    fn detect_hybrid_architecture() -> bool {
        // Check for Intel Alder Lake or later (hybrid architecture)
        if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
            // Intel 12th gen+ has hybrid architecture
            if cpuinfo.contains("Intel") {
                // Check for different core types in /sys/devices/system/cpu/cpu*/topology/core_cpus_list
                // This is a simplified check - real implementation would parse topology
                return cpuinfo.contains("12th Gen")
                    || cpuinfo.contains("13th Gen")
                    || cpuinfo.contains("14th Gen")
                    || cpuinfo.contains("Ultra");
            }
        }
        false
    }

    /// RAPL energy tracking state (for power calculation)
    /// We need to track previous energy readings and timestamps to calculate power
    fn read_rapl_power_tracked(&self, domain: &str) -> Option<f32> {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::Instant;

        // Static state for energy tracking
        static PREV_ENERGY: AtomicU64 = AtomicU64::new(0);
        static PREV_TIME_NS: AtomicU64 = AtomicU64::new(0);

        // Initialize start time on first call
        static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        let start = START_TIME.get_or_init(Instant::now);

        let rapl_base = "/sys/class/powercap/intel-rapl";

        if let Ok(entries) = fs::read_dir(rapl_base) {
            for entry in entries.flatten() {
                let path = entry.path();

                // Check if this is the domain we're looking for
                if let Ok(name) = fs::read_to_string(path.join("name")) {
                    let name = name.trim();
                    if name == domain || (domain == "package" && name.starts_with("package-")) {
                        // Read current energy consumption
                        if let Ok(energy_uj) = fs::read_to_string(path.join("energy_uj")) {
                            if let Ok(current_energy) = energy_uj.trim().parse::<u64>() {
                                // Get current time in nanoseconds since start
                                let now_ns = start.elapsed().as_nanos() as u64;

                                // Get previous readings
                                let prev_energy =
                                    PREV_ENERGY.swap(current_energy, Ordering::Relaxed);
                                let prev_time = PREV_TIME_NS.swap(now_ns, Ordering::Relaxed);

                                // Calculate power if we have valid previous readings
                                if prev_time > 0 && prev_energy > 0 {
                                    // The wrap range is read only when the counter
                                    // wrapped, but it is one small file either way.
                                    let max_range =
                                        fs::read_to_string(path.join("max_energy_range_uj"))
                                            .ok()
                                            .and_then(|s| s.trim().parse::<u64>().ok());
                                    let energy_delta =
                                        rapl_energy_delta(prev_energy, current_energy, max_range);

                                    let time_delta_s =
                                        (now_ns - prev_time) as f64 / 1_000_000_000.0;

                                    if let Some(energy_delta) = energy_delta {
                                        if time_delta_s > 0.0 {
                                            // Power = Energy / Time (microjoules / seconds = microwatts)
                                            let power_uw = energy_delta as f64 / time_delta_s;
                                            let power_w = power_uw / 1_000_000.0;
                                            return Some(power_w as f32);
                                        }
                                    }
                                }

                                // No previous sample yet, or an invalid delta. Power is
                                // energy over time and needs two samples, so this reports
                                // nothing. It used to return half of
                                // constraint_0_power_limit_uw (the TDP) as an "idle
                                // estimate", a constant presented as a reading.
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Read RAPL (Running Average Power Limit) power consumption
    /// Returns power in watts for a specific domain (package, core, uncore, dram)
    fn read_rapl_power(&self, domain: &str) -> Option<f32> {
        // Use the tracked version for more accurate readings
        self.read_rapl_power_tracked(domain)
    }

    /// Get total package power from RAPL
    fn get_package_power(&self) -> Option<f32> {
        // `read_rapl_power("package")` already matches every `package-*` domain.
        // A second read in the same cycle would difference the shared energy
        // counter over microseconds and report that as the package power.
        self.read_rapl_power("package")
    }

    /// Read CPU temperature for a specific core
    fn read_cpu_temperature(&self, cpu_id: u32) -> Option<i32> {
        // Try hwmon (modern systems)
        if let Some(temp) = self.read_hwmon_temperature(cpu_id) {
            return Some(temp);
        }

        // Try thermal_zone
        if let Some(temp) = self.read_thermal_zone_temperature(cpu_id) {
            return Some(temp);
        }

        None
    }

    fn read_hwmon_temperature(&self, cpu_id: u32) -> Option<i32> {
        let hwmon_dir = "/sys/class/hwmon";
        if let Ok(entries) = fs::read_dir(hwmon_dir) {
            for entry in entries.flatten() {
                let hwmon_path = entry.path();

                if let Ok(name) = fs::read_to_string(hwmon_path.join("name")) {
                    let name = name.trim();
                    if name.contains("coretemp")
                        || name.contains("k10temp")
                        || name.contains("zenpower")
                    {
                        // Try to find the specific core's temperature
                        for i in 2..20 {
                            let label_path = hwmon_path.join(format!("temp{}_label", i));
                            let input_path = hwmon_path.join(format!("temp{}_input", i));

                            if let Ok(label) = fs::read_to_string(&label_path) {
                                if label.trim().contains(&format!("Core {}", cpu_id)) {
                                    if let Ok(temp_str) = fs::read_to_string(&input_path) {
                                        if let Ok(temp_millic) = temp_str.trim().parse::<i32>() {
                                            return Some(temp_millic / 1000);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn read_thermal_zone_temperature(&self, cpu_id: u32) -> Option<i32> {
        let thermal_dir = "/sys/class/thermal";
        if let Ok(entries) = fs::read_dir(thermal_dir) {
            for entry in entries.flatten() {
                let zone_path = entry.path();

                if let Ok(zone_type) = fs::read_to_string(zone_path.join("type")) {
                    let zone_type = zone_type.trim();
                    if zone_type.contains(&format!("cpu{}", cpu_id))
                        || zone_type.contains(&format!("core{}", cpu_id))
                    {
                        if let Ok(temp_str) = fs::read_to_string(zone_path.join("temp")) {
                            if let Ok(temp_millic) = temp_str.trim().parse::<i32>() {
                                return Some(temp_millic / 1000);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Read CPU frequency for a specific core
    fn read_cpu_frequency(&self, cpu_id: u32) -> Option<u32> {
        let freq_path = format!(
            "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq",
            cpu_id
        );
        fs::read_to_string(&freq_path)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .map(|khz| khz / 1000) // Convert kHz to MHz
    }

    /// Read CPU utilization from /proc/stat
    ///
    /// /proc/stat holds ticks since boot, so one reading is the average since
    /// boot and not the current load. This differences against the previous
    /// call's reading, kept in `PREVIOUS_TICKS`. The first call in a process has
    /// nothing to difference, so it reports no utilization for any CPU. It used
    /// to convert the since-boot counters into a percentage directly, which is
    /// the uptime average presented as the current load.
    fn read_cpu_utilization(&self) -> HashMap<u32, u8> {
        static PREVIOUS_TICKS: std::sync::Mutex<Option<HashMap<u32, (u64, u64)>>> =
            std::sync::Mutex::new(None);

        let current = fs::read_to_string("/proc/stat")
            .map(|stat| parse_proc_stat_ticks(&stat))
            .unwrap_or_default();

        let mut previous = PREVIOUS_TICKS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let utilization = match previous.as_ref() {
            Some(prev) => cpu_utilization_between(prev, &current),
            None => HashMap::new(),
        };
        *previous = Some(current);
        utilization
    }

    /// Determine cluster type for hybrid CPUs
    fn determine_cluster_type(&self, cpu_id: u32) -> CpuClusterType {
        if !self.has_hybrid_cpu {
            return CpuClusterType::Standard;
        }

        // For Intel hybrid CPUs, E-cores typically have lower max frequency
        // This is a heuristic - real implementation would use topology data
        let max_freq_path = format!(
            "/sys/devices/system/cpu/cpu{}/cpufreq/cpuinfo_max_freq",
            cpu_id
        );
        if let Ok(max_freq_str) = fs::read_to_string(&max_freq_path) {
            if let Ok(max_freq_khz) = max_freq_str.trim().parse::<u32>() {
                // E-cores typically have max freq < 4 GHz (4000000 kHz)
                // P-cores typically have max freq >= 4 GHz
                if max_freq_khz < 4_000_000 {
                    return CpuClusterType::Efficiency;
                } else {
                    return CpuClusterType::Performance;
                }
            }
        }

        CpuClusterType::Standard
    }

    /// Read NPU utilization using runtime PM active/suspended time deltas.
    ///
    /// The kernel exposes cumulative `runtime_active_time` and `runtime_suspended_time`
    /// in milliseconds.  By comparing two readings we can derive a duty-cycle percentage.
    /// NPU utilization from runtime-PM time deltas, or `None`.
    ///
    /// Two paths used to return `0`. The sysfs files being unreadable is not
    /// an idle NPU; and the **first** call has no previous sample to difference
    /// against, so it cannot know the utilization at all — it returned 0% for
    /// every NPU on the first tick after startup, which is the same defect as
    /// an intrusion baseline reporting "clean" on its first run.
    fn read_npu_utilization(accel_path: &Path) -> Option<u8> {
        use std::collections::HashMap;
        use std::sync::Mutex;

        // Static storage for previous readings keyed by sysfs path
        static PREV: std::sync::OnceLock<Mutex<HashMap<String, (u64, u64)>>> =
            std::sync::OnceLock::new();
        let prev_map = PREV.get_or_init(|| Mutex::new(HashMap::new()));

        let key = accel_path.to_string_lossy().to_string();

        let active = std::fs::read_to_string(accel_path.join("device/power/runtime_active_time"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok());
        let suspended =
            std::fs::read_to_string(accel_path.join("device/power/runtime_suspended_time"))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok());

        if let (Some(active_ms), Some(suspended_ms)) = (active, suspended) {
            if let Ok(mut map) = prev_map.lock() {
                if let Some(&(prev_active, prev_suspended)) = map.get(&key) {
                    let delta_active = active_ms.saturating_sub(prev_active);
                    let delta_suspended = suspended_ms.saturating_sub(prev_suspended);
                    let delta_total = delta_active + delta_suspended;

                    // Update stored values
                    map.insert(key, (active_ms, suspended_ms));

                    if let Some(pct) = (delta_active * 100).checked_div(delta_total) {
                        return Some(pct.min(100) as u8);
                    }
                    // `delta_total` was zero: no time passed between samples,
                    // so there is no ratio to report yet.
                } else {
                    // First reading — store the baseline. There is nothing to
                    // difference against, so nothing is known yet.
                    map.insert(key, (active_ms, suspended_ms));
                }
            }
        }

        None
    }
}

/// One `/sys/class/accel` device as an NPU.
///
/// The vendor is read from the PCI vendor id: 0x8086 Intel, 0x1022 AMD, 0x1ae0
/// Google. Any other id, or none, leaves the vendor unknown rather than naming
/// the device's class.
fn accel_npu(path: &Path, dev_name: &str, index: usize) -> NpuInfo {
    let vendor_id = fs::read_to_string(path.join("device/vendor"))
        .ok()
        .map(|s| s.trim().to_string());
    let device_id = fs::read_to_string(path.join("device/device"))
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let (vendor, name) = match vendor_id.as_deref() {
        Some("0x8086") => ("Intel", format!("Intel AI Boost NPU {index}")),
        Some("0x1022") => {
            let name = match device_id.as_str() {
                "0x1502" => "AMD Ryzen AI (Phoenix XDNA)".to_string(),
                "0x17f0" => "AMD Ryzen AI (Hawk Point XDNA)".to_string(),
                "0x1569" => "AMD Ryzen AI (Strix Point XDNA2)".to_string(),
                _ => format!("AMD XDNA NPU ({dev_name})"),
            };
            ("AMD", name)
        }
        // A TPU is named by its device node, not a version: the sysfs entry
        // carries no version and the index is not one.
        Some("0x1ae0") => ("Google", format!("Google TPU ({dev_name})")),
        _ => {
            return NpuInfo {
                name: format!("Accelerator ({dev_name})"),
                vendor: None,
                cores: None,
                utilization: None,
                power_watts: None,
                frequency_mhz: None,
            }
        }
    };

    NpuInfo {
        name,
        vendor: Some(vendor.to_string()),
        // Neither the core count nor a TPU utilization API is read here.
        cores: None,
        utilization: LinuxSiliconMonitor::read_npu_utilization(path),
        power_watts: fs::read_to_string(path.join("device/power/power_usage"))
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .map(|p| p / 1_000_000.0), // Convert from microwatts
        frequency_mhz: None,
    }
}

impl SiliconMonitor for LinuxSiliconMonitor {
    fn cpu_info(&self) -> Result<(Vec<CpuCore>, Vec<CpuCluster>)> {
        let mut cores = Vec::new();
        let utilization_map = self.read_cpu_utilization();

        // Group cores by cluster type
        let mut p_cores = Vec::new();
        let mut e_cores = Vec::new();
        let mut std_cores = Vec::new();

        // Without a count there is no core list to build, and an empty one would
        // read as a machine with no CPUs.
        let cpu_count = self.cpu_count.ok_or_else(|| {
            crate::error::IronError::System(
                "CPU count could not be read from /sys/devices/system/cpu".into(),
            )
        })?;

        for cpu_id in 0..cpu_count as u32 {
            let cluster = self.determine_cluster_type(cpu_id);
            // A core whose scaling_cur_freq could not be read has no
            // frequency, not a frequency of zero.
            let frequency = self.read_cpu_frequency(cpu_id);
            // /proc/stat carries a line per cpu; a core missing from it was
            // not measured.
            let utilization = utilization_map.get(&cpu_id).copied();
            let temperature = self.read_cpu_temperature(cpu_id);

            let core = CpuCore {
                id: cpu_id,
                cluster,
                frequency_mhz: frequency,
                utilization,
                temperature,
            };

            cores.push(core.clone());

            match cluster {
                CpuClusterType::Performance => p_cores.push(core),
                CpuClusterType::Efficiency => e_cores.push(core),
                CpuClusterType::Standard => std_cores.push(core),
            }
        }

        // Read package power from RAPL once (expensive operation)
        let package_power = self.get_package_power();

        // Create clusters
        let mut clusters = Vec::new();

        if !p_cores.is_empty() {
            let avg_freq = average_reported_mhz(&p_cores);
            let avg_util = average_reported_util(&p_cores);

            // RAPL has one domain for the package and none per cluster, so a
            // P-cluster figure cannot be read. It used to be 60% of the package
            // power, a fixed split with no measurement behind it.
            clusters.push(CpuCluster {
                cluster_type: CpuClusterType::Performance,
                core_ids: Some(p_cores.iter().map(|c| c.id).collect()),
                frequency_mhz: avg_freq,
                utilization: avg_util,
                power_watts: None,
            });
        }

        if !e_cores.is_empty() {
            let avg_freq = average_reported_mhz(&e_cores);
            let avg_util = average_reported_util(&e_cores);

            // As for the P-cluster: no per-cluster RAPL domain exists. This used
            // to be a fixed 40% of the package power.
            clusters.push(CpuCluster {
                cluster_type: CpuClusterType::Efficiency,
                core_ids: Some(e_cores.iter().map(|c| c.id).collect()),
                frequency_mhz: avg_freq,
                utilization: avg_util,
                power_watts: None,
            });
        }

        if !std_cores.is_empty() {
            let avg_freq = average_reported_mhz(&std_cores);
            let avg_util = average_reported_util(&std_cores);

            clusters.push(CpuCluster {
                cluster_type: CpuClusterType::Standard,
                core_ids: Some(std_cores.iter().map(|c| c.id).collect()),
                frequency_mhz: avg_freq,
                utilization: avg_util,
                power_watts: package_power, // Full package power for standard cores
            });
        }

        Ok((cores, clusters))
    }

    fn npu_info(&self) -> Result<Vec<NpuInfo>> {
        let mut npus = Vec::new();

        // Every NPU and AI accelerator the kernel exposes is a /sys/class/accel
        // device, and each is listed once. The vendor comes from the device's PCI
        // vendor id. This used to label every device "Intel" in one loop, and a
        // second loop then listed the AMD XDNA devices again.
        if let Ok(entries) = std::fs::read_dir("/sys/class/accel") {
            for (index, entry) in entries.flatten().enumerate() {
                let dev_name = entry.file_name().to_string_lossy().to_string();
                npus.push(accel_npu(&entry.path(), &dev_name, index));
            }
        }

        // Qualcomm Hexagon DSP detection (if available)
        if std::path::Path::new("/sys/kernel/debug/msm_fastrpc").exists() {
            npus.push(NpuInfo {
                name: "Qualcomm Hexagon DSP".to_string(),
                vendor: Some("Qualcomm".to_string()),
                cores: None,
                utilization: None,
                power_watts: None,
                frequency_mhz: None,
            });
        }

        Ok(npus)
    }

    fn io_info(&self) -> Result<Vec<IoController>> {
        let mut controllers = Vec::new();

        // PCIe controller monitoring via /sys/bus/pci
        if let Ok(entries) = std::fs::read_dir("/sys/bus/pci/devices") {
            for entry in entries.flatten() {
                let path = entry.path();
                let device_name = entry.file_name().to_string_lossy().to_string();

                // Read device class to identify controller types
                if let Ok(class) = std::fs::read_to_string(path.join("class")) {
                    let class = class.trim();

                    // NVMe Controller (0x010802)
                    if class.starts_with("0x0108") {
                        // Read current link speed. `None` when the attribute is
                        // missing or unparsable: the capacity below is then unknown,
                        // not zero or one lane.
                        let cur_speed = std::fs::read_to_string(path.join("current_link_speed"))
                            .ok()
                            .and_then(|s| {
                                // Parse "8.0 GT/s" or "16 GT/s PCIe"
                                s.split_whitespace()
                                    .next()
                                    .and_then(|n| n.parse::<f64>().ok())
                            });

                        let cur_width = std::fs::read_to_string(path.join("current_link_width"))
                            .ok()
                            .and_then(|s| s.trim().parse::<u32>().ok());

                        // `cur_speed * cur_width * encoding` is the capacity of
                        // the *negotiated link*, not the traffic on it. It was
                        // assigned to `bandwidth_mbps`, documented as "current
                        // bandwidth", so an idle NVMe drive reported about 3900
                        // MB/s of I/O — the same defect as the Apple
                        // Thunderbolt row, which set current to the theoretical
                        // maximum.
                        //
                        // It is a real and useful ceiling, so it becomes the
                        // maximum. The negotiated link is the honest one to
                        // use: a device that supports PCIe 4.0 x4 but trained
                        // at 3.0 x2 cannot exceed what it trained at.
                        let max_bandwidth_mbps = match (cur_speed, cur_width) {
                            (Some(speed), Some(width)) => Some(pcie_link_mb_per_s(speed, width)),
                            _ => None,
                        };

                        // Try to read device name
                        let name = std::fs::read_to_string(path.join("device"))
                            .ok()
                            .map(|id| format!("NVMe Controller {}", id.trim()))
                            .unwrap_or_else(|| format!("NVMe Controller ({})", device_name));

                        controllers.push(IoController {
                            controller_type: "NVMe".to_string(),
                            name,
                            // No traffic counter is read on this path.
                            bandwidth_mbps: None,
                            max_bandwidth_mbps,
                            power_watts: None,
                        });
                    }
                    // USB Controller (0x0c03)
                    else if class.starts_with("0x0c03") {
                        let name = format!("USB Controller ({})", device_name);

                        controllers.push(IoController {
                            controller_type: "USB".to_string(),
                            name,
                            // Not monitored, and 2500 was USB 3.2 Gen 2x2 — the
                            // fastest variant — applied to any xHCI controller.
                            // A USB 3.0 controller is 500 MB/s.
                            bandwidth_mbps: None,
                            max_bandwidth_mbps: None,
                            power_watts: None,
                        });
                    }
                    // Thunderbolt Controller (0x0c0a)
                    else if class.starts_with("0x0c0a") {
                        let name = format!("Thunderbolt Controller ({})", device_name);

                        controllers.push(IoController {
                            controller_type: "Thunderbolt".to_string(),
                            name,
                            // Neither figure is read; 5000 assumed TB4, and the
                            // class code does not distinguish TB3 from TB4.
                            bandwidth_mbps: None,
                            max_bandwidth_mbps: None,
                            power_watts: None,
                        });
                    }
                    // SATA Controller (0x0106)
                    else if class.starts_with("0x0106") {
                        let name = format!("SATA Controller ({})", device_name);

                        controllers.push(IoController {
                            controller_type: "SATA".to_string(),
                            name,
                            // Not monitored. 600 is SATA 3.0 per *port*, and
                            // this row is a controller, which has several.
                            bandwidth_mbps: None,
                            max_bandwidth_mbps: None,
                            power_watts: None,
                        });
                    }
                }
            }
        }

        Ok(controllers)
    }

    fn network_info(&self) -> Result<Vec<NetworkSilicon>> {
        let mut network_devices = Vec::new();

        // Read network interfaces from /sys/class/net
        if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
            for entry in entries.flatten() {
                let interface = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();

                // Skip loopback and virtual interfaces
                if interface == "lo"
                    || interface.starts_with("veth")
                    || interface.starts_with("docker")
                {
                    continue;
                }

                // Read link speed (in Mbps)
                // A down interface reports a negative speed, and an interface
                // whose `speed` file cannot be read reports nothing. Neither is
                // a link running at zero megabits.
                let link_speed_mbps = std::fs::read_to_string(path.join("speed"))
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .filter(|&s| s > 0)
                    .map(|s| s as u32);

                // Read statistics
                let _rx_bytes = std::fs::read_to_string(path.join("statistics/rx_bytes"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0);

                let _tx_bytes = std::fs::read_to_string(path.join("statistics/tx_bytes"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0);

                // rx_packets and tx_packets are cumulative since boot. A packet
                // rate needs two samples, so none is reported from one read.

                // Read power state if available
                let power_state = std::fs::read_to_string(path.join("device/power_state"))
                    .ok()
                    .map(|s| s.trim().to_string());

                // Detect interface type
                let device_path = path.join("device");
                let interface_type = if device_path.exists() {
                    // Check if it's WiFi
                    if std::path::Path::new(&format!("/sys/class/net/{}/phy80211", interface))
                        .exists()
                    {
                        "WiFi"
                    }
                    // Check PCI device class for Ethernet
                    else if let Ok(class) = std::fs::read_to_string(device_path.join("class")) {
                        if class.trim().starts_with("0x0200") {
                            "Ethernet"
                        } else {
                            "Network"
                        }
                    } else {
                        "Network"
                    }
                } else {
                    "Virtual"
                };

                // Only add physical network devices
                if interface_type != "Virtual" {
                    network_devices.push(NetworkSilicon {
                        interface,
                        link_speed_mbps,
                        // One read cannot give a rate; see the field docs.
                        rx_bandwidth_mbps: None,
                        tx_bandwidth_mbps: None,
                        packet_rate: None,
                        power_state,
                    });
                }
            }
        }

        // Try to read WiFi-specific information
        if let Ok(entries) = std::fs::read_dir("/sys/class/ieee80211") {
            for entry in entries.flatten() {
                // The phy's netdev name comes from its device/net directory. A
                // phy with no netdev has no interface to report, so it is
                // skipped. It used to be named "wlan{index}", a made-up
                // interface name that matches nothing on the machine.
                let Some(interface_name) = std::fs::read_dir(entry.path().join("device/net"))
                    .ok()
                    .and_then(|mut entries| entries.next())
                    .and_then(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                else {
                    continue;
                };

                // Only add if not already in list
                if !network_devices
                    .iter()
                    .any(|d| d.interface == interface_name)
                {
                    network_devices.push(NetworkSilicon {
                        interface: interface_name,
                        // Needs iwconfig or nl80211, neither of which is
                        // called here.
                        link_speed_mbps: None,
                        rx_bandwidth_mbps: None,
                        tx_bandwidth_mbps: None,
                        packet_rate: None,
                        power_state: None,
                    });
                }
            }
        }

        Ok(network_devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gen 4 x4 is 16 GT/s with 128b/130b: about 7877 MB/s, not 63000.
    #[test]
    fn pcie_link_bandwidth_is_megabytes_per_second() {
        let gen4_x4 = pcie_link_mb_per_s(16.0, 4);
        assert!((gen4_x4 - 7877.0).abs() < 1.0, "{gen4_x4}");
        // Gen 2 uses 8b/10b: 5 GT/s x1 is 500 MB/s.
        assert!((pcie_link_mb_per_s(5.0, 1) - 500.0).abs() < 1e-9);
    }

    /// A monotonic counter gives its gain. A wrap needs the counter's range,
    /// and without one the interval has no energy figure.
    #[test]
    fn rapl_energy_delta_handles_a_wrap_or_reports_none() {
        assert_eq!(rapl_energy_delta(100, 250, None), Some(150));
        assert_eq!(rapl_energy_delta(900, 100, Some(1000)), Some(200));
        assert_eq!(rapl_energy_delta(900, 100, None), None);
        // A range no larger than the previous reading cannot be right.
        assert_eq!(rapl_energy_delta(900, 100, Some(800)), None);
    }

    /// The aggregate `cpu ` line is not CPU 0, and utilization is the busy
    /// share of the ticks gained between two readings, not the since-boot share.
    #[test]
    fn cpu_utilization_is_differenced_between_readings() {
        let first = concat!(
            "cpu  900 0 100 8000 0 0 0 0 0 0\n",
            "cpu0 450 0 50 4000 0 0 0 0 0 0\n",
            "cpu1 450 0 50 4000 0 0 0 0 0 0\n",
        );
        // Since boot, cpu0 is about 11% busy (500 of 4500 ticks). Over the
        // interval it was 75% busy (75 of 100 ticks), and cpu1 was 20%.
        let second = concat!(
            "cpu  1000 0 200 8400 0 0 0 0 0 0\n",
            "cpu0 575 0 0 4025 0 0 0 0 0 0\n",
            "cpu1 470 0 50 4080 0 0 0 0 0 0\n",
        );
        let before = parse_proc_stat_ticks(first);
        assert_eq!(before.len(), 2);
        assert_eq!(before[&0], (4000, 4500));
        let after = parse_proc_stat_ticks(second);
        let util = cpu_utilization_between(&before, &after);
        assert_eq!(util.get(&0), Some(&75));
        assert_eq!(util.get(&1), Some(&20));
        // No previous reading means no utilization, not the since-boot figure.
        assert!(cpu_utilization_between(&HashMap::new(), &after).is_empty());
    }

    /// A counter that went backwards is a reset, and the CPU reads as unmeasured.
    #[test]
    fn cpu_utilization_drops_a_counter_reset() {
        let before = parse_proc_stat_ticks("cpu0 500 0 0 500 0 0 0 0 0 0\n");
        let after = parse_proc_stat_ticks("cpu0 10 0 0 90 0 0 0 0 0 0\n");
        assert!(cpu_utilization_between(&before, &after).is_empty());
    }

    /// An accelerator's vendor is its PCI vendor id, and an id nothing names is
    /// unknown rather than the class the device was found under.
    #[test]
    fn accel_vendor_comes_from_the_pci_vendor_id() {
        let root = std::env::temp_dir().join(format!(
            "ironmon-accel-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let make = |dir: &str, vendor: Option<&str>, device: Option<&str>| {
            let path = root.join(dir);
            fs::create_dir_all(path.join("device")).unwrap();
            if let Some(v) = vendor {
                fs::write(path.join("device/vendor"), format!("{v}\n")).unwrap();
            }
            if let Some(d) = device {
                fs::write(path.join("device/device"), format!("{d}\n")).unwrap();
            }
            path
        };
        let amd = make("accel1", Some("0x1022"), Some("0x17f0"));
        let intel = make("accel0", Some("0x8086"), None);
        let other = make("accel2", Some("0xabcd"), None);
        let bare = make("accel3", None, None);

        let amd_npu = accel_npu(&amd, "accel1", 1);
        assert_eq!(amd_npu.vendor.as_deref(), Some("AMD"));
        assert_eq!(amd_npu.name, "AMD Ryzen AI (Hawk Point XDNA)");
        let intel_npu = accel_npu(&intel, "accel0", 0);
        assert_eq!(intel_npu.vendor.as_deref(), Some("Intel"));
        assert_eq!(accel_npu(&other, "accel2", 2).vendor, None);
        assert_eq!(accel_npu(&bare, "accel3", 3).vendor, None);

        let _ = fs::remove_dir_all(&root);
    }

    /// A parsed `/proc/stat` line with no fields after the name is skipped, not
    /// read as a zero-tick CPU.
    #[test]
    fn short_proc_stat_lines_are_not_cpus() {
        assert!(parse_proc_stat_ticks("cpu3\ncpu\n").is_empty());
    }
}
