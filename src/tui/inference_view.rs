//! The Inference tab's panels: each inference source, what it is running, and the
//! tiers and requests behind it.
//!
//! Every figure is drawn from a [`SourceState`] and is either a value the engine
//! reported this poll, a rate or ratio the sampler derived from two polls, or the
//! word "unknown" with the reason it is absent. Nothing here substitutes zero for a
//! value that was not obtained, and nothing shows a number from a poll that did
//! not succeed.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Sparkline, Table},
    Frame,
};

use crate::inference::ironworks::{
    self, GpuMemory, MonitorGpu, NvmeCounters, NvmeTier, PrefetchCounters, RamCounters, RamTier,
    RecentRequest, Section, Tier, VramCounters, VramTier,
};
use crate::inference::{Derived, ProviderKind, ProviderSample, SourceError, SourceState};

/// A GPU IronMonitor enumerated, with what the engine-to-GPU join and the memory
/// bar need.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuRef {
    pub monitor: MonitorGpu,
    pub name: String,
    /// Total VRAM in bytes, or `None` where the device reported none.
    pub memory_total: Option<u64>,
}

const UNKNOWN: &str = "unknown";
const BAR_WIDTH: usize = 16;

/// Draw every inference source, one band per source, top to bottom.
pub fn draw(f: &mut Frame, area: Rect, states: &[SourceState], gpus: &[GpuRef]) {
    if states.is_empty() {
        let empty = Paragraph::new("no inference providers are configured").block(
            Block::default()
                .borders(Borders::ALL)
                .title("Inference")
                .title_style(title_style()),
        );
        f.render_widget(empty, area);
        return;
    }
    // IronWorks has the full panel. Every other provider is a line in a Local or
    // Remote group, so the tab stays readable with several providers running.
    let ironworks: Vec<&SourceState> = states
        .iter()
        .filter(|s| s.kind == ProviderKind::IronWorks)
        .collect();
    let group = |locality: crate::inference::Locality| -> Vec<&SourceState> {
        states
            .iter()
            .filter(|s| s.kind != ProviderKind::IronWorks && s.locality == locality)
            .collect()
    };
    let local = group(crate::inference::Locality::Local);
    let remote = group(crate::inference::Locality::Remote);

    let mut constraints = Vec::new();
    if !ironworks.is_empty() {
        constraints.push(Constraint::Min(14));
    }
    if !local.is_empty() {
        constraints.push(Constraint::Length(local.len() as u16 + 2));
    }
    if !remote.is_empty() {
        constraints.push(Constraint::Length(remote.len() as u16 + 2));
    }
    let bands = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    let mut band = bands.iter();
    for state in &ironworks {
        if let Some(area) = band.next() {
            draw_provider(f, *area, state, gpus);
        }
    }
    if !local.is_empty() {
        if let Some(area) = band.next() {
            draw_group(f, *area, "Local providers", &local);
        }
    }
    if !remote.is_empty() {
        if let Some(area) = band.next() {
            draw_group(f, *area, "Remote providers", &remote);
        }
    }
}

/// One group of compact provider lines under a bordered title.
fn draw_group(f: &mut Frame, area: Rect, title: &str, states: &[&SourceState]) {
    let lines: Vec<Line<'static>> = states.iter().map(|s| compact_line(s)).collect();
    f.render_widget(Paragraph::new(lines).block(bordered(title)), area);
}

/// A provider's name, its state word and its detail on one line.
fn compact_line(state: &SourceState) -> Line<'static> {
    let row = crate::inference::compact_row(state);
    Line::from(vec![
        Span::styled(
            format!("{:<12}", state.kind.display_name()),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("{:<16}", row.status)),
        Span::raw(row.detail),
    ])
}

