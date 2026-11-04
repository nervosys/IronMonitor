//! Inference engines as the Inference tab sees them.
//!
//! This file is the provider-neutral core: the sample shape every adapter
//! produces, the [`InferenceSource`] trait an adapter implements, the rate and
//! ratio sampler that works on any named `_total` counter, and the poller that
//! keeps one [`SourceState`] per source. Provider-specific detail (IronWorks's
//! expert-cache tiers, prefetch and per-GPU memory) lives in that provider's own
//! submodule and reaches the core only as an optional section of the sample.
//!
//! The rules are the ontology's. A value an engine did not report is absent from
//! the sample, never zero. A rate needs two samples. Rates and ratios are taken
//! over the engine's monotonic uptime, because the wall clock can jump.

pub mod http;
pub mod ironworks;
pub mod local;

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Points kept per history series. At one poll a second, about two minutes.
pub const HISTORY_CAPACITY: usize = 120;

/// Request deadline for each background poll.
pub const POLL_TIMEOUT: Duration = Duration::from_millis(1000);

/// What an operator is told when an engine runs but its metrics endpoint is off.
pub const METRICS_NOT_ENABLED: &str =
    "engine running, metrics not enabled — start it with --metrics";

/// The inference engines this crate knows how to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProviderKind {
    IronWorks,
    Ollama,
    LlamaCpp,
    Vllm,
    LmStudio,
    MlxLm,
}

impl ProviderKind {
    pub const ALL: &'static [ProviderKind] = &[
        ProviderKind::IronWorks,
        ProviderKind::Ollama,
        ProviderKind::LlamaCpp,
        ProviderKind::Vllm,
        ProviderKind::LmStudio,
        ProviderKind::MlxLm,
    ];

    /// The provider segment of its ontology ids, such as `inference.ironworks.*`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IronWorks => "ironworks",
            Self::Ollama => "ollama",
            Self::LlamaCpp => "llamacpp",
            Self::Vllm => "vllm",
            Self::LmStudio => "lmstudio",
            Self::MlxLm => "mlx",
        }
    }

    /// The name an operator reads on screen.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::IronWorks => "IronWorks",
            Self::Ollama => "Ollama",
            Self::LlamaCpp => "llama.cpp",
            Self::Vllm => "vLLM",
            Self::LmStudio => "LM Studio",
            Self::MlxLm => "MLX-LM",
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
    /// Something answered with a status other than 200, and not with the body an
    /// engine sends when its metrics are off.
    Status(u16),
    /// The engine answered 404 with the body it sends when started without
    /// `--metrics`. The engine is running; its metrics are not enabled.
    MetricsNotEnabled,
    /// The body was not JSON.
    NotJson(String),
    /// The body was JSON, but not one this adapter reads.
    Rejected(String),
}

impl SourceError {
    /// Whether something answered at the endpoint. A status, a body, or the
    /// metrics-off reply all mean an engine is there, though it gave no sample.
    pub fn engine_answered(&self) -> bool {
        matches!(
            self,
            Self::Status(_) | Self::MetricsNotEnabled | Self::NotJson(_) | Self::Rejected(_)
        )
    }

    /// The sentence an operator or agent reads about why there is no sample,
    /// naming the endpoint. The three surfaces and the ontology all use this text,
    /// so a state reads the same wherever it is shown.
    pub fn summary(&self, endpoint: &str) -> String {
        match self {
            Self::MetricsNotEnabled => METRICS_NOT_ENABLED.to_string(),
            Self::Unreachable(why) => format!("no engine answered at {endpoint}: {why}"),
            Self::NotSupported(why) => {
                format!("the metrics at {endpoint} cannot be read in this build: {why}")
            }
            Self::Status(code) => {
                format!("{endpoint} answered HTTP {code}, not IronWorks metrics")
            }
            Self::NotJson(why) => format!("{endpoint} did not return JSON: {why}"),
            Self::Rejected(why) => {
                format!("{endpoint} did not return IronWorks metrics this build reads: {why}")
            }
        }
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MetricsNotEnabled => f.write_str(METRICS_NOT_ENABLED),
            Self::NotSupported(why) | Self::Unreachable(why) | Self::Rejected(why) => {
                f.write_str(why)
            }
            Self::Status(code) => write!(f, "HTTP {code}"),
            Self::NotJson(why) => write!(f, "the body is not JSON: {why}"),
        }
    }
}

