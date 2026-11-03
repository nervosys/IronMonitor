//! Inference engines as the Inference tab sees them.
//!
//! This file is the provider-neutral core: the sample shape every adapter
//! produces, the [`InferenceSource`] trait an adapter implements, the rate and
//! history sampler that works on any named `_total` counter, and the poller that
//! keeps one [`SourceState`] per source. Provider-specific detail (IronWorks's
//! expert-cache tiers, prefetch and per-GPU memory) lives in that provider's own
//! submodule and reaches the core only as an optional section of the sample.
//!
//! The rules are the ontology's. A value an engine did not report is absent from
//! the sample, never zero. A rate needs two samples. An engine's own tok/s figure
//! is kept apart from anything ironmon derives.

pub mod ironworks;

use serde::Deserialize;
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Points kept per history series. At one poll a second, about two minutes.
pub const HISTORY_CAPACITY: usize = 120;

/// Request deadline for each background poll.
pub const POLL_TIMEOUT: Duration = Duration::from_millis(1000);

/// The inference engines this crate knows how to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProviderKind {
    IronWorks,
}

impl ProviderKind {
    pub const ALL: &'static [ProviderKind] = &[ProviderKind::IronWorks];

    /// The provider segment of its ontology ids, such as `inference.ironworks.*`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IronWorks => "ironworks",
        }
    }
}

/// Whether an engine runs on this machine or somewhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locality {
    Local,
    Remote,
}

/// Why a source could not give a sample. Each variant is a reason an agent can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// This build cannot make the request at all.
    NotSupported(String),
    /// Nothing answered: refused, timed out, or the connection failed.
    Unreachable(String),
    /// Something answered with a status other than 200.
    Status(u16),
    /// The body was not JSON.
    NotJson(String),
    /// The body was JSON, but not one this adapter reads.
    Rejected(String),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSupported(why) | Self::Unreachable(why) | Self::Rejected(why) => {
                f.write_str(why)
            }
            Self::Status(code) => write!(f, "HTTP {code}"),
            Self::NotJson(why) => write!(f, "the body is not JSON: {why}"),
        }
    }
}

impl std::error::Error for SourceError {}

/// A figure an engine reports together with the window it was averaged over.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub struct Windowed {
    pub value: Option<f64>,
    pub window_ms: Option<u64>,
}

/// What an engine says it is running. Each field is absent when not reported.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Identity {
    pub model: Option<String>,
    pub arch: Option<String>,
    pub backend: Option<String>,
}

/// Token throughput as the engine itself computes it. Shown as the engine's
/// figure, never substituted for a rate ironmon derives.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EngineReported {
    pub output_tok_s: Option<Windowed>,
    pub prompt_tok_s: Option<Windowed>,
}

/// One poll's worth of an engine, in the shape every provider maps onto.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProviderSample {
    /// The engine's monotonic clock at the poll, in nanoseconds.
    pub sampled_at_ns: Option<u64>,
    pub uptime_ns: Option<u64>,
    pub identity: Identity,
    /// Cumulative counters, by dotted name, for the names the engine reported.
    /// A name missing here was not reported this poll, which is not zero. Only
    /// `_total` counters belong here; gauges go in the provider section.
    pub counters: BTreeMap<String, u64>,
    pub engine_reported: EngineReported,
    /// Provider-specific detail. Only the provider that owns it fills it in.
    pub ironworks: Option<ironworks::Section>,
}

/// One inference engine that can be polled.
pub trait InferenceSource: Send + Sync {
    fn kind(&self) -> ProviderKind;
    /// Where the engine is reached, for display and for the ontology's reasons.
    fn endpoint(&self) -> String;
    fn locality(&self) -> Locality;
    /// One blocking poll that gives up after `timeout`.
    fn poll(&self, timeout: Duration) -> Result<ProviderSample, SourceError>;
}

/// A derived value at one moment: the window it covers and the figure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub at_ns: u64,
    pub value: f64,
}

