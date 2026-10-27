//! Linux CPU monitoring

use crate::core::cpu::{CpuCore, CpuFrequency, CpuStats, CpuTotal};
use crate::error::Result;
use crate::platform::common::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// Read CPU statistics
pub fn read_cpu_stats() -> Result<CpuStats> {
    let mut stats = CpuStats::empty();

    // Tick deltas over the sampling interval, which is what every percentage is
    // a share of. `latest` is the raw reading, used to count the CPUs.
    let (latest, cpu_times) = sample_cpu_ticks()?;

    // Get number of CPUs. `/proc/stat` is passed as the last resort because it
    // is already in hand and lists one line per online CPU.
    let cpu_count = get_cpu_count(&latest);

    // Read per-core information
    for cpu_id in 0..cpu_count {
        let core = read_cpu_core(cpu_id, &cpu_times)?;
        stats.cores.push(core);
    }

    // Calculate totals
    stats.total = calculate_total(&cpu_times, &stats.cores).ok_or_else(|| {
        crate::error::IronError::Parse(
            "/proc/stat gave no CPU times to total: neither the aggregate line nor any \
             online core could be read"
                .to_string(),
        )
    })?;

    Ok(stats)
}

/// How long the first call waits between its two `/proc/stat` reads.
///
/// The kernel counts ticks at `USER_HZ`, 100 a second (10 ms) on the builds that
/// matter, so a 200 ms gap is about twenty ticks per CPU. That is enough for the
/// split to be a real share rather than rounding noise, and short enough that a
/// one-shot command does not feel the wait.
const PRIME_INTERVAL: Duration = Duration::from_millis(200);

/// The shortest gap between the two readings a percentage is taken from.
///
/// Each `/proc/stat` line rounds its nanosecond counter down to whole ticks on its
/// own, so over a window of a tick or two the rounding is the whole delta. The
/// aggregate line and a per-core line then disagree with no load change at all:
/// one core reads fully busy while the aggregate reads fully idle, and the mean of
/// the cores says the machine was half loaded. Two hundred milliseconds is about
/// twenty ticks per CPU, which keeps that rounding to a few percent.
///
/// It used to difference against whatever reading came before, however recent.
/// The collector's first tick after its warm-up publish did exactly that, so the
/// first real snapshot could be a one-tick window reported as a measurement.
const MIN_WINDOW: Duration = PRIME_INTERVAL;

/// The `cpu` aggregate and each `cpuN` line of one `/proc/stat` reading, as
/// `(name, cumulative ticks)`.
type CpuTicks = Vec<(String, Vec<u64>)>;

/// The most recent `/proc/stat` reading in this process and when it was taken,
/// which the next call differences against.
static PREVIOUS_TICKS: Mutex<Option<(Instant, CpuTicks)>> = Mutex::new(None);

/// Read `/proc/stat` and return it with the tick deltas since the previous
/// reading, as `(latest reading, deltas)`.
///
/// Percentages come from the deltas. The cumulative counters are the average
/// since boot, not the current load. This reader used to compute percentages
/// from them directly, so every one-shot `ironmon get` or `snapshot`, which is
/// always the first read in its process, reported the uptime average as the
/// current load.
///
/// The first read in a process has nothing to difference against, so it reads,
/// waits `PRIME_INTERVAL`, and reads again.
fn sample_cpu_ticks() -> Result<(CpuTicks, CpuTicks)> {
    let (before_at, before) = match previous_ticks() {
        Some(previous) => previous,
        None => {
            let first = read_ticks()?;
            let first_at = Instant::now();
            thread::sleep(PRIME_INTERVAL);
            (first_at, first)
        }
    };
    let mut latest = read_ticks()?;

    // A reading taken too soon after the base is not a measurement, however many
    // ticks it shows. Wait out the rest of `MIN_WINDOW` and read again from the
    // same base, so the window is never shorter than that.
    let shortfall = window_shortfall(before_at.elapsed());
    if !shortfall.is_zero() {
        thread::sleep(shortfall);
        latest = read_ticks()?;
    }
    let mut deltas = tick_deltas(&before, &latest);

    // Two reads inside one tick difference to zero. Wait once more and difference
    // again from the same base. If that is still zero, the deltas pass through
    // unchanged, and the callers then report no reading rather than an idle
    // machine.
    if deltas
        .iter()
        .all(|(_, ticks)| ticks.iter().all(|&t| t == 0))
    {
        thread::sleep(PRIME_INTERVAL);
        latest = read_ticks()?;
        deltas = tick_deltas(&before, &latest);
    }

    store_previous_ticks(Instant::now(), latest.clone());
    Ok((latest, deltas))
}

