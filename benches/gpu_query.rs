// SPDX-License-Identifier: AGPL-3.0-or-later
//! Benchmark for GPU enumeration and querying.
//!
//! Measures the cost of auto-detecting GPUs and reading their properties.
//! This exercises the vendor-specific backends (NVML, AMD sysfs, Intel, DXGI).

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_gpu_auto_detect(c: &mut Criterion) {
    c.bench_function("gpu_auto_detect", |b| {
        b.iter(|| {
            let _ = ironmonlib::gpu::GpuCollection::auto_detect();
        });
    });
}

fn bench_gpu_snapshot_all(c: &mut Criterion) {
    // Create collection once, then benchmark repeated snapshots
    if let Ok(collection) = ironmonlib::gpu::GpuCollection::auto_detect() {
        c.bench_function("gpu_snapshot_all", |b| {
            b.iter(|| {
                let _ = collection.snapshot_all();
            });
        });
    } else {
        // No GPUs available — register a no-op so criterion doesn't fail
        c.bench_function("gpu_snapshot_all (no gpu)", |b| {
            b.iter(|| {});
        });
    }
}

criterion_group!(benches, bench_gpu_auto_detect, bench_gpu_snapshot_all);
criterion_main!(benches);
