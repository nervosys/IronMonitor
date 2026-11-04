//! Inference tab: what each inference engine reports, and the agent box that asks
//! questions about this machine.
//!
//! The figures come from `crate::inference`, which polls every engine on its own
//! thread. This file only draws them. The rule it follows is the ontology's: a
//! value the engine did not report is shown as unknown, with the engine's reason
//! where it gave one, and is never shown as zero. A rate or ratio needs two samples,
//! so it is shown only once the poller has two; each is named for what it is.

use std::collections::VecDeque;
use std::fmt::Display;
use std::time::Duration;

use egui::RichText;

use crate::gpu::{GpuDynamicInfo, GpuStaticInfo};
use crate::inference::ironworks::{
    match_gpu, GpuMemory, IronWorksSource, MonitorGpu, NvmeCounters, NvmeTier, PrefetchCounters,
    RamCounters, RamTier, RecentRequest, Section, Tier, VramCounters, VramTier,
};
use crate::inference::{
    Derived, Identity, InferencePoller, InferenceSource, Locality, ProviderKind, SourceError,
    SourceState,
};

use super::app::IronMonitorApp;
use super::theme::{self, CyberColors};
use super::widgets::{domain_section_title, SectionHeader, SparklineChart};

/// How often each engine is polled.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// What a value reads as when the engine did not give it.
const UNKNOWN: &str = "unknown";

/// Counters the throughput figures are derived from.
const COMPLETION_TOKENS: &str = "decode.completion_tokens_total";
const PROMPT_TOKENS: &str = "decode.prompt_tokens_total";
/// The history series the speed sparkline is drawn from.
const SPARK_SERIES: &str = "rate:decode.completion_tokens_total";

/// The background poller for every inference engine IronMonitor knows about.
///
/// Called once, from `IronMonitorApp::with_context`, so each app owns one poller and
/// dropping the app stops its threads.
pub(super) fn spawn_poller() -> InferencePoller {
    let mut sources: Vec<Box<dyn InferenceSource>> = vec![Box::new(IronWorksSource::from_env())];
    sources.extend(crate::inference::local::all_from_env());
    InferencePoller::spawn(sources, POLL_INTERVAL)
}

/// Whether some engine has not yet produced its first answer or failure. Until it
/// has, the tab says it is waiting rather than reporting that nothing answered.
pub(super) fn awaiting_first_answer(poller: &InferencePoller) -> bool {
    poller
        .states()
        .iter()
        .any(|s| s.last_success.is_none() && s.last_error.is_none())
}

/// One GPU IronMonitor enumerates, with what is needed to join an engine's GPU to it.
#[derive(Debug, Clone)]
struct GpuView {
    monitor: MonitorGpu,
    name: String,
    vram_total: Option<u64>,
}

/// The IronMonitor GPUs, paired with their static and dynamic readings.
fn gpu_views(statics: &[GpuStaticInfo], dynamics: &[GpuDynamicInfo]) -> Vec<GpuView> {
    statics
        .iter()
        .enumerate()
        .map(|(i, s)| GpuView {
            monitor: MonitorGpu {
                index: s.index,
                pci_bus_id: s.pci_bus_id.clone(),
                uuid: s.uuid.clone(),
            },
            name: s.name.clone(),
            vram_total: dynamics.get(i).and_then(|d| d.memory.total),
        })
        .collect()
}

