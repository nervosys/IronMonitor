//! The Inference monitor, laid out like a model-serving dashboard: a model-state
//! strip, a grid of tiles with sparklines, gauges, and the engine's recent requests.
//!
//! It is built here once, from the engine states and the machine's own readings,
//! and both the TUI and the GUI draw the same [`Monitor`]. A tile whose value could
//! not be obtained has `value: None` and its reason in `detail`; nothing here puts
//! a zero, a previous sample or a constant in place of a reading.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use super::{
    energy, fmt_seconds, mean_label, EngineState, Locality, ProviderKind, ServingDetail,
    SourceState,
};

/// Points kept per hardware sparkline: about two minutes at one point a second.
pub const SPARK_POINTS: usize = 120;

/// One GPU as the machine reports it, in display units.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpuHw {
    pub name: String,
    pub utilization: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub temperature: Option<f64>,
    pub power_w: Option<f64>,
    pub power_limit_w: Option<f64>,
    pub pcie_gen: Option<u32>,
    pub pcie_width: Option<u32>,
    /// Bytes per second the GPU receives over PCIe, in MB/s.
    pub pcie_mb_s: Option<f64>,
}

impl GpuHw {
    /// One GPU's readings in display units: watts from milliwatts, MB/s from bytes.
    /// The link generation and width are the link's maximum, which an idle GPU
    /// drops below to save power.
    pub fn from_readings(
        s: &crate::gpu::GpuStaticInfo,
        d: Option<&crate::gpu::GpuDynamicInfo>,
    ) -> Self {
        GpuHw {
            name: s.name.clone(),
            utilization: d.and_then(|d| d.utilization).map(f64::from),
            memory_used: d.and_then(|d| d.memory.used),
            memory_total: d.and_then(|d| d.memory.total),
            temperature: d.and_then(|d| d.thermal.temperature).map(f64::from),
            power_w: d
                .and_then(|d| d.power.draw)
                .map(|mw| f64::from(mw) / 1000.0),
            power_limit_w: d
                .and_then(|d| d.power.limit)
                .map(|mw| f64::from(mw) / 1000.0),
            pcie_gen: d
                .and_then(|d| d.pcie.max_gen.or(d.pcie.current_gen))
                .map(u32::from),
            pcie_width: d
                .and_then(|d| d.pcie.max_width.or(d.pcie.current_width))
                .map(u32::from),
            pcie_mb_s: d.and_then(|d| d.pcie.rx_throughput).map(|b| b as f64 / 1e6),
        }
    }
}

/// The machine's readings the monitor needs, gathered by each front end from its
/// own state so the tiles agree with the rest of that front end.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hardware {
    /// Whether the machine has been sampled at all. Before the first sample an
    /// empty GPU list means nothing yet, not a machine without GPUs.
    pub sampled: bool,
    pub gpus: Vec<GpuHw>,
    pub cpu_utilization: Option<f64>,
    pub cores: Option<usize>,
    pub threads: Option<usize>,
    pub ram_used: Option<u64>,
    pub ram_total: Option<u64>,
    /// Bytes per second read across every disk with a rate, or `None` if none has one.
    pub disk_read: Option<f64>,
    pub disk_write: Option<f64>,
    /// The snapshot these readings came from, so counters are differenced only
    /// across two different snapshots.
    pub generation: u64,
    /// Bytes read and written since boot, summed over the devices that report both,
    /// and how many devices that is. A rate is taken from two of these when the
    /// disks report no rate of their own.
    pub disk_bytes: Option<(u64, u64, usize)>,
}

impl Hardware {
    /// Mean utilization over the GPUs that report one.
    pub fn gpu_load(&self) -> Option<f64> {
        mean(self.gpus.iter().filter_map(|g| g.utilization))
    }

    /// Used and total VRAM, summed over the GPUs that report both.
    pub fn vram(&self) -> Option<(u64, u64)> {
        let pairs: Vec<(u64, u64)> = self
            .gpus
            .iter()
            .filter_map(|g| Some((g.memory_used?, g.memory_total?)))
            .collect();
        (!pairs.is_empty()).then(|| {
            pairs
                .iter()
                .fold((0, 0), |(u, t), (gu, gt)| (u + gu, t + gt))
        })
    }

    /// The hottest GPU's temperature.
    pub fn hottest(&self) -> Option<f64> {
        self.gpus
            .iter()
            .filter_map(|g| g.temperature)
            .fold(None, |m: Option<f64>, t| Some(m.map_or(t, |m| m.max(t))))
    }

    /// Total GPU power draw, and the total limit when every GPU drawing power has one.
    pub fn power(&self) -> Option<(f64, Option<f64>)> {
        let drawing: Vec<&GpuHw> = self.gpus.iter().filter(|g| g.power_w.is_some()).collect();
        if drawing.is_empty() {
            return None;
        }
        let draw = drawing.iter().filter_map(|g| g.power_w).sum();
        let limit = drawing.iter().map(|g| g.power_limit_w).sum::<Option<f64>>();
        Some((draw, limit))
    }

    /// PCIe throughput summed over the GPUs that report it.
    pub fn pcie_mb_s(&self) -> Option<f64> {
        let rates: Vec<f64> = self.gpus.iter().filter_map(|g| g.pcie_mb_s).collect();
        (!rates.is_empty()).then(|| rates.iter().sum())
    }
}

impl Hardware {
    /// The readings from a pipeline snapshot. Physical cores are not in it, so
    /// `cores` is left unread rather than set to the thread count.
    pub fn from_snapshot(snapshot: &crate::pipeline::Snapshot) -> Self {
        let rates = |f: fn(&crate::pipeline::DiskSnapshot) -> Option<f64>| {
            let known: Vec<f64> = snapshot.disks.iter().filter_map(f).collect();
            (!known.is_empty()).then(|| known.iter().sum())
        };
        // RamInfo counts kilobytes.
        let ram = snapshot
            .memory
            .as_ref()
            .filter(|m| m.ram.total > 0)
            .map(|m| (m.ram.used * 1024, m.ram.total * 1024));
        let io: Vec<(u64, u64)> = snapshot
            .disk_io
            .iter()
            .filter_map(|d| Some((d.read_bytes?, d.write_bytes?)))
            .collect();
        let disk_bytes = (!io.is_empty()).then(|| {
            let (r, w) = io.iter().fold((0, 0), |(r, w), (dr, dw)| (r + dr, w + dw));
            (r, w, io.len())
        });
        Hardware {
            sampled: snapshot.generation > 0,
            gpus: snapshot
                .gpu_static
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    GpuHw::from_readings(s, snapshot.gpu_dynamic.get(i).and_then(|d| d.as_ref()))
                })
                .collect(),
            cpu_utilization: snapshot
                .cpu
                .as_ref()
                .map(|c| f64::from(100.0 - c.total.idle)),
            cores: None,
            threads: snapshot
                .cpu
                .as_ref()
                .map(|c| c.cores.len())
                .filter(|n| *n > 0),
            ram_used: ram.map(|r| r.0),
            ram_total: ram.map(|r| r.1),
            disk_read: rates(|d| d.read_rate),
            disk_write: rates(|d| d.write_rate),
            generation: snapshot.generation,
            disk_bytes,
        }
    }
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values.fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f64)
}