impl std::error::Error for SourceError {}

/// What an engine says it is running. Each field is absent when not reported.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Identity {
    pub model: Option<String>,
    pub backend: Option<String>,
    pub version: Option<String>,
    /// The health status an engine reports about itself, such as `ok`.
    pub status: Option<String>,
}

/// A hit ratio, defined by a numerator counter over the sum of a named list of
/// denominator counters. The list is written out per provider so each ratio's
/// formula is visible in the code and in the ontology's description.
///
/// A ratio is unavailable, not zero, when any counter it names is missing from
/// either sample, and when the denominator gained nothing in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RatioDef {
    /// The key the ratio is published under, such as `tier.vram`.
    pub key: &'static str,
    pub numerator: &'static str,
    /// Counters whose window deltas are summed to form the denominator. A
    /// numerator that is also a denominator term is listed here too.
    pub denominator: &'static [&'static str],
}

/// One poll's worth of an engine, in the shape every provider maps onto.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProviderSample {
    /// The engine's wall clock at the poll, in nanoseconds since the UNIX epoch.
    /// It is shown for display only. It can jump, so no rate is taken over it.
    pub sampled_at_ns: Option<u64>,
    /// The engine's monotonic clock at the poll, in nanoseconds since its start.
    /// Rates and ratios are taken over its deltas. A fall means the engine restarted.
    pub uptime_ns: Option<u64>,
    pub identity: Identity,
    /// Cumulative counters, by dotted name, for the names the engine reported.
    /// A name missing here was not reported this poll, which is not zero. Only
    /// `_total` counters belong here; gauges and peaks go in the provider section.
    pub counters: BTreeMap<String, u64>,
    /// Current values the engine reported, by name. Gauges are instantaneous
    /// figures, not cumulative, so they never produce a rate. A name missing here
    /// was not reported this poll, which is not zero.
    pub gauges: BTreeMap<String, f64>,
    /// The ratios this provider publishes, each with its named counters.
    pub ratio_defs: &'static [RatioDef],
    /// Provider-specific detail. Only the provider that owns it fills it in.
    pub ironworks: Option<ironworks::Section>,
}

/// Longest wait a source that does not answer backs off to, for local sources.
pub const BACKOFF_CAP_LOCAL: Duration = Duration::from_secs(10);

/// One inference engine that can be polled.
pub trait InferenceSource: Send + Sync {
    fn kind(&self) -> ProviderKind;
    /// Where the engine is reached, for display and for the ontology's reasons.
    fn endpoint(&self) -> String;
    fn locality(&self) -> Locality;
    /// One blocking poll that gives up after `poll_timeout()`.
    fn poll(&self, timeout: Duration) -> Result<ProviderSample, SourceError>;
    /// The deadline the poller passes to `poll`.
    fn poll_timeout(&self) -> Duration {
        POLL_TIMEOUT
    }
    /// The least time between polls, whatever the poller's own interval. A remote
    /// source that bills per request sets this high.
    fn min_interval(&self) -> Duration {
        Duration::ZERO
    }
    /// The longest wait a run of failed polls backs off to.
    fn backoff_cap(&self) -> Duration {
        BACKOFF_CAP_LOCAL
    }
}

/// The wait before the next poll: the base interval after a success, doubling
/// after each consecutive failure, never beyond `cap` (or the base, if larger).
pub fn next_wait(base: Duration, cap: Duration, failures: u32) -> Duration {
    if failures <= 1 {
        return base;
    }
    let shift = (failures - 1).min(16);
    base.saturating_mul(1u32 << shift).min(cap.max(base))
}

