//! IronWorks, the first inference provider.
//!
//! Reads schema v1 (final, agreed 2026-11-03) of `GET {base}/metrics`. Every
//! field of the schema structs is optional, because an absent field is an absent
//! reading. A counter the engine did not report is omitted from the sample, never
//! defaulted to zero. A tier the engine has switched off carries its reason and
//! no counters at all. Ids the schema adds later are ignored, since the schema
//! only ever grows.

use super::{
    Identity, InferenceSource, Locality, ProviderKind, ProviderSample, RatioDef, SourceError,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;

/// Default endpoint for a local `ironworks serve`.
///
/// The address is spelled out rather than given as `localhost`. On this machine
/// `localhost` resolves to `::1` first, and the engine listens on IPv4 only, so
/// every request spent about 2.2 s failing over before it connected. The agent
/// module's `DEFAULT_ENDPOINT` still says `localhost`, and is left as it is.
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8080";

/// Environment variable that overrides [`DEFAULT_ENDPOINT`].
pub const ENDPOINT_ENV: &str = "IRONMON_IRONWORKS_URL";

/// The only schema version this adapter reads.
pub const SCHEMA_VERSION: u64 = 1;

/// The endpoint from `IRONMON_IRONWORKS_URL`, or the default when unset or blank.
pub fn endpoint_from_env() -> String {
    std::env::var(ENDPOINT_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string())
}

/// The hit ratios IronWorks publishes, each over the named counters the engine
/// documents for it. A ratio is unavailable when any of its counters is missing,
/// and when its denominator gained nothing in the window.
///
/// VRAM = hits / (hits + fills + bypass + not_admitted), over every deduplicated
/// pick the VRAM tier saw. RAM = hits / (hits + misses).
pub const RATIOS: &[RatioDef] = &[
    RatioDef {
        key: "tier.vram",
        numerator: "tier.vram.hits_total",
        denominator: &[
            "tier.vram.hits_total",
            "tier.vram.fills_total",
            "tier.vram.bypass_total",
            "tier.vram.not_admitted_total",
        ],
    },
    RatioDef {
        key: "tier.ram",
        numerator: "tier.ram.hits_total",
        denominator: &["tier.ram.hits_total", "tier.ram.misses_total"],
    },
];

// ── Schema v1 ────────────────────────────────────────────────────────────────

/// The whole `/metrics` body, as the schema describes it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetricsV1 {
    pub schema_version: Option<u64>,
    /// Wall clock, nanoseconds since the UNIX epoch. Display only.
    pub sampled_at_ns: Option<u64>,
    /// Monotonic, nanoseconds since the engine started. Windows are taken on it.
    pub uptime_ns: Option<u64>,
    pub engine: Option<Engine>,
    pub requests: Option<Requests>,
    pub decode: Option<Decode>,
    pub tiers: Option<Tiers>,
    pub memory: Option<Memory>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Engine {
    pub model: Option<String>,
    pub backend: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Requests {
    /// Cumulative requests since the engine started.
    pub total: Option<u64>,
    /// Requests being served right now. A current figure, not a peak.
    pub active: Option<u64>,
    pub recent_capacity: Option<u64>,
    pub recent: Option<Vec<RecentRequest>>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct RecentRequest {
    /// The schema does not fix the id's JSON type, so it is kept as given.
    pub id: Option<serde_json::Value>,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub duration_ms: Option<u64>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Decode {
    /// Prompt tokens across every request since the engine started.
    pub prompt_tokens_total: Option<u64>,
    /// Completion tokens across every request since the engine started, from the
    /// engine's token accounting. Throughput is derived from this counter.
    pub completion_tokens_total: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Tiers {
    pub vram: Option<VramTier>,
    pub ram: Option<RamTier>,
    pub nvme: Option<NvmeTier>,
    /// Present only when an expert store is configured. Absent means unavailable.
    pub prefetch: Option<PrefetchCounters>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Memory {
    pub per_gpu: Option<Vec<RawGpuMemory>>,
    pub host: Option<HostMemory>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawGpuMemory {
    /// The engine's own index. Never used to find the GPU; see [`match_gpu`].
    pub device_index: Option<u64>,
    pub pci_bus_id: Option<String>,
    pub uuid: Option<String>,
    pub engine_vram_used_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HostMemory {
    pub engine_pinned_bytes: Option<u64>,
}

// Tier objects. Each deserializes through `RawTier`, which reads `configured` and
// `reason` and flattens the rest into the tier's counters. A tier that is not
// configured keeps only its reason: its counters are not stored, so none can leak
// into a reading.

#[derive(Debug, Clone, Deserialize)]
struct RawTier<T> {
    configured: Option<bool>,
    reason: Option<String>,
    #[serde(flatten)]
    body: T,
}

/// A tier, either running with its counters or switched off with the engine's
/// reason. A tier that omits `configured` says so rather than claiming the engine
/// lacks it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "RawTier<T>", bound(deserialize = "T: serde::Deserialize<'de>"))]
pub enum Tier<T> {
    Configured(T),
    NotConfigured { reason: Option<String> },
}

impl<T> From<RawTier<T>> for Tier<T> {
    fn from(raw: RawTier<T>) -> Self {
        match raw.configured {
            Some(true) => Tier::Configured(raw.body),
            Some(false) => Tier::NotConfigured { reason: raw.reason },
            None => Tier::NotConfigured {
                reason: Some("the engine omitted the configured flag for this tier".into()),
            },
        }
    }
}

pub type VramTier = Tier<VramCounters>;
pub type RamTier = Tier<RamCounters>;
pub type NvmeTier = Tier<NvmeCounters>;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct VramCounters {
    pub policy: Option<String>,
    pub experts_resident: Option<u64>,
    pub capacity_experts: Option<u64>,
    pub bytes_resident: Option<u64>,
    pub hits_total: Option<u64>,
    pub fills_total: Option<u64>,
    pub bypass_total: Option<u64>,
    pub not_admitted_total: Option<u64>,
    pub evictions_total: Option<u64>,
    pub fill_bytes_total: Option<u64>,
    pub warm_total: Option<u64>,
    pub fills_aborted_total: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct RamCounters {
    pub experts_resident: Option<u64>,
    pub pinned_bytes: Option<u64>,
    pub hits_total: Option<u64>,
    pub misses_total: Option<u64>,
    pub waits_total: Option<u64>,
    pub evictions_total: Option<u64>,
    pub bytes_loaded_total: Option<u64>,
    pub puts_total: Option<u64>,
    pub discards_total: Option<u64>,
    pub loads_abandoned_total: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct NvmeCounters {
    pub reads_total: Option<u64>,
    pub bytes_total: Option<u64>,
    pub batches_total: Option<u64>,
    /// High-water mark of in-flight reads since start. A peak, not a current depth.
    pub inflight_peak: Option<u64>,
    pub latency_p50_ns: Option<u64>,
    pub latency_p99_ns: Option<u64>,
    /// Largest single read latency since start. A peak.
    pub latency_max_ns: Option<u64>,
    /// Reads queued right now. Not instrumented by every engine build.
    pub queue_depth: Option<u64>,
}

/// Expert prefetch. Present in the body only when an expert store exists.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct PrefetchCounters {
    pub requested_total: Option<u64>,
    pub scheduled_total: Option<u64>,
    pub dropped_total: Option<u64>,
    pub loaded_total: Option<u64>,
    pub useful_total: Option<u64>,
    pub wasted_total: Option<u64>,
    /// High-water mark of the prefetch queue since start. A peak.
    pub queue_peak: Option<u64>,
    pub errors_total: Option<u64>,
}

// ── The IronWorks section of a sample ────────────────────────────────────────

/// Everything IronWorks reports that has no counterpart in the shared core.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// `None` when the body carries no such object at all.
    pub vram: Option<VramTier>,
    pub ram: Option<RamTier>,
    pub nvme: Option<NvmeTier>,
    /// `None` when the body has no prefetch object, which means no expert store.
    pub prefetch: Option<PrefetchCounters>,
    /// `None` when the body has no per-GPU section.
    pub per_gpu: Option<Vec<GpuMemory>>,
    pub host_pinned_bytes: Option<u64>,
    pub requests_active: Option<u64>,
    pub recent_capacity: Option<u64>,
    pub recent: Option<Vec<RecentRequest>>,
}

/// One GPU the engine uses, as it reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuMemory {
    /// The engine's own index. It is kept for display only and is never used to
    /// identify the GPU: under `CUDA_VISIBLE_DEVICES` the engine's numbering can
    /// differ from the physical numbering IronMonitor uses. See [`match_gpu`].
    pub device_index: Option<u64>,
    pub pci_bus_id: Option<String>,
    pub uuid: Option<String>,
    pub engine_vram_used_bytes: Option<u64>,
}

/// A GPU as IronMonitor enumerates it, with its own `gpu.{n}` index.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorGpu {
    pub index: usize,
    pub pci_bus_id: Option<String>,
    pub uuid: Option<String>,
}

/// Find the IronMonitor GPU an engine GPU is.
///
/// The join is by PCI bus address, which identifies a physical card. UUID is the
/// fallback, used only when the engine gave no usable PCI address. The engine's
/// device index is never consulted. Under `CUDA_VISIBLE_DEVICES` the engine's
/// index for a card can differ from the physical index IronMonitor reports, so
/// joining on the index would attach one card's VRAM to another card's `gpu.{n}`.
///
/// Returns the matched `gpu.{n}` index, or the reason there is none.
pub fn match_gpu(engine: &GpuMemory, monitors: &[MonitorGpu]) -> Result<usize, String> {
    if let Some(bus) = engine.pci_bus_id.as_deref().and_then(normalize_pci) {
        let hits: Vec<usize> = monitors
            .iter()
            .filter(|m| {
                m.pci_bus_id
                    .as_deref()
                    .and_then(normalize_pci)
                    .is_some_and(|b| b == bus)
            })
            .map(|m| m.index)
            .collect();
        return match hits.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!(
                "no IronMonitor GPU has PCI bus address {bus}, so the engine's GPU is not matched"
            )),
            _ => Err(format!(
                "more than one IronMonitor GPU has PCI bus address {bus}, so the engine's GPU is not matched"
            )),
        };
    }
    if let Some(uuid) = engine
        .uuid
        .as_deref()
        .map(normalize_uuid)
        .filter(|u| !u.is_empty())
    {
        let hits: Vec<usize> = monitors
            .iter()
            .filter(|m| {
                m.uuid
                    .as_deref()
                    .map(normalize_uuid)
                    .is_some_and(|u| u == uuid)
            })
            .map(|m| m.index)
            .collect();
        return match hits.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!(
                "no IronMonitor GPU has UUID {uuid}, so the engine's GPU is not matched"
            )),
            _ => Err(format!(
                "more than one IronMonitor GPU has UUID {uuid}, so the engine's GPU is not matched"
            )),
        };
    }
    Err(
        "the engine gave neither a PCI bus address nor a UUID for this GPU, so it is not matched"
            .into(),
    )
}

/// A PCI address in one spelling: `dddd:bb:dd.f`, lowercase, domain without
/// padding. Engines write the domain as 8 digits and IronMonitor as 4.
fn normalize_pci(raw: &str) -> Option<String> {
    let mut parts = raw.trim().split(':');
    let domain = parts.next()?;
    let bus = parts.next()?;
    let devfn = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let domain = u32::from_str_radix(domain.trim(), 16).ok()?;
    let bus = bus.trim().to_ascii_lowercase();
    let devfn = devfn.trim().to_ascii_lowercase();
    if bus.is_empty() || devfn.is_empty() {
        return None;
    }
    Some(format!("{domain:04x}:{bus}:{devfn}"))
}

fn normalize_uuid(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

// ── Parsing ──────────────────────────────────────────────────────────────────

/// Parse a `/metrics` body. Rejects a missing or unknown `schema_version`.
pub fn parse_metrics(body: &str) -> Result<MetricsV1, SourceError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| SourceError::NotJson(e.to_string()))?;
    match value.get("schema_version").and_then(|v| v.as_u64()) {
        Some(SCHEMA_VERSION) => {}
        Some(other) => {
            return Err(SourceError::Rejected(format!(
                "the endpoint reports schema_version {other}, and this build reads only version {SCHEMA_VERSION}"
            )))
        }
        None => {
            return Err(SourceError::Rejected(
                "the body has no schema_version, so it is not IronWorks metrics".into(),
            ))
        }
    }
    serde_json::from_value(value)
        .map_err(|e| SourceError::Rejected(format!("the body does not match schema v1: {e}")))
}

/// Whether a 404 body is the one an engine sends when started without `--metrics`:
/// `{"error":"Metrics not enabled. ..."}`. Any other 404 body is not this state.
pub fn is_metrics_disabled_body(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.as_str())
                .map(|e| e.starts_with("Metrics not enabled"))
        })
        .unwrap_or(false)
}