/// Sparkline history for the hardware tiles. An absent reading adds no point, so a
/// gap is never drawn as an idle zero.
#[derive(Debug, Clone, Default)]
pub struct History {
    series: BTreeMap<&'static str, VecDeque<f64>>,
    last: Option<Instant>,
    /// The disk counters of the last snapshot seen: its generation, when it was
    /// first seen, read and write totals, and the device count.
    disk_last: Option<(u64, Instant, u64, u64, usize)>,
    /// Read and write bytes per second between the last two snapshots.
    disk_rate: Option<(f64, f64)>,
    /// When the engine's own speed was last recorded.
    speed_last: Option<Instant>,
}

impl History {
    /// Record one point per series, at most once a second however often it is called.
    pub fn record(&mut self, now: Instant, hw: &Hardware) {
        self.difference_disks(now, hw);
        if let Some(last) = self.last {
            if now.saturating_duration_since(last) < Duration::from_secs(1) {
                return;
            }
        }
        self.last = Some(now);
        let points = [
            ("gpu", hw.gpu_load()),
            ("vram", hw.vram().map(|(used, _)| used as f64)),
            ("temp", hw.hottest()),
            ("power", hw.power().map(|(draw, _)| draw)),
            ("pcie", hw.pcie_mb_s()),
            ("cpu", hw.cpu_utilization),
            ("disk", self.disk_read(hw).map(|(r, _)| r)),
        ];
        for (key, value) in points {
            if let Some(value) = value {
                let series = self.series.entry(key).or_default();
                if series.len() == SPARK_POINTS {
                    series.pop_front();
                }
                series.push_back(value);
            }
        }
    }

    /// A disk rate from two snapshots' counters. Only a new snapshot is
    /// differenced, over the time between first seeing each, and a change in the
    /// device set or a counter going backwards gives no rate rather than a wrong one.
    fn difference_disks(&mut self, now: Instant, hw: &Hardware) {
        let Some((read, write, devices)) = hw.disk_bytes else {
            self.disk_last = None;
            self.disk_rate = None;
            return;
        };
        if let Some((generation, ..)) = self.disk_last {
            if generation == hw.generation {
                return;
            }
        }
        self.disk_rate = match self.disk_last {
            Some((_, at, r0, w0, n0)) if n0 == devices && read >= r0 && write >= w0 => {
                let secs = now.saturating_duration_since(at).as_secs_f64();
                (secs >= 0.5).then(|| ((read - r0) as f64 / secs, (write - w0) as f64 / secs))
            }
            _ => None,
        };
        self.disk_last = Some((hw.generation, now, read, write, devices));
    }

    /// Disk read and write rates: the disks' own when they report one, else from
    /// their counters across snapshots.
    pub fn disk_read(&self, hw: &Hardware) -> Option<(f64, Option<f64>)> {
        match hw.disk_read {
            Some(r) => Some((r, hw.disk_write)),
            None => self.disk_rate.map(|(r, w)| (r, Some(w))),
        }
    }

    /// Record the followed engine's own generation speed, at most once a second:
    /// its measured tok/s while generating, and zero while it says it is idle. A
    /// state with no speed adds no point.
    pub fn record_engine(&mut self, now: Instant, states: &[SourceState]) {
        if let Some(last) = self.speed_last {
            if now.saturating_duration_since(last) < Duration::from_secs(1) {
                return;
            }
        }
        let detail = focus(states)
            .map(|i| &states[i])
            .filter(|s| s.reachable)
            .and_then(serving_of);
        let Some(detail) = detail else {
            return;
        };
        let speed = match detail.state {
            Some(EngineState::Generating) => detail.tok_s_now,
            Some(EngineState::Idle) => Some(0.0),
            _ => None,
        };
        self.speed_last = Some(now);
        if let Some(v) = speed {
            let series = self.series.entry("speed").or_default();
            if series.len() == SPARK_POINTS {
                series.pop_front();
            }
            series.push_back(v);
        }
    }

    pub fn series(&self, key: &str) -> Vec<f64> {
        self.series
            .get(key)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
    }
}

/// What the engine is doing, as far as its own figures show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Reading,
    Generating,
    Queued,
    Error,
}

impl Phase {
    pub const ALL: [Phase; 5] = [
        Phase::Idle,
        Phase::Reading,
        Phase::Generating,
        Phase::Queued,
        Phase::Error,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Phase::Idle => "Idle",
            Phase::Reading => "Reading",
            Phase::Generating => "Generating",
            Phase::Queued => "Queued",
            Phase::Error => "Error",
        }
    }
}

/// The strip across the top: which engine, what it is doing, and how that is known.
#[derive(Debug, Clone, PartialEq)]
pub struct StateStrip {
    /// Engine name, model and endpoint, or that no engine answered.
    pub engine: String,
    /// The followed engine's name, endpoint, model and context, for a header.
    pub name: Option<&'static str>,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub context: Option<u64>,
    /// The lit phase, or `None` when the engine's figures do not say.
    pub lit: Option<Phase>,
    /// The engine's own name for its step, where it gives one.
    pub step: Option<String>,
    pub detail: String,
    /// How far the request in flight is, 0 to 1, where the engine counts it.
    pub progress: Option<f64>,
    /// Why there is no progress to draw, when there is none.
    pub progress_note: Option<String>,
}

impl StateStrip {
    fn bare(engine: String, detail: String) -> Self {
        StateStrip {
            engine,
            name: None,
            endpoint: None,
            model: None,
            context: None,
            lit: None,
            step: None,
            detail,
            progress: None,
            progress_note: None,
        }
    }
}

/// A count with thousands separators, as the monitor prints counts.
pub fn fmt_count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A token count in thousands, as a context size reads: 32768 is 32K.
pub fn fmt_k(n: u64) -> String {
    if n >= 1024 && n.is_multiple_of(1024) {
        format!("{}K", n / 1024)
    } else if n >= 1000 {
        format!("{}K", (n as f64 / 1000.0).round() as u64)
    } else {
        n.to_string()
    }
}

fn serving_of(state: &SourceState) -> Option<&ServingDetail> {
    state.latest.as_ref()?.serving.as_ref()
}

/// A mean since the counters started: a seconds total over a count, from one sample.
fn since_start(state: &SourceState, seconds: &str, count: &str) -> Option<f64> {
    let sample = state.latest.as_ref()?;
    let s = *sample.seconds.get(seconds)?;
    let n = *sample.counters.get(count)?;
    (s > 0.0 && n > 0).then(|| n as f64 / s)
}

/// One tile: a headline value with its unit, a line of detail, and a sparkline.
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub title: &'static str,
    /// The headline, or `None` when it could not be obtained; `detail` says why.
    pub value: Option<String>,
    pub unit: String,
    pub detail: String,
    /// Oldest first. Empty when there is no history to draw.
    pub spark: Vec<f64>,
    /// The top of the sparkline's scale, where one is fixed (a percentage, a limit).
    pub spark_max: Option<f64>,
}

