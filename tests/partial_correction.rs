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
    EXACT.contains(&name)
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
const BASELINE: &[(&str, &str, &str)] = &[
    ("src/agent/backend.rs", "BackendCapabilities", "max_context_length"),
    ("src/agent/local/mod.rs", "InferenceResponse", "duration_ms"),
    ("src/agent/state.rs", "CpuState", "threads"),
    ("src/ai_api/types.rs", "MemorySummary", "total_mb used_mb usage_percent"),
    ("src/ai_api/types.rs", "NetworkSummary", "rx_total_mb tx_total_mb"),
    ("src/ai_api/types.rs", "ProcessSummary", "cpu_percent memory_mb"),
    ("src/ai_api/types.rs", "SensorReading", "value"),
    ("src/ai_workload.rs", "DistributedConfig", "world_size local_rank"),
    ("src/ai_workload.rs", "TrainingMetrics", "current_epoch total_epochs current_step steps_per_epoch current_loss"),
    ("src/anomaly.rs", "Anomaly", "current_value timestamp_secs"),
    ("src/backend.rs", "CpuState", "threads utilization"),
    ("src/backend.rs", "MemoryState", "total_bytes used_bytes available_bytes usage_percent"),
    ("src/backend.rs", "NetworkState", "rx_bytes tx_bytes"),
    ("src/backend.rs", "ProcessState", "cpu_percent memory_bytes"),
    ("src/codec/mod.rs", "CodecCapability", "confidence"),
    ("src/connections.rs", "ConnectionInfo", "local_port"),
    ("src/consent.rs", "ConsentConfig", "consent_version"),
    ("src/core/engine.rs", "EngineInfo", "current"),
    ("src/core/memory.rs", "IramInfo", "total used"),
    ("src/core/memory.rs", "RamInfo", "total used"),
    ("src/core/temperature.rs", "TemperatureSensor", "temp"),
    ("src/cpu_cache/mod.rs", "CpuCacheInfo", "size_kb"),
    ("src/cpu_microarch/mod.rs", "CpuMicroarchReport", "physical_cores logical_cores"),
    ("src/cpufreq.rs", "CpuFreqSummary", "online_cpus"),
    ("src/cpufreq.rs", "CpuIdleState", "latency_us usage time_us"),
    ("src/datacenter/ipmi.rs", "IpmiSensor", "value"),
    ("src/datacenter/ipmi.rs", "PowerReading", "current_watts"),
    ("src/disk/nvme_log.rs", "HealthLog", "critical_warning available_spare_percent available_spare_threshold_percent percentage_used"),
    ("src/fan_control.rs", "FanSummary", "total_fans running_fans"),
    ("src/fan_control.rs", "TripPoint", "temp_celsius"),
    ("src/firmware/mod.rs", "FirmwareEntry", "inferred_risk_score"),
    ("src/fleet.rs", "HostMetrics", "cpu_usage_percent memory_usage_percent disk_usage_percent network_rx_bytes_sec network_tx_bytes_sec uptime_seconds"),
    ("src/fleet.rs", "TagGroupMetrics", "avg_cpu avg_memory total_alerts"),
    ("src/fleet_store/rows.rs", "MetricRow", "collected_at_us"),
    ("src/gpu/traits.rs", "PciInfo", "domain bus device function"),
    ("src/gpu_topology/mod.rs", "GpuLink", "from_gpu to_gpu bandwidth_per_link_gbs"),
    ("src/gpu_topology/mod.rs", "GpuTopologyNode", "numa_node pcie_gen pcie_width pcie_speed_gts pcie_bandwidth_gbs"),
    ("src/hardware_ai/mod.rs", "HardwareAge", "estimated_age_years confidence"),
    ("src/hwmon/mod.rs", "HwSensor", "value"),
    ("src/interconnect/mod.rs", "ChipletTopology", "compute_dies io_dies on_package_bandwidth_gbs"),
    ("src/memory_bandwidth/mod.rs", "BandwidthAnalysis", "estimated_latency_ns"),
    ("src/memory_management.rs", "MemorySummary", "memory_percent swap_percent total_memory available_memory total_swap used_swap health_score"),
    ("src/memory_topology/mod.rs", "MemoryAnalysis", "total_capacity_bytes max_capacity_bytes populated_slots total_slots efficiency_score"),
    ("src/network_monitor.rs", "NetworkInterfaceInfo", "rx_bytes rx_packets rx_errors rx_drops tx_bytes tx_packets tx_errors tx_drops"),
    ("src/network_tools.rs", "CaptureConfig", "timeout_secs"),
    ("src/network_tools.rs", "CapturedPacket", "length"),
    ("src/network_tools.rs", "NmapScanResult", "scan_duration_secs"),
    ("src/network_tools.rs", "OsFingerprint", "confidence"),
    ("src/network_tools.rs", "PingResult", "packets_sent packets_received packets_lost packet_loss_percent rtt_min_ms rtt_max_ms rtt_avg_ms"),
    ("src/network_tools.rs", "TracerouteHop", "ttl"),
    ("src/observability/context.rs", "CpuMetrics", "utilization_percent"),
    ("src/observability/context.rs", "MemoryContext", "total_gb"),
    ("src/observability/context.rs", "MemoryMetrics", "used_mb total_mb"),
    ("src/observability/context.rs", "NetworkMetrics", "rx_bytes_total tx_bytes_total rx_packets tx_packets errors dropped"),
    ("src/observability/context.rs", "ProcessMetrics", "cpu_percent memory_mb threads"),
    ("src/observability/metrics.rs", "CpuMetricSnapshot", "usage_percent"),
    ("src/observability/metrics.rs", "DiskMetricSnapshot", "used_bytes free_bytes total_bytes usage_percent"),
    ("src/observability/metrics.rs", "GpuMetricSnapshot", "usage_percent memory_used_mb memory_total_mb"),
    ("src/pci_devices/mod.rs", "PciDeviceInfo", "sriov_vfs numa_node"),
    ("src/pcie.rs", "PcieBandwidthSummary", "gpu_devices downgraded_devices unreadable_devices"),
    ("src/pcie.rs", "PcieDevice", "numa_node current_link_width max_link_width"),
    ("src/pipeline/mod.rs", "NetSnapshot", "rx_bytes tx_bytes"),
    ("src/predictive.rs", "MaintenanceAlert", "current_value threshold confidence"),
    ("src/process_monitor.rs", "CategoryStats", "total_cpu_percent total_memory_bytes"),
    ("src/process_monitor.rs", "ProcessMonitorInfo", "cpu_percent memory_bytes virtual_memory_bytes private_bytes io_read_bytes io_write_bytes cpu_time_us"),
    ("src/rapl/mod.rs", "EnergyReading", "socket energy_uj"),
    ("src/scheduler/mod.rs", "PressureInfo", "some_avg10 some_avg60 some_avg300 some_total_us"),
    ("src/silicon/mod.rs", "NetworkSilicon", "rx_bandwidth_mbps tx_bandwidth_mbps packet_rate"),
    ("src/storage_controller/mod.rs", "RaidArrayInfo", "active_devices failed_devices spare_devices size_bytes"),
    ("src/tsdb/mod.rs", "DatabaseStats", "max_size current_size"),
    ("src/tsdb/mod.rs", "ProcessSnapshot", "cpu_percent memory_bytes"),
    ("src/tui/app.rs", "CpuInfo", "threads utilization"),
    ("src/tui/app.rs", "MemoryInfo", "total used available"),
    ("src/tui/app.rs", "NetworkInfo", "total_rx_bytes total_tx_bytes"),
    ("src/tui/app.rs", "NetworkInterfaceInfo", "rx_bytes tx_bytes"),
];

#[test]
fn no_struct_gains_a_bare_reading_beside_optional_ones() {
    let baseline: BTreeSet<(String, String, String)> = BASELINE
        .iter()
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
                    "not a new BASELINE entry."
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
    let stale: Vec<String> = BASELINE
        .iter()
        .filter(|(f, s, b)| !found.contains(&(f.to_string(), s.to_string(), b.to_string())))
        .map(|(f, s, b)| {
            format!("{f}: `{s}` [{b}] no longer matches; update or remove its BASELINE entry")
        })
        .collect();
    assert!(stale.is_empty(), "{}", stale.join("\n"));
}