impl MetricsV1 {
    /// Map a parsed body onto the shared sample shape.
    pub fn into_sample(self) -> ProviderSample {
        let mut counters = BTreeMap::new();
        let mut put = |name: &str, value: Option<u64>| {
            if let Some(v) = value {
                counters.insert(name.to_string(), v);
            }
        };

        let tiers = self.tiers.clone().unwrap_or_default();
        if let Some(Tier::Configured(v)) = &tiers.vram {
            put("tier.vram.hits_total", v.hits_total);
            put("tier.vram.fills_total", v.fills_total);
            put("tier.vram.bypass_total", v.bypass_total);
            put("tier.vram.not_admitted_total", v.not_admitted_total);
            put("tier.vram.evictions_total", v.evictions_total);
            put("tier.vram.fill_bytes_total", v.fill_bytes_total);
            put("tier.vram.warm_total", v.warm_total);
            put("tier.vram.fills_aborted_total", v.fills_aborted_total);
        }
        if let Some(Tier::Configured(r)) = &tiers.ram {
            put("tier.ram.hits_total", r.hits_total);
            put("tier.ram.misses_total", r.misses_total);
            put("tier.ram.waits_total", r.waits_total);
            put("tier.ram.evictions_total", r.evictions_total);
            put("tier.ram.bytes_loaded_total", r.bytes_loaded_total);
            put("tier.ram.puts_total", r.puts_total);
            put("tier.ram.discards_total", r.discards_total);
            put("tier.ram.loads_abandoned_total", r.loads_abandoned_total);
        }
        if let Some(Tier::Configured(n)) = &tiers.nvme {
            put("tier.nvme.reads_total", n.reads_total);
            put("tier.nvme.bytes_total", n.bytes_total);
            put("tier.nvme.batches_total", n.batches_total);
        }
        if let Some(p) = &tiers.prefetch {
            put("tier.prefetch.requested_total", p.requested_total);
            put("tier.prefetch.scheduled_total", p.scheduled_total);
            put("tier.prefetch.dropped_total", p.dropped_total);
            put("tier.prefetch.loaded_total", p.loaded_total);
            put("tier.prefetch.useful_total", p.useful_total);
            put("tier.prefetch.wasted_total", p.wasted_total);
            put("tier.prefetch.errors_total", p.errors_total);
        }
        if let Some(r) = &self.requests {
            put("requests.total", r.total);
        }
        if let Some(d) = &self.decode {
            put("decode.prompt_tokens_total", d.prompt_tokens_total);
            put("decode.completion_tokens_total", d.completion_tokens_total);
        }

        let engine = self.engine.clone().unwrap_or_default();
        let requests = self.requests.clone().unwrap_or_default();
        let memory = self.memory.clone().unwrap_or_default();

        ProviderSample {
            sampled_at_ns: self.sampled_at_ns,
            uptime_ns: self.uptime_ns,
            identity: Identity {
                model: engine.model,
                backend: engine.backend,
                version: None,
                status: None,
            },
            counters,
            gauges: BTreeMap::new(),
            ratio_defs: RATIOS,
            ironworks: Some(Section {
                vram: tiers.vram,
                ram: tiers.ram,
                nvme: tiers.nvme,
                prefetch: tiers.prefetch,
                per_gpu: memory.per_gpu.map(|gpus| {
                    gpus.into_iter()
                        .map(|g| GpuMemory {
                            device_index: g.device_index,
                            pci_bus_id: g.pci_bus_id,
                            uuid: g.uuid,
                            engine_vram_used_bytes: g.engine_vram_used_bytes,
                        })
                        .collect()
                }),
                host_pinned_bytes: memory.host.and_then(|h| h.engine_pinned_bytes),
                requests_active: requests.active,
                recent_capacity: requests.recent_capacity,
                recent: requests.recent,
            }),
        }
    }
}