/// A labelled bar, with no bar when the ratio is unknown.
#[derive(Debug, Clone, PartialEq)]
pub struct Gauge {
    pub label: &'static str,
    pub text: String,
    pub ratio: Option<f64>,
}

/// The engine's recent requests, or why there are none to list.
#[derive(Debug, Clone, PartialEq)]
pub struct Requests {
    pub columns: Vec<&'static str>,
    pub rows: Vec<Vec<String>>,
    pub note: String,
}

/// Everything the monitor shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub strip: StateStrip,
    pub tiles: Vec<Tile>,
    pub gauges: Vec<Gauge>,
    pub requests: Requests,
    /// The index in the states of the engine the monitor follows.
    pub focused: Option<usize>,
}

/// The engine to follow: the first local engine answering now, IronWorks first; else
/// the first local engine that has answered before and has stopped.
pub fn focus(states: &[SourceState]) -> Option<usize> {
    let local = |s: &SourceState| s.locality == Locality::Local;
    let answering = |s: &SourceState| local(s) && s.reachable && s.latest.is_some();
    states
        .iter()
        .position(|s| answering(s) && s.kind == ProviderKind::IronWorks)
        .or_else(|| states.iter().position(answering))
        .or_else(|| {
            states
                .iter()
                .position(|s| local(s) && s.last_success.is_some())
        })
}

/// The prompt-token counter each engine reports, if any.
fn prompt_counter(state: &SourceState) -> Option<&'static str> {
    let sample = state.latest.as_ref()?;
    let name = match state.kind {
        ProviderKind::IronWorks => "decode.prompt_tokens_total",
        _ => "prompt_tokens_total",
    };
    sample.counters.contains_key(name).then_some(name)
}

/// Requests running and waiting, from whichever figures the engine publishes.
fn load_of(state: &SourceState) -> (Option<u64>, Option<u64>) {
    let Some(sample) = state.latest.as_ref() else {
        return (None, None);
    };
    let gauge = |k: &str| sample.gauges.get(k).map(|v| v.max(0.0) as u64);
    match state.kind {
        ProviderKind::IronWorks => (
            sample.ironworks.as_ref().and_then(|s| s.requests_active),
            None,
        ),
        ProviderKind::Vllm => (gauge("num_requests_running"), gauge("num_requests_waiting")),
        ProviderKind::LlamaCpp => (gauge("requests_processing"), gauge("requests_deferred")),
        _ => match serving_of(state) {
            Some(d) => (
                d.state.as_ref().and_then(|s| match s {
                    EngineState::Reading | EngineState::Generating => Some(1),
                    EngineState::Idle | EngineState::Unloaded => Some(0),
                    EngineState::Other(_) => None,
                }),
                d.queued,
            ),
            None => (None, None),
        },
    }
}

fn history_values(state: &SourceState, key: &str) -> Vec<f64> {
    state
        .history
        .get(key)
        .map(|s| s.iter().map(|p| p.value).collect())
        .unwrap_or_default()
}

fn strip(states: &[SourceState], focused: Option<usize>) -> StateStrip {
    let Some(state) = focused.map(|i| &states[i]) else {
        let local: Vec<&SourceState> = states
            .iter()
            .filter(|s| s.locality == Locality::Local)
            .collect();
        return StateStrip::bare(
            "no engine answered".into(),
            match local.as_slice() {
                [] => "no local inference engine is configured".into(),
                [only] => super::compact_row(only).detail,
                many => format!("{} local engines polled; none has answered", many.len()),
            },
        );
    };
    let name = state.kind.display_name();
    let model = state
        .latest
        .as_ref()
        .and_then(|s| s.identity.model.clone())
        .unwrap_or_else(|| "model not reported".into());
    let engine = format!("{name} · {model} · {}", state.endpoint);
    let serving = serving_of(state);
    let base = StateStrip {
        name: Some(name),
        endpoint: Some(state.endpoint.clone()),
        model: state.latest.as_ref().and_then(|s| s.identity.model.clone()),
        context: serving.and_then(|d| d.context_max),
        ..StateStrip::bare(engine, String::new())
    };
    if !state.reachable {
        let why = state
            .last_error
            .as_ref()
            .map(|f| f.error.to_string())
            .unwrap_or_else(|| "the last poll failed".into());
        return StateStrip {
            lit: Some(Phase::Error),
            detail: format!("stopped answering: {why}"),
            ..base
        };
    }
    // An engine that says what it is doing is taken at its word.
    if let Some((d, es)) = serving.and_then(|d| Some((d, d.state.as_ref()?))) {
        let progress = d.progress.as_ref().filter(|p| p.total > 0);
        let ratio = progress.map(|p| (p.done as f64 / p.total as f64).min(1.0));
        let counted = progress.map(|p| {
            format!(
                "{} / {} {} · {:.0}%",
                fmt_count(p.done),
                fmt_count(p.total),
                p.what,
                ratio.unwrap_or(0.0) * 100.0
            )
        });
        let queued = d.queued.unwrap_or(0);
        let (lit, detail, progress) = match es {
            _ if queued > 0 => (Some(Phase::Queued), format!("{queued} waiting"), ratio),
            EngineState::Reading => (
                Some(Phase::Reading),
                counted.unwrap_or_else(|| "reading the prompt".into()),
                ratio,
            ),
            EngineState::Generating => {
                let speed = d.tok_s_now.map(|t| format!(" · {t:.1} tok/s"));
                (
                    Some(Phase::Generating),
                    format!(
                        "{}{}",
                        counted.unwrap_or_else(|| "generating".into()),
                        speed.unwrap_or_default()
                    ),
                    ratio,
                )
            }
            EngineState::Idle => {
                let last = d.recent.as_ref().and_then(|r| r.first());
                let detail = match last.and_then(|r| r.output) {
                    Some(n) => match last.and_then(|r| r.decode_tok_s) {
                        Some(t) => format!("last: {} tokens at {t:.1} tok/s", fmt_count(n)),
                        None => format!("last: {} tokens", fmt_count(n)),
                    },
                    None => "waiting for a request".into(),
                };
                (Some(Phase::Idle), detail, None)
            }
            EngineState::Unloaded => (None, "running with no model loaded".into(), None),
            EngineState::Other(s) => (None, format!("{name} reports its state as {s}"), None),
        };
        return StateStrip {
            lit,
            step: d.phase.clone(),
            detail,
            progress,
            ..base
        };
    }
    let d = &state.derived;
    let generated = state
        .latest
        .as_ref()
        .and_then(|s| s.generated_counter)
        .and_then(|c| d.rate(c));
    let prompt = prompt_counter(state).and_then(|c| d.rate(c));
    let (running, waiting) = load_of(state);
    let (lit, detail) = match (running, waiting) {
        (_, Some(w)) if w > 0 => (
            Some(Phase::Queued),
            format!(
                "{w} waiting · {} running",
                running.map_or("unknown".into(), |r| r.to_string())
            ),
        ),
        (Some(0), _) => (Some(Phase::Idle), "waiting for a request".into()),
        (running, _) => {
            let count = match running {
                Some(r) => format!("{r} running"),
                None => format!("{name} reports no request count"),
            };
            match (generated, prompt) {
                (Some(g), _) if g > 0.0 => (
                    Some(Phase::Generating),
                    format!("{count} · {g:.1} tok/s generated over the last window"),
                ),
                (_, Some(p)) if p > 0.0 => (
                    Some(Phase::Reading),
                    format!("{count} · {p:.0} prompt tok/s over the last window"),
                ),
                (None, None) => (None, format!("{count} · phase needs a second sample")),
                _ => (
                    None,
                    format!("{count} · no tokens counted in the last window"),
                ),
            }
        }
    };
    StateStrip {
        lit,
        detail,
        ..base
    }
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / (1u64 << 30) as f64
}