/// One provider's line in a compact group: a state word and its detail. A
/// provider that is not running says so with its reason; it is never shown as
/// idle, and nothing it did not report is shown as zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactRow {
    pub status: &'static str,
    pub detail: String,
}

/// The compact row for a provider the tabs do not draw in full.
pub fn compact_row(state: &SourceState) -> CompactRow {
    if let (true, Some(sample)) = (state.reachable, state.latest.as_ref()) {
        return CompactRow {
            status: "running",
            detail: sample_detail(sample),
        };
    }
    match state.last_error.as_ref() {
        None => CompactRow {
            status: "waiting",
            detail: "the first poll has not answered yet".into(),
        },
        Some(failure) => {
            let status = match &failure.error {
                SourceError::Unreachable(_) => "not running",
                _ => "not answering",
            };
            CompactRow {
                status,
                detail: local::reason(state.kind, &state.endpoint, &failure.error),
            }
        }
    }
}

/// What a sample reports, as short phrases. Only values the provider reported
/// appear, so a missing one is absent rather than shown as zero.
fn sample_detail(sample: &ProviderSample) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(model) = &sample.identity.model {
        parts.push(format!("model {model}"));
    }
    if let Some(version) = &sample.identity.version {
        parts.push(format!("version {version}"));
    }
    if let Some(status) = &sample.identity.status {
        parts.push(format!("health {status}"));
    }
    if let Some(n) = sample.gauges.get("models_loaded") {
        parts.push(format!("{} loaded", *n as u64));
    }
    if let Some(n) = sample.gauges.get("num_requests_running") {
        parts.push(format!("{} running", *n as u64));
    }
    if let Some(n) = sample.gauges.get("num_requests_waiting") {
        parts.push(format!("{} waiting", *n as u64));
    }
    if let Some(n) = sample.gauges.get("requests_processing") {
        parts.push(format!("{} processing", *n as u64));
    }
    if let Some(p) = sample.gauges.get("kv_cache_usage_perc") {
        parts.push(format!("KV cache {:.0}%", p * 100.0));
    }
    if let Some(n) = sample.counters.get("prompt_tokens_total") {
        parts.push(format!("{n} prompt tokens"));
    }
    if let Some(n) = sample.counters.get("generation_tokens_total") {
        parts.push(format!("{n} generated"));
    }
    if let Some(n) = sample.counters.get("tokens_predicted_total") {
        parts.push(format!("{n} generated"));
    }
    if parts.is_empty() {
        "running; the server reported none of the values this tab reads".into()
    } else {
        parts.join(" · ")
    }
}

/// A derived value at one moment: the engine uptime it was taken at and the figure.
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
    /// Engine uptime between the two samples, when it advanced.
    pub window_ns: Option<u64>,
    /// Per-second rate for each counter present in both samples, by counter name.
    pub rates: BTreeMap<String, f64>,
    /// Ratio for each [`RatioDef`] whose counters were present and whose
    /// denominator gained something in the window, by the def's key.
    pub ratios: BTreeMap<String, f64>,
}

impl Derived {
    pub fn rate(&self, counter: &str) -> Option<f64> {
        self.rates.get(counter).copied()
    }

    pub fn ratio(&self, key: &str) -> Option<f64> {
        self.ratios.get(key).copied()
    }
}