impl IronMonitorApp {
    /// The Inference tab: each engine's header, speed, memory, tiers and recent
    /// requests, then the agent box shared with the AI and Overview tabs.
    pub(super) fn draw_inference_tab(&mut self, ui: &mut egui::Ui) {
        // Engines answer about once a second; wake the tab so each answer shows.
        ui.ctx().request_repaint_after(POLL_INTERVAL);

        let states = self.inference.states();
        let gpus = gpu_views(&self.gpu_static_info, &self.gpu_dynamic_info);
        let reply = self.latest_agent_reply();
        let secondary = theme::text_color(ui.ctx(), CyberColors::TEXT_SECONDARY);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if states.is_empty() {
                    ui.label(RichText::new("No inference engine is configured.").color(secondary));
                }
                // IronWorks has the full panel. Every other provider is a compact line,
                // grouped as Local or Remote.
                for state in states.iter().filter(|s| s.kind == ProviderKind::IronWorks) {
                    draw_source(ui, state, &gpus);
                    ui.add_space(12.0);
                }
                for (title, locality) in [
                    ("Local providers", Locality::Local),
                    ("Remote providers", Locality::Remote),
                ] {
                    let group: Vec<&SourceState> = states
                        .iter()
                        .filter(|s| s.kind != ProviderKind::IronWorks && s.locality == locality)
                        .collect();
                    if group.is_empty() {
                        continue;
                    }
                    ui.add(SectionHeader::new(title));
                    for state in group {
                        draw_compact(ui, state);
                    }
                    ui.add_space(12.0);
                }

                ui.add(SectionHeader::new("Agent"));
                self.draw_agent_composer(ui);
                if let Some((text, ms)) = reply {
                    ui.add_space(4.0);
                    let primary = theme::text_color(ui.ctx(), CyberColors::TEXT_PRIMARY);
                    ui.label(RichText::new(text).color(primary));
                    if let Some(ms) = ms {
                        ui.label(RichText::new(format!("{ms} ms")).small().color(secondary));
                    }
                }
            });
    }
}

/// One engine, top to bottom.
fn draw_source(ui: &mut egui::Ui, state: &SourceState, gpus: &[GpuView]) {
    ui.heading(
        RichText::new(provider_name(state.kind)).color(theme::color(ui.ctx(), CyberColors::CYAN)),
    );
    draw_identity(ui, state);

    if !state.reachable {
        draw_unanswered(ui, state);
        return;
    }
    let Some(section) = state.latest.as_ref().and_then(|s| s.ironworks.as_ref()) else {
        ui.label(
            RichText::new("This engine answered with detail IronMonitor does not read.")
                .color(theme::text_color(ui.ctx(), CyberColors::TEXT_SECONDARY)),
        );
        return;
    };

    draw_speed(ui, state);
    draw_memory(ui, section, gpus);
    draw_tiers(ui, state, section);
    draw_requests(ui, state, section);
}

fn provider_name(kind: ProviderKind) -> &'static str {
    kind.display_name()
}

/// A provider the tab does not draw in full: its name, its state word, and its
/// detail or reason on one line.
fn draw_compact(ui: &mut egui::Ui, state: &SourceState) {
    let row = crate::inference::compact_row(state);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{:<12}", provider_name(state.kind)))
                .strong()
                .color(theme::color(ui.ctx(), CyberColors::CYAN)),
        );
        ui.label(
            RichText::new(format!("{:<13}", row.status))
                .color(theme::text_color(ui.ctx(), CyberColors::TEXT_PRIMARY)),
        );
        ui.label(
            RichText::new(row.detail)
                .color(theme::text_color(ui.ctx(), CyberColors::TEXT_SECONDARY)),
        );
    });
}

/// Where the engine runs, where it is, and what it says it is running. Identity is
/// shown only from an engine that answered this poll; an earlier answer is stale.
fn draw_identity(ui: &mut egui::Ui, state: &SourceState) {
    let identity = match (&state.latest, state.reachable) {
        (Some(sample), true) => sample.identity.clone(),
        _ => Identity::default(),
    };
    let locality = match state.locality {
        Locality::Local => "Local",
        Locality::Remote => "Remote",
    };
    let secondary = theme::text_color(ui.ctx(), CyberColors::TEXT_SECONDARY);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(locality).color(secondary));
        ui.label(RichText::new(&state.endpoint).color(secondary));
        let (text, color) = if state.reachable {
            ("reachable", CyberColors::SUCCESS)
        } else if matches!(
            state.last_error.as_ref().map(|f| &f.error),
            Some(SourceError::MetricsNotEnabled)
        ) {
            ("running, metrics off", CyberColors::WARNING)
        } else {
            ("not reachable", CyberColors::WARNING)
        };
        ui.label(RichText::new(text).color(theme::text_color(ui.ctx(), color)));
    });
    kv(
        ui,
        "Model",
        identity.model.unwrap_or_else(|| UNKNOWN.to_string()),
    );
    kv(
        ui,
        "Backend",
        identity.backend.unwrap_or_else(|| UNKNOWN.to_string()),
    );
}