/// A tile with no value, and why.
fn absent(title: &'static str, reason: impl Into<String>) -> Tile {
    Tile {
        title,
        value: None,
        unit: String::new(),
        detail: reason.into(),
        spark: Vec::new(),
        spark_max: None,
    }
}

/// Per-GPU detail when there are several, else the one GPU's name.
fn per_gpu(hw: &Hardware, f: impl Fn(&GpuHw) -> Option<String>) -> String {
    if hw.gpus.len() > 1 {
        hw.gpus
            .iter()
            .enumerate()
            .map(|(i, g)| format!("GPU{i} {}", f(g).unwrap_or_else(|| "—".into())))
            .collect::<Vec<_>>()
            .join(" · ")
    } else {
        hw.gpus.first().map(|g| g.name.clone()).unwrap_or_default()
    }
}

const NO_GPU: &str = "no GPU enumerated on this machine";
const NO_ENGINE: &str = "no engine answered";
const NO_WINDOW: &str = "needs a second sample";

const NOT_SAMPLED: &str = "waiting for the first hardware sample";

pub fn hardware_tiles(hw: &Hardware, history: &History) -> Vec<Tile> {
    if !hw.sampled {
        return [
            "GPU load",
            "VRAM",
            "GPU temp",
            "Power",
            "PCIe",
            "CPU",
            "Disk read",
        ]
        .into_iter()
        .map(|t| absent(t, NOT_SAMPLED))
        .collect();
    }
    let mut tiles = Vec::new();
    let no_gpu = hw.gpus.is_empty();

    tiles.push(match hw.gpu_load() {
        Some(load) => Tile {
            title: "GPU load",
            value: Some(format!("{load:.0}")),
            unit: "%".into(),
            detail: per_gpu(hw, |g| g.utilization.map(|u| format!("{u:.0}%"))),
            spark: history.series("gpu"),
            spark_max: Some(100.0),
        },
        None if no_gpu => absent("GPU load", NO_GPU),
        None => absent("GPU load", "no GPU reports a utilization counter"),
    });
    tiles.push(match hw.vram() {
        Some((used, total)) => Tile {
            title: "VRAM",
            value: Some(format!("{:.1}", gib(used))),
            unit: format!("/ {:.0} GB", gib(total)),
            detail: per_gpu(hw, |g| g.memory_used.map(|u| format!("{:.1} GB", gib(u)))),
            spark: history.series("vram"),
            spark_max: Some(total as f64),
        },
        None if no_gpu => absent("VRAM", NO_GPU),
        None => absent("VRAM", "no GPU reports both used and total memory"),
    });
    tiles.push(match hw.hottest() {
        Some(t) => Tile {
            title: "GPU temp",
            value: Some(format!("{t:.0}")),
            unit: "°C".into(),
            detail: if hw.gpus.len() > 1 {
                per_gpu(hw, |g| g.temperature.map(|t| format!("{t:.0}°")))
            } else {
                String::new()
            },
            spark: history.series("temp"),
            spark_max: Some(100.0),
        },
        None if no_gpu => absent("GPU temp", NO_GPU),
        None => absent("GPU temp", "no GPU reports a temperature"),
    });
    tiles.push(match hw.power() {
        Some((draw, limit)) => Tile {
            title: "Power",
            value: Some(format!("{draw:.0}")),
            unit: "W".into(),
            detail: match limit {
                Some(l) => format!("of {l:.0} W limit"),
                None => "limit not reported".into(),
            },
            spark: history.series("power"),
            spark_max: limit,
        },
        None if no_gpu => absent("Power", NO_GPU),
        None => absent("Power", "no GPU reports its power draw"),
    });
    let link = hw.gpus.iter().find(|g| g.pcie_gen.is_some());
    tiles.push(match link {
        Some(g) => Tile {
            title: "PCIe",
            value: Some(format!("Gen{}", g.pcie_gen.unwrap_or_default())),
            unit: g.pcie_width.map(|w| format!("x{w}")).unwrap_or_default(),
            detail: match hw.pcie_mb_s() {
                Some(mb) => format!("to GPU {mb:.1} MB/s"),
                None => "throughput not reported".into(),
            },
            spark: history.series("pcie"),
            spark_max: None,
        },
        None if no_gpu => absent("PCIe", NO_GPU),
        None => absent("PCIe", "no GPU reports its PCIe link"),
    });
    tiles.push(match hw.cpu_utilization {
        Some(u) => Tile {
            title: "CPU",
            value: Some(format!("{u:.0}")),
            unit: "%".into(),
            detail: match (hw.cores, hw.threads) {
                (Some(c), Some(t)) => format!("{c} cores · {t} threads"),
                (None, Some(t)) => format!("{t} threads"),
                (Some(c), None) => format!("{c} cores"),
                (None, None) => String::new(),
            },
            spark: history.series("cpu"),
            spark_max: Some(100.0),
        },
        None => absent("CPU", "no CPU sample yet"),
    });
    tiles.push(match history.disk_read(hw) {
        Some((read, write)) => Tile {
            title: "Disk read",
            value: Some(format!("{:.1}", read / 1e6)),
            unit: "MB/s".into(),
            detail: match write {
                Some(w) => format!("write {:.1} MB/s", w / 1e6),
                None => "write not reported".into(),
            },
            spark: history.series("disk"),
            spark_max: None,
        },
        None if hw.disk_bytes.is_some() => absent("Disk read", NO_WINDOW),
        None => absent("Disk read", "no disk reports read counters"),
    });
    tiles
}