fn draw_provider(f: &mut Frame, area: Rect, state: &SourceState, gpus: &[GpuRef]) {
    let current = current_sample(state);
    let reason = no_sample_reason(state);
    let header = header_lines(state);
    let memory = memory_lines(current, reason, gpus);
    let tiers = tier_lines(state, current, reason);
    let has_speed_history = state
        .history
        .get(SPEED_HISTORY)
        .is_some_and(|s| !s.is_empty());
    // Two text rows, plus one for the sparkline when it has points to draw.
    let speed_inner = if has_speed_history { 3 } else { 2 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header.len() as u16 + 2),
            Constraint::Length(speed_inner + 2),
            Constraint::Length(memory.len() as u16 + 2),
            Constraint::Length(tiers.len() as u16 + 2),
            Constraint::Min(3),
        ])
        .split(area);

    let title = format!(
        "{} · {} — {}",
        provider_name(state.kind),
        match state.locality {
            crate::inference::Locality::Local => "Local",
            crate::inference::Locality::Remote => "Remote",
        },
        status_title(state)
    );
    f.render_widget(Paragraph::new(header).block(bordered(&title)), chunks[0]);
    draw_speed(f, chunks[1], state, reason, has_speed_history);
    f.render_widget(Paragraph::new(memory).block(bordered("Memory")), chunks[2]);
    f.render_widget(Paragraph::new(tiers).block(bordered("Tiers")), chunks[3]);
    draw_requests(f, chunks[4], current, reason);
}

// ── Header ───────────────────────────────────────────────────────────────────

fn header_lines(state: &SourceState) -> Vec<Line<'static>> {
    let identity = current_sample(state).map(|s| s.identity.clone());
    let field = |pick: fn(&crate::inference::Identity) -> Option<String>| {
        identity
            .as_ref()
            .and_then(pick)
            .unwrap_or_else(|| UNKNOWN.to_string())
    };
    let model = field(|i| i.model.clone());
    let backend = field(|i| i.backend.clone());

    vec![
        Line::from(format!("endpoint  {}", state.endpoint)),
        Line::from(format!("model {model} · backend {backend}")),
    ]
}

/// Whether the engine answered, as the band's title states it. It lives in the
/// border rather than the body so it survives a short terminal: the body is the
/// first thing a squeezed layout gives up. A metrics-off engine is named as running
/// with its metrics disabled, not as one that did not answer.
fn status_title(state: &SourceState) -> String {
    if state.reachable {
        let age = state
            .last_success
            .map(|t| format!(" · last sample {:.1} s ago", t.elapsed().as_secs_f64()))
            .unwrap_or_default();
        format!("reachable{age}")
    } else if let Some(failure) = &state.last_error {
        failure.error.summary(&state.endpoint)
    } else {
        format!("polling {}", state.endpoint)
    }
}

// ── Speed ────────────────────────────────────────────────────────────────────

const SPEED_HISTORY: &str = "rate:decode.completion_tokens_total";

fn draw_speed(f: &mut Frame, area: Rect, state: &SourceState, reason: &str, sparkline: bool) {
    let block = bordered("Speed");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = if state.reachable {
        vec![
            throughput_line("output", &state.derived, "decode.completion_tokens_total"),
            throughput_line("prompt", &state.derived, "decode.prompt_tokens_total"),
        ]
    } else {
        vec![
            unknown_line("output tok/s", reason),
            unknown_line("prompt tok/s", reason),
        ]
    };

    let rows = if sparkline {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Length(1)])
            .split(inner)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2)])
            .split(inner)
    };
    f.render_widget(Paragraph::new(text), rows[0]);

    if !sparkline {
        return;
    }
    if let Some(series) = state.history.get(SPEED_HISTORY) {
        // Sparkline takes integers; scale by 100 to keep two decimals of tok/s.
        let data: Vec<u64> = series
            .iter()
            .map(|p| (p.value.max(0.0) * 100.0).round() as u64)
            .collect();
        let spark = Sparkline::default()
            .data(&data)
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(spark, rows[1]);
    }
}

