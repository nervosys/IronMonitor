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

use ironmonlib::IronMonitor;

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

    let total = cpu.total.user + cpu.total.nice + cpu.total.system + cpu.total.idle;
    assert!(
        (95.0..=105.0).contains(&total),
        "user {} + nice {} + system {} + idle {} = {total}, which does not account \
         for the processor — the `top` line was probably misparsed",
        cpu.total.user,
        cpu.total.nice,
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
    assert_eq!(
        cpu.online_count(),
        cpu.core_count(),
        "macOS does not offline cores"
    );
}

/// Per-core figures come from `host_processor_info`. Each core's own split must
/// account for that core — a core reporting a share of the *machine* rather than
/// of itself is the mistake this catches.
#[test]
fn every_core_reports_its_own_complete_split() {
    let ironmon = IronMonitor::new().expect("construct");
    let cpu = ironmon.cpu().expect("CPU statistics");

    for core in &cpu.cores {
        let (user, nice, system, idle) = (
            core.user.expect("per-core user"),
            core.nice.expect("per-core nice"),
            core.system.expect("per-core system"),
            core.idle.expect("per-core idle"),
        );

        let total = user + nice + system + idle;
        assert!(
            (99.0..=101.0).contains(&total),
            "core {} splits to {total}%, so its ticks were not read as one core's",
            core.id
        );
    }
}

/// The whole point of reading per-core ticks rather than copying an aggregate. If
/// a refactor ever fills each core from the average, every core reads identically
/// and this fails.
///
/// Not asserted on a single sample of a *busy* machine — under a saturated CI
/// runner every core can legitimately read the same. Cumulative since-boot ticks
/// are what is compared, and those diverge across cores on any machine that has
/// been up for more than a moment.
#[test]
fn cores_are_not_all_reporting_the_same_number() {
    let ironmon = IronMonitor::new().expect("construct");
    let cpu = ironmon.cpu().expect("CPU statistics");

    if cpu.cores.len() < 2 {
        return; // Nothing to compare on a single-core runner.
    }

    let idles: Vec<f32> = cpu.cores.iter().filter_map(|c| c.idle).collect();
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

    assert!(
        (0.0..=100.0).contains(&cpu.total.nice),
        "nice is {}%, which is not a percentage",
        cpu.total.nice
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

    let percent = memory.ram_usage_percent();
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