fn speed_tile(state: Option<&SourceState>, history: &History) -> Tile {
    let Some(state) = state.filter(|s| s.reachable) else {
        return absent("Speed", NO_ENGINE);
    };
    // An engine that measures its own speed: now while generating, else its run
    // average from its totals, else its last request.
    if let Some(d) = serving_of(state).filter(|d| d.state.is_some()) {
        let prefill = d
            .prefill_tok_s
            .or_else(|| since_start(state, "prompt_seconds_total", "prompt_read_tokens_total"));
        let average = since_start(state, "decode_seconds_total", "output_tokens_total");
        let last = d
            .recent
            .as_ref()
            .and_then(|r| r.first())
            .and_then(|r| r.decode_tok_s);
        let (value, basis) = match (&d.state, d.tok_s_now, average, last) {
            (Some(EngineState::Generating), Some(now), _, _) => (Some(now), "now"),
            (_, _, Some(avg), _) => (Some(avg), "decode · run average"),
            (_, _, None, Some(last)) => (Some(last), "decode last request"),
            _ => (None, ""),
        };
        let detail = |basis: &str| match prefill {
            Some(p) => format!("{basis} · prefill {} tok/s", fmt_count(p.round() as u64)),
            None => basis.to_string(),
        };
        if let Some(v) = value {
            return Tile {
                title: "Speed",
                value: Some(format!("{v:.1}")),
                unit: "tok/s".into(),
                detail: detail(basis),
                spark: history.series("speed"),
                spark_max: None,
            };
        }
        // No figure of its own: its token counter across two polls, if it has one.
        if state
            .latest
            .as_ref()
            .and_then(|s| s.generated_counter)
            .is_none()
        {
            return absent("Speed", detail("no request finished yet"));
        }
    }
    let Some(generated) = state.latest.as_ref().and_then(|s| s.generated_counter) else {
        return absent(
            "Speed",
            format!("{} reports no token counter", state.kind.display_name()),
        );
    };
    let d = &state.derived;
    let prompt = prompt_counter(state).and_then(|c| d.rate(c));
    match d.rate(generated) {
        Some(rate) => Tile {
            title: "Speed",
            value: Some(format!("{rate:.1}")),
            unit: "tok/s".into(),
            detail: match prompt {
                Some(p) => format!("decode · prompt {p:.0} tok/s"),
                None => "decode".into(),
            },
            spark: history_values(state, &format!("rate:{generated}")),
            spark_max: None,
        },
        None => absent("Speed", NO_WINDOW),
    }
}

fn latency_tile(state: Option<&SourceState>) -> Tile {
    let Some(state) = state.filter(|s| s.reachable) else {
        return absent("Latency", NO_ENGINE);
    };
    let Some(sample) = state.latest.as_ref() else {
        return absent("Latency", NO_ENGINE);
    };
    let name = state.kind.display_name();
    if sample.mean_defs.is_empty() {
        return absent("Latency", format!("{name} publishes no latency figures"));
    }
    let d = &state.derived;
    let windowed: Vec<(&str, f64)> = sample
        .mean_defs
        .iter()
        .filter_map(|def| Some((def.key, d.mean(def.key)?)))
        .collect();
    // With nothing finished in the last window, the means since the engine's
    // counters started, from one sample, labelled as such.
    let (known, since_start) = if windowed.is_empty() {
        let totals: Vec<(&str, f64)> = sample
            .mean_defs
            .iter()
            .filter_map(|def| {
                let s = *sample.seconds.get(def.seconds)?;
                let n = *sample.counters.get(def.count)?;
                (n > 0).then(|| (def.key, s / n as f64))
            })
            .collect();
        (totals, true)
    } else {
        (windowed, false)
    };
    let Some(&(first, value)) = known.first() else {
        return absent(
            "Latency",
            if d.window_ns.is_none() {
                NO_WINDOW
            } else {
                "no request completed in the last window"
            },
        );
    };
    let rest: Vec<String> = known[1..]
        .iter()
        .map(|(k, v)| format!("{} {}", mean_label(k).to_lowercase(), fmt_seconds(*v)))
        .collect();
    let mut detail = mean_label(first).to_lowercase();
    if !rest.is_empty() {
        detail = format!("{detail} · {}", rest.join(" · "));
    }
    if since_start {
        detail = format!("{detail} · since start");
    }
    Tile {
        title: "Latency",
        value: Some(fmt_seconds(value)),
        unit: String::new(),
        detail,
        spark: history_values(state, &format!("mean:{first}")),
        spark_max: None,
    }
}

/// Energy over the window, or the reason there is none.
fn energy_window(state: &SourceState) -> Result<super::EnergyWindow, String> {
    let sample = state.latest.as_ref().ok_or(NO_ENGINE)?;
    match (&sample.energy, &state.derived.energy) {
        (Some(Err(reason)), _) => Err(reason.clone()),
        (Some(Ok(_)), None) => Err(NO_WINDOW.into()),
        (_, Some(Err(reason))) => Err(reason.clone()),
        (_, Some(Ok(window))) => Ok(window.clone()),
        (None, None) => Err("energy is not read for this engine".into()),
    }
}

fn energy_tile(state: Option<&SourceState>) -> Tile {
    let Some(state) = state.filter(|s| s.reachable) else {
        return absent("Energy", NO_ENGINE);
    };
    match energy_window(state) {
        Err(reason) => absent("Energy", reason),
        Ok(window) => match window.joules_per_token {
            Ok(j) => Tile {
                title: "Energy",
                value: Some(format!("{j:.3}")),
                unit: "J/tok".into(),
                detail: format!("{:.0} W, whole GPU", window.watts),
                spark: history_values(state, "energy:joules_per_token"),
                spark_max: None,
            },
            Err(reason) => Tile {
                title: "Energy",
                value: None,
                unit: String::new(),
                detail: format!("{:.0} W, whole GPU · no J/tok: {reason}", window.watts),
                spark: history_values(state, "energy:watts"),
                spark_max: None,
            },
        },
    }
}

/// What each remote provider reports spending this month.
fn remote_spend(states: &[SourceState]) -> String {
    let spent: Vec<String> = states
        .iter()
        .filter(|s| s.locality == Locality::Remote && s.reachable)
        .filter_map(|s| match s.latest.as_ref()?.cost.as_ref()? {
            super::remote::CostReading::Known {
                amount, currency, ..
            } => Some(format!(
                "{} {amount:.2} {currency} this month",
                s.kind.display_name()
            )),
            super::remote::CostReading::Unavailable { .. } => None,
        })
        .collect();
    if spent.is_empty() {
        "no remote spend read".into()
    } else {
        spent.join(" · ")
    }
}

fn cost_tile(states: &[SourceState], state: Option<&SourceState>) -> Tile {
    let remote = remote_spend(states);
    let electricity = match state.filter(|s| s.reachable) {
        None => Err(NO_ENGINE.to_string()),
        Some(state) => energy_window(state)
            .and_then(|w| w.joules_per_token)
            .and_then(|j| {
                let price = energy::price_from_env()?;
                Ok((energy::cost_of(j * 1e6, &price), price.currency))
            }),
    };
    match electricity {
        Ok((cost, currency)) => Tile {
            title: "Cost",
            value: Some(format!("{cost:.4}")),
            unit: format!("{currency}/M tok"),
            detail: format!("electricity · {remote}"),
            spark: Vec::new(),
            spark_max: None,
        },
        Err(reason) => absent("Cost", format!("electricity: {reason} · {remote}")),
    }
}

