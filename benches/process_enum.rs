// SPDX-License-Identifier: AGPL-3.0-or-later
//! Benchmark for process enumeration.
//!
//! Measures the cost of listing all system processes, which involves
//! reading /proc (Linux) or ToolHelp32 (Windows) and optionally merging
//! GPU process data.

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_process_list(c: &mut Criterion) {
    c.bench_function("process_list", |b| {
        b.iter(|| {
            if let Ok(mut pm) = ironmonlib::process_monitor::ProcessMonitor::new() {
                let _ = pm.processes();
            }
        });
    });
}

fn bench_process_classify(c: &mut Criterion) {
    use ironmonlib::process_monitor::ProcessCategory;

    let names = [
        "chrome",
        "python3",
        "nvidiagpud",
        "systemd",
        "vlc",
        "code",
        "postgres",
        "steam",
        "unknown_app",
    ];

    c.bench_function("process_classify", |b| {
        b.iter(|| {
            for name in &names {
                let _ = ProcessCategory::classify(name, None, false);
            }
        });
    });
}

criterion_group!(benches, bench_process_list, bench_process_classify);
criterion_main!(benches);