/// What an engine that did not answer this poll shows in place of its figures. A
/// running engine with metrics off is named as running, not as unanswering.
fn draw_unanswered(ui: &mut egui::Ui, state: &SourceState) {
    let warning = theme::text_color(ui.ctx(), CyberColors::WARNING);
    let secondary = theme::text_color(ui.ctx(), CyberColors::TEXT_SECONDARY);
    match &state.last_error {
        Some(failure) => {
            ui.label(RichText::new(failure.error.summary(&state.endpoint)).color(warning));
            if matches!(
                failure.error,
                crate::inference::SourceError::MetricsNotEnabled
            ) {
                ui.label(
                    RichText::new(
                        "The engine answers, but it serves no figures until it starts with metrics on.",
                    )
                    .color(secondary),
                );
            }
        }
        None => {
            ui.label(
                RichText::new(format!(
                    "waiting for the first answer from {}",
                    state.endpoint
                ))
                .color(secondary),
            );
        }
    }
    if let Some(at) = state.last_success {
        ui.label(
            RichText::new(format!(
                "last answered {} s ago; its figures are not shown as current",
                at.elapsed().as_secs()
            ))
            .color(secondary),
        );
    }
    ui.label(
        RichText::new("Speed, memory, tiers and requests appear once the engine answers.")
            .color(secondary),
    );
}

fn draw_speed(ui: &mut egui::Ui, state: &SourceState) {
    ui.add(SectionHeader::new(&domain_section_title(
        "inference",
        "Speed",
    )));
    let derived = &state.derived;
    kv(ui, "Output", throughput(derived, COMPLETION_TOKENS));
    kv(ui, "Prompt", throughput(derived, PROMPT_TOKENS));
    draw_sparkline(ui, state);
}

/// Tokens per second, derived from a decode counter across two polls. The engine
/// does not report a figure of its own, so none is shown in its place.
fn throughput(derived: &Derived, counter: &str) -> String {
    match derived.rate(counter) {
        Some(rate) => format!(
            "{rate:.1} tok/s, measured over {}",
            derived
                .window_ns
                .map(|ns| format!("{:.1} s", ns as f64 / 1e9))
                .unwrap_or_else(|| "one poll".to_string())
        ),
        None => format!("{UNKNOWN} (needs a second sample)"),
    }
}

/// The speed history, drawn only when there is one. An empty series is not a flat
/// line at zero: it is no measurement yet.
fn draw_sparkline(ui: &mut egui::Ui, state: &SourceState) {
    let Some(series) = state.history.get(SPARK_SERIES) else {
        return;
    };
    if series.is_empty() {
        return;
    }
    let data: VecDeque<f32> = series.iter().map(|p| p.value as f32).collect();
    ui.add(SparklineChart::from_history(&data).height(40.0));
}

fn draw_memory(ui: &mut egui::Ui, section: &Section, gpus: &[GpuView]) {
    ui.add(SectionHeader::new(&domain_section_title(
        "inference",
        "Memory",
    )));
    match &section.per_gpu {
        None => note(ui, "The engine reported no per-GPU memory."),
        Some(list) if list.is_empty() => note(ui, "The engine reported no GPUs."),
        Some(list) => {
            for gpu in list {
                draw_gpu_memory(ui, gpu, gpus);
            }
        }
    }
    kv(
        ui,
        "Host pinned",
        bytes_or_unknown(section.host_pinned_bytes),
    );
}

