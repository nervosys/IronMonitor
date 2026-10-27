//! Per-provider snapshot timing — lets users (and AI agents) see which
//! providers are slow to enumerate and decide whether to cache or skip.

use super::{ProfileProvider, Subsystem};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBench {
    pub subsystem: Subsystem,
    pub elapsed_ms: f64,
    /// Counts are zero when `error` is set; they are not a reading.
    pub group_count: usize,
    pub setting_count: usize,
    /// Throughput: settings produced per millisecond of wall-clock time.
    /// Useful for comparing providers of different sizes.
    pub settings_per_ms: f64,
    /// Why the provider's read failed, if it did.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchReport {
    pub providers: Vec<ProviderBench>,
    pub total_elapsed_ms: f64,
    pub total_groups: usize,
    pub total_settings: usize,
}

/// Run each built-in provider once and time it. The inspector is rebuilt so
/// no warm cache from a prior call skews the result.
pub fn run_bench() -> BenchReport {
    let mut providers: Vec<Box<dyn ProfileProvider>> = vec![
        Box::new(super::gpu::GpuProfileProvider::new()),
        Box::new(super::cpu::CpuProfileProvider::new()),
        Box::new(super::nvme::NvmeProfileProvider::new()),
        Box::new(super::display::DisplayProfileProvider::new()),
        Box::new(super::memory::MemoryProfileProvider::new()),
    ];

    let mut report = BenchReport {
        providers: Vec::with_capacity(providers.len()),
        total_elapsed_ms: 0.0,
        total_groups: 0,
        total_settings: 0,
    };

    for p in providers.iter_mut() {
        let sub = p.subsystem();
        let start = Instant::now();
        let result = p.snapshot();
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let groups = result.as_deref().unwrap_or_default();
        let setting_count: usize = groups.iter().map(|g| g.settings.len()).sum();
        let bench = ProviderBench {
            subsystem: sub,
            elapsed_ms,
            group_count: groups.len(),
            setting_count,
            settings_per_ms: if elapsed_ms > 0.0 {
                setting_count as f64 / elapsed_ms
            } else {
                0.0
            },
            error: result.as_ref().err().cloned(),
        };
        report.total_elapsed_ms += elapsed_ms;
        report.total_groups += groups.len();
        report.total_settings += setting_count;
        report.providers.push(bench);
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bench_runs_all_providers() {
        let r = run_bench();
        assert_eq!(r.providers.len(), 5);
        // Total time is the sum of per-provider times.
        let sum: f64 = r.providers.iter().map(|p| p.elapsed_ms).sum();
        assert!((r.total_elapsed_ms - sum).abs() < 1e-6);
    }

    #[test]
    fn bench_serializable() {
        let r = run_bench();
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("providers"));
    }
}
