// SPDX-License-Identifier: AGPL-3.0-or-later
//! Benchmark for CPU statistics collection.
//!
//! Measures the cost of reading per-core CPU stats from the platform.
//! On Windows this exercises the cached wmic path; on Linux it reads /proc/stat.

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_read_cpu_stats(c: &mut Criterion) {
    c.bench_function("read_cpu_stats", |b| {
        b.iter(|| {
            #[cfg(target_os = "linux")]
            {
                let _ = ironmonlib::platform::linux::cpu::read_cpu_stats();
            }
            #[cfg(windows)]
            {
                let _ = ironmonlib::platform::windows::read_cpu_stats();
            }
        });
    });
}

fn bench_read_memory_stats(c: &mut Criterion) {
    c.bench_function("read_memory_stats", |b| {
        b.iter(|| {
            #[cfg(target_os = "linux")]
            {
                let _ = ironmonlib::platform::linux::memory::read_memory_stats();
            }
            #[cfg(windows)]
            {
                let _ = ironmonlib::platform::windows::read_memory_stats();
            }
        });
    });
}

criterion_group!(benches, bench_read_cpu_stats, bench_read_memory_stats);
criterion_main!(benches);
