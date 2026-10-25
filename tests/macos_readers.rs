// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! The macOS CPU and memory readers, exercised on macOS.
//!
//! These were written by cross-compilation from a Windows machine and had never
//! executed anywhere when they were committed. That is the same position the Linux
//! SMART paths were in for 3.0.0, and the answer there turned out to be that CI
//! *is* the missing hardware: the test matrix runs on `macos-latest`, so a test
//! gated to macOS runs the code on a real Mac on every push.
//!
//! So these assert what a reading must satisfy to be a reading — a CPU split that
//! accounts for the whole processor, a memory total that matches installed RAM,
//! used memory that fits inside it. A parser that silently misreads `vm_stat` or
//! `top` fails here rather than shipping numbers nobody checked.
//!
//! On every other platform this file compiles to nothing.

#![cfg(target_os = "macos")]

use ironmonitor::IronMonitor;

#[cfg(feature = "cli")]
#[test]
fn status_reports_the_running_macos_version() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_imon"))
        .args(["status", "--format", "json"])
        .output()
        .expect("run imon status");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("status JSON");
    let version = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .unwrap();
    assert!(version.status.success());
    let expected = String::from_utf8(version.stdout).unwrap();
    let reported = rows
        .iter()
        .find(|row| row["label"] == "OS version")
        .expect("OS version row");
    assert_eq!(reported["value"], expected.trim());
    let model = rows
        .iter()
        .find(|row| row["label"] == "Model")
        .expect("model row");
    assert!(
        model["value"].as_str().is_some_and(|s| !s.is_empty()),
        "{model}"
    );
}

/// `IronMonitor::new` calls `detect_platform_info` in its constructor, so this failing
/// is what previously made every other reader unreachable on macOS regardless of
/// whether it worked.
#[test]
fn ironmon_can_be_constructed_on_macos() {
    let ironmon = IronMonitor::new().expect("IronMonitor::new must succeed on macOS");
    let board = ironmon.board_info();

    assert_eq!(board.platform.system, "Darwin");
    assert!(
        !board.platform.release.is_empty(),
        "kern.osrelease should be readable"
    );
    assert!(
        !board.hardware.model.is_empty(),
        "hw.model should be readable"
    );
}

#[test]
fn the_cpu_split_accounts_for_the_whole_processor() {
    let ironmon = IronMonitor::new().expect("construct");
    let cpu = ironmon
        .cpu()
        .expect("CPU statistics must be readable on macOS");

    let nice = cpu
        .total
        .nice
        .expect("macOS total nice time is read from the ticks");
    let total = cpu.total.user + nice + cpu.total.system + cpu.total.idle;
    assert!(
        (95.0..=105.0).contains(&total),
        "user {} + nice {} + system {} + idle {} = {total}, which does not account \
         for the processor — the `top` line was probably misparsed",
        cpu.total.user,
        nice,
        cpu.total.system,
        cpu.total.idle
    );

    for (name, value) in [
        ("user", cpu.total.user),
        ("system", cpu.total.system),
        ("idle", cpu.total.idle),
    ] {
        assert!(
            (0.0..=100.0).contains(&value),
            "{name} is {value}%, which is not a percentage"
        );
    }
}

#[test]
fn every_logical_core_is_enumerated() {
    let ironmon = IronMonitor::new().expect("construct");
    let cpu = ironmon.cpu().expect("CPU statistics");

    assert!(
        cpu.core_count() > 0,
        "hw.logicalcpu should report at least one core"
    );
    // No macOS reader fills the per-core online flag, so the count is unknown
    // rather than the core count.
    assert_eq!(
        cpu.online_count(),
        None,
        "macOS has no online reader, so the online count must be unknown"
    );
}

/// Per-core figures come from `host_processor_info`, which returns cumulative ticks.
/// A single reading is an average since boot, so the reader takes its own second
/// reading on the first call and differences the two. Each core therefore reports a
/// current split, or nothing: its four fields are present together or absent together,
/// and a core that reports one sums to about 100 percent.
#[test]
fn a_first_sample_reports_a_per_core_split() {
    let ironmon = IronMonitor::new().expect("construct");
    let cpu = ironmon.cpu().expect("CPU statistics");

    let mut reporting = 0;
    for core in &cpu.cores {
        let split = [core.user, core.nice, core.system, core.idle];
        let present = split.iter().filter(|field| field.is_some()).count();
        assert!(
            present == 0 || present == 4,
            "core {} reports {present} of its four split fields, not all or none",
            core.id
        );
        if let (Some(user), Some(nice), Some(system), Some(idle)) =
            (core.user, core.nice, core.system, core.idle)
        {
            reporting += 1;
            let total = user + nice + system + idle;
            assert!(
                (total - 100.0).abs() < 1.0,
                "core {} split sums to {total}, expected about 100",
                core.id
            );
        }
    }
    assert!(
        reporting > 0,
        "no core reports a split on the first call, so the per-core figures are still \
         the since-boot average or absent"
    );
}

