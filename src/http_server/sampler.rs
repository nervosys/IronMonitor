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
//!   are serialised by one lock: `resolve::sample_tier` takes
//!   `crate::pipeline::reader_pass`, as does the collector's own pass.
//! - A pass reads one [`Tier`] of one segment with [`crate::ontology::resolve::sample_tier`].
//!   Live readers are cheap and change constantly. Inventory readers are where the time
//!   goes, and mostly describe what the machine is. Reading them separately is what
//!   keeps a segment's counters moving while its slow readers run.
//! - A route marks the segments it reads as wanted. A segment that nobody has
//!   wanted within the idle period is not sampled. When no segment is wanted the
//!   thread blocks on a condition variable and does no work at all.
//! - A wanted segment's live tier is resampled once it is older than the resample
//!   interval, and its inventory tier once it is older than the inventory interval.
//!   After an inventory pass, each wanted segment's due live tier runs before the next
//!   inventory pass, so at most one inventory pass separates two live rounds. Passes are
//!   never interrupted, so a live reading can still lag by one inventory pass.
//! - Requests never run a pass themselves. They wait, for a bounded time, for a
//!   sample that the thread produces. A segment answers only once both tiers have
//!   been read, because rows about what is absent are claims about both.

use crate::ontology::resolve::{self, Reading, Tier};
use crate::ontology::Domain;
use chrono::{DateTime, SecondsFormat, Utc};
use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// What a pass reads from. Production uses the resolver; tests substitute a source
/// that counts passes and takes a chosen time.
pub(crate) trait SegmentSource: Send + Sync + 'static {
    fn sample(&self, segment: &str, tier: Tier) -> Vec<Reading>;

    /// Rows about what neither tier emitted.
    fn absences(&self, segment: &str, live: &[Reading], inventory: &[Reading]) -> Vec<Reading>;
}

/// The production source: the resolver's own tier pass.
pub(crate) struct ResolverSource;

impl SegmentSource for ResolverSource {
    fn sample(&self, segment: &str, tier: Tier) -> Vec<Reading> {
        resolve::sample_tier(segment, tier)
    }

    fn absences(&self, segment: &str, live: &[Reading], inventory: &[Reading]) -> Vec<Reading> {
        resolve::absence_rows(segment, live, inventory)
    }
}

/// How long a segment stays wanted after its last request, and how often a wanted
/// segment's tiers are resampled. Injected for tests; the server uses
/// [`Timing::PRODUCTION`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    pub idle_after: Duration,
    pub resample_every: Duration,
    pub inventory_every: Duration,
}

impl Timing {
    pub(crate) const PRODUCTION: Timing = Timing {
        idle_after: Duration::from_secs(60),
        resample_every: Duration::from_secs(2),
        inventory_every: Duration::from_secs(60),
    };
}

/// When a pass started.
///
/// A pass is stamped with its start, not its end: a reading is as old as the moment it
/// began to be taken, so it never appears fresher than it was.
pub(crate) struct Stamp {
    sampled_at: DateTime<Utc>,
    taken: Instant,
}

impl Stamp {
    /// Milliseconds since the pass started, measured when asked.
    pub(crate) fn age_ms(&self) -> u64 {
        self.taken.elapsed().as_millis() as u64
    }

    /// When the pass started, in nanoseconds since the Unix epoch, as OTLP wants it.
    pub(crate) fn sampled_at_unix_nanos(&self) -> u64 {
        self.sampled_at
            .timestamp_nanos_opt()
            .map_or(0, |n| n.max(0) as u64)
    }

    pub(crate) fn sampled_at_rfc3339(&self) -> String {
        self.sampled_at.to_rfc3339_opts(SecondsFormat::Millis, true)
    }
}

/// One tier's readings from one pass.
struct Pass {
    readings: Vec<Reading>,
    stamp: Stamp,
}

/// A segment's rows: the latest pass of each tier, and the rows about what both left out.
pub(crate) struct Sample {
    live: Arc<Pass>,
    inventory: Arc<Pass>,
    absences: Vec<Reading>,
}