// ── Fetching ─────────────────────────────────────────────────────────────────

/// `{base}/metrics`, tolerating a trailing slash on the base.
pub fn metrics_url(base: &str) -> String {
    format!("{}/metrics", base.trim_end_matches('/'))
}

/// One blocking fetch and parse, with a fresh client.
///
/// The client is built on its own thread. A blocking client creates and drops an
/// internal runtime, and that panics when done inside an async context. Snapshots
/// can be taken from one, so the work must not run there.
#[cfg(feature = "remote-backends")]
pub fn fetch(base_url: &str, timeout: Duration) -> Result<MetricsV1, SourceError> {
    let base = base_url.to_string();
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let client = build_client(timeout)?;
                fetch_with(&client, &base, timeout)
            })
            .join()
            .unwrap_or_else(|_| {
                Err(SourceError::Unreachable(
                    "the metrics request panicked".into(),
                ))
            })
    })
}

/// Without `remote-backends` there is no HTTP client to make the request with.
#[cfg(not(feature = "remote-backends"))]
pub fn fetch(base_url: &str, timeout: Duration) -> Result<MetricsV1, SourceError> {
    let _ = (base_url, timeout);
    Err(SourceError::NotSupported(
        "this build has no HTTP client, because the remote-backends feature is off".into(),
    ))
}