/// A bounded run of derived points, oldest first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Series {
    points: VecDeque<Point>,
}

impl Series {
    fn push(&mut self, point: Point) {
        if self.points.len() == HISTORY_CAPACITY {
            self.points.pop_front();
        }
        self.points.push_back(point);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Point> {
        self.points.iter()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn latest(&self) -> Option<&Point> {
        self.points.back()
    }
}

/// Values derived from the two most recent samples. A value absent from a map is
/// `None`: it could not be derived this time.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Derived {
    /// Nanoseconds between the two samples, when the clock advanced.
    pub window_ns: Option<u64>,
    /// Per-second rate for each counter present in both samples, by counter name.
    pub rates: BTreeMap<String, f64>,
    /// Hit ratio for each `<prefix>.hit_total` / `<prefix>.miss_total` pair, keyed
    /// by `<prefix>`. Absent when the pair gained nothing in the window.
    pub ratios: BTreeMap<String, f64>,
}

impl Derived {
    pub fn rate(&self, counter: &str) -> Option<f64> {
        self.rates.get(counter).copied()
    }

    pub fn ratio(&self, prefix: &str) -> Option<f64> {
        self.ratios.get(prefix).copied()
    }
}

/// Turns successive samples into rates and hit ratios, and keeps history.
///
/// Works on any source's named counters. A counter going backwards, or the
/// engine's clock going backwards, means the engine restarted: the sampler drops
/// its previous sample, so the new one starts a fresh window and yields nothing.
#[derive(Debug, Default)]
pub struct InferenceSampler {
    previous: Option<ProviderSample>,
    derived: Derived,
    history: BTreeMap<String, Series>,
}

impl InferenceSampler {
    pub fn update(&mut self, sample: ProviderSample) {
        self.derived = match &self.previous {
            Some(prev) if !regressed(prev, &sample) => derive(prev, &sample),
            _ => Derived::default(),
        };
        if let Some(at_ns) = sample.sampled_at_ns {
            let history = &mut self.history;
            for (name, value) in &self.derived.rates {
                push_point(history, format!("rate:{name}"), at_ns, *value);
            }
            for (prefix, value) in &self.derived.ratios {
                push_point(history, format!("ratio:{prefix}"), at_ns, *value);
            }
        }
        self.previous = Some(sample);
    }

    /// Values from the latest two samples.
    pub fn derived(&self) -> &Derived {
        &self.derived
    }

