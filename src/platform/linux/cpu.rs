//! Linux CPU monitoring

use crate::core::cpu::{CpuCore, CpuFrequency, CpuStats, CpuTotal};
use crate::error::Result;
use crate::platform::common::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Read CPU statistics
pub fn read_cpu_stats() -> Result<CpuStats> {
    let mut stats = CpuStats::empty();

    // Read CPU times from /proc/stat
    let proc_stat = fs::read_to_string("/proc/stat")?;
    let cpu_times = parse_proc_stat(&proc_stat)?;

    // Get number of CPUs. `/proc/stat` is passed as the last resort because it
    // is already in hand and lists one line per online CPU.
    let cpu_count = get_cpu_count(&cpu_times);

    // Read per-core information
    for cpu_id in 0..cpu_count {
        let core = read_cpu_core(cpu_id, &cpu_times)?;
        stats.cores.push(core);
    }

    // Calculate totals
    stats.total = calculate_total(&stats.cores);

    Ok(stats)
}

/// The number of CPUs to enumerate, or 0 when none of the three sources could be
/// read.
///
/// Zero is deliberate. This count is a loop bound, so a wrong value under-reports
/// rather than inventing a figure — but it used to fall back to `1`, and a
/// container with a restricted `/sys` would then describe a 24-core host as a
/// single-core machine. One is inside the range of counts a real machine has, so
/// every guard downstream that tests `physical_cores > 0` accepts it; zero fails
/// that guard, which is the whole point of the guard. Same class as the
/// `unwrap_or(1)` in `cpu_microarch` and the base-M1 default in `silicon/apple`.
fn get_cpu_count(cpu_times: &[(String, Vec<u64>)]) -> usize {
    if let Some(count) = fs::read_to_string("/sys/devices/system/cpu/online")
        .ok()
        .and_then(|s| parse_cpu_range(&s))
    {
        return count;
    }

    // Then the per-CPU directories, which are present even for offline CPUs.
    let dirs = fs::read_dir("/sys/devices/system/cpu")
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| is_cpu_n(&e.file_name().to_string_lossy()))
                .count()
        })
        .unwrap_or(0);
    if dirs > 0 {
        return dirs;
    }

    // Finally `/proc/stat`, which the caller has already read. It carries one
    // `cpuN` line per online CPU beside the `cpu` aggregate, which is skipped.
    count_proc_stat_cpus(cpu_times)
}