#[cfg(feature = "remote-backends")]
fn build_client(timeout: Duration) -> Result<reqwest::blocking::Client, SourceError> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| SourceError::Unreachable(format!("could not start an HTTP client: {e}")))
}

#[cfg(feature = "remote-backends")]
fn fetch_with(
    client: &reqwest::blocking::Client,
    base: &str,
    timeout: Duration,
) -> Result<MetricsV1, SourceError> {
    let response = client
        .get(metrics_url(base))
        .timeout(timeout)
        .send()
        .map_err(|e| classify(&e, timeout))?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        // The engine answers 404 with its own body when `--metrics` is off. Only
        // that body makes this the metrics-off state; another 404 is just a 404.
        let body = response.text().unwrap_or_default();
        return Err(if is_metrics_disabled_body(&body) {
            SourceError::MetricsNotEnabled
        } else {
            SourceError::Status(status.as_u16())
        });
    }
    if status != reqwest::StatusCode::OK {
        return Err(SourceError::Status(status.as_u16()));
    }
    let body = response.text().map_err(|e| classify(&e, timeout))?;
    parse_metrics(&body)
}

#[cfg(feature = "remote-backends")]
fn classify(e: &reqwest::Error, timeout: Duration) -> SourceError {
    if e.is_timeout() {
        return SourceError::Unreachable(format!("timed out after {} ms", timeout.as_millis()));
    }
    let mut root: &dyn std::error::Error = e;
    while let Some(next) = root.source() {
        root = next;
    }
    let cause = root.to_string();
    if cause.to_ascii_lowercase().contains("refused") {
        SourceError::Unreachable("connection refused".into())
    } else {
        SourceError::Unreachable(format!("could not connect ({cause})"))
    }
}

