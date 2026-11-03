//! IronWorks, the first inference provider.
//!
//! Reads schema v1 of `GET {base}/metrics`. Every field of the schema structs is
//! optional, because an absent field is an absent reading. A counter the engine
//! did not report is omitted from the sample, never defaulted to zero. Ids the
//! schema adds later are ignored, since the schema only ever grows.

use super::{
    EngineReported, Identity, InferenceSource, Locality, ProviderKind, ProviderSample, SourceError,
    Windowed,
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

// ── Schema v1 ────────────────────────────────────────────────────────────────

/// The whole `/metrics` body, as the schema describes it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetricsV1 {
    pub schema_version: Option<u64>,
    pub sampled_at_ns: Option<u64>,
    pub uptime_ns: Option<u64>,
    pub engine: Option<Engine>,
    pub tiers: Option<Tiers>,
    pub prefetch: Option<RawPrefetch>,
    pub masked_miss: Option<MaskedMiss>,
    pub decode: Option<Decode>,
    pub memory: Option<Memory>,
    pub requests: Option<Requests>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Engine {
    pub model: Option<String>,
    pub arch: Option<String>,
    pub backend: Option<String>,
    // `gpu_device` is deliberately not read. The engine sends it as an integer,
    // and nothing here uses it: GPUs are matched by PCI bus, not by index.
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Tiers {
    pub vram: Option<VramTier>,
    pub ram: Option<RamTier>,
    pub nvme: Option<NvmeTier>,
}

/// Raw prefetch object. Converted to [`PrefetchFeature`] in the section.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawPrefetch {
    pub configured: Option<bool>,
    pub reason: Option<String>,
    pub inflight: Option<u64>,
    pub overlapped_layers_total: Option<u64>,
    pub stall_total: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MaskedMiss {
    pub enabled: Option<bool>,
    pub masked_pick_total: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Decode {
    pub tok_s: Option<Windowed>,
    pub prompt_tok_s: Option<Windowed>,
    /// Prompt tokens across every request since the engine started. Omitted until
    /// the engine instruments it; output and prompt throughput are derived from
    /// these two counters across polls.
    pub prompt_tokens_total: Option<u64>,
    /// Completion tokens across every request since the engine started, from the
    /// engine's token accounting rather than a request's `max_tokens`.
    pub completion_tokens_total: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Memory {
    pub per_gpu: Option<Vec<RawGpuMemory>>,
    pub host: Option<HostMemory>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawGpuMemory {
    pub device_index: Option<u64>,
    pub pci_bus_id: Option<String>,
    pub uuid: Option<String>,
    pub engine_vram_used_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HostMemory {
    pub engine_pinned_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Requests {
    pub total: Option<u64>,
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
    pub tok_s: Option<Windowed>,
}

// Tier objects. Each deserializes through `RawTier` and is keyed on `configured`:
// `true` gives the counters, anything else gives the reason, and the counters
// are never present in that case.

#[derive(Debug, Clone, Default, Deserialize)]
struct RawTier {
    configured: Option<bool>,
    reason: Option<String>,
    policy: Option<String>,
    experts_resident: Option<u64>,
    capacity_experts: Option<u64>,
    bytes_resident: Option<u64>,
    pinned_bytes: Option<u64>,
    hit_total: Option<u64>,
    miss_total: Option<u64>,
    eviction_total: Option<u64>,
    read_total: Option<u64>,
    bytes_streamed_total: Option<u64>,
    queue_depth: Option<u64>,
}

/// Why a tier is not configured. A tier that is absent from the body, or that
/// omits `configured`, says so rather than claiming the engine lacks it.
fn not_configured_reason(raw: &RawTier) -> Option<String> {
    match (raw.configured, &raw.reason) {
        (None, _) => Some("the engine omitted the configured flag for this tier".into()),
        (Some(_), reason) => reason.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "RawTier")]
pub enum VramTier {
    Configured {
        policy: Option<String>,
        experts_resident: Option<u64>,
        capacity_experts: Option<u64>,
        bytes_resident: Option<u64>,
        hit_total: Option<u64>,
        miss_total: Option<u64>,
        eviction_total: Option<u64>,
    },
    NotConfigured {
        reason: Option<String>,
    },
}

impl From<RawTier> for VramTier {
    fn from(raw: RawTier) -> Self {
        if raw.configured == Some(true) {
            VramTier::Configured {
                policy: raw.policy,
                experts_resident: raw.experts_resident,
                capacity_experts: raw.capacity_experts,
                bytes_resident: raw.bytes_resident,
                hit_total: raw.hit_total,
                miss_total: raw.miss_total,
                eviction_total: raw.eviction_total,
            }
        } else {
            VramTier::NotConfigured {
                reason: not_configured_reason(&raw),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "RawTier")]
pub enum RamTier {
    Configured {
        experts_resident: Option<u64>,
        pinned_bytes: Option<u64>,
        hit_total: Option<u64>,
        miss_total: Option<u64>,
    },
    NotConfigured {
        reason: Option<String>,
    },
}

impl From<RawTier> for RamTier {
    fn from(raw: RawTier) -> Self {
        if raw.configured == Some(true) {
            RamTier::Configured {
                experts_resident: raw.experts_resident,
                pinned_bytes: raw.pinned_bytes,
                hit_total: raw.hit_total,
                miss_total: raw.miss_total,
            }
        } else {
            RamTier::NotConfigured {
                reason: not_configured_reason(&raw),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "RawTier")]
pub enum NvmeTier {
    Configured {
        read_total: Option<u64>,
        bytes_streamed_total: Option<u64>,
        queue_depth: Option<u64>,
        hit_total: Option<u64>,
        miss_total: Option<u64>,
    },
    NotConfigured {
        reason: Option<String>,
    },
}

impl From<RawTier> for NvmeTier {
    fn from(raw: RawTier) -> Self {
        if raw.configured == Some(true) {
            NvmeTier::Configured {
                read_total: raw.read_total,
                bytes_streamed_total: raw.bytes_streamed_total,
                queue_depth: raw.queue_depth,
                hit_total: raw.hit_total,
                miss_total: raw.miss_total,
            }
        } else {
            NvmeTier::NotConfigured {
                reason: not_configured_reason(&raw),
            }
        }
    }
}

/// Prefetch, configured or not. Its fields are gauges and counters of the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum PrefetchFeature {
    Configured {
        inflight: Option<u64>,
        overlapped_layers_total: Option<u64>,
        stall_total: Option<u64>,
    },
    NotConfigured {
        reason: Option<String>,
    },
}

impl From<RawPrefetch> for PrefetchFeature {
    fn from(raw: RawPrefetch) -> Self {
        if raw.configured == Some(true) {
            PrefetchFeature::Configured {
                inflight: raw.inflight,
                overlapped_layers_total: raw.overlapped_layers_total,
                stall_total: raw.stall_total,
            }
        } else {
            PrefetchFeature::NotConfigured {
                reason: match raw.configured {
                    None => Some("the engine omitted the configured flag for prefetch".into()),
                    Some(_) => raw.reason,
                },
            }
        }
    }
}

// ── The IronWorks section of a sample ────────────────────────────────────────

/// Everything IronWorks reports that has no counterpart in the shared core.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// `None` when the body carries no such object at all.
    pub vram: Option<VramTier>,
    pub ram: Option<RamTier>,
    pub nvme: Option<NvmeTier>,
    pub prefetch: Option<PrefetchFeature>,
    pub masked_miss_enabled: Option<bool>,
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
        if let Some(VramTier::Configured {
            hit_total,
            miss_total,
            eviction_total,
            ..
        }) = &tiers.vram
        {
            put("tier.vram.hit_total", *hit_total);
            put("tier.vram.miss_total", *miss_total);
            put("tier.vram.eviction_total", *eviction_total);
        }
        if let Some(RamTier::Configured {
            hit_total,
            miss_total,
            ..
        }) = &tiers.ram
        {
            put("tier.ram.hit_total", *hit_total);
            put("tier.ram.miss_total", *miss_total);
        }
        if let Some(NvmeTier::Configured {
            read_total,
            bytes_streamed_total,
            hit_total,
            miss_total,
            ..
        }) = &tiers.nvme
        {
            put("tier.nvme.read_total", *read_total);
            put("tier.nvme.bytes_streamed_total", *bytes_streamed_total);
            put("tier.nvme.hit_total", *hit_total);
            put("tier.nvme.miss_total", *miss_total);
        }
        let prefetch = self.prefetch.clone().map(PrefetchFeature::from);
        if let Some(PrefetchFeature::Configured {
            overlapped_layers_total,
            stall_total,
            ..
        }) = &prefetch
        {
            put("prefetch.overlapped_layers_total", *overlapped_layers_total);
            put("prefetch.stall_total", *stall_total);
        }
        if let Some(mm) = &self.masked_miss {
            put("masked_miss.masked_pick_total", mm.masked_pick_total);
        }
        if let Some(r) = &self.requests {
            put("requests.total", r.total);
        }
        if let Some(d) = &self.decode {
            put("decode.prompt_tokens_total", d.prompt_tokens_total);
            put("decode.completion_tokens_total", d.completion_tokens_total);
        }

        let engine = self.engine.clone().unwrap_or_default();
        let decode = self.decode.clone().unwrap_or_default();
        let memory = self.memory.clone().unwrap_or_default();
        let requests = self.requests.clone().unwrap_or_default();

        ProviderSample {
            sampled_at_ns: self.sampled_at_ns,
            uptime_ns: self.uptime_ns,
            identity: Identity {
                model: engine.model,
                arch: engine.arch,
                backend: engine.backend,
            },
            counters,
            engine_reported: EngineReported {
                output_tok_s: decode.tok_s,
                prompt_tok_s: decode.prompt_tok_s,
            },
            ironworks: Some(Section {
                vram: tiers.vram,
                ram: tiers.ram,
                nvme: tiers.nvme,
                prefetch,
                masked_miss_enabled: self.masked_miss.as_ref().and_then(|m| m.enabled),
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

    const A_NVME_CONFIGURED: &str =
        include_str!("../../tests/fixtures/inference/a_nvme_configured.json");
    const B_NVME_NOT_CONFIGURED: &str =
        include_str!("../../tests/fixtures/inference/b_nvme_not_configured.json");
    const C_COUNTER_OMITTED: &str =
        include_str!("../../tests/fixtures/inference/c_counter_omitted.json");
    const A2_TWO_GPUS: &str = include_str!("../../tests/fixtures/inference/a2_two_gpus.json");
    const UNKNOWN_SCHEMA: &str = include_str!("../../tests/fixtures/inference/unknown_schema.json");
    const NOT_JSON: &str = include_str!("../../tests/fixtures/inference/not_json.txt");

    #[test]
    fn parses_configured_nvme_fixture() {
        let s = parse_metrics(A_NVME_CONFIGURED)
            .expect("fixture a parses")
            .into_sample();
        assert_eq!(s.identity.model.as_deref(), Some("glm-5.3-flash"));
        assert_eq!(s.counters["tier.nvme.read_total"], 4808);
        assert_eq!(s.counters["tier.nvme.bytes_streamed_total"], 45_550_822_784);
        assert_eq!(s.counters["tier.vram.eviction_total"], 198_159);
        assert_eq!(s.counters["masked_miss.masked_pick_total"], 0);
        assert_eq!(
            s.engine_reported.output_tok_s.and_then(|w| w.value),
            Some(18.4)
        );
        assert_eq!(
            s.engine_reported.output_tok_s.and_then(|w| w.window_ms),
            Some(1000)
        );
        assert_eq!(
            s.engine_reported.prompt_tok_s.and_then(|w| w.value),
            Some(312.7)
        );
        let ir = s.ironworks.expect("ironworks section");
        assert!(matches!(
            ir.nvme,
            Some(NvmeTier::Configured {
                read_total: Some(4808),
                queue_depth: Some(2),
                ..
            })
        ));
        assert!(matches!(
            ir.prefetch,
            Some(PrefetchFeature::Configured {
                inflight: Some(3),
                ..
            })
        ));
        assert_eq!(ir.masked_miss_enabled, Some(false));
        assert_eq!(ir.host_pinned_bytes, Some(96_635_289_600));
        assert_eq!(ir.requests_active, Some(1));
        let gpus = ir.per_gpu.expect("per gpu");
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].device_index, Some(1));
        assert_eq!(gpus[0].pci_bus_id.as_deref(), Some("00000000:03:00.0"));
    }

    #[test]
    fn parses_not_configured_nvme_fixture() {
        let s = parse_metrics(B_NVME_NOT_CONFIGURED)
            .expect("fixture b parses")
            .into_sample();
        match s.ironworks.expect("section").nvme {
            Some(NvmeTier::NotConfigured { reason }) => {
                assert_eq!(
                    reason.as_deref(),
                    Some("model fits in RAM+VRAM; no NVMe tier")
                );
            }
            other => panic!("expected NotConfigured, got {other:?}"),
        }
        assert!(!s.counters.keys().any(|k| k.starts_with("tier.nvme.")));
        assert_eq!(s.counters["tier.ram.hit_total"], 12_044);
        assert_eq!(
            s.engine_reported.prompt_tok_s.and_then(|w| w.value),
            Some(980.2)
        );
    }

    #[test]
    fn omitted_fields_stay_absent_not_zero() {
        let s = parse_metrics(C_COUNTER_OMITTED)
            .expect("fixture c parses")
            .into_sample();
        // eviction_total is absent from a configured tier.
        assert!(!s.counters.contains_key("tier.vram.eviction_total"));
        assert!(matches!(
            s.ironworks.as_ref().unwrap().vram,
            Some(VramTier::Configured {
                eviction_total: None,
                ..
            })
        ));
        // prompt_tok_s is absent from decode.
        assert_eq!(s.engine_reported.prompt_tok_s, None);
        assert!(s.engine_reported.output_tok_s.is_some());
        // masked_miss is absent entirely.
        assert!(!s.counters.contains_key("masked_miss.masked_pick_total"));
        let ir = s.ironworks.expect("section");
        assert_eq!(ir.masked_miss_enabled, None);
        // The counters that are present are still there.
        assert_eq!(s.counters["tier.vram.hit_total"], 40_221);
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
    }

    #[test]
    fn sampler_over_two_polls_of_fixture_a() {
        let a = parse_metrics(A_NVME_CONFIGURED).unwrap().into_sample();
        let mut b = a.clone();
        b.sampled_at_ns = a.sampled_at_ns.map(|t| t + 2_000_000_000);
        for v in b.counters.values_mut() {
            *v += 100;
        }
        let mut sampler = InferenceSampler::default();
        sampler.update(a);
        sampler.update(b);
        assert_eq!(sampler.derived().rate("tier.nvme.read_total"), Some(50.0));
    }

    fn with_tokens(mut body: serde_json::Value, prompt: u64, completion: u64) -> ProviderSample {
        body["decode"]["prompt_tokens_total"] = prompt.into();
        body["decode"]["completion_tokens_total"] = completion.into();
        parse_metrics(&body.to_string()).unwrap().into_sample()
    }

    #[test]
    fn token_counters_give_derived_throughput() {
        let body: serde_json::Value = serde_json::from_str(A_NVME_CONFIGURED).unwrap();
        let first = with_tokens(body, 1_000, 500);
        let mut second = first.clone();
        second.sampled_at_ns = first.sampled_at_ns.map(|t| t + 2_000_000_000);
        second.uptime_ns = first.uptime_ns.map(|t| t + 2_000_000_000);
        second
            .counters
            .insert("decode.prompt_tokens_total".into(), 1_600);
        second
            .counters
            .insert("decode.completion_tokens_total".into(), 540);

        let mut sampler = InferenceSampler::default();
        sampler.update(first);
        assert_eq!(
            sampler.derived().rate("decode.completion_tokens_total"),
            None
        );
        sampler.update(second);
        assert_eq!(
            sampler.derived().rate("decode.prompt_tokens_total"),
            Some(300.0)
        );
        assert_eq!(
            sampler.derived().rate("decode.completion_tokens_total"),
            Some(20.0)
        );
    }

    #[test]
    fn token_counters_stay_absent_until_instrumented() {
        let s = parse_metrics(A_NVME_CONFIGURED).unwrap().into_sample();
        assert!(!s.counters.contains_key("decode.prompt_tokens_total"));
        assert!(!s.counters.contains_key("decode.completion_tokens_total"));
    }

    #[test]
    fn a_falling_uptime_is_a_restart_even_when_counters_rose() {
        let first = parse_metrics(A_NVME_CONFIGURED).unwrap().into_sample();
        let mut restarted = first.clone();
        restarted.sampled_at_ns = first.sampled_at_ns.map(|t| t + 1_000_000_000);
        restarted.uptime_ns = Some(1_000);
        for v in restarted.counters.values_mut() {
            *v += 10;
        }
        let mut sampler = InferenceSampler::default();
        sampler.update(first);
        sampler.update(restarted);
        assert_eq!(sampler.derived().rate("tier.nvme.read_total"), None);
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