/// One engine GPU. It is joined to IronMonitor's GPU by PCI address, then UUID, and
/// never by index; when neither joins, the engine's usage is shown unattributed.
fn draw_gpu_memory(ui: &mut egui::Ui, gpu: &GpuMemory, gpus: &[GpuView]) {
    let monitors: Vec<MonitorGpu> = gpus.iter().map(|v| v.monitor.clone()).collect();
    let used = gpu.engine_vram_used_bytes;
    match match_gpu(gpu, &monitors) {
        Ok(index) => {
            let view = gpus.iter().find(|v| v.monitor.index == index);
            let title = match view {
                Some(v) => format!("GPU {index} · {}", v.name),
                None => format!("GPU {index}"),
            };
            ui.label(
                RichText::new(title)
                    .strong()
                    .color(theme::text_color(ui.ctx(), CyberColors::TEXT_PRIMARY)),
            );
            draw_vram_use(ui, used, view.and_then(|v| v.vram_total));
        }
        Err(reason) => {
            let device = gpu
                .device_index
                .map(|d| format!("Engine device {d}"))
                .unwrap_or_else(|| "An engine GPU".to_string());
            note(ui, &format!("{device} is not matched to a GPU: {reason}"));
            kv(ui, "VRAM used", bytes_or_unknown(used));
        }
    }
}

/// Used VRAM against the GPU total, when both were read. The bar is drawn only when
/// the usage fits the total: a usage above it is shown as read, not clamped to it.
fn draw_vram_use(ui: &mut egui::Ui, used: Option<u64>, total: Option<u64>) {
    match (used, total) {
        (Some(used), Some(total)) if total > 0 => {
            kv(
                ui,
                "VRAM",
                format!("{} of {}", bytes_text(used), bytes_text(total)),
            );
            if used <= total {
                let fraction = used as f32 / total as f32;
                ui.add(egui::ProgressBar::new(fraction).desired_width(240.0));
            }
        }
        (Some(used), _) => kv(
            ui,
            "VRAM used",
            format!("{} (GPU total not read)", bytes_text(used)),
        ),
        (None, _) => kv(ui, "VRAM used", UNKNOWN),
    }
}

fn draw_tiers(ui: &mut egui::Ui, state: &SourceState, section: &Section) {
    ui.add(SectionHeader::new(&domain_section_title(
        "inference",
        "Tiers",
    )));
    draw_vram_tier(ui, state, section.vram.as_ref());
    draw_ram_tier(ui, state, section.ram.as_ref());
    draw_nvme_tier(ui, state, section.nvme.as_ref());
    // Prefetch is listed only when the engine reports it, which it does only when
    // an expert store exists.
    if let Some(prefetch) = section.prefetch.as_ref() {
        draw_prefetch(ui, prefetch);
    }
}

fn draw_vram_tier(ui: &mut egui::Ui, state: &SourceState, tier: Option<&VramTier>) {
    subheading(ui, "VRAM");
    match tier {
        None => kv(ui, "Status", "the engine reported no VRAM tier"),
        Some(Tier::NotConfigured { reason }) => kv(ui, "Status", not_configured(reason)),
        Some(Tier::Configured(VramCounters {
            experts_resident,
            capacity_experts,
            evictions_total,
            ..
        })) => {
            kv(
                ui,
                "Residency",
                residency(*experts_resident, *capacity_experts),
            );
            kv(ui, "Hit ratio", percent(state.derived.ratio("tier.vram")));
            kv(ui, "Evictions", count(*evictions_total));
        }
    }
}