/// `cpu0`, `cpu17` — not `cpufreq`, `cpuidle`, or the bare `cpu` aggregate.
fn is_cpu_n(name: &str) -> bool {
    match name.strip_prefix("cpu") {
        Some(rest) => !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

fn count_proc_stat_cpus(cpu_times: &[(String, Vec<u64>)]) -> usize {
    cpu_times.iter().filter(|(name, _)| is_cpu_n(name)).count()
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

fn read_cpu_core(cpu_id: usize, cpu_times: &[(String, Vec<u64>)]) -> Result<CpuCore> {
    let cpu_path = format!("/sys/devices/system/cpu/cpu{}", cpu_id);
    let online_path = format!("{}/online", cpu_path);

    // Check if CPU is online
    let online = if Path::new(&online_path).exists() {
        read_file_u32(&online_path)? == 1
    } else {
        true // CPU0 doesn't have online file
    };

    // Read governor
    let governor_path = format!("{}/cpufreq/scaling_governor", cpu_path);
    let governor = read_file_string(&governor_path).unwrap_or_else(|_| "unknown".to_string());

    // Read frequency
    let frequency = if online {
        read_cpu_frequency(&cpu_path).ok()
    } else {
        None
    };

    // Get CPU times for this core
    let cpu_name = format!("cpu{}", cpu_id);
    // A core with no `/proc/stat` line was not measured. It used to fall back to
    // `Some(100.0)` — a *measured, fully idle* core — which is the same expression
    // this crate has now removed from eight consumer surfaces, here at its source.
    // `/proc/stat` lists only online CPUs while `get_cpu_count` counts from the
    // sysfs `online` range, which is `max + 1`, so any machine with an offline CPU
    // below the highest reaches this arm.
    let (user, nice, system, idle) = cpu_times
        .iter()
        .find(|(name, _)| name == &cpu_name)
        .map(|(_, times)| calculate_cpu_percentages(times))
        .unwrap_or((None, None, None, None));

    // Read CPU model
    let model = read_cpu_model();

    Ok(CpuCore {
        id: cpu_id,
        online,
        governor,
        frequency,
        user,
        nice,
        system,
        idle,
        model,
    })
}

/// Read CPU temperature for a specific core
/// Tries multiple sources: hwmon, thermal_zone, coretemp
pub fn read_cpu_temperature(cpu_id: usize) -> Option<i32> {
    // Try hwmon (modern systems)
    if let Some(temp) = read_hwmon_temperature(cpu_id) {
        return Some(temp);
    }

    // Try thermal_zone
    if let Some(temp) = read_thermal_zone_temperature(cpu_id) {
        return Some(temp);
    }

    // Try coretemp
    if let Some(temp) = read_coretemp_temperature(cpu_id) {
        return Some(temp);
    }

    None
}

fn read_hwmon_temperature(cpu_id: usize) -> Option<i32> {
    // Search /sys/class/hwmon for CPU temperature sensors
    let hwmon_dir = "/sys/class/hwmon";
    if let Ok(entries) = fs::read_dir(hwmon_dir) {
        for entry in entries.flatten() {
            let hwmon_path = entry.path();

            // Check if this is a CPU temperature sensor
            if let Ok(name) = fs::read_to_string(hwmon_path.join("name")) {
                let name = name.trim();
                if name.contains("coretemp")
                    || name.contains("k10temp")
                    || name.contains("zenpower")
                {
                    // Try to find the specific core's temperature
                    for i in 2..20 {
                        // temp1 is usually package, temp2+ are cores
                        let label_path = hwmon_path.join(format!("temp{}_label", i));
                        let input_path = hwmon_path.join(format!("temp{}_input", i));

                        if let Ok(label) = fs::read_to_string(&label_path) {
                            let label = label.trim();
                            if label.contains(&format!("Core {}", cpu_id)) {
                                if let Ok(temp_str) = fs::read_to_string(&input_path) {
                                    if let Ok(temp_millic) = temp_str.trim().parse::<i32>() {
                                        return Some(temp_millic / 1000); // Convert to Celsius
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

fn read_thermal_zone_temperature(cpu_id: usize) -> Option<i32> {
    // Try /sys/class/thermal/thermal_zone*/temp
    let thermal_dir = "/sys/class/thermal";
    if let Ok(entries) = fs::read_dir(thermal_dir) {
        for entry in entries.flatten() {
            let zone_path = entry.path();

            if let Ok(zone_type) = fs::read_to_string(zone_path.join("type")) {
                let zone_type = zone_type.trim();
                if zone_type.contains(&format!("cpu{}", cpu_id))
                    || zone_type.contains(&format!("core{}", cpu_id))
                    || zone_type.contains(&"x86_pkg_temp".to_string())
                {
                    if let Ok(temp_str) = fs::read_to_string(zone_path.join("temp")) {
                        if let Ok(temp_millic) = temp_str.trim().parse::<i32>() {
                            return Some(temp_millic / 1000); // Convert to Celsius
                        }
                    }
                }
            }
        }
    }
    None
}

fn read_coretemp_temperature(_cpu_id: usize) -> Option<i32> {
    // Fallback: try the package temperature as an approximation
    let paths = [
        "/sys/class/hwmon/hwmon0/temp1_input",
        "/sys/class/hwmon/hwmon1/temp1_input",
        "/sys/class/thermal/thermal_zone0/temp",
    ];

    for path in &paths {
        if let Ok(temp_str) = fs::read_to_string(path) {
            if let Ok(temp_millic) = temp_str.trim().parse::<i32>() {
                return Some(temp_millic / 1000);
            }
        }
    }

    None
}

/// Get CPU temperatures for all cores
pub fn read_all_cpu_temperatures() -> HashMap<usize, i32> {
    let mut temperatures = HashMap::new();
    // This caller has no `/proc/stat` in hand, so it reads one for the last-resort
    // count. An unreadable `/proc/stat` leaves the slice empty, which is the
    // "could not tell" answer rather than a machine with one core.
    let cpu_times = fs::read_to_string("/proc/stat")
        .ok()
        .and_then(|s| parse_proc_stat(&s).ok())
        .unwrap_or_default();
    let cpu_count = get_cpu_count(&cpu_times);

    for cpu_id in 0..cpu_count {
        if let Some(temp) = read_cpu_temperature(cpu_id) {
            temperatures.insert(cpu_id, temp);
        }
    }

    temperatures
}

fn read_cpu_frequency(cpu_path: &str) -> Result<CpuFrequency> {
    let cur_path = format!("{}/cpufreq/scaling_cur_freq", cpu_path);
    let min_path = format!("{}/cpufreq/scaling_min_freq", cpu_path);
    let max_path = format!("{}/cpufreq/scaling_max_freq", cpu_path);

    Ok(CpuFrequency {
        // `scaling_cur_freq` is a clock the kernel reports, not a nominal
        // scaled by a ratio, so this is a measurement. See the Windows reader,
        // where it is not.
        current_is_derived: false,
        current: Some(read_file_u32(&cur_path)? / 1000), // Convert kHz to MHz
        min: Some(read_file_u32(&min_path)? / 1000),
        max: Some(read_file_u32(&max_path)? / 1000),
    })
}

fn read_cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|content| {
            for line in content.lines() {
                if line.starts_with("model name") || line.starts_with("Processor") {
                    if let Some(pos) = line.find(':') {
                        return Some(line[pos + 1..].trim().to_string());
                    }
                }
            }
            None
        })
        .unwrap_or_else(|| "Unknown".to_string())
}

fn parse_proc_stat(content: &str) -> Result<Vec<(String, Vec<u64>)>> {
    let mut cpu_times = Vec::new();

    for line in content.lines() {
        if line.starts_with("cpu") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let cpu_name = parts[0].to_string();
            let times: Vec<u64> = parts[1..].iter().filter_map(|s| s.parse().ok()).collect();

            cpu_times.push((cpu_name, times));
        }
    }

    Ok(cpu_times)
}

fn calculate_cpu_percentages(
    times: &[u64],
) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
    if times.len() < 4 {
        return (None, None, None, None);
    }

    let user = times[0];
    let nice = times[1];
    let system = times[2];
    let idle = times[3];

    let total = times.iter().sum::<u64>() as f32;

    if total == 0.0 {
        return (Some(0.0), Some(0.0), Some(0.0), Some(100.0));
    }

    (
        Some((user as f32 / total) * 100.0),
        Some((nice as f32 / total) * 100.0),
        Some((system as f32 / total) * 100.0),
        Some((idle as f32 / total) * 100.0),
    )
}

fn calculate_total(cores: &[CpuCore]) -> CpuTotal {
    let online_cores: Vec<&CpuCore> = cores.iter().filter(|c| c.online).collect();

    if online_cores.is_empty() {
        return CpuTotal {
            user: 0.0,
            nice: 0.0,
            system: 0.0,
            idle: 100.0,
        };
    }

    let count = online_cores.len() as f32;

    CpuTotal {
        user: online_cores.iter().filter_map(|c| c.user).sum::<f32>() / count,
        nice: online_cores.iter().filter_map(|c| c.nice).sum::<f32>() / count,
        system: online_cores.iter().filter_map(|c| c.system).sum::<f32>() / count,
        idle: online_cores.iter().filter_map(|c| c.idle).sum::<f32>() / count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_n_directories_are_told_from_the_other_cpu_entries() {
        assert!(is_cpu_n("cpu0"));
        assert!(is_cpu_n("cpu23"));
        // `/sys/devices/system/cpu` holds these beside the per-CPU directories,
        // and the old predicate's `[3..]` slice counted the bare aggregate.
        assert!(!is_cpu_n("cpufreq"));
        assert!(!is_cpu_n("cpuidle"));
        assert!(!is_cpu_n("cpu"));
        assert!(!is_cpu_n("possible"));
    }

    #[test]
    fn proc_stat_cpus_are_counted_without_the_aggregate() {
        let times =
            parse_proc_stat("cpu  100 0 50 900\ncpu0 50 0 25 450\ncpu1 50 0 25 450\nintr 12\n")
                .unwrap();
        // Three lines start with "cpu"; two of them are CPUs.
        assert_eq!(times.len(), 3);
        assert_eq!(count_proc_stat_cpus(&times), 2);
    }

    #[test]
    fn an_unreadable_cpu_count_is_zero_and_not_one() {
        // The property, stated against the last resort: with nothing to count,
        // the answer is 0. A fallback of 1 is inside the range of real counts,
        // so every downstream `> 0` guard would accept a failed read as a
        // single-core machine.
        assert_eq!(count_proc_stat_cpus(&[]), 0);
        assert_eq!(
            count_proc_stat_cpus(&[("cpu".to_string(), vec![1, 2, 3, 4])]),
            0
        );
    }

    #[test]
    fn a_core_absent_from_proc_stat_is_unread_not_idle() {
        // cpu1 is offline, so `/proc/stat` has no line for it while the sysfs
        // `online` range still reports 2 CPUs.
        let times = parse_proc_stat("cpu  100 0 50 900\ncpu0 50 0 25 450\n").unwrap();
        let core = read_cpu_core(1, &times).unwrap();
        assert_eq!(
            core.idle, None,
            "an unmeasured core must not report as 100% idle"
        );
        assert_eq!(core.user, None);
        assert_eq!(core.system, None);
    }

    #[test]
    fn a_core_present_in_proc_stat_reports_its_measurement() {
        let times = parse_proc_stat("cpu  100 0 50 900\ncpu0 50 0 25 425\n").unwrap();
        let core = read_cpu_core(0, &times).unwrap();
        assert!(core.idle.is_some(), "cpu0 has a line and must be measured");
        // 425 idle of 500 total.
        assert!((core.idle.unwrap() - 85.0).abs() < 0.01, "{:?}", core.idle);
    }

    #[test]
    fn cpu_ranges_parse_as_a_count() {
        assert_eq!(parse_cpu_range("0-23"), Some(24));
        assert_eq!(parse_cpu_range("0"), Some(1));
        assert_eq!(parse_cpu_range("0,2-5,7"), Some(8));
        assert_eq!(parse_cpu_range(""), None);
    }
}