    /// History series by key: `rate:<counter>` and `ratio:<prefix>`.
    pub fn history(&self) -> &BTreeMap<String, Series> {
        &self.history
    }
}

fn push_point(history: &mut BTreeMap<String, Series>, key: String, at_ns: u64, value: f64) {
    history.entry(key).or_default().push(Point { at_ns, value });
}

fn regressed(prev: &ProviderSample, cur: &ProviderSample) -> bool {
    if let (Some(a), Some(b)) = (prev.sampled_at_ns, cur.sampled_at_ns) {
        if b < a {
            return true;
        }
    }
    // A falling uptime is a restart even when no counter has yet fallen below its
    // old value, and differencing across it would report a rate nothing produced.
    if let (Some(a), Some(b)) = (prev.uptime_ns, cur.uptime_ns) {
        if b < a {
            return true;
        }
    }
    prev.counters
        .iter()
        .any(|(name, before)| cur.counters.get(name).is_some_and(|now| now < before))
}

fn derive(prev: &ProviderSample, cur: &ProviderSample) -> Derived {
    let window_ns = match (prev.sampled_at_ns, cur.sampled_at_ns) {
        (Some(a), Some(b)) if b > a => Some(b - a),
        _ => None,
    };
    let mut out = Derived {
        window_ns,
        ..Derived::default()
    };
    let Some(window) = window_ns else {
        return out;
    };
    let seconds = window as f64 / 1e9;

    let delta = |name: &str| -> Option<u64> {
        let before = prev.counters.get(name)?;
        let now = cur.counters.get(name)?;
        Some(now - before)
    };

    for name in cur.counters.keys() {
        if let Some(d) = delta(name) {
            out.rates.insert(name.clone(), d as f64 / seconds);
        }
    }

    for name in cur.counters.keys() {
        let Some(prefix) = name.strip_suffix(".hit_total") else {
            continue;
        };
        let miss = format!("{prefix}.miss_total");
        let (Some(hits), Some(misses)) = (delta(name), delta(&miss)) else {
            continue;
        };
        let total = hits + misses;
        if total > 0 {
            out.ratios
                .insert(prefix.to_string(), hits as f64 / total as f64);
        }
    }
    out
}

/// What the poller last knew about one source.
#[derive(Debug, Clone)]
pub struct SourceState {
    pub kind: ProviderKind,
    pub locality: Locality,
    pub endpoint: String,
    /// The latest sample that was read successfully. Check `reachable` before
    /// trusting it as current.
    pub latest: Option<ProviderSample>,
    /// Whether the most recent poll produced a sample.
    pub reachable: bool,
    /// When the most recent successful poll happened.
    pub last_success: Option<Instant>,
    /// The most recent failure and when it happened.
    pub last_error: Option<Failure>,
    pub derived: Derived,
    pub history: BTreeMap<String, Series>,
}

/// A failed poll and the moment it failed.
#[derive(Debug, Clone)]
pub struct Failure {
    pub error: SourceError,
    pub at: Instant,
}

impl SourceState {
    fn new(source: &dyn InferenceSource) -> Self {
        Self {
            kind: source.kind(),
            locality: source.locality(),
            endpoint: source.endpoint(),
            latest: None,
            reachable: false,
            last_success: None,
            last_error: None,
            derived: Derived::default(),
            history: BTreeMap::new(),
        }
    }
}

/// Polls a set of sources on background threads, one thread and one state each.
///
/// Dropping the poller stops every thread: each one waits on a channel whose
/// sender the poller holds, so the wait ends as soon as the sender is dropped.
/// A poll already in flight finishes first, bounded by [`POLL_TIMEOUT`]. Drop
/// never joins, so it never blocks a UI thread.
pub struct InferencePoller {
    slots: Vec<Slot>,
}

struct Slot {
    shared: Arc<Mutex<SourceState>>,
    _stop: Sender<()>,
}

impl InferencePoller {
    /// Start polling every source about once per `interval`.
    pub fn spawn(sources: Vec<Box<dyn InferenceSource>>, interval: Duration) -> Self {
        let slots = sources
            .into_iter()
            .map(|source| {
                let shared = Arc::new(Mutex::new(SourceState::new(source.as_ref())));
                let (stop_tx, stop_rx) = mpsc::channel();
                let thread_shared = Arc::clone(&shared);
                let name = format!("inference-{}", source.kind().as_str());
                let spawned = std::thread::Builder::new()
                    .name(name)
                    .spawn(move || poll_loop(source, interval, thread_shared, stop_rx));
                if let Err(e) = spawned {
                    let mut st = lock(&shared);
                    st.last_error = Some(Failure {
                        error: SourceError::Unreachable(format!(
                            "could not start a poller thread: {e}"
                        )),
                        at: Instant::now(),
                    });
                }
                Slot {
                    shared,
                    _stop: stop_tx,
                }
            })
            .collect();
        Self { slots }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// A copy of one source's state, or `None` for an index out of range.
    pub fn state(&self, index: usize) -> Option<SourceState> {
        self.slots.get(index).map(|s| lock(&s.shared).clone())
    }

    /// Copies of every source's state, in the order the sources were given.
    pub fn states(&self) -> Vec<SourceState> {
        self.slots.iter().map(|s| lock(&s.shared).clone()).collect()
    }
}

fn poll_loop(
    source: Box<dyn InferenceSource>,
    interval: Duration,
    shared: Arc<Mutex<SourceState>>,
    stop: Receiver<()>,
) {
    let mut sampler = InferenceSampler::default();
    loop {
        let outcome = source.poll(POLL_TIMEOUT);
        let at = Instant::now();
        {
            let mut st = lock(&shared);
            match outcome {
                Ok(sample) => {
                    sampler.update(sample.clone());
                    st.latest = Some(sample);
                    st.derived = sampler.derived().clone();
                    st.history = sampler.history().clone();
                    st.reachable = true;
                    st.last_success = Some(at);
                }
                Err(error) => {
                    st.reachable = false;
                    st.last_error = Some(Failure { error, at });
                }
            }
        }
        match stop.recv_timeout(interval) {
            Err(RecvTimeoutError::Timeout) => {}
            // The poller was dropped (or sent a stop, which nothing does).
            _ => return,
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at: Option<u64>, counters: &[(&str, u64)]) -> ProviderSample {
        ProviderSample {
            sampled_at_ns: at,
            counters: counters.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            ..ProviderSample::default()
        }
    }

    const SEC: u64 = 1_000_000_000;

    #[test]
    fn first_sample_gives_no_rates() {
        let mut s = InferenceSampler::default();
        s.update(sample(Some(SEC), &[("requests.total", 5)]));
        assert!(s.derived().rates.is_empty());
        assert!(s.derived().ratios.is_empty());
        assert_eq!(s.derived().rate("requests.total"), None);
    }

    #[test]
    fn two_samples_give_rates_and_ratio() {
        let mut s = InferenceSampler::default();
        s.update(sample(
            Some(SEC),
            &[
                ("requests.total", 10),
                ("tier.vram.hit_total", 100),
                ("tier.vram.miss_total", 20),
            ],
        ));
        s.update(sample(
            Some(3 * SEC),
            &[
                ("requests.total", 14),
                ("tier.vram.hit_total", 130),
                ("tier.vram.miss_total", 30),
            ],
        ));
        let d = s.derived();
        assert_eq!(d.window_ns, Some(2 * SEC));
        assert_eq!(d.rate("requests.total"), Some(2.0));
        assert_eq!(d.rate("tier.vram.hit_total"), Some(15.0));
        // 30 hits and 10 misses in the window: 0.75.
        assert_eq!(d.ratio("tier.vram"), Some(0.75));
    }

    #[test]
    fn counter_going_backwards_resets_the_sampler() {
        let mut s = InferenceSampler::default();
        s.update(sample(Some(SEC), &[("requests.total", 50)]));
        s.update(sample(Some(2 * SEC), &[("requests.total", 60)]));
        assert_eq!(s.derived().rate("requests.total"), Some(10.0));
        // Engine restarted: the counter fell.
        s.update(sample(Some(3 * SEC), &[("requests.total", 2)]));
        assert!(s.derived().rates.is_empty());
        // The restarted sample is the new baseline.
        s.update(sample(Some(4 * SEC), &[("requests.total", 7)]));
        assert_eq!(s.derived().rate("requests.total"), Some(5.0));
    }

    #[test]
    fn clock_going_backwards_resets_the_sampler() {
        let mut s = InferenceSampler::default();
        s.update(sample(Some(5 * SEC), &[("requests.total", 1)]));
        s.update(sample(Some(2 * SEC), &[("requests.total", 9)]));
        assert!(s.derived().rates.is_empty());
        assert_eq!(s.derived().window_ns, None);
    }

    #[test]
    fn missing_counter_gives_no_rate() {
        let mut s = InferenceSampler::default();
        s.update(sample(Some(SEC), &[("requests.total", 1)]));
        s.update(sample(Some(2 * SEC), &[("tier.nvme.read_total", 4)]));
        assert_eq!(s.derived().rate("requests.total"), None);
        assert_eq!(s.derived().rate("tier.nvme.read_total"), None);
    }

    #[test]
    fn non_advancing_clock_gives_no_rate() {
        let mut s = InferenceSampler::default();
        s.update(sample(Some(SEC), &[("requests.total", 1)]));
        s.update(sample(Some(SEC), &[("requests.total", 4)]));
        assert!(s.derived().rates.is_empty());
        assert_eq!(s.derived().window_ns, None);
    }

    #[test]
    fn missing_clock_gives_no_rate() {
        let mut s = InferenceSampler::default();
        s.update(sample(None, &[("requests.total", 1)]));
        s.update(sample(Some(2 * SEC), &[("requests.total", 4)]));
        assert!(s.derived().rates.is_empty());
    }

    #[test]
    fn ratio_is_none_when_pair_gained_nothing() {
        let mut s = InferenceSampler::default();
        let pair = [("tier.ram.hit_total", 5), ("tier.ram.miss_total", 5)];
        s.update(sample(Some(SEC), &pair));
        s.update(sample(Some(2 * SEC), &pair));
        assert_eq!(s.derived().ratio("tier.ram"), None);
        assert_eq!(s.derived().rate("tier.ram.hit_total"), Some(0.0));
    }

    #[test]
    fn history_skips_underived_points() {
        let mut s = InferenceSampler::default();
        let hit = |h: u64, m: u64| [("tier.vram.hit_total", h), ("tier.vram.miss_total", m)];
        s.update(sample(Some(SEC), &hit(0, 0)));
        // No previous sample for this pair's rate yet: nothing recorded.
        assert!(s.history().is_empty());
        s.update(sample(Some(2 * SEC), &hit(3, 1)));
        assert_eq!(s.history()["ratio:tier.vram"].len(), 1);
        // A clock that did not advance derives nothing, so nothing is pushed.
        s.update(sample(Some(2 * SEC), &hit(3, 1)));
        assert_eq!(s.history()["ratio:tier.vram"].len(), 1);
        // Nothing gained in the window: no ratio point either.
        s.update(sample(Some(3 * SEC), &hit(3, 1)));
        assert_eq!(s.history()["ratio:tier.vram"].len(), 1);
        s.update(sample(Some(4 * SEC), &hit(5, 1)));
        let series = &s.history()["ratio:tier.vram"];
        assert_eq!(series.len(), 2);
        assert_eq!(series.latest().map(|p| p.value), Some(1.0));
    }

    #[test]
    fn series_is_bounded() {
        let mut s = InferenceSampler::default();
        for i in 0..(HISTORY_CAPACITY as u64 + 10) {
            s.update(sample(Some((i + 1) * SEC), &[("requests.total", i * 2)]));
        }
        let series = &s.history()["rate:requests.total"];
        assert_eq!(series.len(), HISTORY_CAPACITY);
        assert_eq!(series.iter().next().map(|p| p.at_ns), Some(11 * SEC));
    }

    #[test]
    fn source_error_display_names_the_cause() {
        assert_eq!(SourceError::Status(500).to_string(), "HTTP 500");
        assert_eq!(
            SourceError::Unreachable("connection refused".into()).to_string(),
            "connection refused"
        );
    }
}

#[cfg(test)]
mod poller_tests {
    use super::*;
    use crate::inference::ironworks::IronWorksSource;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    const A_BODY: &str = include_str!("../../tests/fixtures/inference/a_nvme_configured.json");

    /// A tiny HTTP stub. Each connection gets one reply and is then closed. Dropping
    /// the stub stops the accept loop and releases the port, so later connects are
    /// refused.
    struct Stub {
        port: u16,
        reply: Arc<Mutex<(u16, String)>>,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Stub {
        fn start(status: u16, body: &str) -> Stub {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
            listener.set_nonblocking(true).expect("nonblocking");
            let port = listener.local_addr().expect("addr").port();
            let reply = Arc::new(Mutex::new((status, body.to_string())));
            let stop = Arc::new(AtomicBool::new(false));
            let (r, s) = (Arc::clone(&reply), Arc::clone(&stop));
            let thread = std::thread::spawn(move || {
                while !s.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut conn, _)) => {
                            let _ = conn.set_nonblocking(false);
                            let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
                            let mut buf = [0u8; 4096];
                            let _ = conn.read(&mut buf);
                            let (status, body) = r.lock().expect("reply lock").clone();
                            let reason = if status == 200 { "OK" } else { "Error" };
                            let response = format!(
                                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                body.len()
                            );
                            let _ = conn.write_all(response.as_bytes());
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(5)),
                    }
                }
            });
            Stub {
                port,
                reply,
                stop,
                thread: Some(thread),
            }
        }

