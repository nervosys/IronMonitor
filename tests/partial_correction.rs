//! A struct that already says some of its numbers can be missing must not add
//! a new one that cannot.
//!
//! The sweep's most repeated shape was the *partial correction*: somebody
//! learned a reading could be absent and made that field `Option`, while its
//! siblings -- read the same way, from the same source -- stayed bare and kept
//! defaulting to zero. The struct itself records that absence is possible, and
//! the bare field beside it cannot say so. `DimmInfo`, `WatchdogInfo`,
//! `CoolingDeviceInfo` and `GpuFrequency` were all this shape.
//!
//! This scans `src/` for structs with at least one `Option<numeric>` field and
//! at least one bare numeric field, skipping names that cannot fail to exist
//! (identifiers, indices, counts the program produces itself). The rules are
//! those of the structural scan recorded in `HANDOFF.md`, ported from Python.
//!
//! **`BASELINE` is not an allowlist of judged sites.** It is every match that
//! existed when this test was written -- 76 structs, most of them library API
//! nothing reads. Some are correct (a ping reply count is a real zero), some
//! are not, and most have not been triaged. The test is a ratchet: it fails
//! when a struct gains a bare field beside `Option` siblings, or a new struct
//! of the shape appears; and it fails when an entry shrinks or disappears, so
//! the baseline only ever gets smaller.
//!
//! **`JUDGED` is the list of judged sites.** An entry there was traced to
//! every constructor and its bare fields found read on every platform that
//! builds the struct, and it carries the reason. It is held to the same exact
//! match as `BASELINE`, so a judged struct that grows a field fails like any
//! other. Moving an entry from `BASELINE` to `JUDGED` is triage; adding one
//! that was never in `BASELINE` needs the same tracing and a reason that would
//! survive a reader checking it.
//!
//! **What this does not see.** Fields declared over more than one line, types
//! behind an alias, and structs whose fields are all bare -- the case the
//! sweep called a type that could not say "not reported". A clean run says the
//! shape did not spread; it does not say the crate is free of it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const NUMERIC: [&str; 12] = [
    "u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize", "f32", "f64",
];

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// A name that identifies or indexes something: a reader either has it or did
/// not construct the record at all.
fn is_identifier(name: &str) -> bool {
    const EXACT: [&str; 18] = [
        "id", "index", "idx", "pid", "ppid", "tid", "uid", "gid", "slot", "channel", "node", "cpu",
        "core", "rank", "line", "port", "number", "num",
    ];
    EXACT.contains(&name)
        || name.ends_with("_id")
        || name.ends_with("_index")
        || name.ends_with("_number")
        || ["vendor_", "device_", "subsystem_"]
            .iter()
            .any(|p| name.starts_with(p))
}

/// A name for something the program produces itself -- a count of what it
/// found, a size it chose -- rather than a value it read.
fn is_produced(name: &str) -> bool {
    const EXACT: [&str; 19] = [
        "count",
        "len",
        "size",
        "capacity",
        "timestamp",
        "generation",
        "epoch",
        "epochs",
        "step",
        "steps",
        "sample",
        "samples",
        "alpha",
        "score",
        "version",
        "offset",
        "depth",
        "width",
        "height",
    ];
    const TOTALS: [&str; 8] = [
        "cpus",
        "devices",
        "nodes",
        "controllers",
        "hosts",
        "items",
        "entries",
        "processes",
    ];
    // The wall-clock span between two samples the program itself took. Named
    // exactly, not by a `_secs` or `duration` pattern, because a duration the
    // program *reads* -- a boot time, an uptime -- can be missing.
    const MEASURED_BY_THE_PROGRAM: [&str; 1] = ["measurement_duration_secs"];
    EXACT.contains(&name)
        || MEASURED_BY_THE_PROGRAM.contains(&name)
        || name.ends_with("_count")
        || name.ends_with("_len")
        || name.starts_with("num_")
        || name
            .strip_prefix("total_")
            .is_some_and(|rest| TOTALS.contains(&rest))
}