/// One throughput line, derived from a decode counter across two polls. There is
/// no engine-reported figure to fall back on: the engine does not send one.
fn throughput_line(label: &str, derived: &Derived, counter: &str) -> Line<'static> {
    match derived.rate(counter) {
        Some(rate) => {
            let window = derived
                .window_ns
                .map(|ns| format!(" over {:.1} s", ns as f64 / 1e9))
                .unwrap_or_default();
            Line::from(format!("{label:<7} {rate:>9.1} tok/s  measured{window}"))
        }
        None => Line::from(format!("{label:<7} unknown  needs a second sample")),
    }
}

// ── Memory ───────────────────────────────────────────────────────────────────

fn memory_lines(
    current: Option<&ProviderSample>,
    reason: &str,
    gpus: &[GpuRef],
) -> Vec<Line<'static>> {
    let Some(section) = current.and_then(|s| s.ironworks.as_ref()) else {
        return vec![unknown_line("engine VRAM", reason)];
    };
    let monitors: Vec<MonitorGpu> = gpus.iter().map(|g| g.monitor.clone()).collect();
    let mut lines = Vec::new();
    match &section.per_gpu {
        None => lines.push(unknown_line(
            "engine VRAM",
            "the engine reported no per-GPU memory",
        )),
        Some(list) if list.is_empty() => lines.push(Line::from("no GPUs reported by the engine")),
        Some(list) => {
            for engine_gpu in list {
                lines.push(gpu_memory_line(engine_gpu, &monitors, gpus));
            }
        }
    }
    lines.push(Line::from(format!(
        "host pinned {}",
        opt_bytes(section.host_pinned_bytes)
    )));
    lines
}

fn gpu_memory_line(
    engine_gpu: &GpuMemory,
    monitors: &[MonitorGpu],
    gpus: &[GpuRef],
) -> Line<'static> {
    match ironworks::match_gpu(engine_gpu, monitors) {
        Ok(index) => {
            let gpu = gpus.iter().find(|g| g.monitor.index == index);
            let name = gpu.map_or(UNKNOWN.to_string(), |g| g.name.clone());
            let used = engine_gpu.engine_vram_used_bytes;
            let total = gpu.and_then(|g| g.memory_total);
            let total_text = total.map_or("total not read".to_string(), fmt_bytes);
            let mut spans = vec![Span::raw(format!(
                "GPU {index} {name}  engine VRAM {} of {total_text}  ",
                opt_bytes(used)
            ))];
            match (used, total) {
                (Some(u), Some(t)) if t > 0 => {
                    let ratio = (u as f64 / t as f64).clamp(0.0, 1.0);
                    spans.push(Span::styled(bar(ratio), Style::default().fg(Color::Cyan)));
                    spans.push(Span::raw(format!(" {:.1}%", u as f64 / t as f64 * 100.0)));
                }
                _ => spans.push(Span::raw("no bar: a value is unknown")),
            }
            Line::from(spans)
        }
        Err(why) => Line::from(Span::styled(
            format!(
                "engine GPU {} not matched to a GPU here: {why}",
                engine_gpu
                    .device_index
                    .map_or(UNKNOWN.to_string(), |d| d.to_string())
            ),
            Style::default().fg(Color::Yellow),
        )),
    }
}

fn bar(ratio: f64) -> String {
    let filled = (ratio * BAR_WIDTH as f64).round() as usize;
    format!(
        "{}{}",
        "█".repeat(filled.min(BAR_WIDTH)),
        "░".repeat(BAR_WIDTH - filled.min(BAR_WIDTH))
    )
}

// ── Tiers ────────────────────────────────────────────────────────────────────

/// The tier rows. Prefetch is listed only when the engine reports it, which it does
/// only when an expert store exists.
fn tier_lines(
    state: &SourceState,
    current: Option<&ProviderSample>,
    reason: &str,
) -> Vec<Line<'static>> {
    let Some(section) = current.and_then(|s| s.ironworks.as_ref()) else {
        return vec![unknown_line("tiers", reason)];
    };
    let d = &state.derived;
    let mut lines = vec![
        vram_line(section.vram.as_ref(), d),
        ram_line(section.ram.as_ref(), d),
        nvme_line(section.nvme.as_ref(), d),
    ];
    if let Some(prefetch) = section.prefetch.as_ref() {
        lines.push(prefetch_line(prefetch));
    }
    lines
}