fn draw_ram_tier(ui: &mut egui::Ui, state: &SourceState, tier: Option<&RamTier>) {
    subheading(ui, "RAM");
    match tier {
        None => kv(ui, "Status", "the engine reported no RAM tier"),
        Some(Tier::NotConfigured { reason }) => kv(ui, "Status", not_configured(reason)),
        Some(Tier::Configured(RamCounters {
            experts_resident,
            pinned_bytes,
            ..
        })) => {
            kv(ui, "Residency", count(*experts_resident));
            kv(ui, "Hit ratio", percent(state.derived.ratio("tier.ram")));
            kv(ui, "Pinned", bytes_or_unknown(*pinned_bytes));
        }
    }
}

fn draw_nvme_tier(ui: &mut egui::Ui, state: &SourceState, tier: Option<&NvmeTier>) {
    subheading(ui, "NVMe");
    match tier {
        None => kv(ui, "Status", "the engine reported no NVMe tier"),
        Some(Tier::NotConfigured { reason }) => kv(ui, "Status", not_configured(reason)),
        Some(Tier::Configured(NvmeCounters {
            queue_depth,
            inflight_peak,
            latency_max_ns,
            ..
        })) => {
            let derived = &state.derived;
            kv(
                ui,
                "Reads",
                derived
                    .rate("tier.nvme.reads_total")
                    .map(|r| format!("{r:.1} reads/s"))
                    .unwrap_or_else(|| UNKNOWN.to_string()),
            );
            kv(
                ui,
                "Read bytes",
                derived
                    .rate("tier.nvme.bytes_total")
                    .map(|b| format!("{}/s", bytes_text(b.max(0.0) as u64)))
                    .unwrap_or_else(|| UNKNOWN.to_string()),
            );
            kv(ui, "Queue depth", count(*queue_depth));
            kv(ui, "In flight, peak", count(*inflight_peak));
            kv(
                ui,
                "Slowest read, peak",
                latency_max_ns.map_or_else(
                    || UNKNOWN.to_string(),
                    |ns| format!("{:.1} ms", ns as f64 / 1e6),
                ),
            );
        }
    }
}

fn draw_prefetch(ui: &mut egui::Ui, prefetch: &PrefetchCounters) {
    subheading(ui, "Prefetch");
    kv(ui, "Requested", count(prefetch.requested_total));
    kv(ui, "Useful", count(prefetch.useful_total));
    kv(ui, "Wasted", count(prefetch.wasted_total));
    kv(ui, "Queue peak", count(prefetch.queue_peak));
}

/// The recent-request list is drawn only when the engine reports one. Without it,
/// the totals are shown alone.
fn draw_requests(ui: &mut egui::Ui, state: &SourceState, section: &Section) {
    ui.add(SectionHeader::new("Recent Requests"));
    let total = state
        .latest
        .as_ref()
        .and_then(|s| s.counters.get("requests.total").copied());
    let Some(list) = section.recent.as_deref() else {
        kv(
            ui,
            "Requests",
            format!(
                "{} total · {} active now · no recent-request list reported",
                count(total),
                count(section.requests_active)
            ),
        );
        return;
    };
    // The list is the engine's most recent requests, capped. Saying how many of how
    // many keeps the cap visible rather than reading as the whole history.
    match total {
        Some(total) => note(ui, &format!("{} of {total} requests", list.len())),
        None => note(
            ui,
            &format!("{} requests shown; the total is unknown", list.len()),
        ),
    }
    egui::Grid::new(("inference-requests", state.endpoint.as_str()))
        .striped(true)
        .num_columns(5)
        .show(ui, |ui| {
            for header in ["ID", "In", "Out", "Time", "Finish"] {
                ui.label(RichText::new(header).strong());
            }
            ui.end_row();
            for request in list {
                request_row(ui, request);
                ui.end_row();
            }
        });
}