/// `pub name: Type,` on one line, as `(name, Type)`.
fn field(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim_start().strip_prefix("pub")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let (name, ty) = rest.trim_start().split_once(':')?;
    let name = name.trim_end();
    let valid = name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !valid {
        return None;
    }
    let ty = ty.trim_end().strip_suffix(',')?.trim();
    Some((name, ty))
}

/// The name of a struct declared on `line`: `struct X` or `pub struct X`.
fn struct_name(line: &str) -> Option<&str> {
    let t = line.trim();
    let t = match t.strip_prefix("pub") {
        Some(r) if r.starts_with(char::is_whitespace) => r.trim_start(),
        _ => t,
    };
    let r = t.strip_prefix("struct")?;
    if !r.starts_with(char::is_whitespace) {
        return None;
    }
    let r = r.trim_start();
    let end = r
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(r.len());
    (end > 0).then(|| &r[..end])
}

fn braces(line: &str) -> i64 {
    line.matches('{').count() as i64 - line.matches('}').count() as i64
}

/// `(file, struct, bare fields space-separated)` for every match in `src/`.
fn partial_corrections() -> BTreeSet<(String, String, String)> {
    let mut paths = Vec::new();
    rust_sources(Path::new("src"), &mut paths);
    let mut found = BTreeSet::new();
    for path in paths {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let file = path.to_string_lossy().replace('\\', "/");
        let lines: Vec<&str> = text.split('\n').collect();
        let mut i = 0;
        while i < lines.len() {
            let Some(name) = struct_name(lines[i]) else {
                i += 1;
                continue;
            };
            let mut depth = braces(lines[i]);
            let mut j = i + 1;
            let (mut bare, mut optional) = (Vec::new(), 0usize);
            while j < lines.len() && depth > 0 {
                depth += braces(lines[j]);
                if depth <= 0 {
                    break;
                }
                if let Some((fname, ty)) = field(lines[j]) {
                    if NUMERIC.contains(&ty) {
                        if !is_identifier(fname) && !is_produced(fname) {
                            bare.push(fname);
                        }
                    } else if ty
                        .strip_prefix("Option<")
                        .and_then(|t| t.strip_suffix('>'))
                        .is_some_and(|t| NUMERIC.contains(&t))
                    {
                        optional += 1;
                    }
                }
                j += 1;
            }
            if !bare.is_empty() && optional > 0 {
                found.insert((file.clone(), name.to_string(), bare.join(" ")));
            }
            i = j + 1;
        }
    }
    found
}

/// Every match when this test was written. Not a list of approvals -- see the
/// module doc. Remove an entry when its struct is fixed; never add one.
const BASELINE: &[(&str, &str, &str)] = &[];