// ── The source ───────────────────────────────────────────────────────────────

/// An IronWorks engine at a base URL.
pub struct IronWorksSource {
    base: String,
    #[cfg(feature = "remote-backends")]
    client: Option<reqwest::blocking::Client>,
}

impl IronWorksSource {
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            #[cfg(feature = "remote-backends")]
            client: build_client(Duration::from_secs(1)).ok(),
        }
    }

    pub fn from_env() -> Self {
        Self::new(&endpoint_from_env())
    }
}

impl InferenceSource for IronWorksSource {
    fn kind(&self) -> ProviderKind {
        ProviderKind::IronWorks
    }

    fn endpoint(&self) -> String {
        self.base.clone()
    }

    fn locality(&self) -> Locality {
        locality_of(&self.base)
    }

    fn poll(&self, timeout: Duration) -> Result<ProviderSample, SourceError> {
        #[cfg(feature = "remote-backends")]
        {
            let Some(client) = &self.client else {
                return Err(SourceError::Unreachable(
                    "could not start an HTTP client".into(),
                ));
            };
            fetch_with(client, &self.base, timeout).map(MetricsV1::into_sample)
        }

        #[cfg(not(feature = "remote-backends"))]
        {
            fetch(&self.base, timeout).map(MetricsV1::into_sample)
        }
    }
}