fn request_row(ui: &mut egui::Ui, request: &RecentRequest) {
    ui.label(
        request
            .id
            .as_ref()
            .map(|id| match id {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| UNKNOWN.to_string()),
    );
    ui.label(count(request.prompt_tokens));
    ui.label(count(request.completion_tokens));
    ui.label(
        request
            .duration_ms
            .map(|ms| format!("{ms} ms"))
            .unwrap_or_else(|| UNKNOWN.to_string()),
    );
    ui.label(
        request
            .finish_reason
            .clone()
            .unwrap_or_else(|| UNKNOWN.to_string()),
    );
}

// ── Small pieces ─────────────────────────────────────────────────────────────

/// A key and its value on one line, the value in the primary colour.
fn kv(ui: &mut egui::Ui, key: &str, value: impl Into<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(key).color(theme::text_color(ui.ctx(), CyberColors::TEXT_SECONDARY)),
        );
        ui.label(
            RichText::new(value.into())
                .color(theme::text_color(ui.ctx(), CyberColors::TEXT_PRIMARY)),
        );
    });
}

/// A line of secondary text.
fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(theme::text_color(ui.ctx(), CyberColors::TEXT_SECONDARY)));
}

/// A tier's name within a panel, in the cyan the headings use.
fn subheading(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .strong()
            .color(theme::text_color(ui.ctx(), CyberColors::CYAN)),
    );
}

/// The engine's reason for a tier that is off, or a plain statement that it gave none.
fn not_configured(reason: &Option<String>) -> String {
    match reason {
        Some(reason) => format!("not configured: {reason}"),
        None => "not configured; the engine gave no reason".to_string(),
    }
}

fn residency(resident: Option<u64>, capacity: Option<u64>) -> String {
    match (resident, capacity) {
        (Some(r), Some(c)) => format!("{r} of {c} experts"),
        (Some(r), None) => format!("{r} experts, capacity unknown"),
        (None, Some(c)) => format!("unknown of {c} experts"),
        (None, None) => UNKNOWN.to_string(),
    }
}

/// A ratio as a percentage, or unknown when the window had no denominator.
fn percent(ratio: Option<f64>) -> String {
    ratio
        .map(|r| format!("{:.1}%", r * 100.0))
        .unwrap_or_else(|| UNKNOWN.to_string())
}

fn count<T: Display>(value: Option<T>) -> String {
    value.map_or_else(|| UNKNOWN.to_string(), |v| v.to_string())
}

fn bytes_or_unknown(value: Option<u64>) -> String {
    value.map_or_else(|| UNKNOWN.to_string(), bytes_text)
}