/// Matches traced to every constructor and found sound: each bare field is
/// read wherever the struct is built. `(file, struct, bare fields, why)`.
const JUDGED: &[(&str, &str, &str, &str)] = &[
    (
        "src/disk/traits.rs",
        "SmartAttribute",
        "value worst raw_value",
        concat!(
            "Built from an AtaSmartAttribute, whose 12-byte table entry always ",
            "carries the normalized value, worst and raw bytes, or converted from ",
            "smart::SmartAttribute, which has them read. The threshold comes from a ",
            "separate table that may be missing, which is why only it is Option."
        ),
    ),
    (
        "src/smart/mod.rs",
        "SmartAttribute",
        "value worst raw_value",
        concat!(
            "smartctl JSON attributes missing any of value, worst or raw are skipped, ",
            "not pushed with gaps, and ATA attributes come from 12-byte table entries ",
            "that always carry all three. Only the threshold, from a separate table, ",
            "can be absent for an attribute that exists."
        ),
    ),
    (
        "src/backend.rs",
        "CpuState",
        "threads utilization",
        concat!(
            "Built only inside `if let Some(ref cpu) = self.cpu_stats`, from a CPU ",
            "sample that was read: threads is that sample's per-core row count and ",
            "utilization its aggregate. With no sample the whole struct is None."
        ),
    ),
    (
        "src/core/cpu.rs",
        "CpuTotal",
        "user system idle",
        concat!(
            "Every platform reader errors rather than return these unread: Linux ",
            "without the aggregate cpu line of /proc/stat, Windows when ",
            "GetSystemTimes reports no time, macOS when host_processor_info gives ",
            "no ticks or they sum to zero. nice is Option because Windows has no ",
            "nice time at all, not because it can fail to be read."
        ),
    ),
    (
        "src/virtualization/guest.rs",
        "VirtCpuTopology",
        "vcpus",
        concat!(
            "Counted from the processor lines of /proc/cpuinfo, and the topology is ",
            "None when the file is unreadable or the count is zero, so it is never ",
            "built with vcpus unread. The other fields come from separate sysfs ",
            "files that can be missing."
        ),
    ),
    (
        "src/core/memory.rs",
        "RamInfo",
        "total used",
        concat!(
            "Linux errors without MemTotal/MemFree/MemAvailable, macOS without ",
            "vm_stat or hw.memsize, Windows when GlobalMemoryStatusEx fails, so a ",
            "reader never returns these unread. MemoryStats::empty() is documented ",
            "as reading nothing and its zero total is what consumers test for: ",
            "ram_usage_percent and the summaries are None at total 0."
        ),
    ),
    (
        "src/network_monitor.rs",
        "NetworkInterfaceInfo",
        "rx_bytes rx_packets rx_errors rx_drops tx_bytes tx_packets tx_errors",
        concat!(
            "Linux reads each from /sys/class/net/<if>/statistics and fails the ",
            "interface if one is missing; Windows copies them from GetIfTable2's ",
            "MIB_IF_ROW2; macOS takes them from the AF_LINK if_data and leaves out ",
            "an interface that has none. tx_drops is Option: macOS does not count it."
        ),
    ),
    (
        "src/ai_api/types.rs",
        "NetworkSummary",
        "rx_total_mb tx_total_mb",
        "built only in ai_api/mod.rs from NetworkInterfaceInfo's rx_bytes and tx_bytes",
    ),
    (
        "src/backend.rs",
        "NetworkState",
        "rx_bytes tx_bytes",
        "built only in get_full_system_state from NetworkInterfaceInfo",
    ),
    (
        "src/observability/context.rs",
        "NetworkMetrics",
        "rx_bytes_total tx_bytes_total rx_packets tx_packets errors",
        concat!(
            "built only in observability/api.rs as sums of NetworkInterfaceInfo ",
            "counters; dropped is Option because tx_drops is"
        ),
    ),
    (
        "src/datacenter/ipmi.rs",
        "PowerReading",
        "current_watts",
        concat!(
            "parse_power_reading fails rather than build one without an ",
            "instantaneous reading; the sampling-period figures are Option"
        ),
    ),
    (
        "src/fleet.rs",
        "TagGroupMetrics",
        "total_alerts",
        "hosts_by_tag counts the manager's own alerts raised for the group's \
         hosts; the averages beside it are Option",
    ),
    (
        "src/agent/local/mod.rs",
        "InferenceResponse",
        "duration_ms",
        "every backend measures it with start.elapsed() around its own request",
    ),
    (
        "src/agent/state.rs",
        "CpuState",
        "threads",
        concat!(
            "the number of logical cores the CPU reader enumerated, or ",
            "num_cpus::get() on macOS",
        ),
    ),
    (
        "src/anomaly.rs",
        "Anomaly",
        "current_value timestamp_secs",
        concat!(
            "the last sample the caller recorded, and the detector's own ",
            "elapsed time since it started",
        ),
    ),
    (
        "src/consent.rs",
        "ConsentConfig",
        "consent_version",
        concat!(
            "a schema version the program writes (CURRENT_PROMPT_VERSION) or ",
            "reads back from its own config file",
        ),
    ),
    (
        "src/core/temperature.rs",
        "TemperatureSensor",
        "temp",
        concat!(
            "the OHM/LHM Sensor.Value field (required on deserialisation), the ",
            "range-filtered thermal-zone counters, or a Linux thermal_zone temp ",
            "read with ?",
        ),
    ),
    (
        "src/disk/nvme_log.rs",
        "HealthLog",
        "critical_warning available_spare_percent available_spare_threshold_percent percentage_used",
        concat!(
            "bytes 0, 3, 4 and 5 of the device's own log page; parse returns None ",
            "for a buffer shorter than the page",
        ),
    ),
    (
        "src/fan_control.rs",
        "FanSummary",
        "total_fans running_fans",
        concat!(
            "counts of the fans enumerated, and of those whose is_running_opt() ",
            "is Some(true) -- None when neither speed nor RPM was read",
        ),
    ),
    (
        "src/hwmon/mod.rs",
        "HwSensor",
        "value",
        concat!(
            "every constructor sits inside an if-let on its sysfs, NVML, PDH, WMI ",
            "or SMART read",
        ),
    ),
    (
        "src/network_tools.rs",
        "CaptureConfig",
        "timeout_secs",
        "a setting the caller supplies, not a reading",
    ),
    (
        "src/network_tools.rs",
        "NmapScanResult",
        "scan_duration_secs",
        "start.elapsed() around the scan the program ran",
    ),
    (
        "src/network_tools.rs",
        "TracerouteHop",
        "ttl",
        concat!(
            "parsed from the hop line's first column inside an if-let; a line ",
            "that does not parse is not a hop",
        ),
    ),
    (
        "src/observability/context.rs",
        "MemoryContext",
        "total_gb",
        "the platform memory reader's ram.total, used only inside if-let Ok",
    ),
    (
        "src/observability/context.rs",
        "MemoryMetrics",
        "used_mb total_mb",
        concat!(
            "the platform memory reader's ram.used and ram.total, used only ",
            "inside if-let Ok",
        ),
    ),
    (
        "src/tsdb/mod.rs",
        "DatabaseStats",
        "max_size current_size",
        concat!(
            "the configured maximum, and the header plus the data size the ",
            "database tracks as it writes",
        ),
    ),
    (
        "src/gpu/traits.rs",
        "PciInfo",
        "domain bus device function",
        concat!(
            "all three constructors (AMD, Intel, NVIDIA) take the four parts from ",
            "PciInfo::parse_address or NVML's own fields, and fail rather than ",
            "build one from an address that did not parse"
        ),
    ),
    (
        "src/ai_api/types.rs",
        "SensorReading",
        "value",
        "never constructed in the crate; it reaches a value only by deserialisation",
    ),
    (
        "src/ai_workload.rs",
        "TrainingMetrics",
        "current_epoch total_epochs current_step steps_per_epoch current_loss",
        concat!(
            "never constructed: its only producer, try_parse_training_metrics, ",
            "returns Ok(None)"
        ),
    ),
    (
        "src/core/memory.rs",
        "IramInfo",
        "total used",
        concat!(
            "never constructed: read_iram_info always returns Err, and every ",
            "MemoryStats sets iram to None"
        ),
    ),
    (
        "src/anomaly.rs",
        "AnomalySummary",
        "samples_collected uptime_secs",
        concat!(
            "the detector's own bookkeeping, not readings: the length of its CPU ",
            "window and the age of the detector, both always known"
        ),
    ),
    (
        "src/fleet.rs",
        "HostMetrics",
        "uptime_seconds",
        concat!(
            "supplied by the caller through record_metrics, and deserialised with ",
            "no serde defaults; the only literal is a test fixture"
        ),
    ),
    (
        "src/network_tools.rs",
        "PingResult",
        "packets_sent packets_received packets_lost packet_loss_percent",
        concat!(
            "ping() returns Err unless a statistics line parsed, and every count ",
            "is overwritten from it; lost and loss percent are computed from them"
        ),
    ),
    (
        "src/observability/context.rs",
        "CpuMetrics",
        "utilization_percent",
        concat!(
            "100 minus the total idle of a CPU read that succeeded; both readers ",
            "error rather than return a total they did not read"
        ),
    ),
    (
        "src/observability/metrics.rs",
        "CpuMetricSnapshot",
        "usage_percent",
        "never constructed in the crate; it reaches a value only by deserialisation",
    ),
    (
        "src/observability/metrics.rs",
        "DiskMetricSnapshot",
        "used_bytes free_bytes total_bytes usage_percent",
        "never constructed in the crate; it reaches a value only by deserialisation",
    ),
    (
        "src/observability/metrics.rs",
        "GpuMetricSnapshot",
        "usage_percent memory_used_mb memory_total_mb",
        "never constructed in the crate; it reaches a value only by deserialisation",
    ),
    (
        "src/pcie.rs",
        "PcieBandwidthSummary",
        "gpu_devices downgraded_devices unreadable_devices",
        concat!(
            "counts of enumerated devices: GPUs by class code, downgraded only ",
            "where is_downgraded() is Some(true), unreadable where no bandwidth ",
            "could be computed"
        ),
    ),
    (
        "src/scheduler/mod.rs",
        "PressureInfo",
        "some_avg10 some_avg60 some_avg300 some_total_us",
        concat!(
            "parse_psi returns None unless all four fields of the some line ",
            "parsed; the optional full line is the Option beside them"
        ),
    ),
    (
        "src/gpu_topology/mod.rs",
        "GpuLink",
        "from_gpu to_gpu",
        "indices of the GPUs the scan enumerated; each pair is built from its two nodes' index",
    ),
    (
        "src/memory_topology/mod.rs",
        "MemoryAnalysis",
        "populated_slots",
        "a count of the modules the reader enumerated; enumeration failure is an Err",
    ),
    (
        "src/rapl/mod.rs",
        "EnergyReading",
        "socket energy_uj",
        concat!(
            "socket is parsed from the intel-rapl directory name, and a domain ",
            "whose name does not parse is skipped; energy_uj is parsed with `?`"
        ),
    ),
    (
        "src/interconnect/mod.rs",
        "ChipletTopology",
        "compute_dies io_dies",
        concat!(
            "values from the model table only on a match, None otherwise; the two ",
            "compute dies of an Apple Ultra are fixed by UltraFusion, not read"
        ),
    ),
    (
        "src/memory_management.rs",
        "MemoryInfo",
        "total",
        "read on every platform that reports memory (MemTotal, GlobalMemoryStatusEx, hw.memsize), or the refresh fails",
    ),
    (
        "src/memory_management.rs",
        "ZramInfo",
        "disksize compressed_bytes original_bytes mem_used_bytes",
        concat!(
            "linux_read_zram builds the device only when all four of these files parse, ",
            "and leaves it None otherwise; the Option fields beside them are None where ",
            "they cannot be read or computed"
        ),
    ),
    (
        "src/memory_management.rs",
        "MemorySummary",
        "total_memory total_swap used_swap",
        concat!(
            "copied from MemoryInfo's total and SwapInfo's total and used, which are read ",
            "or the refresh fails; memory_summary is the only constructor"
        ),
    ),
    (
        "src/codec/mod.rs",
        "CodecCapability",
        "confidence",
        concat!(
            "each value is one of the CONFIDENCE_ constants named in codec/mod.rs, ",
            "chosen for the detection rule that produced the row and documented on the ",
            "field as not a measured probability; no row is built outside a matched rule"
        ),
    ),
    (
        "src/network_tools.rs",
        "OsFingerprint",
        "confidence",
        concat!(
            "each value is one of the OS_CONFIDENCE_ constants named in network_tools.rs, ",
            "set by the port or banner rule that chose os_family; no fingerprint is built ",
            "without an observed open port or banner"
        ),
    ),
    (
        "src/predictive.rs",
        "MaintenanceAlert",
        "current_value threshold confidence",
        concat!(
            "current_value is the last sample the rule read, threshold is the limit it ",
            "compared against, and confidence is either a named per-rule constant or the ",
            "R-squared of the trend fit; the field docs say so and neither is a measured probability"
        ),
    ),
    (
        "src/ai_api/types.rs",
        "ProcessSummary",
        "memory_mb",
        "copied from ProcessMonitorInfo.memory_bytes, which every reader fills from a read or skips the process",
    ),
    (
        "src/backend.rs",
        "ProcessState",
        "memory_bytes",
        "copied from ProcessMonitorInfo.memory_bytes, which every reader fills from a read or skips the process",
    ),
    (
        "src/observability/context.rs",
        "ProcessMetrics",
        "memory_mb threads",
        concat!(
            "copied from ProcessMonitorInfo, whose memory and thread count every ",
            "reader fills from a read (statm, working set, pti_resident_size; ",
            "num_threads, cntThreads, pti_threadnum) or skips the process"
        ),
    ),
    (
        "src/process_monitor.rs",
        "CategoryStats",
        "total_memory_bytes",
        concat!(
            "a sum over processes that were enumerated, each with a read memory ",
            "figure; categories exist only for groups with members"
        ),
    ),
    (
        "src/process_monitor.rs",
        "ProcessMonitorInfo",
        "memory_bytes virtual_memory_bytes cpu_time_us",
        concat!(
            "every reader fills these from /proc stat/statm, GetProcessMemoryInfo ",
            "and GetProcessTimes (or the process table), or task_info, and skips ",
            "a process none of them could read"
        ),
    ),
    (
        "src/tsdb/mod.rs",
        "ProcessSnapshot",
        "memory_bytes",
        "copied from ProcessMonitorInfo.memory_bytes, which every reader fills from a read or skips the process",
    ),
    (
        "src/ai_api/types.rs",
        "MemorySummary",
        "total_mb used_mb",
        concat!(
            "copied from the Linux reader's ram.total and ram.used, and built only ",
            "inside if-let Ok in ai_api/mod.rs; that reader fails when MemTotal is missing"
        ),
    ),
    (
        "src/backend.rs",
        "MemoryState",
        "total_bytes used_bytes available_bytes",
        concat!(
            "copied from the platform reader's RamInfo, and built only when memory_stats ",
            "is Some; each reader returns Err rather than a total it did not read ",
            "(MemTotal, hw.memsize, GlobalMemoryStatusEx)"
        ),
    ),
    (
        "src/fleet_store/rows.rs",
        "MetricRow",
        "collected_at_us",
        "non-test rows are built only in rows_from_snapshot, which yields none for a snapshot whose collected_at is None",
    ),
    (
        "src/pipeline/mod.rs",
        "Snapshot",
        "collect_us",
        concat!(
            "collect_once measures it with started.elapsed(); the default snapshot's 0 ",
            "never reaches a row, because rows_from_snapshot refuses a snapshot with no ",
            "collected_at"
        ),
    ),
    (
        "src/network_tools.rs",
        "CaptureResult",
        "total_packets duration_secs packets_per_sec",
        concat!(
            "finalize_capture_result produces all three: the number of packets ",
            "parsed, the wall-clock time since the capture began, and their ratio; ",
            "total_bytes is Option because a packet's length may be unprinted"
        ),
    ),
    (
        "src/pipeline/mod.rs",
        "NetSnapshot",
        "rx_bytes tx_bytes",
        concat!(
            "collect_network copies them from NetworkInterfaceInfo; the other ",
            "constructors are tests"
        ),
    ),
    (
        "src/tui/app.rs",
        "NetworkInfo",
        "total_rx_bytes total_tx_bytes",
        "sums over the TUI's interfaces, which come from NetSnapshot",
    ),
    (
        "src/tui/app.rs",
        "NetworkInterfaceInfo",
        "rx_bytes tx_bytes",
        "update_network copies them from the collector's NetSnapshot",
    ),
    (
        "src/storage_controller/mod.rs",
        "RaidArrayInfo",
        "active_devices failed_devices spare_devices",
        concat!(
            "counts of the member tokens parse_mdstat reads from one array line, ",
            "by their (F), (S) and (J) flags; wanted_devices, working_devices and ",
            "size_bytes are Option because they come from the blocks line"
        ),
    ),
    (
        "src/hardware_ai/mod.rs",
        "ThermalEnvelope",
        "gpu_adapters_counted",
        concat!(
            "the number of adapters whose enforced caps were summed into gpu_tdp_watts: ",
            "measured_gpu_power_caps_watts sums caps.len() over a list it returns only when ",
            "non-empty, the name table's figure counts as one card, and no figure counts as ",
            "zero; analyze_thermal_envelope is the only constructor and copies it unfloored"
        ),
    ),
    (
        "src/cgroup_monitor/mod.rs",
        "CgroupMemory",
        "usage_bytes limit_bytes cache_bytes rss_bytes",
        concat!(
            "Built by read_memory_v2, which returns None unless memory.current, ",
            "memory.max and memory.stat were read and parsed; those three files are ",
            "the only source of the bare fields. The Option fields are the ones whose ",
            "source may be missing: swap accounting, the oom_kill key, and a percentage ",
            "with no limit. The one other constructor is a unit test with literal values."
        ),
    ),
    (
        "src/cgroup_monitor/mod.rs",
        "CgroupCpu",
        "quota_us period_us weight usage_us throttled_periods throttled_time_us",
        concat!(
            "Built only by read_cpu_v2, which returns None unless cpu.max, cpu.weight ",
            "and cpu.stat were read and parsed. quota and period come from parse_cpu_max, ",
            "which yields nothing for an unparseable or zero period, and the counters from ",
            "parse_cpu_stat, which requires all three. The Option fields are the ones with ",
            "no source here: shares and per-CPU usage are v1 files this reader never opens, ",
            "and effective_cpus is None for an unlimited quota. The one other constructor ",
            "is a unit test with literal values."
        ),
    ),
];