fn vram_line(tier: Option<&VramTier>, d: &Derived) -> Line<'static> {
    match tier {
        None => unknown_line("VRAM", "the engine reported no VRAM tier"),
        Some(Tier::NotConfigured { reason }) => not_configured("VRAM", reason.as_deref()),
        Some(Tier::Configured(VramCounters {
            experts_resident,
            capacity_experts,
            evictions_total,
            ..
        })) => Line::from(format!(
            "VRAM     resident {} of {} experts · hit {} · evictions {}",
            opt(*experts_resident),
            opt(*capacity_experts),
            hit_text(d, "tier.vram"),
            opt(*evictions_total),
        )),
    }
}

fn ram_line(tier: Option<&RamTier>, d: &Derived) -> Line<'static> {
    match tier {
        None => unknown_line("RAM", "the engine reported no RAM tier"),
        Some(Tier::NotConfigured { reason }) => not_configured("RAM", reason.as_deref()),
        Some(Tier::Configured(RamCounters {
            experts_resident,
            pinned_bytes,
            ..
        })) => Line::from(format!(
            "RAM      resident {} experts · hit {} · pinned {}",
            opt(*experts_resident),
            hit_text(d, "tier.ram"),
            opt_bytes(*pinned_bytes),
        )),
    }
}

fn nvme_line(tier: Option<&NvmeTier>, d: &Derived) -> Line<'static> {
    match tier {
        None => unknown_line("NVMe", "the engine reported no NVMe tier"),
        Some(Tier::NotConfigured { reason }) => not_configured("NVMe", reason.as_deref()),
        Some(Tier::Configured(NvmeCounters {
            queue_depth,
            inflight_peak,
            latency_max_ns,
            ..
        })) => {
            let read = match d.rate("tier.nvme.bytes_total") {
                Some(bps) => format!("{}/s", fmt_bytes(bps.max(0.0) as u64)),
                None => "unknown (needs a second sample)".to_string(),
            };
            Line::from(format!(
                "NVMe     read {read} · queue depth {} · peak in flight {} · slowest read {} (peak)",
                opt(*queue_depth),
                opt(*inflight_peak),
                latency_max_ns.map_or(UNKNOWN.to_string(), |ns| {
                    format!("{:.1} ms", ns as f64 / 1e6)
                }),
            ))
        }
    }
}

fn prefetch_line(p: &PrefetchCounters) -> Line<'static> {
    Line::from(format!(
        "prefetch requested {} · useful {} · wasted {} · queue peak {}",
        opt(p.requested_total),
        opt(p.useful_total),
        opt(p.wasted_total),
        opt(p.queue_peak),
    ))
}

/// A hit percentage over the window between the two most recent polls, or unknown.
fn hit_text(d: &Derived, key: &str) -> String {
    d.ratio(key)
        .map_or(UNKNOWN.to_string(), |r| format!("{:.1}%", r * 100.0))
}

fn not_configured(label: &str, reason: Option<&str>) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            "{label:<8} not configured — {}",
            reason.unwrap_or("the engine gave no reason")
        ),
        Style::default().fg(Color::DarkGray),
    ))
}

// ── Requests ─────────────────────────────────────────────────────────────────