/// Local when the host is this machine, remote otherwise.
pub fn locality_of(base: &str) -> Locality {
    let without_scheme = base.split("://").nth(1).unwrap_or(base);
    let authority = without_scheme.split('/').next().unwrap_or("");
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        authority.rsplit_once(':').map_or(authority, |(h, _)| h)
    };
    match host.to_ascii_lowercase().as_str() {
        "localhost" | "127.0.0.1" | "::1" => Locality::Local,
        _ => Locality::Remote,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::InferenceSampler;

    // Hand-written from the schema; see tests/fixtures/inference/NOTE.txt.
    const A_NVME_CONFIGURED: &str =
        include_str!("../../tests/fixtures/inference/a_nvme_configured.json");
    const B_NVME_NOT_CONFIGURED: &str =
        include_str!("../../tests/fixtures/inference/b_nvme_not_configured.json");
    const C_COUNTER_OMITTED: &str =
        include_str!("../../tests/fixtures/inference/c_counter_omitted.json");
    const A2_TWO_GPUS: &str = include_str!("../../tests/fixtures/inference/a2_two_gpus.json");
    const UNKNOWN_SCHEMA: &str = include_str!("../../tests/fixtures/inference/unknown_schema.json");
    const NOT_JSON: &str = include_str!("../../tests/fixtures/inference/not_json.txt");
    const METRICS_OFF: &str =
        include_str!("../../tests/fixtures/inference/metrics_not_enabled.json");

    const SEC: u64 = 1_000_000_000;

    /// A second poll two seconds of uptime later, with each counter raised by the
    /// amount `bump` gives it.
    fn second_poll(first: &ProviderSample, bump: impl Fn(&str) -> u64) -> ProviderSample {
        let mut later = first.clone();
        later.sampled_at_ns = first.sampled_at_ns.map(|t| t + 2 * SEC);
        later.uptime_ns = first.uptime_ns.map(|t| t + 2 * SEC);
        for (name, value) in later.counters.iter_mut() {
            *value += bump(name);
        }
        later
    }

    fn sampled(body: &str) -> (InferenceSampler, ProviderSample) {
        let first = parse_metrics(body).expect("fixture parses").into_sample();
        let mut sampler = InferenceSampler::default();
        sampler.update(first.clone());
        (sampler, first)
    }

    #[test]
    fn parses_configured_nvme_fixture() {
        let s = parse_metrics(A_NVME_CONFIGURED)
            .expect("fixture a parses")
            .into_sample();
        assert_eq!(s.identity.model.as_deref(), Some("glm-5.3-flash"));
        assert_eq!(s.identity.backend.as_deref(), Some("cuda"));
        assert_eq!(s.counters["tier.nvme.reads_total"], 4808);
        assert_eq!(s.counters["tier.nvme.bytes_total"], 45_550_822_784);
        assert_eq!(s.counters["tier.vram.evictions_total"], 198_159);
        assert_eq!(s.counters["tier.prefetch.requested_total"], 41_922);
        assert_eq!(s.counters["decode.completion_tokens_total"], 500_000);
        assert_eq!(s.counters["requests.total"], 642);
        let ir = s.ironworks.expect("ironworks section");
        assert!(matches!(
            ir.nvme,
            Some(Tier::Configured(NvmeCounters {
                reads_total: Some(4808),
                queue_depth: Some(2),
                ..
            }))
        ));
        assert_eq!(
            ir.prefetch.as_ref().and_then(|p| p.queue_peak),
            Some(16),
            "the prefetch peak is read from the prefetch object"
        );
        assert_eq!(ir.requests_active, Some(1));
        assert_eq!(ir.host_pinned_bytes, Some(96_635_289_600));
        let gpus = ir.per_gpu.expect("per gpu");
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].device_index, Some(1));
        assert_eq!(gpus[0].pci_bus_id.as_deref(), Some("00000000:03:00.0"));
    }

    #[test]
    fn removed_fields_are_not_read() {
        // A body still carrying the retired fields must not produce their values.
        let body = r#"{"schema_version":1,"engine":{"model":"m","arch":"glm5"},
            "tiers":{"vram":{"configured":true,"hit_total":5,"miss_total":5}},
            "masked_miss":{"enabled":true,"masked_pick_total":9},
            "prefetch":{"configured":true,"inflight":3},
            "decode":{"tok_s":{"value":18.4,"window_ms":1000}}}"#;
        let s = parse_metrics(body).expect("parses").into_sample();
        assert!(s.counters.is_empty(), "{:?}", s.counters);
        assert_eq!(s.identity.model.as_deref(), Some("m"));
        let ir = s.ironworks.expect("section");
        assert!(ir.prefetch.is_none(), "a top-level prefetch is not read");
        // The retired `hit_total` key is not the schema's `hits_total`.
        assert!(matches!(
            ir.vram,
            Some(Tier::Configured(ref v)) if v.hits_total.is_none()
        ));
    }

    #[test]
    fn parses_not_configured_nvme_fixture() {
        let s = parse_metrics(B_NVME_NOT_CONFIGURED)
            .expect("fixture b parses")
            .into_sample();
        let ir = s.ironworks.expect("section");
        match ir.nvme {
            Some(Tier::NotConfigured { reason }) => {
                assert_eq!(
                    reason.as_deref(),
                    Some("model fits in RAM+VRAM; no NVMe tier")
                );
            }
            other => panic!("expected NotConfigured, got {other:?}"),
        }
        assert!(!s.counters.keys().any(|k| k.starts_with("tier.nvme.")));
        assert_eq!(s.counters["tier.ram.hits_total"], 12_044);
        // No store, so no prefetch object at all: absent, not an empty object.
        assert!(ir.prefetch.is_none());
    }

    #[test]
    fn a_not_configured_tier_keeps_no_counters() {
        let body = r#"{"schema_version":1,"tiers":{"nvme":{"configured":false,"reason":"off","reads_total":7}}}"#;
        let ir = parse_metrics(body)
            .unwrap()
            .into_sample()
            .ironworks
            .unwrap();
        assert_eq!(
            ir.nvme,
            Some(Tier::NotConfigured {
                reason: Some("off".into())
            })
        );
    }

    #[test]
    fn a_tier_without_the_configured_flag_says_so() {
        let body = r#"{"schema_version":1,"tiers":{"ram":{"hits_total":1}}}"#;
        let ir = parse_metrics(body)
            .unwrap()
            .into_sample()
            .ironworks
            .unwrap();
        match ir.ram {
            Some(Tier::NotConfigured { reason }) => {
                assert_eq!(
                    reason.as_deref(),
                    Some("the engine omitted the configured flag for this tier")
                );
            }
            other => panic!("expected NotConfigured, got {other:?}"),
        }
    }

    #[test]
    fn omitted_fields_stay_absent_not_zero() {
        let s = parse_metrics(C_COUNTER_OMITTED)
            .expect("fixture c parses")
            .into_sample();
        // Two VRAM counters are left out of a configured tier.
        assert!(!s.counters.contains_key("tier.vram.evictions_total"));
        assert!(!s.counters.contains_key("tier.vram.bypass_total"));
        let ir = s.ironworks.as_ref().unwrap();
        assert!(matches!(
            ir.vram,
            Some(Tier::Configured(VramCounters {
                evictions_total: None,
                ..
            }))
        ));
        // No prefetch object: absent.
        assert!(ir.prefetch.is_none());
        // The counters that are present are still there.
        assert_eq!(s.counters["tier.vram.hits_total"], 40_221);
    }

    #[test]
    fn two_gpus_are_both_reported_with_their_engine_indexes() {
        let s = parse_metrics(A2_TWO_GPUS)
            .expect("two-gpu fixture parses")
            .into_sample();
        let gpus = s.ironworks.expect("section").per_gpu.expect("array");
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].pci_bus_id.as_deref(), Some("00000000:01:00.0"));
        assert_eq!(gpus[1].pci_bus_id.as_deref(), Some("00000000:03:00.0"));
    }

    #[test]
    fn gpu_join_prefers_pci_over_engine_index() {
        // The engine's device 1 sits on bus 03. In IronMonitor's numbering bus 03
        // is gpu.0, so joining by index would put it on gpu.1.
        let engine = GpuMemory {
            device_index: Some(1),
            pci_bus_id: Some("00000000:03:00.0".into()),
            uuid: None,
            engine_vram_used_bytes: Some(1),
        };
        let monitors = [
            MonitorGpu {
                index: 0,
                pci_bus_id: Some("0000:03:00.0".into()),
                uuid: None,
            },
            MonitorGpu {
                index: 1,
                pci_bus_id: Some("0000:01:00.0".into()),
                uuid: None,
            },
        ];
        assert_eq!(match_gpu(&engine, &monitors), Ok(0));
    }

    #[test]
    fn gpu_join_falls_back_to_uuid_only_without_pci() {
        let engine = GpuMemory {
            device_index: Some(0),
            pci_bus_id: None,
            uuid: Some("GPU-ABC".into()),
            engine_vram_used_bytes: None,
        };
        let monitors = [
            MonitorGpu {
                index: 0,
                pci_bus_id: None,
                uuid: Some("gpu-xyz".into()),
            },
            MonitorGpu {
                index: 1,
                pci_bus_id: None,
                uuid: Some("gpu-abc".into()),
            },
        ];
        assert_eq!(match_gpu(&engine, &monitors), Ok(1));
    }

    #[test]
    fn unmatched_gpu_says_why() {
        let engine = GpuMemory {
            device_index: Some(0),
            pci_bus_id: Some("0000:09:00.0".into()),
            uuid: None,
            engine_vram_used_bytes: None,
        };
        let monitors = [MonitorGpu {
            index: 0,
            pci_bus_id: Some("0000:01:00.0".into()),
            uuid: None,
        }];
        let err = match_gpu(&engine, &monitors).unwrap_err();
        assert!(err.contains("0000:09:00.0"), "{err}");
    }

    #[test]
    fn unknown_schema_version_is_rejected_with_reason() {
        match parse_metrics(UNKNOWN_SCHEMA).unwrap_err() {
            SourceError::Rejected(why) => assert!(why.contains("schema_version 2"), "{why}"),
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn missing_schema_version_is_rejected() {
        let err = parse_metrics("{\"engine\":{}}").unwrap_err();
        assert!(matches!(err, SourceError::Rejected(_)));
    }

    #[test]
    fn non_json_is_rejected() {
        let err = parse_metrics(NOT_JSON).unwrap_err();
        assert!(matches!(err, SourceError::NotJson(_)));
    }

    #[test]
    fn unknown_extra_fields_are_ignored() {
        let body = "{\"schema_version\":1,\"sampled_at_ns\":7,\"future_field\":{\"x\":1}}";
        let s = parse_metrics(body).expect("parses").into_sample();
        assert_eq!(s.sampled_at_ns, Some(7));
    }

    #[test]
    fn absent_tier_object_is_none_not_not_configured() {
        let s = parse_metrics("{\"schema_version\":1}")
            .unwrap()
            .into_sample();
        let ir = s.ironworks.unwrap();
        assert!(ir.vram.is_none() && ir.nvme.is_none() && ir.per_gpu.is_none());
        assert!(ir.prefetch.is_none());
    }

    #[test]
    fn the_metrics_off_body_is_recognised_only_by_its_text() {
        assert!(is_metrics_disabled_body(METRICS_OFF));
        assert!(is_metrics_disabled_body(
            "{\"error\":\"Metrics not enabled. Start server with --metrics flag.\"}"
        ));
        assert!(!is_metrics_disabled_body("<html>not found</html>"));
        assert!(!is_metrics_disabled_body(
            "{\"error\":\"model not loaded\"}"
        ));
        assert!(!is_metrics_disabled_body(""));
    }

    #[test]
    fn throughput_is_derived_from_the_decode_counters_only() {
        let (mut sampler, first) = sampled(A_NVME_CONFIGURED);
        // One sample has no rate.
        assert_eq!(
            sampler.derived().rate("decode.completion_tokens_total"),
            None
        );
        let later = second_poll(&first, |name| match name {
            "decode.completion_tokens_total" => 40,
            "decode.prompt_tokens_total" => 600,
            _ => 0,
        });
        sampler.update(later);
        assert_eq!(
            sampler.derived().rate("decode.completion_tokens_total"),
            Some(20.0)
        );
        assert_eq!(
            sampler.derived().rate("decode.prompt_tokens_total"),
            Some(300.0)
        );
    }

    #[test]
    fn vram_hit_ratio_is_hits_over_every_pick_the_tier_saw() {
        let (mut sampler, first) = sampled(A_NVME_CONFIGURED);
        // Each VRAM counter in the denominator gains 100 in the window: 100 hits of
        // 400 picks is a quarter. The RAM counters gain 100 each: one half.
        let later = second_poll(&first, |name| {
            if name.starts_with("tier.vram.") || name.starts_with("tier.ram.") {
                100
            } else {
                0
            }
        });
        sampler.update(later);
        assert_eq!(sampler.derived().ratio("tier.vram"), Some(0.25));
        assert_eq!(sampler.derived().ratio("tier.ram"), Some(0.5));
    }

    #[test]
    fn a_ratio_with_a_missing_denominator_counter_is_unavailable() {
        // Fixture c has no bypass counter, so the VRAM ratio cannot be formed; the
        // RAM ratio, whose counters are all present, still can.
        let (mut sampler, first) = sampled(C_COUNTER_OMITTED);
        sampler.update(second_poll(&first, |_| 10));
        assert_eq!(sampler.derived().ratio("tier.vram"), None);
        assert_eq!(sampler.derived().ratio("tier.ram"), Some(0.5));
    }

    #[test]
    fn a_ratio_over_an_idle_window_is_unavailable_not_zero() {
        let (mut sampler, first) = sampled(A_NVME_CONFIGURED);
        sampler.update(second_poll(&first, |_| 0));
        assert_eq!(sampler.derived().ratio("tier.vram"), None);
        assert_eq!(sampler.derived().ratio("tier.ram"), None);
    }

    #[test]
    fn a_falling_uptime_is_a_restart_even_when_counters_rose() {
        let first = parse_metrics(A_NVME_CONFIGURED).unwrap().into_sample();
        let mut restarted = first.clone();
        restarted.sampled_at_ns = first.sampled_at_ns.map(|t| t + SEC);
        restarted.uptime_ns = Some(1_000);
        for v in restarted.counters.values_mut() {
            *v += 10;
        }
        let mut sampler = InferenceSampler::default();
        sampler.update(first);
        sampler.update(restarted);
        assert_eq!(sampler.derived().rate("tier.nvme.reads_total"), None);
        assert_eq!(sampler.derived().ratio("tier.vram"), None);
    }

    #[test]
    fn locality_follows_the_host() {
        assert_eq!(locality_of("http://localhost:8080"), Locality::Local);
        assert_eq!(locality_of("http://127.0.0.1:8080/"), Locality::Local);
        assert_eq!(locality_of("http://[::1]:8080"), Locality::Local);
        assert_eq!(locality_of("http://10.0.0.5:8080"), Locality::Remote);
    }

    #[test]
    fn default_endpoint_is_the_ipv4_loopback() {
        assert_eq!(DEFAULT_ENDPOINT, "http://127.0.0.1:8080");
        assert_eq!(metrics_url("http://h:1/"), "http://h:1/metrics");
    }
}