fn known() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    BASELINE
        .iter()
        .copied()
        .chain(JUDGED.iter().map(|&(f, s, b, _)| (f, s, b)))
}

#[test]
fn judged_entries_say_why_and_are_not_also_untriaged() {
    for &(f, s, b, why) in JUDGED {
        assert!(
            !why.trim().is_empty(),
            "{f}: `{s}` is judged without a reason"
        );
        assert!(
            !BASELINE.contains(&(f, s, b)),
            "{f}: `{s}` is in both BASELINE and JUDGED"
        );
    }
}

#[test]
fn no_struct_gains_a_bare_reading_beside_optional_ones() {
    let baseline: BTreeSet<(String, String, String)> = known()
        .map(|(f, s, b)| (f.to_string(), s.to_string(), b.to_string()))
        .collect();
    let grown: Vec<String> = partial_corrections()
        .difference(&baseline)
        .map(|(f, s, b)| {
            format!(
                concat!(
                    "{}: `{}` has bare numeric field(s) [{}] beside `Option` ones. ",
                    "If the value can fail to be read, make it `Option` too. If it ",
                    "cannot, that is a naming rule for is_identifier or is_produced, ",
                    "or a JUDGED entry saying why -- never a new BASELINE entry."
                ),
                f, s, b
            )
        })
        .collect();
    assert!(grown.is_empty(), "{}", grown.join("\n"));
}

/// An entry that no longer matches is progress, and leaving it in place would
/// let the struct regrow the field unnoticed.
#[test]
fn every_baseline_entry_still_matches() {
    let found = partial_corrections();
    let stale: Vec<String> = known()
        .filter(|(f, s, b)| !found.contains(&(f.to_string(), s.to_string(), b.to_string())))
        .map(|(f, s, b)| format!("{f}: `{s}` [{b}] no longer matches; update or remove its entry"))
        .collect();
    assert!(stale.is_empty(), "{}", stale.join("\n"));
}
