//! The Inference monitor, laid out like a model-serving dashboard: a model-state
//! strip, a grid of tiles with sparklines, gauges, and the engine's recent requests.
//!
//! It is built here once, from the engine states and the machine's own readings,
//! and both the TUI and the GUI draw the same [`Monitor`]. A tile whose value could
//! not be obtained has `value: None` and its reason in `detail`; nothing here puts
//! a zero, a previous sample or a constant in place of a reading.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use super::{energy, fmt_seconds, mean_label, Locality, ProviderKind, SourceState};

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
    /// The lit phase, or `None` when the engine's figures do not say.
    pub lit: Option<Phase>,
    pub detail: String,
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
        _ => (None, None),
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
        return StateStrip {
            engine: "no engine answered".into(),
            lit: None,
            detail: match local.as_slice() {
                [] => "no local inference engine is configured".into(),
                [only] => super::compact_row(only).detail,
                many => format!("{} local engines polled; none has answered", many.len()),
            },
        };
    };
    let name = state.kind.display_name();
    let model = state
        .latest
        .as_ref()
        .and_then(|s| s.identity.model.clone())
        .unwrap_or_else(|| "model not reported".into());
    let engine = format!("{name} · {model} · {}", state.endpoint);
    if !state.reachable {
        let why = state
            .last_error
            .as_ref()
            .map(|f| f.error.to_string())
            .unwrap_or_else(|| "the last poll failed".into());
        return StateStrip {
            engine,
            lit: Some(Phase::Error),
            detail: format!("stopped answering: {why}"),
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
        engine,
        lit,
        detail,
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

fn hardware_tiles(hw: &Hardware, history: &History) -> Vec<Tile> {
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

fn speed_tile(state: Option<&SourceState>) -> Tile {
    let Some(state) = state.filter(|s| s.reachable) else {
        return absent("Speed", NO_ENGINE);
    };
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
    if d.window_ns.is_none() {
        return absent("Latency", NO_WINDOW);
    }
    let known: Vec<(&str, f64)> = sample
        .mean_defs
        .iter()
        .filter_map(|def| Some((def.key, d.mean(def.key)?)))
        .collect();
    let Some(&(first, value)) = known.first() else {
        return absent("Latency", "no request completed in the last window");
    };
    let rest: Vec<String> = known[1..]
        .iter()
        .map(|(k, v)| format!("{} {}", mean_label(k).to_lowercase(), fmt_seconds(*v)))
        .collect();
    let mut detail = mean_label(first).to_lowercase();
    if !rest.is_empty() {
        detail = format!("{detail} · {}", rest.join(" · "));
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
    let kv = answering.and_then(|s| {
        let v = s.latest.as_ref()?.gauges.get("kv_cache_usage_perc")?;
        Some(v.clamp(0.0, 1.0))
    });
    out.push(match (kv, answering) {
        (Some(f), _) => Gauge {
            label: "KV cache",
            text: format!("{:.0}%", f * 100.0),
            ratio: Some(f),
        },
        (None, Some(s)) => Gauge {
            label: "KV cache",
            text: format!("{} does not report it", s.kind.display_name()),
            ratio: None,
        },
        (None, None) => Gauge {
            label: "KV cache",
            text: NO_ENGINE.into(),
            ratio: None,
        },
    });
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

fn requests(state: Option<&SourceState>) -> Requests {
    let columns = vec!["ID", "STATUS", "PROMPT", "OUTPUT", "DURATION"];
    let empty = |note: String| Requests {
        columns: columns.clone(),
        rows: Vec::new(),
        note,
    };
    let Some(state) = state.filter(|s| s.reachable) else {
        return empty(NO_ENGINE.into());
    };
    let name = state.kind.display_name();
    let section = state.latest.as_ref().and_then(|s| s.ironworks.as_ref());
    let Some(recent) = section.and_then(|s| s.recent.as_ref()) else {
        return empty(if state.kind == ProviderKind::IronWorks {
            "IronWorks does not publish its recent requests yet".into()
        } else {
            format!("{name} publishes no per-request history; the Latency tile has its means")
        });
    };
    let dash = |v: Option<u64>| v.map_or_else(|| "—".to_string(), |v| v.to_string());
    let rows: Vec<Vec<String>> = recent
        .iter()
        .map(|r| {
            vec![
                match &r.id {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(other) => other.to_string(),
                    None => "—".into(),
                },
                r.finish_reason.clone().unwrap_or_else(|| "—".into()),
                dash(r.prompt_tokens),
                dash(r.completion_tokens),
                r.duration_ms
                    .map_or_else(|| "—".into(), |ms| format!("{:.1} s", ms as f64 / 1000.0)),
            ]
        })
        .collect();
    let total = state
        .latest
        .as_ref()
        .and_then(|s| s.counters.get("requests.total"));
    let note = match (rows.is_empty(), total) {
        (true, _) => "no requests yet".into(),
        (false, Some(t)) => format!("the last {} of {t} requests", rows.len()),
        (false, None) => format!("the last {} requests", rows.len()),
    };
    Requests {
        columns,
        rows,
        note,
    }
}

/// Build the monitor from the engine states and the machine's readings.
pub fn build(states: &[SourceState], hw: &Hardware, history: &History) -> Monitor {
    let focused = focus(states);
    let state = focused.map(|i| &states[i]);
    let mut tiles = vec![speed_tile(state)];
    tiles.extend(hardware_tiles(hw, history));
    tiles.push(latency_tile(state));
    tiles.push(energy_tile(state));
    tiles.push(cost_tile(states, state));
    tiles.push(requests_tile(state));
    Monitor {
        strip: strip(states, focused),
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
