//! Cached profile inspector with per-subsystem TTL.
//!
//! Profile snapshots are expensive on some platforms: on Windows the NVMe
//! provider hits WMI for ~2.7s, and the NVIDIA DRS scanner reads two
//! 2.5 MB files. For interactive use (GUI, TUI, AI agent re-queries)
//! we don't need to re-snapshot every call — a few-second cache is fine.
//!
//! [`CachedProfileInspector`] wraps a fresh-per-call [`ProfileInspector`]
//! and remembers, per subsystem, the last result and when it was taken.
//! Cache TTL is configurable per subsystem (defaulting to 5s) because the
//! slow providers should cache longer than the fast ones.

use super::{ProfileGroup, ProfileInspector, ProfileSnapshot, Subsystem};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Cache hit/miss counters (process-global, cheap atomic increments).
#[derive(Default, Debug)]
pub struct CacheStats {
    hits: AtomicU64,
    misses: AtomicU64,
}

impl CacheStats {
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }
    pub fn reset(&self) {
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
    }
}

pub static CACHE_STATS: CacheStats = CacheStats {
    hits: AtomicU64::new(0),
    misses: AtomicU64::new(0),
};

struct CacheEntry {
    taken_at: Instant,
    groups: Vec<ProfileGroup>,
}

pub struct CachedProfileInspector {
    inner: ProfileInspector,
    entries: BTreeMap<Subsystem, CacheEntry>,
    ttls: BTreeMap<Subsystem, Duration>,
    default_ttl: Duration,
}

impl CachedProfileInspector {
    pub fn new() -> Self {
        // Conservative defaults: NVMe and GPU benefit most from caching, but
        // cpu / display / memory are cheap and produce volatile data — keep
        // those at the short default.
        let mut ttls = BTreeMap::new();
        ttls.insert(Subsystem::Nvme, Duration::from_secs(30));
        ttls.insert(Subsystem::Gpu, Duration::from_secs(15));
        ttls.insert(Subsystem::Memory, Duration::from_secs(60));
        ttls.insert(Subsystem::Display, Duration::from_secs(5));
        ttls.insert(Subsystem::Cpu, Duration::from_secs(2));
        Self {
            inner: ProfileInspector::new(),
            entries: BTreeMap::new(),
            ttls,
            default_ttl: Duration::from_secs(5),
        }
    }

    /// Override the TTL for one subsystem.
    pub fn set_ttl(&mut self, sub: Subsystem, ttl: Duration) {
        self.ttls.insert(sub, ttl);
    }

    /// Invalidate the cache for one subsystem (or all if `sub` is None).
    pub fn invalidate(&mut self, sub: Option<Subsystem>) {
        match sub {
            Some(s) => {
                self.entries.remove(&s);
            }
            None => self.entries.clear(),
        }
    }

    fn ttl_for(&self, sub: Subsystem) -> Duration {
        *self.ttls.get(&sub).unwrap_or(&self.default_ttl)
    }

    /// Snapshot one subsystem, using the cache when fresh. A failed read is
    /// returned as `Err` and is not cached, so the next call tries again.
    pub fn snapshot(&mut self, sub: Subsystem) -> Result<Vec<ProfileGroup>, String> {
        if let Some(entry) = self.entries.get(&sub) {
            if entry.taken_at.elapsed() < self.ttl_for(sub) {
                CACHE_STATS.hits.fetch_add(1, Ordering::Relaxed);
                return Ok(entry.groups.clone());
            }
        }
        CACHE_STATS.misses.fetch_add(1, Ordering::Relaxed);
        let groups = self.inner.snapshot(sub)?;
        self.entries.insert(
            sub,
            CacheEntry {
                taken_at: Instant::now(),
                groups: groups.clone(),
            },
        );
        Ok(groups)
    }

    /// Snapshot every subsystem, using per-subsystem cache TTLs. Subsystems
    /// whose read failed appear in `errors`, not in `providers`.
    pub fn snapshot_all(&mut self) -> ProfileSnapshot {
        let mut providers = BTreeMap::new();
        let mut errors = BTreeMap::new();
        for sub in Subsystem::ALL {
            match self.snapshot(*sub) {
                Ok(groups) => {
                    providers.insert(*sub, groups);
                }
                Err(reason) => {
                    errors.insert(*sub, reason);
                }
            }
        }
        ProfileSnapshot {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            providers,
            errors,
        }
    }
}

impl Default for CachedProfileInspector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CACHE_STATS` is one process-global counter and cargo runs these tests
    /// as threads in one process, so without this they interleave: a `reset()`
    /// landing between another test's `miss_before` read and its assertion
    /// makes the count go backwards and the assertion fails on scheduling
    /// rather than on behaviour. Every test that resets or reads the global
    /// takes this, `snapshot_all` included -- it takes no reading itself but
    /// increments the same counters the others are asserting on.
    static STATS_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A poisoned guard still serialises correctly: the data is `()`, so an
    /// earlier panic has nothing to have corrupted.
    fn lock_stats() -> std::sync::MutexGuard<'static, ()> {
        STATS_GUARD.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn cold_call_misses_warm_call_hits() {
        let _guard = lock_stats();
        CACHE_STATS.reset();
        let mut c = CachedProfileInspector::new();
        c.set_ttl(Subsystem::Cpu, Duration::from_secs(60));
        let _ = c.snapshot(Subsystem::Cpu);
        let misses_after_cold = CACHE_STATS.misses();
        let _ = c.snapshot(Subsystem::Cpu);
        let hits_after_warm = CACHE_STATS.hits();
        assert!(misses_after_cold >= 1);
        assert!(hits_after_warm >= 1);
    }

    #[test]
    fn invalidate_forces_refresh() {
        let _guard = lock_stats();
        CACHE_STATS.reset();
        let mut c = CachedProfileInspector::new();
        c.set_ttl(Subsystem::Cpu, Duration::from_secs(60));
        let _ = c.snapshot(Subsystem::Cpu);
        c.invalidate(Some(Subsystem::Cpu));
        let miss_before = CACHE_STATS.misses();
        let _ = c.snapshot(Subsystem::Cpu);
        assert!(CACHE_STATS.misses() > miss_before);
    }

    #[test]
    fn snapshot_all_populates_all_subsystems() {
        let _guard = lock_stats();
        let mut c = CachedProfileInspector::new();
        let snap = c.snapshot_all();
        for sub in Subsystem::ALL {
            // Every subsystem is either read or reported as failed.
            assert!(snap.providers.contains_key(sub) ^ snap.errors.contains_key(sub));
        }
    }
}