/// The whole point of reading per-core ticks rather than copying an aggregate. If
/// a refactor ever fills each core from the average, every core reads identically
/// and this fails.
///
/// Not asserted where cores would legitimately read the same: the comparison is over
/// the per-core splits of one window, and a saturated CI runner can pin every core
/// at 100 percent busy.
#[test]
fn cores_are_not_all_reporting_the_same_number() {
    let ironmon = IronMonitor::new().expect("construct");
    let cpu = ironmon.cpu().expect("CPU statistics");

    if cpu.cores.len() < 2 {
        return; // Nothing to compare on a single-core runner.
    }

    let idles: Vec<f32> = cpu.cores.iter().filter_map(|c| c.idle).collect();
    // A core with no movement in the sampling window reports no split, so a copied
    // figure can only be detected where at least two cores report one.
    if idles.len() < 2 {
        return;
    }
    let spread = idles.iter().cloned().fold(f32::MIN, f32::max)
        - idles.iter().cloned().fold(f32::MAX, f32::min);

    assert!(
        spread > 0.0,
        "all {} cores report identical idle time, which means one figure was \
         copied across them rather than each core being read",
        cpu.cores.len()
    );
}

/// Nice time is a real reading now rather than the 0.0 the `top` path had to use.
#[test]
fn nice_time_is_a_reading_rather_than_a_placeholder() {
    let ironmon = IronMonitor::new().expect("construct");
    let cpu = ironmon.cpu().expect("CPU statistics");

    let nice = cpu.total.nice.expect("macOS reports nice time");
    assert!(
        (0.0..=100.0).contains(&nice),
        "nice is {nice}%, which is not a percentage"
    );
}

#[test]
fn memory_totals_are_internally_consistent() {
    let ironmon = IronMonitor::new().expect("construct");
    let memory = ironmon
        .memory()
        .expect("memory statistics must be readable");

    assert!(
        memory.ram.total > 0,
        "hw.memsize should report installed RAM"
    );

    // `RamInfo` is in KB, not bytes: `read_memory_stats` divides `hw.memsize`
    // by 1024 before it stores it. An earlier version of this test compared the
    // KB figure against a byte floor, so a correct 7 GiB reading on the CI
    // runner — 7340032 KB — read as 7 MB and failed.
    //
    // No Mac ships with under 1 GiB, and none has 1 PiB. A page size misread as
    // 4096 on Apple Silicon would land far below this floor.
    const ONE_GIB_IN_KB: u64 = 1024 * 1024;
    assert!(
        memory.ram.total >= ONE_GIB_IN_KB,
        "total RAM reported as {} KB, below any real Mac — suspect the page size",
        memory.ram.total
    );
    assert!(
        memory.ram.total < 1024 * ONE_GIB_IN_KB,
        "total RAM reported as {} KB",
        memory.ram.total
    );

    assert!(
        memory.ram.used > 0,
        "a running system has used memory; 0 means the page counts were not read"
    );
    assert!(
        memory.ram.used <= memory.ram.total,
        "used {} exceeds total {}",
        memory.ram.used,
        memory.ram.total
    );

    // The total was checked above (used > 0 and used <= total), so a None here
    // means the reader failed to fill it, which this test should report.
    let percent = memory
        .ram_usage_percent()
        .expect("RAM total was read on a running system");
    assert!(
        (0.0..=100.0).contains(&percent),
        "usage of {percent}% is not a percentage"
    );
}

#[test]
fn swap_is_reported_without_exceeding_its_own_total() {
    let ironmon = IronMonitor::new().expect("construct");
    let memory = ironmon.memory().expect("memory statistics");

    // Three states since 6.0.0, and only one of them is comparable. A CI runner
    // may have swap disabled, which is a real zero; the platform may not report
    // swap at all, which is `None` and not a small number. Comparing through
    // `unwrap_or(0)` would make this pass for the wrong reason on the second.
    match (memory.swap.used, memory.swap.total) {
        (Some(used), Some(total)) => assert!(
            used <= total,
            "swap used {used} exceeds total {total}"
        ),
        (None, None) => {}
        (used, total) => panic!(
            "swap reported one half and not the other: used={used:?} total={total:?}, which no reader should produce"
        ),
    }
}

#[test]
fn uptime_is_positive_and_not_a_wrapped_duration() {
    let ironmon = IronMonitor::new().expect("construct");
    let uptime = ironmon.uptime().expect("uptime must be readable");

    assert!(
        uptime.as_secs() > 0,
        "a machine running tests has been up for more than a second"
    );
    // A subtraction done the wrong way round would wrap to ~584 billion years.
    assert!(
        uptime.as_secs() < 60 * 60 * 24 * 365 * 20,
        "uptime of {} seconds suggests kern.boottime was misread",
        uptime.as_secs()
    );
}