/// Bytes in binary units, one decimal place above the byte.
fn bytes_text(bytes: u64) -> String {
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
    use crate::gui::headless::{painted_text, themed_context};
    use crate::inference::ironworks::parse_metrics;
    use crate::inference::{InferenceSampler, ProviderSample, SourceError};
    use std::time::Instant;

    const A_NVME_CONFIGURED: &str =
        include_str!("../../tests/fixtures/inference/a_nvme_configured.json");
    const B_NVME_NOT_CONFIGURED: &str =
        include_str!("../../tests/fixtures/inference/b_nvme_not_configured.json");
    const C_COUNTER_OMITTED: &str =
        include_str!("../../tests/fixtures/inference/c_counter_omitted.json");

    const ENDPOINT: &str = "http://127.0.0.1:8080";

    /// A reachable engine with two samples one second of uptime apart, so rates
    /// exist. Each counter grows by 100 between them, and `tweak` can change the
    /// second sample.
    fn reachable_state(body: &str, tweak: impl FnOnce(&mut ProviderSample)) -> SourceState {
        let first = parse_metrics(body).expect("fixture parses").into_sample();
        let mut sampler = InferenceSampler::default();
        sampler.update(first.clone());

        let mut later = first;
        later.uptime_ns = later.uptime_ns.map(|t| t + 1_000_000_000);
        for value in later.counters.values_mut() {
            *value += 100;
        }
        tweak(&mut later);
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

    fn lines_of(state: &SourceState, gpus: &[GpuView]) -> Vec<String> {
        let ctx = themed_context();
        painted_text(&ctx, |ui| draw_source(ui, state, gpus))
    }

    /// The text painted immediately after `label`, which is how a key and its value
    /// appear in paint order.
    fn value_after<'a>(lines: &'a [String], label: &str) -> Option<&'a str> {
        let at = lines.iter().position(|l| l == label)?;
        lines.get(at + 1).map(String::as_str)
    }

    #[test]
    fn a_configured_nvme_tier_shows_its_measured_read_rate() {
        let state = reachable_state(A_NVME_CONFIGURED, |_| {});
        let lines = lines_of(&state, &[]);
        assert!(
            lines.iter().any(|l| l == "100.0 reads/s"),
            "the NVMe read rate was not painted: {lines:?}"
        );
    }

    #[test]
    fn a_not_configured_nvme_tier_shows_the_engines_reason() {
        let state = reachable_state(B_NVME_NOT_CONFIGURED, |_| {});
        let lines = lines_of(&state, &[]);
        assert!(
            lines
                .iter()
                .any(|l| l == "not configured: model fits in RAM+VRAM; no NVMe tier"),
            "the NVMe tier did not show the engine's reason: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.ends_with("reads/s")),
            "a tier that is not configured must not show a read rate: {lines:?}"
        );
    }

    #[test]
    fn an_omitted_counter_is_unknown_not_zero() {
        let state = reachable_state(C_COUNTER_OMITTED, |_| {});
        let lines = lines_of(&state, &[]);
        assert_eq!(
            value_after(&lines, "Evictions"),
            Some("unknown"),
            "the omitted VRAM eviction counter must read as unknown: {lines:?}"
        );
    }

    #[test]
    fn a_missing_denominator_counter_leaves_the_hit_ratio_unknown() {
        // Fixture c has no bypass counter, so the VRAM ratio cannot be formed.
        let state = reachable_state(C_COUNTER_OMITTED, |_| {});
        let lines = lines_of(&state, &[]);
        assert_eq!(
            value_after(&lines, "Hit ratio"),
            Some("unknown"),
            "{lines:?}"
        );
    }

    #[test]
    fn an_unreachable_engine_says_no_engine_answered_with_the_reason() {
        let mut state = reachable_state(A_NVME_CONFIGURED, |_| {});
        state.reachable = false;
        state.latest = None;
        state.last_error = Some(crate::inference::Failure {
            error: SourceError::Unreachable("connection refused".to_string()),
            at: Instant::now(),
        });
        let lines = lines_of(&state, &[]);
        assert!(
            lines
                .iter()
                .any(|l| l == "no engine answered at http://127.0.0.1:8080: connection refused"),
            "the unreachable engine was not reported with its reason: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l == "Speed" || l.ends_with("tok/s")),
            "an unreachable engine must not show figures: {lines:?}"
        );
    }

    /// A running engine with `--metrics` off is named as running, and is not
    /// reported as one that did not answer.
    #[test]
    fn a_metrics_off_engine_is_named_as_running_not_as_unanswering() {
        let mut state = reachable_state(A_NVME_CONFIGURED, |_| {});
        state.reachable = false;
        state.latest = None;
        state.last_error = Some(crate::inference::Failure {
            error: SourceError::MetricsNotEnabled,
            at: Instant::now(),
        });
        let lines = lines_of(&state, &[]);
        assert!(
            lines
                .iter()
                .any(|l| l == "engine running, metrics not enabled — start it with --metrics"),
            "the metrics-off engine was not named: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.starts_with("no engine answered")),
            "a metrics-off engine must not read as unanswering: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l == "running, metrics off"),
            "the header must say the engine is running: {lines:?}"
        );
    }

    #[test]
    fn the_tab_says_it_is_waiting_before_the_first_answer() {
        let mut state = reachable_state(A_NVME_CONFIGURED, |_| {});
        state.reachable = false;
        state.latest = None;
        state.last_success = None;
        state.last_error = None;
        let lines = lines_of(&state, &[]);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("waiting for the first answer from")),
            "{lines:?}"
        );
    }

    #[test]
    fn engine_gpu_joins_by_pci_and_shows_its_total() {
        let state = reachable_state(A_NVME_CONFIGURED, |_| {});
        let gpus = vec![GpuView {
            monitor: MonitorGpu {
                index: 0,
                pci_bus_id: Some("0000:03:00.0".to_string()),
                uuid: None,
            },
            name: "Test Card".to_string(),
            vram_total: Some(48 * 1024 * 1024 * 1024),
        }];
        let lines = lines_of(&state, &gpus);
        assert!(lines.iter().any(|l| l == "GPU 0 · Test Card"), "{lines:?}");
        assert_eq!(
            value_after(&lines, "VRAM"),
            Some("44.0 GiB of 48.0 GiB"),
            "the engine's VRAM was not shown against the GPU total: {lines:?}"
        );
    }

    #[test]
    fn an_unmatched_engine_gpu_is_not_attributed_to_a_card() {
        let state = reachable_state(A_NVME_CONFIGURED, |_| {});
        let lines = lines_of(&state, &[]);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("Engine device 1 is not matched to a GPU")),
            "{lines:?}"
        );
    }

    #[test]
    fn throughput_is_unknown_until_a_second_sample() {
        let first = parse_metrics(A_NVME_CONFIGURED).unwrap().into_sample();
        let mut sampler = InferenceSampler::default();
        sampler.update(first);
        assert_eq!(
            throughput(sampler.derived(), COMPLETION_TOKENS),
            "unknown (needs a second sample)"
        );
    }

    #[test]
    fn throughput_is_measured_from_the_completion_counter_over_uptime() {
        // The derived rate is the decode counter's change over the uptime window;
        // the engine's own figure, if it sent one, is never consulted.
        let mut derived = Derived {
            window_ns: Some(2_000_000_000),
            ..Derived::default()
        };
        derived.rates.insert(COMPLETION_TOKENS.to_string(), 20.0);
        assert_eq!(
            throughput(&derived, COMPLETION_TOKENS),
            "20.0 tok/s, measured over 2.0 s"
        );
    }

    #[test]
    fn a_missing_recent_list_is_stated_not_drawn_as_an_empty_list() {
        let state = reachable_state(
            r#"{"schema_version":1,"requests":{"total":3,"active":0}}"#,
            |_| {},
        );
        let lines = lines_of(&state, &[]);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("no recent-request list reported")),
            "{lines:?}"
        );
    }

    #[test]
    fn prefetch_is_shown_only_when_the_engine_reports_it() {
        let with_store = lines_of(&reachable_state(A_NVME_CONFIGURED, |_| {}), &[]);
        assert_eq!(value_after(&with_store, "Requested"), Some("41922"));
        assert!(with_store.iter().any(|l| l == "Prefetch"), "{with_store:?}");
        let without = lines_of(&reachable_state(B_NVME_NOT_CONFIGURED, |_| {}), &[]);
        assert!(!without.iter().any(|l| l == "Prefetch"), "{without:?}");
    }

    #[test]
    fn the_awaiting_check_clears_once_a_source_has_answered_or_failed() {
        struct Refusing;
        impl InferenceSource for Refusing {
            fn kind(&self) -> ProviderKind {
                ProviderKind::IronWorks
            }
            fn endpoint(&self) -> String {
                ENDPOINT.to_string()
            }
            fn locality(&self) -> Locality {
                Locality::Local
            }
            fn poll(&self, _: Duration) -> Result<ProviderSample, SourceError> {
                Err(SourceError::Unreachable("connection refused".to_string()))
            }
        }

        let poller = InferencePoller::spawn(vec![Box::new(Refusing)], Duration::from_secs(60));
        let deadline = Instant::now() + Duration::from_secs(5);
        while awaiting_first_answer(&poller) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!awaiting_first_answer(&poller), "the poller never reported");
        let state = poller.state(0).expect("one source");
        assert!(!state.reachable);
        assert!(state.last_error.is_some());
    }
}