        fn url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }

        #[allow(dead_code)]
        fn set_reply(&self, status: u16, body: &str) {
            *self.reply.lock().expect("reply lock") = (status, body.to_string());
        }

        fn stop(mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    fn wait_until(limit: Duration, mut cond: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + limit;
        while Instant::now() < end {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        cond()
    }

    fn poller_for(url: &str) -> InferencePoller {
        let sources: Vec<Box<dyn InferenceSource>> = vec![Box::new(IronWorksSource::new(url))];
        InferencePoller::spawn(sources, Duration::from_millis(50))
    }

    #[test]
    fn poller_reports_a_value_from_the_stub() {
        let stub = Stub::start(200, A_BODY);
        let poller = poller_for(&stub.url());
        let ok = wait_until(Duration::from_secs(10), || {
            poller
                .state(0)
                .is_some_and(|s| s.reachable && s.latest.is_some())
        });
        assert!(ok, "poller never reported a value");
        let st = poller.state(0).expect("state");
        assert_eq!(st.kind, ProviderKind::IronWorks);
        assert_eq!(st.locality, Locality::Local);
        assert!(st.last_success.is_some());
        let latest = st.latest.expect("latest sample");
        assert_eq!(latest.identity.model.as_deref(), Some("glm-5.3-flash"));
        drop(poller);
        stub.stop();
    }

    #[test]
    fn poller_reports_a_500_with_its_status() {
        let stub = Stub::start(500, "internal failure");
        let poller = poller_for(&stub.url());
        let ok = wait_until(Duration::from_secs(10), || {
            poller.state(0).is_some_and(|s| s.last_error.is_some())
        });
        assert!(ok, "poller never recorded the error");
        let st = poller.state(0).expect("state");
        assert!(!st.reachable);
        assert!(st.latest.is_none());
        let failure = st.last_error.expect("error");
        assert_eq!(failure.error, SourceError::Status(500));
        drop(poller);
        stub.stop();
    }

    #[test]
    fn poller_reports_unreachable_after_the_stub_stops() {
        let stub = Stub::start(200, A_BODY);
        let poller = poller_for(&stub.url());
        assert!(wait_until(Duration::from_secs(10), || {
            poller.state(0).is_some_and(|s| s.reachable)
        }));
        stub.stop();
        let gone = wait_until(Duration::from_secs(10), || {
            poller.state(0).is_some_and(|s| {
                !s.reachable
                    && matches!(
                        s.last_error.as_ref().map(|f| &f.error),
                        Some(SourceError::Unreachable(_))
                    )
            })
        });
        assert!(gone, "poller did not report the engine unreachable");
        drop(poller);
    }

    #[test]
    fn dropping_the_poller_ends_its_thread() {
        let stub = Stub::start(200, A_BODY);
        let poller = poller_for(&stub.url());
        assert!(wait_until(Duration::from_secs(10), || {
            poller.state(0).is_some_and(|s| s.reachable)
        }));
        // The poller and its thread each hold one reference to the state.
        let weak = Arc::downgrade(&poller.slots[0].shared);
        drop(poller);
        let exited = wait_until(Duration::from_secs(5), || weak.strong_count() == 0);
        assert!(exited, "the poller thread kept its state alive after drop");
        stub.stop();
    }
}