/// The recent-request table is drawn only when the engine reports a list. Without
/// one, the panel carries the totals alone rather than an empty table.
fn draw_requests(f: &mut Frame, area: Rect, current: Option<&ProviderSample>, reason: &str) {
    let Some(section) = current.and_then(|s| s.ironworks.as_ref()) else {
        let text = unknown_line("requests", reason);
        f.render_widget(Paragraph::new(text).block(bordered("Requests")), area);
        return;
    };
    let total = current
        .and_then(|s| s.counters.get("requests.total"))
        .copied();
    let Some(recent) = section.recent.as_deref() else {
        f.render_widget(
            Paragraph::new(requests_totals_line(section, total)).block(bordered("Requests")),
            area,
        );
        return;
    };

    let title = format!(
        "Recent requests — {} of {} total · {} active now",
        recent.len(),
        opt(total),
        opt(section.requests_active),
    );
    let header = Row::new(["id", "in", "out", "ms", "finish"])
        .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row> = recent.iter().map(request_row).collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(9),
            Constraint::Min(8),
        ],
    )
    .header(header)
    .block(bordered(&title));
    f.render_widget(table, area);
}

fn requests_totals_line(section: &Section, total: Option<u64>) -> Line<'static> {
    Line::from(format!(
        "{} total · {} active now · no recent-request list reported",
        opt(total),
        opt(section.requests_active),
    ))
}

fn request_row(r: &RecentRequest) -> Row<'static> {
    let id = match &r.id {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => UNKNOWN.to_string(),
    };
    Row::new(vec![
        id,
        opt(r.prompt_tokens),
        opt(r.completion_tokens),
        opt(r.duration_ms),
        r.finish_reason
            .clone()
            .unwrap_or_else(|| UNKNOWN.to_string()),
    ])
}

// ── Shared helpers ───────────────────────────────────────────────────────────

/// The sample to read: only when the most recent poll succeeded. A stale sample is
/// not presented as current.
fn current_sample(state: &SourceState) -> Option<&ProviderSample> {
    if state.reachable {
        state.latest.as_ref()
    } else {
        None
    }
}

/// Why the panels have no current figures. A metrics-off engine is said to be
/// running, so its panels do not read as though nothing answered.
fn no_sample_reason(state: &SourceState) -> &'static str {
    if state.reachable {
        return "the sample has no IronWorks section";
    }
    match state.last_error.as_ref().map(|f| &f.error) {
        Some(SourceError::MetricsNotEnabled) => {
            "the engine is running, but its metrics are not enabled"
        }
        _ => "no engine answered",
    }
}

fn provider_name(kind: ProviderKind) -> &'static str {
    kind.display_name()
}

fn title_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn bordered(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(title.to_string())
        .title_style(title_style())
}

fn unknown_line(label: &str, reason: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!("{label:<8} ")),
        Span::styled(
            format!("unknown — {reason}"),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

fn opt<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or(UNKNOWN.to_string(), |v| v.to_string())
}

fn opt_bytes(value: Option<u64>) -> String {
    value.map_or(UNKNOWN.to_string(), fmt_bytes)
}