/// Turns successive samples into rates and ratios, and keeps history.
///
/// Works on any source's named counters. A counter going backwards, or the
/// engine's uptime going backwards, means the engine restarted: the sampler drops
/// its previous sample, so the new one starts a fresh window and yields nothing.
/// The wall clock is not consulted for windows at all.
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
        if let Some(at_ns) = sample.uptime_ns {
            let history = &mut self.history;
            for (name, value) in &self.derived.rates {
                push_point(history, format!("rate:{name}"), at_ns, *value);
            }
            for (key, value) in &self.derived.ratios {
                push_point(history, format!("ratio:{key}"), at_ns, *value);
            }
        }
        self.previous = Some(sample);
    }

    /// Values from the latest two samples.
    pub fn derived(&self) -> &Derived {
        &self.derived
    }

    /// History series by key: `rate:<counter>` and `ratio:<key>`.
    pub fn history(&self) -> &BTreeMap<String, Series> {
        &self.history
    }
}

fn push_point(history: &mut BTreeMap<String, Series>, key: String, at_ns: u64, value: f64) {
    history.entry(key).or_default().push(Point { at_ns, value });
}

fn regressed(prev: &ProviderSample, cur: &ProviderSample) -> bool {
    // A falling uptime is the restart signal. The wall clock is not compared: it
    // can jump backwards without the engine having restarted at all.
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
    let window_ns = match (prev.uptime_ns, cur.uptime_ns) {
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

    for def in cur.ratio_defs {
        let Some(numerator) = delta(def.numerator) else {
            continue;
        };
        // Every denominator term must be present: a sum with a missing term is
        // not the formula the engine documents, so it is not given.
        let Some(total) = def
            .denominator
            .iter()
            .try_fold(0u64, |acc, name| delta(name).map(|d| acc + d))
        else {
            continue;
        };
        if total > 0 {
            out.ratios
                .insert(def.key.to_string(), numerator as f64 / total as f64);
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
    let base = interval.max(source.min_interval());
    let cap = source.backoff_cap();
    let mut failures: u32 = 0;
    loop {
        let outcome = source.poll(source.poll_timeout());
        let at = Instant::now();
        {
            let mut st = lock(&shared);
            match outcome {
                Ok(sample) => {
                    failures = 0;
                    sampler.update(sample.clone());
                    st.latest = Some(sample);
                    st.derived = sampler.derived().clone();
                    st.history = sampler.history().clone();
                    st.reachable = true;
                    st.last_success = Some(at);
                }
                Err(error) => {
                    // Something that does not answer is asked again more slowly,
                    // so a provider that is not running costs little.
                    failures = failures.saturating_add(1);
                    st.reachable = false;
                    st.last_error = Some(Failure { error, at });
                }
            }
        }
        let wait = next_wait(base, cap, failures.max(1));
        match stop.recv_timeout(wait) {
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

    /// Ratios the tests use: a numerator over a three-term denominator that
    /// includes it, and a two-term pair.
    const TEST_RATIOS: &[RatioDef] = &[
        RatioDef {
            key: "tier.vram",
            numerator: "tier.vram.hits_total",
            denominator: &[
                "tier.vram.hits_total",
                "tier.vram.fills_total",
                "tier.vram.bypass_total",
            ],
        },
        RatioDef {
            key: "tier.ram",
            numerator: "tier.ram.hits_total",
            denominator: &["tier.ram.hits_total", "tier.ram.misses_total"],
        },
    ];

    fn sample(at: Option<u64>, counters: &[(&str, u64)]) -> ProviderSample {
        ProviderSample {
            uptime_ns: at,
            counters: counters.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            ratio_defs: TEST_RATIOS,
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
    fn two_samples_give_rates_and_a_named_denominator_ratio() {
        let mut s = InferenceSampler::default();
        s.update(sample(
            Some(SEC),
            &[
                ("requests.total", 10),
                ("tier.vram.hits_total", 100),
                ("tier.vram.fills_total", 10),
                ("tier.vram.bypass_total", 10),
            ],
        ));
        s.update(sample(
            Some(3 * SEC),
            &[
                ("requests.total", 14),
                ("tier.vram.hits_total", 130),
                ("tier.vram.fills_total", 15),
                ("tier.vram.bypass_total", 15),
            ],
        ));
        let d = s.derived();
        assert_eq!(d.window_ns, Some(2 * SEC));
        assert_eq!(d.rate("requests.total"), Some(2.0));
        assert_eq!(d.rate("tier.vram.hits_total"), Some(15.0));
        // 30 hits over (30 + 5 + 5) = 40: 0.75.
        assert_eq!(d.ratio("tier.vram"), Some(0.75));
    }

    #[test]
    fn a_ratio_needs_every_denominator_counter() {
        let mut s = InferenceSampler::default();
        // The bypass counter is absent from both samples.
        s.update(sample(
            Some(SEC),
            &[("tier.vram.hits_total", 1), ("tier.vram.fills_total", 1)],
        ));
        s.update(sample(
            Some(2 * SEC),
            &[("tier.vram.hits_total", 9), ("tier.vram.fills_total", 2)],
        ));
        assert_eq!(s.derived().ratio("tier.vram"), None);
        // The rates of the counters that are present still derive.
        assert_eq!(s.derived().rate("tier.vram.hits_total"), Some(8.0));
    }

    #[test]
    fn a_ratio_is_none_when_its_denominator_gained_nothing() {
        let mut s = InferenceSampler::default();
        let pair = [("tier.ram.hits_total", 5), ("tier.ram.misses_total", 5)];
        s.update(sample(Some(SEC), &pair));
        s.update(sample(Some(2 * SEC), &pair));
        assert_eq!(s.derived().ratio("tier.ram"), None);
        assert_eq!(s.derived().rate("tier.ram.hits_total"), Some(0.0));
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
    fn uptime_going_backwards_resets_the_sampler() {
        let mut s = InferenceSampler::default();
        s.update(sample(Some(5 * SEC), &[("requests.total", 1)]));
        s.update(sample(Some(2 * SEC), &[("requests.total", 9)]));
        assert!(s.derived().rates.is_empty());
        assert_eq!(s.derived().window_ns, None);
    }

    #[test]
    fn a_wall_clock_jump_does_not_reset_or_skew_the_window() {
        // The wall clock steps backwards by an hour while the engine's uptime
        // advances two seconds. The rate is over the uptime window.
        let mut first = sample(Some(SEC), &[("requests.total", 10)]);
        first.sampled_at_ns = Some(10_000 * SEC);
        let mut second = sample(Some(3 * SEC), &[("requests.total", 14)]);
        second.sampled_at_ns = Some(6_400 * SEC);
        let mut s = InferenceSampler::default();
        s.update(first);
        s.update(second);
        assert_eq!(s.derived().window_ns, Some(2 * SEC));
        assert_eq!(s.derived().rate("requests.total"), Some(2.0));
    }

    #[test]
    fn missing_counter_gives_no_rate() {
        let mut s = InferenceSampler::default();
        s.update(sample(Some(SEC), &[("requests.total", 1)]));
        s.update(sample(Some(2 * SEC), &[("tier.nvme.reads_total", 4)]));
        assert_eq!(s.derived().rate("requests.total"), None);
        assert_eq!(s.derived().rate("tier.nvme.reads_total"), None);
    }

    #[test]
    fn non_advancing_uptime_gives_no_rate() {
        let mut s = InferenceSampler::default();
        s.update(sample(Some(SEC), &[("requests.total", 1)]));
        s.update(sample(Some(SEC), &[("requests.total", 4)]));
        assert!(s.derived().rates.is_empty());
        assert_eq!(s.derived().window_ns, None);
    }

    #[test]
    fn missing_uptime_gives_no_rate() {
        let mut s = InferenceSampler::default();
        s.update(sample(None, &[("requests.total", 1)]));
        s.update(sample(Some(2 * SEC), &[("requests.total", 4)]));
        assert!(s.derived().rates.is_empty());
    }

    #[test]
    fn history_skips_underived_points() {
        let mut s = InferenceSampler::default();
        let hit = |h: u64, f: u64| {
            [
                ("tier.vram.hits_total", h),
                ("tier.vram.fills_total", f),
                ("tier.vram.bypass_total", 0),
            ]
        };
        s.update(sample(Some(SEC), &hit(0, 0)));
        // No previous sample for the ratio yet: nothing recorded.
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
        assert_eq!(
            SourceError::MetricsNotEnabled.to_string(),
            METRICS_NOT_ENABLED
        );
    }

    #[test]
    fn only_a_metrics_off_reply_or_a_body_counts_as_an_engine_answering() {
        assert!(SourceError::MetricsNotEnabled.engine_answered());
        assert!(SourceError::Status(500).engine_answered());
        assert!(!SourceError::Unreachable("connection refused".into()).engine_answered());
    }

    #[test]
    fn summaries_tell_the_metrics_off_state_from_no_engine() {
        let off = SourceError::MetricsNotEnabled.summary("http://127.0.0.1:8080");
        let none =
            SourceError::Unreachable("connection refused".into()).summary("http://127.0.0.1:8080");
        assert!(
            off.starts_with("engine running, metrics not enabled"),
            "{off}"
        );
        assert!(off.contains("--metrics"), "{off}");
        assert!(none.starts_with("no engine answered at"), "{none}");
        assert_ne!(off, none);
    }
}

#[cfg(test)]
pub(crate) mod poller_tests {
    use super::*;
    use crate::inference::ironworks::IronWorksSource;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    pub(crate) const A_BODY: &str =
        include_str!("../../tests/fixtures/inference/a_nvme_configured.json");

    /// The body an IronWorks engine sends with 404 when started without `--metrics`.
    pub(crate) const METRICS_OFF_BODY: &str =
        include_str!("../../tests/fixtures/inference/metrics_not_enabled.json");

    /// A tiny HTTP stub on loopback. Each connection gets one reply and is then
    /// closed. Dropping it with `stop` releases the port, so later connects are
    /// refused.
    pub(crate) struct Stub {
        port: u16,
        reply: Arc<Mutex<(u16, String)>>,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Stub {
        pub(crate) fn start(status: u16, body: &str) -> Stub {
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
                            let reason = match status {
                                200 => "OK",
                                404 => "Not Found",
                                _ => "Error",
                            };
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

        pub(crate) fn url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }

        #[allow(dead_code)]
        pub(crate) fn set_reply(&self, status: u16, body: &str) {
            *self.reply.lock().expect("reply lock") = (status, body.to_string());
        }

        pub(crate) fn stop(mut self) {
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

    /// The engine is running with `--metrics` off. That is a different state from
    /// nothing answering, and the poller must say so.
    #[test]
    fn poller_reports_metrics_not_enabled_on_a_404_with_the_engines_body() {
        let stub = Stub::start(404, METRICS_OFF_BODY);
        let poller = poller_for(&stub.url());
        let ok = wait_until(Duration::from_secs(10), || {
            poller.state(0).is_some_and(|s| s.last_error.is_some())
        });
        assert!(ok, "poller never recorded the error");
        let st = poller.state(0).expect("state");
        assert!(!st.reachable);
        let failure = st.last_error.expect("error");
        assert_eq!(failure.error, SourceError::MetricsNotEnabled);
        assert!(failure.error.engine_answered());
        drop(poller);
        stub.stop();
    }

    /// A 404 is not evidence of the metrics-off state by itself. Another body on
    /// the same status stays what it is: a status the engine answered with.
    #[test]
    fn a_404_with_some_other_body_is_not_claimed_as_metrics_off() {
        let stub = Stub::start(404, "<html>no such page</html>");
        let poller = poller_for(&stub.url());
        let ok = wait_until(Duration::from_secs(10), || {
            poller.state(0).is_some_and(|s| s.last_error.is_some())
        });
        assert!(ok, "poller never recorded the error");
        let failure = poller.state(0).expect("state").last_error.expect("error");
        assert_eq!(failure.error, SourceError::Status(404));
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