fn requests_tile(state: Option<&SourceState>) -> Tile {
    let Some(state) = state.filter(|s| s.reachable) else {
        return absent("Requests", NO_ENGINE);
    };
    let (running, waiting) = load_of(state);
    let sample = state.latest.as_ref();
    let total = sample.and_then(|s| {
        s.counters
            .get("requests.total")
            .or_else(|| s.counters.get("request_success_total"))
    });
    let mut detail = Vec::new();
    if let Some(t) = total {
        detail.push(format!("{t} since start"));
    }
    if let Some(w) = waiting {
        detail.push(format!("{w} waiting"));
    }
    match running {
        Some(r) => Tile {
            title: "Requests",
            value: Some(r.to_string()),
            unit: "running".into(),
            detail: detail.join(" · "),
            spark: Vec::new(),
            spark_max: None,
        },
        None => absent(
            "Requests",
            format!(
                "{} reports no running count{}",
                state.kind.display_name(),
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", detail.join(" · "))
                }
            ),
        ),
    }
}

fn gauges(hw: &Hardware, state: Option<&SourceState>) -> Vec<Gauge> {
    let mut out = Vec::new();
    let answering = state.filter(|s| s.reachable);
    let name = answering.map(|s| s.kind.display_name());
    let detail = answering.and_then(serving_of);
    out.push(context_gauge(answering, detail));
    out.push(match (detail.and_then(|d| d.experts_cached), name) {
        (Some((n, bytes)), _) => Gauge {
            label: "Experts in VRAM",
            text: match bytes {
                Some(b) => format!("{} · {:.1} GB", fmt_count(n), gib(b)),
                None => fmt_count(n),
            },
            ratio: match (bytes, hw.vram().map(|(_, t)| t)) {
                (Some(b), Some(t)) if t > 0 => Some((b as f64 / t as f64).min(1.0)),
                _ => None,
            },
        },
        (None, Some(name)) => Gauge {
            label: "Experts in VRAM",
            text: format!("{name} does not report expert placement"),
            ratio: None,
        },
        (None, None) => Gauge {
            label: "Experts in VRAM",
            text: NO_ENGINE.into(),
            ratio: None,
        },
    });
    // Cache use across all requests, where the engine reports it.
    if let Some(f) = answering.and_then(|s| {
        let v = s.latest.as_ref()?.gauges.get("kv_cache_usage_perc")?;
        Some(v.clamp(0.0, 1.0))
    }) {
        out.push(Gauge {
            label: "KV cache",
            text: format!("{:.0}%", f * 100.0),
            ratio: Some(f),
        });
    }
    if let Some(s) = answering {
        let (label, key) = match s.kind {
            ProviderKind::IronWorks => ("Expert VRAM hits", "tier.vram"),
            ProviderKind::Vllm => ("Prefix cache hits", "prefix_cache"),
            _ => ("", ""),
        };
        if !key.is_empty() {
            let r = s.derived.ratio(key);
            out.push(Gauge {
                label,
                text: r.map_or_else(|| NO_WINDOW.into(), |r| format!("{:.1}%", r * 100.0)),
                ratio: r,
            });
        }
    }
    out.push(match (hw.ram_used, hw.ram_total) {
        (Some(u), Some(t)) if t > 0 => Gauge {
            label: "System RAM",
            text: format!("{:.1} / {:.0} GB", gib(u), gib(t)),
            ratio: Some(u as f64 / t as f64),
        },
        _ => Gauge {
            label: "System RAM",
            text: if hw.sampled {
                "not read".into()
            } else {
                NOT_SAMPLED.into()
            },
            ratio: None,
        },
    });
    out.push(match hw.hottest() {
        Some(t) => Gauge {
            label: "GPU temperature",
            text: format!("{t:.0} °C"),
            ratio: Some((t / 100.0).clamp(0.0, 1.0)),
        },
        None => Gauge {
            label: "GPU temperature",
            text: if !hw.sampled {
                NOT_SAMPLED.into()
            } else if hw.gpus.is_empty() {
                NO_GPU.into()
            } else {
                "not reported".into()
            },
            ratio: None,
        },
    });
    out
}

/// Context in use against the context the model is served with.
fn context_gauge(answering: Option<&SourceState>, detail: Option<&ServingDetail>) -> Gauge {
    let gauge = |text: String, ratio| Gauge {
        label: "Context fill",
        text,
        ratio,
    };
    let Some(state) = answering else {
        return gauge(NO_ENGINE.into(), None);
    };
    let name = state.kind.display_name();
    let max = detail.and_then(|d| d.context_max).filter(|m| *m > 0);
    let used = detail.and_then(|d| d.context_used).map(|(u, _)| u);
    let idle = matches!(
        detail.and_then(|d| d.state.as_ref()),
        Some(EngineState::Idle)
    );
    match (used, max) {
        (Some(u), Some(m)) => {
            let r = (u as f64 / m as f64).min(1.0);
            gauge(
                format!("{} / {}   {:.0}%", fmt_k(u), fmt_k(m), r * 100.0),
                Some(r),
            )
        }
        (None, Some(m)) if idle => gauge(format!("— / {} · no request in flight", fmt_k(m)), None),
        (None, Some(m)) => gauge(
            format!(
                "— / {} · {name} does not report the tokens in use",
                fmt_k(m)
            ),
            None,
        ),
        (Some(u), None) => gauge(
            format!("{} · {name} does not report its context size", fmt_k(u)),
            None,
        ),
        (None, None) => gauge(format!("{name} does not report its context"), None),
    }
}

/// The columns of the recent-requests table, whichever engine fills it.
const REQUEST_COLUMNS: [&str; 8] = [
    "TIME",
    "STATUS",
    "PROMPT",
    "REUSED",
    "OUTPUT",
    "PREFILL T/S",
    "TOK/S",
    "DURATION",
];

fn requests(state: Option<&SourceState>) -> Requests {
    let empty = |note: String| Requests {
        columns: REQUEST_COLUMNS.to_vec(),
        rows: Vec::new(),
        note,
    };
    let Some(state) = state.filter(|s| s.reachable) else {
        return empty(NO_ENGINE.into());
    };
    let detail = serving_of(state);
    match detail.and_then(|d| d.recent.as_ref()) {
        Some(recent) => serving_requests(recent, detail.and_then(|d| d.recent_kept)),
        None if state.kind == ProviderKind::IronWorks => {
            empty("IronWorks does not publish its recent requests yet".into())
        }
        None => empty(format!(
            "{} publishes no per-request history; the Latency tile has its means",
            state.kind.display_name()
        )),
    }
}

/// The words a finish reason reads as, the way a serving monitor shows them.
fn status_word(reason: &str) -> String {
    match reason {
        "stop" => "Done".into(),
        "length" => "Max tokens".into(),
        "cancel" => "Stopped".into(),
        "disconnect" => "Closed".into(),
        "error" => "Error".into(),
        "timeout" => "Timeout".into(),
        other => other.to_string(),
    }
}

