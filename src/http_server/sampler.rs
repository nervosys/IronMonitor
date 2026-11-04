//! Background sampling of ontology segments for the HTTP surface.
//!
//! The HTTP routes answer from samples rather than resolving on each request, so
//! that a slow segment cannot hold a request open and so that concurrent clients
//! share one read of the hardware.
//!
//! Shape of the thing:
//!
//! - One thread runs every pass, one pass at a time. The readers share static state
//!   (`PREV_CPU_COUNTERS`, PDH and WMI handles), so two passes at once would corrupt
//!   each other's rates. The Prometheus collector reads the same state, and the two
//!   are serialised by one lock: `resolve::sample_segment` takes
//!   `crate::pipeline::reader_pass`, as does the collector's own pass.
//! - A pass samples one segment with [`crate::ontology::resolve::sample_segment`], the
//!   same readers `get` runs. Segments are passed one at a time, so a cheap segment
//!   is not held up behind an expensive one for a whole snapshot.
//! - A route marks the segments it reads as wanted. A segment that nobody has
//!   wanted within the idle period is not sampled. When no segment is wanted the
//!   thread blocks on a condition variable and does no work at all.
//! - A wanted segment is resampled once its last sample is older than the resample
//!   interval. A slow segment therefore lags, and the age the reader sees says so.
//! - Requests never run a pass themselves. They wait, for a bounded time, for a
//!   sample that the thread produces.

use crate::ontology::resolve::{self, Reading};
use crate::ontology::Domain;
use chrono::{DateTime, SecondsFormat, Utc};
use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// What a pass reads from. Production uses the resolver; tests substitute a source
/// that counts passes and takes a chosen time.
pub(crate) trait SegmentSource: Send + Sync + 'static {
    fn sample(&self, segment: &str) -> Vec<Reading>;
}

/// The production source: the resolver's own segment pass.
pub(crate) struct ResolverSource;

impl SegmentSource for ResolverSource {
    fn sample(&self, segment: &str) -> Vec<Reading> {
        resolve::sample_segment(segment)
    }
}

/// How long a segment stays wanted after its last request, and how often a wanted
/// segment is resampled. Injected for tests; the server uses [`Timing::PRODUCTION`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    pub idle_after: Duration,
    pub resample_every: Duration,
}

impl Timing {
    pub(crate) const PRODUCTION: Timing = Timing {
        idle_after: Duration::from_secs(60),
        resample_every: Duration::from_secs(2),
    };
}

/// One pass's readings, with the instant the pass started.
///
/// The sample is stamped with the start of the pass, not its end: a reading is as
/// old as the moment it began to be taken, so it never appears fresher than it was.
pub(crate) struct Sample {
    pub readings: Vec<Reading>,
    pub sampled_at: DateTime<Utc>,
    taken: Instant,
}

impl Sample {
    /// Milliseconds since the pass started, measured when asked.
    pub(crate) fn age_ms(&self) -> u64 {
        self.taken.elapsed().as_millis() as u64
    }

    pub(crate) fn sampled_at_rfc3339(&self) -> String {
        self.sampled_at.to_rfc3339_opts(SecondsFormat::Millis, true)
    }
}

struct Slot {
    wanted_at: Option<Instant>,
    sample: Option<Arc<Sample>>,
}

struct State {
    slots: BTreeMap<String, Slot>,
    started: bool,
    passes: u64,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    timing: Timing,
    source: Arc<dyn SegmentSource>,
}

/// Handle to the sampler. Cheap to clone; every clone shares one thread and one set
/// of samples.
#[derive(Clone)]
pub(crate) struct Sampler {
    shared: Arc<Shared>,
}

/// Per-segment state for `/api/v1/health`.
pub(crate) struct SegmentStatus {
    pub name: String,
    /// The last sample's start time and its age in milliseconds, if one exists.
    pub last_sampled: Option<(String, u64)>,
}

/// Whether the sampler is busy, and what it last read.
pub(crate) struct Status {
    pub running: bool,
    pub segments: Vec<SegmentStatus>,
}

fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    // A panicking reader must not wedge every later request, so a poisoned lock is
    // taken over. The state it guards is only ever replaced whole, never half-written.
    shared.state.lock().unwrap_or_else(|e| e.into_inner())
}