impl Sample {
    /// Every row with the stamp of the pass it came from. An absence row is a claim about
    /// both tiers, so it carries the older stamp.
    pub(crate) fn rows(&self) -> impl Iterator<Item = (&Reading, &Stamp)> {
        let older = self.older();
        let live = &self.live.stamp;
        let inventory = &self.inventory.stamp;
        self.live
            .readings
            .iter()
            .map(move |r| (r, live))
            .chain(self.inventory.readings.iter().map(move |r| (r, inventory)))
            .chain(self.absences.iter().map(move |r| (r, older)))
    }

    fn older(&self) -> &Stamp {
        if self.live.stamp.taken < self.inventory.stamp.taken {
            &self.live.stamp
        } else {
            &self.inventory.stamp
        }
    }
}

struct Slot {
    wanted_at: Option<Instant>,
    live: Option<Arc<Pass>>,
    inventory: Option<Arc<Pass>>,
    sample: Option<Arc<Sample>>,
}

struct State {
    slots: BTreeMap<String, Slot>,
    started: bool,
    passes: u64,
    /// When the last inventory pass finished. A live tier older than this is owed a pass
    /// before the next inventory pass runs.
    last_inventory_end: Option<Instant>,
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
    /// When the older tier of the current sample was read and its age in milliseconds,
    /// if the segment has a sample. A segment is never reported fresher than its oldest row.
    pub last_sampled: Option<(String, u64)>,
    pub live_age_ms: Option<u64>,
    pub inventory_age_ms: Option<u64>,
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
                        live: None,
                        inventory: None,
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
                    last_inventory_end: None,
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
                last_sampled: slot.sample.as_ref().map(|s| {
                    let older = s.older();
                    (older.sampled_at_rfc3339(), older.age_ms())
                }),
                live_age_ms: slot.live.as_ref().map(|p| p.stamp.age_ms()),
                inventory_age_ms: slot.inventory.as_ref().map(|p| p.stamp.age_ms()),
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

/// Never-sampled work sorts first, in the order it was first wanted, so a cold first round
/// follows the requests rather than the alphabet; then the oldest sample.
type Priority = (bool, Option<Instant>);

/// Keep whichever of `current` and the new candidate has the lower priority key.
fn consider(current: &mut Option<(String, Priority)>, name: &str, key: Priority) {
    if current.as_ref().is_none_or(|(_, held)| key < *held) {
        *current = Some((name.to_string(), key));
    }
}

/// The next pass to run, if one is due, and otherwise when one will be.
///
/// In order: a live tier that has never been read or that predates the last inventory
/// pass, then a due inventory tier, then any other due live tier. Inventory ranks above
/// ordinary live work so that a live tier always due cannot starve it, and the first rule
/// then gives every wanted live tier its turn before the next inventory pass.
fn choose(
    state: &State,
    timing: Timing,
    now: Instant,
) -> (Option<(String, Tier)>, Option<Instant>) {
    let mut owed = None;
    let mut inventory = None;
    let mut live = None;
    let mut wake_at: Option<Instant> = None;
    let mut wake = |at: Instant| wake_at = Some(wake_at.map_or(at, |w: Instant| w.min(at)));

    for (name, slot) in &state.slots {
        let Some(wanted) = slot.wanted_at else {
            continue;
        };
        if now.duration_since(wanted) >= timing.idle_after {
            continue;
        }

        match &slot.live {
            None => consider(&mut owed, name, (false, slot.wanted_at)),
            Some(pass) => {
                let due_at = pass.stamp.taken + timing.resample_every;
                if due_at > now {
                    wake(due_at);
                } else if state
                    .last_inventory_end
                    .is_some_and(|end| pass.stamp.taken < end)
                {
                    consider(&mut owed, name, (true, Some(pass.stamp.taken)));
                } else {
                    consider(&mut live, name, (true, Some(pass.stamp.taken)));
                }
            }
        }

        match &slot.inventory {
            None => consider(&mut inventory, name, (false, slot.wanted_at)),
            Some(pass) => {
                let due_at = pass.stamp.taken + timing.inventory_every;
                if due_at > now {
                    wake(due_at);
                } else {
                    consider(&mut inventory, name, (true, Some(pass.stamp.taken)));
                }
            }
        }
    }

    let next = owed
        .map(|(name, _)| (name, Tier::Live))
        .or(inventory.map(|(name, _)| (name, Tier::Inventory)))
        .or(live.map(|(name, _)| (name, Tier::Live)));
    (next, wake_at)
}

fn run(shared: Arc<Shared>) {
    let timing = shared.timing;
    let mut state = lock(&shared);
    loop {
        let (next, wake_at) = choose(&state, timing, Instant::now());

        if let Some((name, tier)) = next {
            let stamp = Stamp {
                sampled_at: Utc::now(),
                taken: Instant::now(),
            };
            drop(state);
            // Lock released for the whole pass: requests must be able to wait on
            // the condition variable while it runs.
            let readings = shared.source.sample(&name, tier);
            let finished = Instant::now();
            state = lock(&shared);
            state.passes += 1;
            if tier == Tier::Inventory {
                state.last_inventory_end = Some(finished);
            }
            if let Some(slot) = state.slots.get_mut(&name) {
                let pass = Arc::new(Pass { readings, stamp });
                match tier {
                    Tier::Live => slot.live = Some(pass),
                    Tier::Inventory => slot.inventory = Some(pass),
                }
                if let (Some(live), Some(inventory)) = (&slot.live, &slot.inventory) {
                    let absences =
                        shared
                            .source
                            .absences(&name, &live.readings, &inventory.readings);
                    slot.sample = Some(Arc::new(Sample {
                        live: Arc::clone(live),
                        inventory: Arc::clone(inventory),
                        absences,
                    }));
                }
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
    /// largest number of passes it ever saw running at once. `count` counts live passes.
    struct Counting {
        pause: Duration,
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
        inventory_pause: Duration,
        per_segment: Mutex<BTreeMap<String, usize>>,
        log: Mutex<Vec<(String, Tier)>>,
    }

    impl Counting {
        fn new(pause: Duration) -> Arc<Self> {
            Arc::new(Self {
                pause,
                inventory_pause: pause,
                in_flight: AtomicUsize::new(0),
                max_in_flight: AtomicUsize::new(0),
                per_segment: Mutex::new(BTreeMap::new()),
                log: Mutex::new(Vec::new()),
            })
        }

        fn with_inventory_pause(pause: Duration, inventory_pause: Duration) -> Arc<Self> {
            Arc::new(Self {
                pause,
                inventory_pause,
                in_flight: AtomicUsize::new(0),
                max_in_flight: AtomicUsize::new(0),
                per_segment: Mutex::new(BTreeMap::new()),
                log: Mutex::new(Vec::new()),
            })
        }

        fn passes_logged(&self) -> Vec<(String, Tier)> {
            self.log.lock().unwrap().clone()
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
        fn sample(&self, segment: &str, tier: Tier) -> Vec<Reading> {
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_in_flight.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(match tier {
                Tier::Live => self.pause,
                Tier::Inventory => self.inventory_pause,
            });
            if tier == Tier::Live {
                *self
                    .per_segment
                    .lock()
                    .unwrap()
                    .entry(segment.to_string())
                    .or_insert(0) += 1;
            }
            self.log.lock().unwrap().push((segment.to_string(), tier));
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            vec![Reading {
                id: format!("{segment}.probe.{tier:?}"),
                value: Some(serde_json::json!(1)),
                provenance: crate::ontology::Provenance::Measured,
                unit: None,
                note: None,
            }]
        }

        fn absences(&self, segment: &str, _: &[Reading], _: &[Reading]) -> Vec<Reading> {
            vec![Reading {
                id: format!("{segment}.<none>"),
                value: None,
                provenance: crate::ontology::Provenance::Unavailable,
                unit: None,
                note: Some("nothing here".into()),
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
            inventory_every: Duration::from_millis(resample_ms),
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

    /// With both tiers always due, every inventory pass is followed by a live pass of each
    /// wanted segment before the next inventory pass. This is what keeps a segment's
    /// counters moving while slow inventory is being read.
    #[test]
    fn a_live_pass_of_every_segment_follows_each_inventory_pass() {
        let source = Counting::new(Duration::from_millis(1));
        let sampler = Sampler::new(source.clone(), timing(60_000, 0));
        sampler
            .sample_for(&names(&["cpu", "disk", "memory"]), Duration::from_secs(5))
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(10);
        while source.passes_logged().len() < 60 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let log = source.passes_logged();
        assert!(log.len() >= 60, "only {} passes ran", log.len());

        let inventories: Vec<usize> = log
            .iter()
            .enumerate()
            .filter(|(_, (_, tier))| *tier == Tier::Inventory)
            .map(|(i, _)| i)
            .collect();
        assert!(inventories.len() > 3, "inventory starved: {log:?}");
        for pair in inventories.windows(2) {
            let between: std::collections::BTreeSet<&str> = log[pair[0] + 1..pair[1]]
                .iter()
                .map(|(name, _)| name.as_str())
                .collect();
            assert_eq!(between.len(), 3, "between {pair:?}: {log:?}");
        }
    }

    /// Live rows are refreshed while an inventory sample ages, each row reports the age of
    /// its own pass, and a row about both tiers reports the older of the two.
    #[test]
    fn each_row_carries_the_age_of_its_own_tier() {
        let source = Counting::new(Duration::from_millis(1));
        let sampler = Sampler::new(
            source.clone(),
            Timing {
                idle_after: Duration::from_secs(60),
                resample_every: Duration::from_millis(1),
                inventory_every: Duration::from_secs(600),
            },
        );
        let wanted = names(&["cpu"]);
        sampler.sample_for(&wanted, Duration::from_secs(5)).unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut ages = None;
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
            let got = sampler.sample_for(&wanted, Duration::from_secs(1)).unwrap();
            let age = |id: &str| {
                got[0]
                    .1
                    .rows()
                    .find(|(r, _)| r.id == id)
                    .map(|(_, stamp)| stamp.age_ms())
                    .unwrap()
            };
            let (live, inventory, absent) = (
                age("cpu.probe.Live"),
                age("cpu.probe.Inventory"),
                age("cpu.<none>"),
            );
            if live < inventory {
                ages = Some((live, inventory, absent));
                break;
            }
        }
        let (live, inventory, absent) = ages.expect("the live rows never became fresher");
        assert!(live < inventory, "live {live}, inventory {inventory}");
        assert!(
            absent >= inventory,
            "absent {absent}, inventory {inventory}"
        );
        assert_eq!(
            source
                .passes_logged()
                .iter()
                .filter(|(_, tier)| *tier == Tier::Inventory)
                .count(),
            1,
            "inventory is not resampled inside its interval"
        );
    }

    /// A slow inventory reader delays a request's first answer, never its later ones.
    #[test]
    fn a_slow_inventory_does_not_hold_back_later_live_samples() {
        let source =
            Counting::with_inventory_pause(Duration::from_millis(1), Duration::from_millis(50));
        let sampler = Sampler::new(
            source.clone(),
            Timing {
                idle_after: Duration::from_secs(60),
                resample_every: Duration::from_millis(1),
                inventory_every: Duration::from_secs(600),
            },
        );
        sampler
            .sample_for(&names(&["cpu"]), Duration::from_secs(5))
            .unwrap();
        let after_first = source.count("cpu");
        let deadline = Instant::now() + Duration::from_secs(5);
        while source.count("cpu") < after_first + 10 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(source.count("cpu") >= after_first + 10);
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