/// Finished requests with the columns a serving monitor lists, from the engine's
/// own per-request figures. A figure it did not report is a dash.
fn serving_requests(recent: &[super::RequestRow], kept: Option<u64>) -> Requests {
    let dash = || "—".to_string();
    let count = |v: Option<u64>| v.map_or_else(dash, fmt_count);
    let rows: Vec<Vec<String>> = recent
        .iter()
        .map(|r| {
            vec![
                r.time_unix
                    .and_then(|t| chrono::DateTime::from_timestamp(t as i64, 0))
                    .map(|t| {
                        t.with_timezone(&chrono::Local)
                            .format("%H:%M:%S")
                            .to_string()
                    })
                    .unwrap_or_else(dash),
                r.status.as_deref().map_or_else(dash, status_word),
                count(r.prompt),
                count(r.reused),
                count(r.output),
                r.prefill_tok_s
                    .map_or_else(dash, |v| fmt_count(v.round() as u64)),
                r.decode_tok_s.map_or_else(dash, |v| format!("{v:.1}")),
                r.duration_s.map_or_else(dash, |v| format!("{v:.1} s")),
            ]
        })
        .collect();
    let note = match (rows.len(), kept) {
        (0, _) => "no requests yet".into(),
        (n, Some(k)) if k as usize > n => format!("{n} of {k} kept"),
        (n, _) => format!("{n} kept"),
    };
    Requests {
        columns: REQUEST_COLUMNS.to_vec(),
        rows,
        note,
    }
}