/// Binary units, one decimal place above bytes.
fn fmt_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::ironworks::parse_metrics;
    use crate::inference::{Failure, InferenceSampler, Locality};
    use ratatui::{backend::TestBackend, Terminal};
    use std::time::Instant;

    const A_NVME_CONFIGURED: &str =
        include_str!("../../tests/fixtures/inference/a_nvme_configured.json");
    const B_NVME_NOT_CONFIGURED: &str =
        include_str!("../../tests/fixtures/inference/b_nvme_not_configured.json");
    const C_COUNTER_OMITTED: &str =
        include_str!("../../tests/fixtures/inference/c_counter_omitted.json");

    const ENDPOINT: &str = "http://127.0.0.1:8080";

    /// A reachable source built the way the poller builds one: the body is parsed,
    /// mapped onto the shared sample, and fed through the sampler.
    fn reachable(body: &str) -> SourceState {
        let sample = parse_metrics(body).expect("fixture parses").into_sample();
        let mut sampler = InferenceSampler::default();
        sampler.update(sample.clone());
        SourceState {
            kind: ProviderKind::IronWorks,
            locality: Locality::Local,
            endpoint: ENDPOINT.to_string(),
            latest: Some(sample),
            reachable: true,
            last_success: Some(Instant::now()),
            last_error: None,
            derived: sampler.derived().clone(),
            history: sampler.history().clone(),
        }
    }

    /// A reachable source with two polls two seconds of uptime apart. Every counter
    /// rises by `bump`, so the rates and ratios are the bump over the window.
    fn with_window(body: &str, bump: impl Fn(&str) -> u64) -> SourceState {
        let first = parse_metrics(body).expect("fixture parses").into_sample();
        let mut sampler = InferenceSampler::default();
        sampler.update(first.clone());
        let mut later = first;
        later.uptime_ns = later.uptime_ns.map(|t| t + 2_000_000_000);
        for (name, value) in later.counters.iter_mut() {
            *value += bump(name);
        }
        sampler.update(later.clone());
        SourceState {
            kind: ProviderKind::IronWorks,
            locality: Locality::Local,
            endpoint: ENDPOINT.to_string(),
            latest: Some(later),
            reachable: true,
            last_success: Some(Instant::now()),
            last_error: None,
            derived: sampler.derived().clone(),
            history: sampler.history().clone(),
        }
    }

    fn unreachable(error: SourceError) -> SourceState {
        SourceState {
            kind: ProviderKind::IronWorks,
            locality: Locality::Local,
            endpoint: ENDPOINT.to_string(),
            latest: None,
            reachable: false,
            last_success: None,
            last_error: Some(Failure {
                error,
                at: Instant::now(),
            }),
            derived: Derived::default(),
            history: Default::default(),
        }
    }

    fn gpus() -> Vec<GpuRef> {
        vec![GpuRef {
            monitor: MonitorGpu {
                index: 0,
                pci_bus_id: Some("0000:03:00.0".into()),
                uuid: None,
            },
            name: "NVIDIA RTX 4090".into(),
            memory_total: Some(24 * 1024 * 1024 * 1024),
        }]
    }

    /// The rendered text of the panels, one string per row.
    fn render(states: &[SourceState], gpus: &[GpuRef]) -> String {
        let (w, h) = (130u16, 44u16);
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("test backend");
        terminal
            .draw(|f| draw(f, f.area(), states, gpus))
            .expect("draw into memory");
        let buffer = terminal.backend().buffer();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn unreachable_engine_says_who_did_not_answer_and_why() {
        let frame = render(
            &[unreachable(SourceError::Unreachable(
                "connection refused".into(),
            ))],
            &[],
        );
        assert!(
            frame.contains("no engine answered at http://127.0.0.1:8080: connection refused"),
            "{frame}"
        );
        assert!(!frame.contains("tok/s  measured"), "{frame}");
    }

    #[test]
    fn metrics_off_engine_is_named_as_running_not_as_unanswering() {
        let frame = render(&[unreachable(SourceError::MetricsNotEnabled)], &[]);
        assert!(
            frame.contains("engine running, metrics not enabled — start it with --metrics"),
            "{frame}"
        );
        assert!(!frame.contains("no engine answered"), "{frame}");
        assert!(
            frame.contains("the engine is running, but its metrics are not enabled"),
            "{frame}"
        );
    }

    #[test]
    fn configured_nvme_shows_its_queue_and_its_peak_labelled_as_one() {
        let frame = render(&[reachable(A_NVME_CONFIGURED)], &gpus());
        assert!(frame.contains("queue depth 2"), "{frame}");
        assert!(frame.contains("peak in flight 4"), "{frame}");
        assert!(frame.contains("(peak)"), "{frame}");
        assert!(!frame.contains("not configured"), "{frame}");
    }

    #[test]
    fn not_configured_nvme_shows_the_engines_reason() {
        let frame = render(&[reachable(B_NVME_NOT_CONFIGURED)], &gpus());
        assert!(
            frame.contains("NVMe     not configured — model fits in RAM+VRAM; no NVMe tier"),
            "{frame}"
        );
    }

    #[test]
    fn omitted_counter_is_unknown_not_zero() {
        // A single sample has no prompt rate, so the line must read unknown.
        let frame = render(&[reachable(C_COUNTER_OMITTED)], &gpus());
        let prompt = frame
            .lines()
            .find(|l| l.contains("prompt"))
            .expect("the speed panel has a prompt row");
        assert!(prompt.contains("unknown"), "{prompt}");
        assert!(!prompt.contains("0.0 tok/s"), "{prompt}");
    }

    #[test]
    fn throughput_is_unknown_until_a_second_sample_and_then_measured() {
        let single = render(&[reachable(A_NVME_CONFIGURED)], &gpus());
        assert!(
            single.contains("unknown  needs a second sample"),
            "{single}"
        );
        assert!(!single.contains("engine-reported"), "{single}");

        let two = render(
            &[with_window(A_NVME_CONFIGURED, |n| {
                if n == "decode.completion_tokens_total" {
                    40
                } else {
                    0
                }
            })],
            &gpus(),
        );
        assert!(two.contains("20.0 tok/s  measured over 2.0 s"), "{two}");
    }

    #[test]
    fn hit_ratios_are_shown_over_the_window_between_two_polls() {
        // Every VRAM and RAM counter gains 100 in the window: VRAM is 100 of 400
        // picks, a quarter; RAM is 100 of 200 lookups, a half.
        let frame = render(
            &[with_window(A_NVME_CONFIGURED, |n| {
                if n.starts_with("tier.vram.") || n.starts_with("tier.ram.") {
                    100
                } else {
                    0
                }
            })],
            &gpus(),
        );
        assert!(frame.contains("hit 25.0%"), "{frame}");
        assert!(frame.contains("hit 50.0%"), "{frame}");
    }

    #[test]
    fn a_single_poll_shows_no_hit_ratio() {
        let frame = render(&[reachable(A_NVME_CONFIGURED)], &gpus());
        assert!(frame.contains("hit unknown"), "{frame}");
    }

    #[test]
    fn prefetch_is_shown_when_the_engine_reports_it_and_not_otherwise() {
        let with_store = render(&[reachable(A_NVME_CONFIGURED)], &gpus());
        assert!(
            with_store.contains("prefetch requested 41922"),
            "{with_store}"
        );
        assert!(with_store.contains("queue peak 16"), "{with_store}");
        let without = render(&[reachable(B_NVME_NOT_CONFIGURED)], &gpus());
        assert!(!without.contains("prefetch"), "{without}");
    }

    #[test]
    fn engine_gpu_is_joined_by_pci_and_shows_its_total() {
        let frame = render(&[reachable(A_NVME_CONFIGURED)], &gpus());
        assert!(
            frame.contains("GPU 0 NVIDIA RTX 4090  engine VRAM 44.0 GiB of 24.0 GiB"),
            "{frame}"
        );
    }

    #[test]
    fn engine_gpu_with_no_monitor_match_says_why() {
        let frame = render(&[reachable(A_NVME_CONFIGURED)], &[]);
        assert!(frame.contains("not matched to a GPU here"), "{frame}");
    }

    #[test]
    fn no_sources_says_so_rather_than_drawing_nothing() {
        let frame = render(&[], &[]);
        assert!(
            frame.contains("no inference providers are configured"),
            "{frame}"
        );
    }

    #[test]
    fn recent_requests_show_how_many_of_the_total_are_listed() {
        let frame = render(&[reachable(A_NVME_CONFIGURED)], &gpus());
        assert!(frame.contains("1 of 642 total"), "{frame}");
        assert!(frame.contains("cmpl-7f3"), "{frame}");
    }

    #[test]
    fn a_missing_recent_list_is_not_drawn_as_an_empty_table() {
        let frame = render(
            &[reachable(
                r#"{"schema_version":1,"requests":{"total":3,"active":0}}"#,
            )],
            &gpus(),
        );
        assert!(frame.contains("no recent-request list reported"), "{frame}");
    }
}