/// Raw output of a macOS tool, for checking a parser against what the tool
/// actually printed on this machine.
#[cfg(feature = "apple")]
fn raw(program: &str, args: &[&str]) -> String {
    let output = std::process::Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .unwrap_or_else(|e| panic!("{program} must run on macOS: {e}"));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The SoC readers were written without a Mac to run them on, from captured
/// output. So each is checked against the tool's own output here: where the
/// raw text carries a key, the parser must have read it.
#[cfg(feature = "apple")]
#[test]
fn performance_levels_account_for_every_logical_cpu() {
    use ironmonitor::silicon::apple::read_perf_levels;
    let raw_count = raw("sysctl", &["-n", "hw.nperflevels"]);
    let Ok(count) = raw_count.trim().parse::<usize>() else {
        // Intel Macs have no performance levels; the reader must say so.
        assert!(read_perf_levels().is_err());
        return;
    };
    let levels = read_perf_levels().expect("hw.nperflevels read, so the levels must");
    assert_eq!(levels.len(), count, "{levels:?}");
    let logical: Option<u32> = levels.iter().map(|l| l.logical_cores).sum();
    let total: u32 = raw("sysctl", &["-n", "hw.logicalcpu"])
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        logical,
        Some(total),
        "the performance levels must cover every logical CPU: {levels:?}"
    );
    for level in &levels {
        assert!(level.name.is_some(), "every level has a name: {levels:?}");
    }
}

#[cfg(feature = "apple")]
#[test]
fn gpu_counters_read_every_key_ioreg_printed() {
    use ironmonitor::silicon::apple::{parse_ioreg_accelerators, read_gpu_counters};
    let text = raw(
        "ioreg",
        &["-r", "-d", "1", "-w", "0", "-c", "IOAccelerator"],
    );
    eprintln!("ioreg IOAccelerator:\n{text}");
    let gpus = read_gpu_counters().expect("ioreg runs on every Mac");
    let parsed = parse_ioreg_accelerators(&text);
    // Live counters move between the two reads; the device count does not.
    assert_eq!(gpus.len(), parsed.len(), "{gpus:?} vs {parsed:?}");
    assert_eq!(
        parsed.len(),
        text.lines()
            .filter(|l| l.trim_start().starts_with("+-o "))
            .count(),
        "one entry per accelerator ioreg listed"
    );
    if text.contains("\"gpu-core-count\"") {
        assert!(parsed.iter().any(|g| g.core_count.is_some()), "{parsed:?}");
    }
    if text.contains("\"Device Utilization %\"") {
        assert!(
            parsed.iter().any(|g| g.device_utilization.is_some()),
            "{parsed:?}"
        );
    }
    if text.contains("\"In use system memory\"") {
        assert!(
            parsed.iter().any(|g| g.in_use_system_memory.is_some()),
            "{parsed:?}"
        );
    }
}

#[cfg(feature = "apple")]
#[test]
fn neural_engine_count_matches_ioreg() {
    use ironmonitor::silicon::apple::read_neural_engines;
    let text = raw("ioreg", &["-r", "-d", "1", "-w", "0", "-c", "H11ANEIn"]);
    let listed = text
        .lines()
        .filter(|l| l.trim_start().starts_with("+-o "))
        .count() as u32;
    match read_neural_engines() {
        Ok(count) => assert_eq!(count, listed),
        // None listed is reported as a lookup that found nothing, not as zero.
        Err(why) => assert_eq!(listed, 0, "{why}"),
    }
}

#[cfg(feature = "apple")]
#[test]
fn unified_memory_is_read_where_system_profiler_lists_no_slots() {
    use ironmonitor::memory_topology::{FormFactor, MemoryTopologyMonitor};
    use ironmonitor::silicon::apple::read_unified_memory;
    let text = raw("system_profiler", &["SPMemoryDataType", "-json"]);
    eprintln!("system_profiler SPMemoryDataType:\n{text}");
    let root: serde_json::Value = serde_json::from_str(&text).expect("system_profiler JSON");
    let items = root["SPMemoryDataType"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let on_package = items
        .iter()
        .any(|item| item.get("_items").is_none() && item.get("dimm_type").is_some());

    let memory = read_unified_memory();
    let memsize: u64 = raw("sysctl", &["-n", "hw.memsize"]).trim().parse().unwrap();
    assert_eq!(memory.total_bytes, Some(memsize));

    if on_package {
        let topology = MemoryTopologyMonitor::new().expect("topology reads");
        assert!(
            topology
                .topology()
                .dimms
                .iter()
                .any(|d| d.form_factor == FormFactor::OnPackage && d.populated == Some(true)),
            "system_profiler listed on-package memory and the topology has none"
        );
        let named = items[0]["dimm_type"].as_str().unwrap_or("");
        if ["LPDDR4", "LPDDR5"].iter().any(|t| named.contains(t)) {
            assert!(
                memory.memory_type.is_some(),
                "{named} was not read: {memory:?}"
            );
        }
    }
}