impl Sampler {
    pub(crate) fn new(source: Arc<dyn SegmentSource>, timing: Timing) -> Self {
        let slots = Domain::ALL
            .iter()
            .map(|d| {
                (
                    d.as_str().to_string(),
                    Slot {
                        wanted_at: None,
                        sample: None,
                    },
                )
            })
            .collect();
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    slots,
                    started: false,
                    passes: 0,
                }),
                changed: Condvar::new(),
                timing,
                source,
            }),
        }
    }

    /// The sampler the server uses: the resolver, with production timing.
    pub(crate) fn production() -> Self {
        Self::new(Arc::new(ResolverSource), Timing::PRODUCTION)
    }

    /// Mark `segments` as wanted now and wait up to `bound` for a sample of each.
    ///
    /// Returns the samples in the order asked, or the segments still without one when
    /// the bound passes. A segment that is still missing is not an error to report
    /// as a reading: nothing has been learned about it yet.
    pub(crate) fn sample_for(
        &self,
        segments: &[String],
        bound: Duration,
    ) -> Result<Vec<(String, Arc<Sample>)>, Vec<String>> {
        let deadline = Instant::now() + bound;
        let mut state = lock(&self.shared);
        let now = Instant::now();
        for name in segments {
            if let Some(slot) = state.slots.get_mut(name) {
                slot.wanted_at = Some(now);
            }
        }
        self.ensure_thread(&mut state);
        self.shared.changed.notify_all();

        loop {
            let missing: Vec<String> = segments
                .iter()
                .filter(|name| {
                    state
                        .slots
                        .get(*name)
                        .is_none_or(|slot| slot.sample.is_none())
                })
                .cloned()
                .collect();
            if missing.is_empty() {
                let found = segments
                    .iter()
                    .filter_map(|name| {
                        let sample = state.slots.get(name)?.sample.clone()?;
                        Some((name.clone(), sample))
                    })
                    .collect();
                return Ok(found);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(missing);
            }
            state = self
                .shared
                .changed
                .wait_timeout(state, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    /// Whether a sampler thread has been started and what each segment last read.
    ///
    /// `running` is true while some segment is wanted within the idle period. The
    /// thread may exist while `running` is false: it is then parked, not sampling.
    pub(crate) fn status(&self) -> Status {
        let state = lock(&self.shared);
        let now = Instant::now();
        let idle = self.shared.timing.idle_after;
        let running = state
            .slots
            .values()
            .any(|slot| slot.wanted_at.is_some_and(|w| now.duration_since(w) < idle));
        let segments = state
            .slots
            .iter()
            .map(|(name, slot)| SegmentStatus {
                name: name.clone(),
                last_sampled: slot
                    .sample
                    .as_ref()
                    .map(|s| (s.sampled_at_rfc3339(), s.age_ms())),
            })
            .collect();
        Status { running, segments }
    }

    /// Passes completed since the sampler was created.
    #[cfg(test)]
    pub(crate) fn passes(&self) -> u64 {
        lock(&self.shared).passes
    }

    fn ensure_thread(&self, state: &mut State) {
        if state.started {
            return;
        }
        let shared = Arc::clone(&self.shared);
        // If the thread cannot be started, no sample will arrive and the waiting
        // request answers 503 at its bound. That is the honest outcome, so the
        // failure is not escalated further.
        if std::thread::Builder::new()
            .name("ironmon-sampler".into())
            .spawn(move || run(shared))
            .is_ok()
        {
            state.started = true;
        }
    }
}

fn run(shared: Arc<Shared>) {
    let timing = shared.timing;
    let mut state = lock(&shared);
    loop {
        let now = Instant::now();

        // The most overdue wanted segment, if any is due. Never-sampled segments sort
        // first, in the order they were first wanted, so a cold first round follows the
        // requests rather than the alphabet; then the oldest sample. A segment outside
        // its idle window is skipped.
        type Priority = (bool, Option<Instant>);
        let mut next_due: Option<(String, Priority)> = None;
        let mut wake_at: Option<Instant> = None;
        for (name, slot) in state.slots.iter() {
            let Some(wanted) = slot.wanted_at else {
                continue;
            };
            if now.duration_since(wanted) >= timing.idle_after {
                continue;
            }
            let due_at = match &slot.sample {
                None => now,
                Some(sample) => sample.taken + timing.resample_every,
            };
            if due_at <= now {
                let key: Priority = (
                    slot.sample.is_some(),
                    slot.sample
                        .as_ref()
                        .map_or(slot.wanted_at, |s| Some(s.taken)),
                );
                if next_due.as_ref().is_none_or(|(_, current)| key < *current) {
                    next_due = Some((name.clone(), key));
                }
            } else {
                wake_at = Some(wake_at.map_or(due_at, |w| w.min(due_at)));
            }
        }

        if let Some((name, _)) = next_due {
            let started = Instant::now();
            let sampled_at = Utc::now();
            drop(state);
            // Lock released for the whole pass: requests must be able to wait on
            // the condition variable while it runs.
            let readings = shared.source.sample(&name);
            state = lock(&shared);
            state.passes += 1;
            if let Some(slot) = state.slots.get_mut(&name) {
                slot.sample = Some(Arc::new(Sample {
                    readings,
                    sampled_at,
                    taken: started,
                }));
            }
            shared.changed.notify_all();
            continue;
        }

        // Nothing due. With nothing wanted the thread waits without a timeout, so an
        // idle server has no timer running and no pass pending.
        state = match wake_at {
            None => shared
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner()),
            Some(at) => {
                let wait = at.saturating_duration_since(Instant::now());
                shared
                    .changed
                    .wait_timeout(state, wait)
                    .unwrap_or_else(|e| e.into_inner())
                    .0
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A source whose passes are counted and take a fixed time, and which records the
    /// largest number of passes it ever saw running at once.
    struct Counting {
        pause: Duration,
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
        per_segment: Mutex<BTreeMap<String, usize>>,
    }

    impl Counting {
        fn new(pause: Duration) -> Arc<Self> {
            Arc::new(Self {
                pause,
                in_flight: AtomicUsize::new(0),
                max_in_flight: AtomicUsize::new(0),
                per_segment: Mutex::new(BTreeMap::new()),
            })
        }

        fn count(&self, segment: &str) -> usize {
            self.per_segment
                .lock()
                .unwrap()
                .get(segment)
                .copied()
                .unwrap_or(0)
        }
    }

    impl SegmentSource for Counting {
        fn sample(&self, segment: &str) -> Vec<Reading> {
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_in_flight.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(self.pause);
            *self
                .per_segment
                .lock()
                .unwrap()
                .entry(segment.to_string())
                .or_insert(0) += 1;
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            vec![Reading {
                id: format!("{segment}.probe"),
                value: Some(serde_json::json!(1)),
                provenance: crate::ontology::Provenance::Measured,
                unit: None,
                note: None,
            }]
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn timing(idle_ms: u64, resample_ms: u64) -> Timing {
        Timing {
            idle_after: Duration::from_millis(idle_ms),
            resample_every: Duration::from_millis(resample_ms),
        }
    }

    /// Once nothing is wanted, the sampler stops: no further passes run, however long
    /// the server is left alone. The idle period is injected so this takes a second.
    #[test]
    fn sampling_stops_once_the_segment_has_been_idle() {
        let source = Counting::new(Duration::from_millis(1));
        let sampler = Sampler::new(source.clone(), timing(150, 20));

        let got = sampler.sample_for(&names(&["cpu"]), Duration::from_secs(2));
        assert!(
            got.is_ok(),
            "a cheap segment should answer within the bound"
        );
        assert!(sampler.status().running, "a fresh request keeps it running");

        // Resampling happens while wanted: the count rises past the first pass. Poll
        // against a generous deadline, so a loaded machine delays the check, not the
        // verdict.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while source.count("cpu") <= 1 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(source.count("cpu") > 1, "a wanted segment is resampled");

        // Let the idle period pass, then confirm nothing more runs.
        while sampler.status().running && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!sampler.status().running, "idle period has elapsed");
        let settled = sampler.passes();
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(
            sampler.passes(),
            settled,
            "no pass may run once nothing is being read"
        );
    }

    /// Many clients asking for one segment while it is being read cause one pass,
    /// not one each.
    #[test]
    fn concurrent_requests_for_one_segment_share_one_pass() {
        let source = Counting::new(Duration::from_millis(150));
        // Resample interval far longer than the test, so only the first pass is due.
        let sampler = Sampler::new(source.clone(), timing(60_000, 60_000));

        let mut clients = Vec::new();
        for _ in 0..20 {
            let s = sampler.clone();
            clients.push(std::thread::spawn(move || {
                s.sample_for(&names(&["gpu"]), Duration::from_secs(5))
                    .map(|v| v.len())
            }));
        }
        for c in clients {
            assert_eq!(c.join().unwrap(), Ok(1));
        }
        assert_eq!(source.count("gpu"), 1, "twenty requests, one pass");
    }

    /// Passes never overlap, even when several segments are wanted together.
    #[test]
    fn passes_never_run_two_at_once() {
        let source = Counting::new(Duration::from_millis(30));
        let sampler = Sampler::new(source.clone(), timing(60_000, 60_000));

        let all = names(&["cpu", "memory", "disk", "network", "gpu"]);
        let got = sampler.sample_for(&all, Duration::from_secs(10));
        assert_eq!(got.map(|v| v.len()), Ok(5));
        assert_eq!(source.max_in_flight.load(Ordering::SeqCst), 1);
    }

    /// A request for a segment still being sampled waits only its bound, and reports
    /// the segment as missing rather than blocking; a later request then gets it.
    #[test]
    fn a_slow_segment_answers_missing_within_the_bound_then_present() {
        let source = Counting::new(Duration::from_millis(400));
        let sampler = Sampler::new(source.clone(), timing(60_000, 60_000));

        let started = Instant::now();
        let first = sampler.sample_for(&names(&["inference"]), Duration::from_millis(100));
        assert_eq!(first.map(|_| ()), Err(names(&["inference"])));
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "the first request must not wait for the pass"
        );

        let second = sampler.sample_for(&names(&["inference"]), Duration::from_secs(5));
        assert!(second.is_ok(), "the sample arrives in the background");
    }

    /// Status reports the last sample time for segments that have one and nothing for
    /// those that have never been read.
    #[test]
    fn status_reports_last_sampled_per_segment() {
        let source = Counting::new(Duration::from_millis(1));
        let sampler = Sampler::new(source, timing(60_000, 60_000));
        sampler
            .sample_for(&names(&["cpu"]), Duration::from_secs(2))
            .unwrap();

        let status = sampler.status();
        let cpu = status.segments.iter().find(|s| s.name == "cpu").unwrap();
        assert!(cpu.last_sampled.is_some());
        let gpu = status.segments.iter().find(|s| s.name == "gpu").unwrap();
        assert!(gpu.last_sampled.is_none());
    }
}