/// Build the monitor from the engine states and the machine's readings.
pub fn build(states: &[SourceState], hw: &Hardware, history: &History) -> Monitor {
    let focused = focus(states);
    let state = focused.map(|i| &states[i]);
    let mut tiles = vec![speed_tile(state, history)];
    tiles.extend(hardware_tiles(hw, history));
    let experts = state
        .filter(|s| s.reachable)
        .and_then(serving_of)
        .and_then(|d| d.experts_cached);
    if let (Some((n, _)), Some(vram)) = (experts, tiles.iter_mut().find(|t| t.title == "VRAM")) {
        if vram.value.is_some() && hw.gpus.len() <= 1 {
            vram.detail = format!("{} experts cached", fmt_count(n));
        }
    }
    tiles.push(latency_tile(state));
    tiles.push(energy_tile(state));
    tiles.push(cost_tile(states, state));
    tiles.push(requests_tile(state));
    let mut strip = strip(states, focused);
    if strip.progress.is_none() {
        strip.progress_note = state.filter(|s| s.reachable).map(|s| {
            let d = serving_of(s);
            match d.and_then(|d| d.state.as_ref()) {
                Some(EngineState::Idle) => "no request in flight".to_string(),
                Some(EngineState::Unloaded) => "no model loaded".to_string(),
                Some(_) if d.is_some_and(|d| d.progress.is_none()) => format!(
                    "{} does not say how far the request in flight is",
                    s.kind.display_name()
                ),
                _ => format!(
                    "{} does not report per-request progress",
                    s.kind.display_name()
                ),
            }
        });
    }
    Monitor {
        strip,
        tiles,
        gauges: gauges(hw, state),
        requests: requests(state),
        focused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::{InferenceSampler, ProviderSample};

    fn vllm(samples: &[(u64, u64, u64, f64)]) -> SourceState {
        let mut sampler = InferenceSampler::default();
        let mut last = None;
        for &(uptime_s, generated, prompt, running) in samples {
            let mut s = ProviderSample {
                uptime_ns: Some(uptime_s * 1_000_000_000),
                generated_counter: Some("generation_tokens_total"),
                ..Default::default()
            };
            s.identity.model = Some("stand-in".into());
            s.counters
                .insert("generation_tokens_total".into(), generated);
            s.counters.insert("prompt_tokens_total".into(), prompt);
            s.gauges.insert("num_requests_running".into(), running);
            s.gauges.insert("num_requests_waiting".into(), 0.0);
            sampler.update(s.clone());
            last = Some(s);
        }
        SourceState {
            kind: ProviderKind::Vllm,
            locality: Locality::Local,
            endpoint: "http://127.0.0.1:8000".into(),
            latest: last,
            reachable: true,
            last_success: Some(Instant::now()),
            last_error: None,
            derived: sampler.derived().clone(),
            history: sampler.history().clone(),
        }
    }

    fn tile<'a>(m: &'a Monitor, title: &str) -> &'a Tile {
        m.tiles.iter().find(|t| t.title == title).unwrap()
    }

    /// An engine that reports its own state, progress, context and requests.
    fn serving(detail: crate::inference::ServingDetail) -> SourceState {
        let mut s = vllm(&[(1, 100, 1000, 1.0)]);
        s.latest.as_mut().unwrap().serving = Some(detail);
        s
    }

    fn generating() -> crate::inference::ServingDetail {
        use crate::inference::{EngineState, Progress, RequestRow, ServingDetail};
        ServingDetail {
            state: Some(EngineState::Generating),
            phase: Some("reply".into()),
            queued: Some(0),
            progress: Some(Progress {
                done: 3197,
                total: 9830,
                what: "tokens of max_tokens",
            }),
            tok_s_now: Some(45.1),
            context_max: Some(32768),
            context_used: Some((10197, "the request in flight")),
            experts_cached: Some((3099, Some(3 << 30))),
            recent: Some(vec![
                RequestRow {
                    time_unix: Some(1_791_212_863.0),
                    status: Some("stop".into()),
                    prompt: Some(18187),
                    reused: Some(17593),
                    output: Some(261),
                    decode_tok_s: Some(36.3),
                    duration_s: Some(8.7),
                    ..Default::default()
                },
                RequestRow {
                    time_unix: Some(1_791_212_850.0),
                    status: Some("length".into()),
                    prompt: Some(100),
                    output: Some(5),
                    ..Default::default()
                },
            ]),
            recent_kept: Some(36),
            ..Default::default()
        }
    }

    #[test]
    fn an_engine_that_states_its_phase_is_taken_at_its_word_with_progress() {
        let m = build(
            &[serving(generating())],
            &Hardware::default(),
            &History::default(),
        );
        assert_eq!(m.strip.lit, Some(Phase::Generating));
        assert_eq!(m.strip.step.as_deref(), Some("reply"));
        assert_eq!(m.strip.context, Some(32768));
        let p = m.strip.progress.unwrap();
        assert!((p - 3197.0 / 9830.0).abs() < 1e-9);
        assert!(
            m.strip.detail.contains("3,197 / 9,830"),
            "{}",
            m.strip.detail
        );
        let speed = tile(&m, "Speed");
        assert_eq!(speed.value.as_deref(), Some("45.1"));
        assert!(speed.detail.starts_with("now"), "{}", speed.detail);
        let fill = m.gauges.iter().find(|g| g.label == "Context fill").unwrap();
        assert_eq!(fill.text, "10K / 32K   31%");
        let experts = m
            .gauges
            .iter()
            .find(|g| g.label == "Experts in VRAM")
            .unwrap();
        assert_eq!(experts.text, "3,099 · 3.0 GB");
    }

    #[test]
    fn recent_requests_read_as_words_and_unreported_figures_as_dashes() {
        let m = build(
            &[serving(generating())],
            &Hardware::default(),
            &History::default(),
        );
        let r = &m.requests;
        assert_eq!(r.columns, REQUEST_COLUMNS, "every column, reported or not");
        assert!(
            r.rows.iter().all(|row| row[5] == "—"),
            "no prefill rate reported"
        );
        assert_eq!(r.rows[0][1], "Done");
        assert_eq!(r.rows[1][1], "Max tokens");
        assert_eq!(r.rows[0][2], "18,187");
        assert_eq!(
            r.rows[1][3], "—",
            "an unreported figure is a dash, not zero"
        );
        assert_eq!(r.note, "2 of 36 kept");
    }

    #[test]
    fn an_engine_with_requests_but_no_stated_phase_keeps_its_counter_speed() {
        use crate::inference::{RequestRow, ServingDetail};
        let mut s = vllm(&[(1, 100, 1000, 2.0), (3, 500, 1000, 2.0)]);
        s.latest.as_mut().unwrap().serving = Some(ServingDetail {
            recent: Some(vec![RequestRow {
                id: Some("r1".into()),
                status: Some("stop".into()),
                ..Default::default()
            }]),
            ..Default::default()
        });
        let m = build(&[s], &Hardware::default(), &History::default());
        assert_eq!(m.strip.lit, Some(Phase::Generating));
        assert_eq!(tile(&m, "Speed").value.as_deref(), Some("200.0"));
        assert_eq!(m.requests.rows[0][1], "Done");
        let fill = m.gauges.iter().find(|g| g.label == "Context fill").unwrap();
        assert_eq!(fill.ratio, None);
        assert!(
            fill.text.contains("does not report its context"),
            "{}",
            fill.text
        );
        assert!(m.strip.progress_note.is_some(), "a missing bar says why");
    }

    #[test]
    fn nothing_answering_lights_no_phase_and_every_engine_tile_says_why() {
        let sampled = Hardware {
            sampled: true,
            ..Default::default()
        };
        let m = build(&[], &sampled, &History::default());
        assert_eq!(m.strip.lit, None);
        assert_eq!(m.focused, None);
        for title in ["Speed", "Latency", "Energy", "Requests"] {
            let t = tile(&m, title);
            assert!(t.value.is_none(), "{title}");
            assert!(t.detail.contains(NO_ENGINE), "{title}: {}", t.detail);
        }
        assert_eq!(tile(&m, "GPU load").detail, NO_GPU);
    }

    #[test]
    fn a_generating_engine_lights_generating_from_its_own_counters() {
        let s = vllm(&[(1, 100, 1000, 2.0), (3, 500, 1000, 2.0)]);
        let m = build(&[s], &Hardware::default(), &History::default());
        assert_eq!(m.strip.lit, Some(Phase::Generating));
        assert!(m.strip.engine.contains("vLLM · stand-in"));
        assert_eq!(tile(&m, "Speed").value.as_deref(), Some("200.0"));
        assert_eq!(tile(&m, "Requests").value.as_deref(), Some("2"));
    }

    #[test]
    fn prompt_tokens_without_output_light_reading_and_none_running_is_idle() {
        let reading = vllm(&[(1, 100, 1000, 1.0), (2, 100, 3000, 1.0)]);
        let m = build(&[reading], &Hardware::default(), &History::default());
        assert_eq!(m.strip.lit, Some(Phase::Reading));
        let idle = vllm(&[(1, 100, 1000, 0.0), (2, 100, 1000, 0.0)]);
        let m = build(&[idle], &Hardware::default(), &History::default());
        assert_eq!(m.strip.lit, Some(Phase::Idle));
    }

    #[test]
    fn one_sample_lights_no_phase_rather_than_guessing() {
        let s = vllm(&[(1, 100, 1000, 1.0)]);
        let m = build(&[s], &Hardware::default(), &History::default());
        assert_eq!(m.strip.lit, None);
        assert!(
            m.strip.detail.contains("second sample"),
            "{}",
            m.strip.detail
        );
        assert!(tile(&m, "Speed").value.is_none());
    }

    #[test]
    fn hardware_tiles_aggregate_only_what_each_gpu_reports() {
        let hw = Hardware {
            sampled: true,
            gpus: vec![
                GpuHw {
                    name: "A".into(),
                    utilization: Some(40.0),
                    temperature: Some(50.0),
                    power_w: Some(100.0),
                    power_limit_w: Some(300.0),
                    ..Default::default()
                },
                GpuHw {
                    name: "B".into(),
                    utilization: None,
                    temperature: Some(70.0),
                    power_w: Some(50.0),
                    power_limit_w: None,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let m = build(&[], &hw, &History::default());
        assert_eq!(tile(&m, "GPU load").value.as_deref(), Some("40"));
        assert_eq!(tile(&m, "GPU load").detail, "GPU0 40% · GPU1 —");
        assert_eq!(tile(&m, "GPU temp").value.as_deref(), Some("70"));
        assert_eq!(tile(&m, "Power").value.as_deref(), Some("150"));
        assert_eq!(tile(&m, "Power").detail, "limit not reported");
        assert!(tile(&m, "VRAM").value.is_none());
    }

    #[test]
    fn history_skips_absent_readings_and_records_once_a_second() {
        let mut h = History::default();
        let t0 = Instant::now();
        let hw = Hardware {
            cpu_utilization: Some(10.0),
            ..Default::default()
        };
        h.record(t0, &hw);
        h.record(t0 + Duration::from_millis(200), &hw);
        h.record(t0 + Duration::from_secs(1), &Hardware::default());
        h.record(t0 + Duration::from_secs(2), &hw);
        assert_eq!(h.series("cpu"), vec![10.0, 10.0]);
        assert!(h.series("gpu").is_empty());
    }

    #[test]
    fn disk_rates_come_from_two_snapshots_and_not_from_a_repeated_one() {
        let mut h = History::default();
        let t0 = Instant::now();
        let at = |generation, read| Hardware {
            sampled: true,
            generation,
            disk_bytes: Some((read, 0, 1)),
            ..Default::default()
        };
        h.record(t0, &at(1, 1_000_000));
        assert_eq!(h.disk_read(&at(1, 1_000_000)), None);
        h.record(t0 + Duration::from_secs(1), &at(1, 1_000_000));
        assert_eq!(h.disk_read(&at(1, 1_000_000)), None);
        h.record(t0 + Duration::from_secs(2), &at(2, 5_000_000));
        assert_eq!(
            h.disk_read(&at(2, 5_000_000)),
            Some((2_000_000.0, Some(0.0)))
        );
        // A counter that goes backwards is a different device set or a reset.
        h.record(t0 + Duration::from_secs(3), &at(3, 10));
        assert_eq!(h.disk_read(&at(3, 10)), None);
    }

    #[test]
    fn an_unsampled_machine_is_not_reported_as_having_no_gpu() {
        let m = build(&[], &Hardware::default(), &History::default());
        assert_eq!(tile(&m, "GPU load").detail, NOT_SAMPLED);
        assert!(!m.tiles.iter().any(|t| t.detail == NO_GPU));
    }

    #[test]
    fn engines_without_request_history_say_so() {
        let s = vllm(&[(1, 1, 1, 0.0)]);
        let m = build(&[s], &Hardware::default(), &History::default());
        assert!(m.requests.rows.is_empty());
        assert!(m
            .requests
            .note
            .contains("vLLM publishes no per-request history"));
    }
}