/// How much longer a window that is already `elapsed` long must wait to reach
/// `MIN_WINDOW`. Zero when it is long enough.
fn window_shortfall(elapsed: Duration) -> Duration {
    MIN_WINDOW.saturating_sub(elapsed)
}

fn read_ticks() -> Result<CpuTicks> {
    let content = fs::read_to_string("/proc/stat")?;
    parse_proc_stat(&content)
}

fn previous_ticks() -> Option<(Instant, CpuTicks)> {
    PREVIOUS_TICKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn store_previous_ticks(taken: Instant, ticks: CpuTicks) {
    *PREVIOUS_TICKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((taken, ticks));
}

/// The ticks each line gained between two readings.
///
/// A line is kept only if it appears in both readings with the same number of
/// fields and no field went backwards. A line missing from either reading (a CPU
/// that came or went between them) and a counter that fell (a reset, not a
/// measurement) give no information, so the line is dropped and its CPU reads as
/// unmeasured.
fn tick_deltas(before: &[(String, Vec<u64>)], after: &[(String, Vec<u64>)]) -> CpuTicks {
    after
        .iter()
        .filter_map(|(name, now)| {
            let (_, then) = before.iter().find(|(prev, _)| prev == name)?;
            if then.len() != now.len() || now.iter().zip(then).any(|(n, t)| n < t) {
                return None;
            }
            let deltas = now
                .iter()
                .zip(then)
                .map(|(n, t)| n - t)
                .collect::<Vec<u64>>();
            Some((name.clone(), deltas))
        })
        .collect()
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
    // Only the boot processor lacks an `online` file, and the kernel does not
    // allow it to be taken offline, so `true` there is that rule rather than a
    // read. Any other core without the file is unknown. This used to be `true`
    // for every such core.
    let online = if Path::new(&online_path).exists() {
        Some(read_file_u32(&online_path)? == 1)
    } else if cpu_id == 0 {
        Some(true)
    } else {
        None
    };

    // Read governor
    let governor_path = format!("{}/cpufreq/scaling_governor", cpu_path);
    // Absent when unreadable. This used to be the placeholder "unknown".
    let governor = read_file_string(&governor_path).ok();

    // Read frequency
    let frequency = if online != Some(false) {
        read_cpu_frequency(&cpu_path).ok()
    } else {
        None
    };

    // This core's tick deltas over the sampling interval.
    let cpu_name = format!("cpu{}", cpu_id);
    // A core with no tick delta (no `/proc/stat` line in one of the two readings)
    // was not measured. It used to fall back to
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

    // There is no further fallback. The old coretemp step read the package
    // temperature (hwmon0/temp1_input or thermal_zone0) and reported it for
    // every core, so a per-core reading was the package value. An unmatched
    // core is now None.
    None
}

/// Whether a hwmon `temp*_label` names exactly this core.
///
/// The comparison is exact after trimming. A substring match is wrong:
/// "Core 1" is contained in "Core 10", and the label of the package sensor,
/// "Package id 0", names no core at all.
fn is_core_label(label: &str, cpu_id: usize) -> bool {
    label.trim() == format!("Core {}", cpu_id)
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
                            if is_core_label(&label, cpu_id) {
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
                // `x86_pkg_temp` is the package zone. It used to match here, so
                // every core reported the package temperature. It is no longer a
                // per-core match.
                if zone_type == format!("cpu{}", cpu_id) || zone_type == format!("core{}", cpu_id) {
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

/// The model name from `/proc/cpuinfo`, or `None` when the file is unreadable or
/// names no model. This used to return the literal "Unknown", which is a name
/// the reader made up.
fn read_cpu_model() -> Option<String> {
    parse_cpuinfo_model(&fs::read_to_string("/proc/cpuinfo").ok()?)
}

fn parse_cpuinfo_model(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        if line.starts_with("model name") || line.starts_with("Processor") {
            let (_, value) = line.split_once(':')?;
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod model_tests {
    use super::parse_cpuinfo_model;

    #[test]
    fn reads_model_name_from_x86_cpuinfo() {
        let content = "processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: 12th Gen Intel(R) Core(TM) i7-12700K\n";
        assert_eq!(
            parse_cpuinfo_model(content).as_deref(),
            Some("12th Gen Intel(R) Core(TM) i7-12700K")
        );
    }

    #[test]
    fn absent_or_empty_model_is_none_not_a_placeholder() {
        assert_eq!(
            parse_cpuinfo_model("processor\t: 0\nvendor_id\t: ARM\n"),
            None
        );
        assert_eq!(parse_cpuinfo_model("model name\t: \n"), None);
    }
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

    // No ticks at all is no information, not an idle machine. This answered
    // 100% idle, which a caller turned into 0% busy.
    if total == 0.0 {
        return (None, None, None, None);
    }

    (
        Some((user as f32 / total) * 100.0),
        Some((nice as f32 / total) * 100.0),
        Some((system as f32 / total) * 100.0),
        Some((idle as f32 / total) * 100.0),
    )
}

/// The whole-CPU split, or `None` if nothing was read to make it from.
///
/// The kernel's own aggregate `cpu` line first: it is the sum of every CPU's
/// ticks, which is the total this is meant to be. Failing that, the mean over
/// the online cores whose four figures were all read.
///
/// It used to average every online core's figures over the count of online
/// cores, read or not, so a core missing from `/proc/stat` added nothing to the
/// sum and one to the divisor and the machine read idler than it was; and with
/// no online core at all it answered 100% idle, a machine doing nothing,
/// assembled from no reading.
fn calculate_total(cpu_times: &[(String, Vec<u64>)], cores: &[CpuCore]) -> Option<CpuTotal> {
    if let Some((_, times)) = cpu_times.iter().find(|(name, _)| name == "cpu") {
        if let (Some(user), Some(nice), Some(system), Some(idle)) = calculate_cpu_percentages(times)
        {
            return Some(CpuTotal {
                user,
                nice: Some(nice),
                system,
                idle,
            });
        }
    }

    let read: Vec<(f32, f32, f32, f32)> = cores
        .iter()
        // A core with a split was read from `/proc/stat`, which lists only online
        // CPUs, so only an explicit offline flag excludes it.
        .filter(|c| c.online != Some(false))
        .filter_map(|c| Some((c.user?, c.nice?, c.system?, c.idle?)))
        .collect();
    if read.is_empty() {
        return None;
    }
    let count = read.len() as f32;
    let mean = |pick: fn(&(f32, f32, f32, f32)) -> f32| read.iter().map(pick).sum::<f32>() / count;
    Some(CpuTotal {
        user: mean(|r| r.0),
        nice: Some(mean(|r| r.1)),
        system: mean(|r| r.2),
        idle: mean(|r| r.3),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticks(text: &str) -> CpuTicks {
        parse_proc_stat(text).unwrap()
    }

    #[test]
    fn a_core_label_names_exactly_one_core() {
        // Captured coretemp labels: "Core 1" and "Core 10" are distinct cores.
        assert!(is_core_label("Core 1", 1));
        assert!(is_core_label("Core 10\n", 10));
        assert!(!is_core_label("Core 10", 1));
        assert!(!is_core_label("Core 1", 10));
        // The package sensor names no core.
        assert!(!is_core_label("Package id 0", 0));
        assert!(!is_core_label("Core 0", 1));
    }

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
        // cpu1 is offline, so neither reading has a line for it while the sysfs
        // `online` range still reports 2 CPUs.
        let before = ticks("cpu  100 0 50 900\ncpu0 50 0 25 450\n");
        let after = ticks("cpu  200 0 100 1800\ncpu0 100 0 50 900\n");
        let deltas = tick_deltas(&before, &after);
        let core = read_cpu_core(1, &deltas).unwrap();
        assert_eq!(
            core.idle, None,
            "an unmeasured core must not report as 100% idle"
        );
        assert_eq!(core.user, None);
        assert_eq!(core.system, None);
    }

    #[test]
    fn a_core_present_in_both_readings_reports_its_share_of_the_interval() {
        let before = ticks("cpu0 50 0 25 425\n");
        let after = ticks("cpu0 100 0 50 850\n");
        let core = read_cpu_core(0, &tick_deltas(&before, &after)).unwrap();
        // Deltas are 50 user, 0 nice, 25 system, 425 idle: 85% idle of 500.
        assert!((core.idle.unwrap() - 85.0).abs() < 0.01, "{:?}", core.idle);
        assert!((core.user.unwrap() - 10.0).abs() < 0.01, "{:?}", core.user);
    }

    #[test]
    fn the_per_line_delta_is_the_difference_of_the_two_readings() {
        let before = ticks("cpu  100 0 50 850\ncpu0 50 0 25 425\n");
        let after = ticks("cpu  150 0 100 1750\ncpu0 100 0 50 850\n");
        assert_eq!(
            tick_deltas(&before, &after),
            vec![
                ("cpu".to_string(), vec![50, 0, 50, 900]),
                ("cpu0".to_string(), vec![50, 0, 25, 425]),
            ]
        );
    }

    #[test]
    fn the_total_comes_from_the_kernel_aggregate_over_the_interval() {
        let before = ticks("cpu  100 0 50 850\ncpu0 50 0 25 425\n");
        let after = ticks("cpu  150 0 100 1750\ncpu0 100 0 50 850\n");
        let total = calculate_total(&tick_deltas(&before, &after), &[])
            .expect("the aggregate line was read");
        // 900 idle of 1000 ticks in the interval.
        assert!((total.idle - 90.0).abs() < 0.01, "{total:?}");
    }

    #[test]
    fn an_unread_core_does_not_dilute_the_mean() {
        // No aggregate line: the fallback averages only the cores it read.
        let before = ticks("cpu0 50 0 25 425\n");
        let after = ticks("cpu0 100 0 50 850\n");
        let deltas = tick_deltas(&before, &after);
        let read = read_cpu_core(0, &deltas).unwrap();
        let unread = read_cpu_core(1, &deltas).unwrap();
        let total = calculate_total(&deltas, &[read, unread]).expect("cpu0 was read");
        assert!((total.idle - 85.0).abs() < 0.01, "{total:?}");
    }

    #[test]
    fn a_zero_delta_pair_is_no_reading_rather_than_an_idle_machine() {
        // Two reads inside one tick: every delta is zero.
        let times = ticks("cpu  0 0 0 0\ncpu0 0 0 0 0\n");
        let deltas = tick_deltas(&times, &times);
        assert!(deltas.iter().all(|(_, t)| t.iter().all(|&x| x == 0)));
        assert!(calculate_total(&deltas, &[]).is_none());
        assert_eq!(read_cpu_core(0, &deltas).unwrap().idle, None);
        assert_eq!(
            calculate_cpu_percentages(&[0, 0, 0, 0]),
            (None, None, None, None)
        );
    }

    #[test]
    fn a_counter_that_went_backwards_is_no_information() {
        // cpu0's user and idle counters fell between the reads: a reset, not a
        // measurement. The line is dropped, so cpu0 reads as unmeasured.
        let before = ticks("cpu0 100 0 50 900\n");
        let after = ticks("cpu0 90 0 60 1000\n");
        let deltas = tick_deltas(&before, &after);
        assert!(deltas.is_empty());
        assert_eq!(read_cpu_core(0, &deltas).unwrap().idle, None);
    }

    #[test]
    fn a_cpu_that_appeared_between_the_readings_is_no_information() {
        // cpu1 has no line in the first reading, so it has no delta.
        let before = ticks("cpu0 50 0 25 425\n");
        let after = ticks("cpu0 100 0 50 850\ncpu1 10 0 5 85\n");
        let deltas = tick_deltas(&before, &after);
        assert_eq!(deltas.len(), 1);
        assert_eq!(read_cpu_core(1, &deltas).unwrap().idle, None);
    }

    #[test]
    fn a_window_shorter_than_the_minimum_waits_out_the_rest() {
        // A reading one tick behind the collector's warm-up is not a measurement.
        assert_eq!(
            window_shortfall(Duration::from_millis(10)),
            MIN_WINDOW - Duration::from_millis(10)
        );
        assert_eq!(window_shortfall(MIN_WINDOW), Duration::ZERO);
        assert_eq!(window_shortfall(Duration::from_secs(1)), Duration::ZERO);
    }

    #[test]
    fn a_one_tick_window_can_show_a_busy_core_beside_an_idle_aggregate() {
        // The rounding `MIN_WINDOW` exists for: the aggregate line rounded down to
        // an idle tick while cpu1 rounded up to a user tick. Both lines are real
        // reads, and together they say the machine is 100% idle and cpu1 is 100%
        // busy at once.
        let before = ticks("cpu  100 0 50 900\ncpu0 50 0 25 450\ncpu1 50 0 25 450\n");
        let after = ticks("cpu  100 0 50 901\ncpu0 50 0 25 451\ncpu1 51 0 25 450\n");
        let deltas = tick_deltas(&before, &after);
        let total = calculate_total(&deltas, &[]).expect("the aggregate line was read");
        assert!((total.idle - 100.0).abs() < 0.01, "{total:?}");
        let busy_core = read_cpu_core(1, &deltas).unwrap();
        assert!(busy_core.idle.unwrap().abs() < 0.01, "{busy_core:?}");
    }

    #[test]
    fn cpu_ranges_parse_as_a_count() {
        assert_eq!(parse_cpu_range("0-23"), Some(24));
        assert_eq!(parse_cpu_range("0"), Some(1));
        assert_eq!(parse_cpu_range("0,2-5,7"), Some(8));
        assert_eq!(parse_cpu_range(""), None);
    }
}
