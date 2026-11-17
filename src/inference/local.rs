//! Local inference servers other than IronWorks: Ollama, llama.cpp, vLLM,
//! LM Studio and MLX-LM.
//!
//! Each adapter reads only what its server documents, and the source it was
//! verified against is cited beside its parser. Nothing here treats an open port
//! as the provider: a reply counts as the provider only when its body carries the
//! provider's own markers. An answer that lacks them is
//! [`SourceError::Rejected`], which is a different state from nothing answering.
//!
//! Defaults are loopback addresses, not `localhost`: on this machine `localhost`
//! costs about 2.2 s per request (see [`super::ironworks::DEFAULT_ENDPOINT`]).

use super::http;
use super::ironworks::locality_of;
use super::{
    EngineState, Identity, InferenceSource, Locality, MeanDef, Progress, ProviderKind,
    ProviderSample, RatioDef, ServingDetail, SourceError,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Override for each local server's base URL, in the `IRONMON_IRONWORKS_URL` pattern.
pub const OLLAMA_ENV: &str = "IRONMON_OLLAMA_URL";
pub const LLAMACPP_ENV: &str = "IRONMON_LLAMACPP_URL";
pub const VLLM_ENV: &str = "IRONMON_VLLM_URL";
pub const SGLANG_ENV: &str = "IRONMON_SGLANG_URL";
pub const LMSTUDIO_ENV: &str = "IRONMON_LMSTUDIO_URL";
pub const MLX_ENV: &str = "IRONMON_MLX_URL";

/// Ollama's default listener.
pub const OLLAMA_DEFAULT: &str = "http://127.0.0.1:11434";
/// llama-server's default listener, per its README ("listens on 127.0.0.1:9931").
/// Older builds listened on 8080, which IronWorks uses by default, so a machine
/// running an old llama-server on 8080 sets `IRONMON_LLAMACPP_URL`.
pub const LLAMACPP_DEFAULT: &str = "http://127.0.0.1:9931";
/// vLLM's `vllm serve` default is port 8000; its metrics are at `/metrics` on it.
pub const VLLM_DEFAULT: &str = "http://127.0.0.1:8000";
/// SGLang's `launch_server` port in its docs, which serve `/metrics` on it.
pub const SGLANG_DEFAULT: &str = "http://127.0.0.1:30000";
/// LM Studio's documented port in its REST examples.
pub const LMSTUDIO_DEFAULT: &str = "http://127.0.0.1:1234";
/// The MLX-LM port this repository's setup guide uses (AI_INTEGRATION.md).
pub const MLX_DEFAULT: &str = "http://127.0.0.1:8082";

/// The environment variable and default for a local provider, or `None` for
/// IronWorks and the remote providers, which have their own configuration.
pub fn env_and_default(kind: ProviderKind) -> Option<(&'static str, &'static str)> {
    match kind {
        ProviderKind::Ollama => Some((OLLAMA_ENV, OLLAMA_DEFAULT)),
        ProviderKind::LlamaCpp => Some((LLAMACPP_ENV, LLAMACPP_DEFAULT)),
        ProviderKind::Vllm => Some((VLLM_ENV, VLLM_DEFAULT)),
        ProviderKind::Sglang => Some((SGLANG_ENV, SGLANG_DEFAULT)),
        ProviderKind::LmStudio => Some((LMSTUDIO_ENV, LMSTUDIO_DEFAULT)),
        ProviderKind::MlxLm => Some((MLX_ENV, MLX_DEFAULT)),
        ProviderKind::IronWorks | ProviderKind::OpenAi | ProviderKind::Anthropic => None,
    }
}

/// Every local provider as a source, each at its env override or default.
pub fn all_from_env() -> Vec<Box<dyn InferenceSource>> {
    [
        ProviderKind::Ollama,
        ProviderKind::LlamaCpp,
        ProviderKind::Vllm,
        ProviderKind::Sglang,
        ProviderKind::LmStudio,
        ProviderKind::MlxLm,
    ]
    .into_iter()
    .map(|kind| Box::new(LocalSource::from_env(kind)) as Box<dyn InferenceSource>)
    .collect()
}

/// The base URL from the environment variable, or the default when unset or blank.
fn base_from_env(var: &str, default: &str) -> String {
    std::env::var(var)
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// Counters the llama.cpp server exports under `llamacpp:` (README, `/metrics`):
/// https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md
const LLAMACPP_COUNTERS: &[&str] = &[
    "prompt_tokens_total",
    "tokens_predicted_total",
    "n_decode_total",
];
const LLAMACPP_GAUGES: &[&str] = &["requests_processing", "requests_deferred"];
/// Time totals in seconds beside the token totals: "Prompt process time" and
/// "Predict process time" in the README's metric table.
const LLAMACPP_SECONDS: &[&str] = &["prompt_seconds_total", "tokens_predicted_seconds_total"];
/// Seconds per token while the server was working on prompts, and while it was
/// generating. Unlike `prompt_tokens_total / window`, these exclude idle time.
const LLAMACPP_MEANS: &[MeanDef] = &[
    MeanDef {
        key: "prefill_per_token",
        seconds: "prompt_seconds_total",
        count: "prompt_tokens_total",
    },
    MeanDef {
        key: "decode_per_token",
        seconds: "tokens_predicted_seconds_total",
        count: "tokens_predicted_total",
    },
];
const NO_RATIOS: &[RatioDef] = &[];

/// Counters vLLM exports under `vllm:` (https://docs.vllm.ai/en/latest/usage/metrics.html).
/// The docs name each counter without the `_total` suffix that a Prometheus
/// client appends on exposition; the suffixed name is the one read here, and it
/// is not yet confirmed against a running server.
const VLLM_COUNTERS: &[&str] = &[
    "generation_tokens_total",
    "prompt_tokens_total",
    "request_success_total",
    "num_preemptions_total",
    "prefix_cache_hits_total",
    "prefix_cache_queries_total",
    "time_to_first_token_seconds_count",
    "request_queue_time_seconds_count",
    "request_prefill_time_seconds_count",
    "request_decode_time_seconds_count",
    "inter_token_latency_seconds_count",
    "e2e_request_latency_seconds_count",
];
/// The `_sum` of each latency histogram vLLM documents. Each is divided by its own
/// `_count` over a window, which is the mean the engine measured. Buckets are not read.
const VLLM_SECONDS: &[&str] = &[
    "time_to_first_token_seconds_sum",
    "request_queue_time_seconds_sum",
    "request_prefill_time_seconds_sum",
    "request_decode_time_seconds_sum",
    "inter_token_latency_seconds_sum",
    "e2e_request_latency_seconds_sum",
];
const VLLM_MEANS: &[MeanDef] = &[
    MeanDef {
        key: "ttft",
        seconds: "time_to_first_token_seconds_sum",
        count: "time_to_first_token_seconds_count",
    },
    MeanDef {
        key: "queue",
        seconds: "request_queue_time_seconds_sum",
        count: "request_queue_time_seconds_count",
    },
    MeanDef {
        key: "prefill",
        seconds: "request_prefill_time_seconds_sum",
        count: "request_prefill_time_seconds_count",
    },
    MeanDef {
        key: "decode",
        seconds: "request_decode_time_seconds_sum",
        count: "request_decode_time_seconds_count",
    },
    MeanDef {
        key: "inter_token",
        seconds: "inter_token_latency_seconds_sum",
        count: "inter_token_latency_seconds_count",
    },
    MeanDef {
        key: "e2e",
        seconds: "e2e_request_latency_seconds_sum",
        count: "e2e_request_latency_seconds_count",
    },
];
const VLLM_GAUGES: &[&str] = &[
    "num_requests_running",
    "num_requests_waiting",
    "kv_cache_usage_perc",
];
const VLLM_RATIOS: &[RatioDef] = &[RatioDef {
    key: "prefix_cache",
    numerator: "prefix_cache_hits_total",
    denominator: &["prefix_cache_queries_total"],
}];

/// Counters SGLang exports under `sglang:` when started with `--enable-metrics`
/// (https://docs.sglang.io/references/production_metrics.html). It documents no
/// request counter, so requests are not counted.
const SGLANG_COUNTERS: &[&str] = &[
    "prompt_tokens_total",
    "generation_tokens_total",
    "time_to_first_token_seconds_count",
    "e2e_request_latency_seconds_count",
    "inter_token_latency_seconds_count",
];
const SGLANG_SECONDS: &[&str] = &[
    "time_to_first_token_seconds_sum",
    "e2e_request_latency_seconds_sum",
    "inter_token_latency_seconds_sum",
];
const SGLANG_MEANS: &[MeanDef] = &[
    MeanDef {
        key: "ttft",
        seconds: "time_to_first_token_seconds_sum",
        count: "time_to_first_token_seconds_count",
    },
    MeanDef {
        key: "inter_token",
        seconds: "inter_token_latency_seconds_sum",
        count: "inter_token_latency_seconds_count",
    },
    MeanDef {
        key: "e2e",
        seconds: "e2e_request_latency_seconds_sum",
        count: "e2e_request_latency_seconds_count",
    },
];
/// Current figures. `token_usage` and `cache_hit_rate` are fractions of one, and
/// `gen_throughput` is the engine's own tokens per second.
const SGLANG_GAUGES: &[&str] = &[
    "num_running_reqs",
    "num_queue_reqs",
    "num_used_tokens",
    "token_usage",
    "cache_hit_rate",
    "gen_throughput",
];

/// What one Prometheus provider exports, and what is derived from it.
pub struct MetricSpec {
    pub prefix: &'static str,
    pub counters: &'static [&'static str],
    pub gauges: &'static [&'static str],
    /// Cumulative totals in seconds, which may be fractional.
    pub seconds: &'static [&'static str],
    pub ratios: &'static [RatioDef],
    pub means: &'static [MeanDef],
    /// The counter of generated tokens.
    pub generated: &'static str,
}

pub const LLAMACPP: MetricSpec = MetricSpec {
    prefix: "llamacpp:",
    counters: LLAMACPP_COUNTERS,
    gauges: LLAMACPP_GAUGES,
    seconds: LLAMACPP_SECONDS,
    ratios: NO_RATIOS,
    means: LLAMACPP_MEANS,
    generated: "tokens_predicted_total",
};

pub const VLLM: MetricSpec = MetricSpec {
    prefix: "vllm:",
    counters: VLLM_COUNTERS,
    gauges: VLLM_GAUGES,
    seconds: VLLM_SECONDS,
    ratios: VLLM_RATIOS,
    means: VLLM_MEANS,
    generated: "generation_tokens_total",
};

pub const SGLANG: MetricSpec = MetricSpec {
    prefix: "sglang:",
    counters: SGLANG_COUNTERS,
    gauges: SGLANG_GAUGES,
    seconds: SGLANG_SECONDS,
    ratios: NO_RATIOS,
    means: SGLANG_MEANS,
    generated: "generation_tokens_total",
};

/// A local engine at a base URL, read by the adapter for its provider.
pub struct LocalSource {
    kind: ProviderKind,
    base: String,
}

impl LocalSource {
    pub fn new(kind: ProviderKind, base: &str) -> Self {
        Self {
            kind,
            base: base.trim().trim_end_matches('/').to_string(),
        }
    }

    /// The source for `kind` at its environment override or default. For a kind
    /// with no local adapter (IronWorks and the remote providers) the base is
    /// empty, and polling it reports that the kind is not supported here.
    pub fn from_env(kind: ProviderKind) -> Self {
        let base = match env_and_default(kind) {
            Some((var, default)) => base_from_env(var, default),
            None => String::new(),
        };
        Self::new(kind, &base)
    }
}

impl InferenceSource for LocalSource {
    fn kind(&self) -> ProviderKind {
        self.kind
    }

    fn endpoint(&self) -> String {
        self.base.clone()
    }

    fn locality(&self) -> Locality {
        locality_of(&self.base)
    }

    fn poll(&self, timeout: Duration) -> Result<ProviderSample, SourceError> {
        let base = self.base.as_str();
        match self.kind {
            ProviderKind::Ollama => poll_ollama(base, timeout),
            ProviderKind::LlamaCpp => poll_metrics(base, timeout, &LLAMACPP).map(|mut s| {
                s.serving = llamacpp_serving(base, timeout);
                s
            }),
            ProviderKind::Vllm => poll_metrics(base, timeout, &VLLM).map(|mut s| {
                s.serving = vllm_serving(base, timeout);
                s
            }),
            ProviderKind::Sglang => poll_metrics(base, timeout, &SGLANG)
                .map(|mut s| {
                    s.serving = sglang_serving(base, timeout, &s);
                    s
                })
                .map_err(|e| match e {
                    // A 404 is SGLang with its metrics off only if SGLang answers
                    // on the same port; any other server's 404 stays a status.
                    SourceError::Status(404) if sglang_answers(base, timeout) => {
                        SourceError::MetricsNotEnabled
                    }
                    other => other,
                }),
            ProviderKind::LmStudio => poll_lmstudio(base, timeout),
            ProviderKind::MlxLm => poll_mlx(base, timeout),
            ProviderKind::IronWorks | ProviderKind::OpenAi | ProviderKind::Anthropic => Err(
                SourceError::NotSupported("this provider is read by its own adapter".into()),
            ),
        }
    }
}

/// The sentence an operator reads about why a local provider gave no sample,
/// naming the endpoint. Each state reads differently, so "nothing answered" is
/// never shown as "answered".
pub fn reason(kind: ProviderKind, endpoint: &str, error: &SourceError) -> String {
    let name = kind.display_name();
    match error {
        SourceError::Unreachable(why) => format!("no {name} answered at {endpoint}: {why}"),
        SourceError::Status(code @ (401 | 403))
            if matches!(kind, ProviderKind::OpenAi | ProviderKind::Anthropic) =>
        {
            format!("{name} rejected the configured key (HTTP {code})")
        }
        SourceError::Status(code) => {
            let hint = if kind == ProviderKind::LlamaCpp {
                "; llama-server serves /metrics only when started with --metrics"
            } else {
                ""
            };
            format!("{endpoint} answered HTTP {code}, not {name} data{hint}")
        }
        SourceError::MetricsNotEnabled if kind == ProviderKind::Sglang => {
            format!(
                "{name} answered, but its metrics are not enabled; start it with --enable-metrics"
            )
        }
        SourceError::MetricsNotEnabled => {
            format!("{name} answered, but its metrics are not enabled")
        }
        SourceError::NotSupported(why) => format!("{name} cannot be read in this build: {why}"),
        SourceError::NotJson(why) => format!("{endpoint} did not return JSON: {why}"),
        SourceError::Rejected(why) => {
            format!("{endpoint} answered, but not as {name}: {why}")
        }
        SourceError::NotConfigured(hint) => format!("{name} is not configured: {hint}"),
    }
}

/// Monotonic time since this process first asked, in nanoseconds. Used as the
/// uptime of servers that report no uptime. A rate is then over this process's
/// observation window, and a restart shows as the counters going backwards.
fn observed_uptime_ns() -> u64 {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN
        .get_or_init(Instant::now)
        .elapsed()
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64
}

// ── Ollama ───────────────────────────────────────────────────────────────────

/// `GET /api/ps` lists the models currently loaded into memory, and
/// `GET /api/version` returns the server version. Ollama documents no request or
/// token counters, so none are reported.
/// Source: https://raw.githubusercontent.com/ollama/ollama/main/docs/api.md
#[derive(Debug, Deserialize)]
struct PsReply {
    models: Option<Vec<PsModel>>,
}

#[derive(Debug, Deserialize)]
struct PsModel {
    name: Option<String>,
    model: Option<String>,
    /// The context the model was loaded with, in tokens.
    context_length: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct VersionReply {
    version: Option<String>,
}

/// The loaded models: their count, and the first name when one is reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub count: usize,
    pub first: Option<String>,
    /// The first loaded model's context, in tokens, where the listing gives it.
    pub context: Option<u64>,
}

/// Serving detail carrying only a context size, or none without one.
fn context_only(context: Option<u64>) -> Option<ServingDetail> {
    context.map(|c| ServingDetail {
        context_max: Some(c),
        ..ServingDetail::default()
    })
}

pub fn parse_ps(body: &str) -> Result<Loaded, SourceError> {
    let reply: PsReply =
        serde_json::from_str(body).map_err(|e| SourceError::NotJson(e.to_string()))?;
    let Some(models) = reply.models else {
        return Err(SourceError::Rejected("the reply has no models list".into()));
    };
    let first = models
        .iter()
        .find_map(|m| m.name.clone().or(m.model.clone()));
    Ok(Loaded {
        count: models.len(),
        first,
        context: models.first().and_then(|m| m.context_length),
    })
}

pub fn parse_version(body: &str) -> Result<String, SourceError> {
    let reply: VersionReply =
        serde_json::from_str(body).map_err(|e| SourceError::NotJson(e.to_string()))?;
    reply
        .version
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| SourceError::Rejected("the reply has no version".into()))
}

fn poll_ollama(base: &str, timeout: Duration) -> Result<ProviderSample, SourceError> {
    let ps = http::get(&format!("{base}/api/ps"), timeout, &[])?;
    if ps.status != 200 {
        return Err(SourceError::Status(ps.status));
    }
    let loaded = parse_ps(&ps.body)?;
    // The version is a second, optional fact: its failure leaves it unreported
    // rather than failing the poll.
    let version = http::get(&format!("{base}/api/version"), timeout, &[])
        .ok()
        .filter(|r| r.status == 200)
        .and_then(|r| parse_version(&r.body).ok());
    let mut gauges = BTreeMap::new();
    gauges.insert("models_loaded".to_string(), loaded.count as f64);
    Ok(ProviderSample {
        identity: Identity {
            model: loaded.first,
            backend: None,
            version,
            status: None,
        },
        gauges,
        serving: context_only(loaded.context),
        ..ProviderSample::default()
    })
}

// ── Prometheus text: llama.cpp and vLLM ──────────────────────────────────────

/// The series of one Prometheus exposition that belong to one provider prefix.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Prometheus {
    /// Whether any series carried the prefix. False means the body is not this
    /// provider's metrics, whatever its status.
    pub recognised: bool,
    /// Summed over label sets, for the counters asked for.
    pub counters: BTreeMap<String, u64>,
    /// Summed over label sets, for the gauges asked for.
    pub gauges: BTreeMap<String, f64>,
    /// Summed over label sets, for the seconds totals asked for.
    pub seconds: BTreeMap<String, f64>,
    /// The `model_name` label, when a series carries one.
    pub model: Option<String>,
}

/// Parse the text exposition format, keeping only the names asked for.
///
/// Counters are summed across their label sets, which gives the engine total.
/// A histogram's `_sum` and `_count` are read only when named, as a seconds total and
/// a counter; `_bucket` series are never read. A value that is not a finite
/// non-negative integer for a counter, not finite and non-negative for a seconds
/// total, or not finite for a gauge, is skipped: an unreadable value is absent, not zero.
pub fn parse_prometheus(
    body: &str,
    prefix: &str,
    counters: &[&str],
    gauges: &[&str],
    seconds: &[&str],
) -> Prometheus {
    let mut out = Prometheus::default();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let name_end = line
            .find(|c: char| c == '{' || c.is_whitespace())
            .unwrap_or(line.len());
        let name = &line[..name_end];
        let after = &line[name_end..];
        let (labels, value_part) = if let Some(rest) = after.strip_prefix('{') {
            match rest.find('}') {
                Some(close) => (&rest[..close], &rest[close + 1..]),
                None => continue,
            }
        } else {
            ("", after)
        };
        let Some(short) = name.strip_prefix(prefix) else {
            continue;
        };
        out.recognised = true;
        let Some(raw_value) = value_part.split_whitespace().next() else {
            continue;
        };
        let Ok(value) = raw_value.parse::<f64>() else {
            continue;
        };
        if !value.is_finite() {
            continue;
        }
        if out.model.is_none() {
            out.model = label(labels, "model_name");
        }
        if counters.contains(&short) {
            if value >= 0.0 && value.fract() == 0.0 && value <= u64::MAX as f64 {
                let total = out.counters.entry(short.to_string()).or_insert(0);
                *total = total.saturating_add(value as u64);
            }
        } else if gauges.contains(&short) {
            *out.gauges.entry(short.to_string()).or_insert(0.0) += value;
        } else if seconds.contains(&short) && value >= 0.0 {
            *out.seconds.entry(short.to_string()).or_insert(0.0) += value;
        }
    }
    out
}

fn label(labels: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=\"");
    let start = labels.find(&pattern)? + pattern.len();
    let end = labels[start..].find('"')? + start;
    let value = &labels[start..end];
    (!value.is_empty()).then(|| value.to_string())
}

/// `GET /metrics`, the Prometheus text endpoint.
///
/// llama.cpp: "This endpoint is only accessible if `--metrics` is set."
/// Source: https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md
/// vLLM: `GET /metrics` on the OpenAI-compatible server.
/// Source: https://docs.vllm.ai/en/latest/usage/metrics.html
fn poll_metrics(
    base: &str,
    timeout: Duration,
    spec: &MetricSpec,
) -> Result<ProviderSample, SourceError> {
    let prefix = spec.prefix;
    let reply = http::get(&format!("{base}/metrics"), timeout, &[])?;
    if reply.status != 200 {
        return Err(SourceError::Status(reply.status));
    }
    let parsed = parse_prometheus(
        &reply.body,
        prefix,
        spec.counters,
        spec.gauges,
        spec.seconds,
    );
    if !parsed.recognised {
        return Err(SourceError::Rejected(format!(
            "the body has no {prefix} series"
        )));
    }
    Ok(ProviderSample {
        uptime_ns: Some(observed_uptime_ns()),
        identity: Identity {
            model: parsed.model,
            backend: None,
            version: None,
            status: None,
        },
        counters: parsed.counters,
        gauges: parsed.gauges,
        ratio_defs: spec.ratios,
        seconds: parsed.seconds,
        mean_defs: spec.means,
        generated_counter: Some(spec.generated),
        ..ProviderSample::default()
    })
}

// ── Serving detail: llama.cpp and vLLM ──────────────────────────────────────

/// One llama-server slot, as `GET /slots` reports it. Only the counts are read: a
/// slot also carries its prompt text, which is never parsed.
/// Source: https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md
#[derive(Debug, Deserialize)]
struct Slot {
    n_ctx: Option<u64>,
    is_processing: Option<bool>,
    params: Option<SlotParams>,
    /// An object in most builds; an array of them in some.
    next_token: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct SlotParams {
    /// Tokens to generate; -1 for no limit.
    n_predict: Option<i64>,
}

/// What llama-server says through its slots: busy or idle, how far the busy slot
/// has generated, and the context each slot holds.
pub fn parse_slots(body: &str) -> Result<ServingDetail, SourceError> {
    let slots: Vec<Slot> =
        serde_json::from_str(body).map_err(|e| SourceError::NotJson(e.to_string()))?;
    let Some(first) = slots.first() else {
        return Err(SourceError::Rejected("the server lists no slots".into()));
    };
    let busy = slots.iter().find(|s| s.is_processing == Some(true));
    let token = |s: &Slot, key: &str| -> Option<i64> {
        let t = s.next_token.as_ref()?;
        let t = if t.is_array() { t.get(0)? } else { t };
        t.get(key)?.as_i64()
    };
    let (state, progress) = match busy {
        Some(slot) => {
            let decoded = token(slot, "n_decoded").filter(|n| *n >= 0);
            let total = match (decoded, token(slot, "n_remain")) {
                (Some(d), Some(r)) if r >= 0 => Some(d + r),
                _ => slot
                    .params
                    .as_ref()
                    .and_then(|p| p.n_predict)
                    .filter(|n| *n > 0),
            };
            let state = match decoded {
                Some(d) if d > 0 => EngineState::Generating,
                Some(_) => EngineState::Reading,
                None => EngineState::Other("processing".into()),
            };
            let progress = match (&state, decoded, total) {
                (EngineState::Generating, Some(d), Some(t)) if t > 0 => Some(Progress {
                    done: d as u64,
                    total: t as u64,
                    what: "tokens of n_predict",
                }),
                _ => None,
            };
            (Some(state), progress)
        }
        None if slots.iter().all(|s| s.is_processing.is_some()) => (Some(EngineState::Idle), None),
        None => (None, None),
    };
    Ok(ServingDetail {
        state,
        queued: None,
        progress,
        context_max: first.n_ctx,
        ..ServingDetail::default()
    })
}

/// `GET /props`: the context size from `default_generation_settings.n_ctx`.
fn llamacpp_context(base: &str, timeout: Duration) -> Option<u64> {
    let reply = http::get(&format!("{base}/props"), timeout, &[]).ok()?;
    if reply.status != 200 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&reply.body).ok()?;
    v.pointer("/default_generation_settings/n_ctx")?.as_u64()
}

/// llama-server's slots, else its context size alone. Both are second, optional
/// reads: a server started with `--no-slots` still gives its metrics.
fn llamacpp_serving(base: &str, timeout: Duration) -> Option<ServingDetail> {
    let slots = http::get(&format!("{base}/slots"), timeout, &[])
        .ok()
        .filter(|r| r.status == 200)
        .and_then(|r| parse_slots(&r.body).ok());
    match slots {
        Some(mut d) => {
            if d.context_max.is_none() {
                d.context_max = llamacpp_context(base, timeout);
            }
            Some(d)
        }
        None => context_only(llamacpp_context(base, timeout)),
    }
}

/// vLLM's `GET /v1/models` gives each served model's `max_model_len`.
/// Source: https://docs.vllm.ai/en/latest/serving/openai_compatible_server.html
pub fn parse_vllm_models(body: &str) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("data")?
        .as_array()?
        .iter()
        .find_map(|m| m.get("max_model_len")?.as_u64())
}

fn vllm_serving(base: &str, timeout: Duration) -> Option<ServingDetail> {
    let reply = http::get(&format!("{base}/v1/models"), timeout, &[]).ok()?;
    if reply.status != 200 {
        return None;
    }
    context_only(parse_vllm_models(&reply.body))
}

/// Whether SGLang itself answers at `base`: its `GET /model_info` reply names the
/// `model_path` it loaded. Source: sglang/srt/entrypoints/http_server.py.
fn sglang_answers(base: &str, timeout: Duration) -> bool {
    http::get(&format!("{base}/model_info"), timeout, &[])
        .ok()
        .filter(|r| r.status == 200)
        .and_then(|r| serde_json::from_str::<serde_json::Value>(&r.body).ok())
        .is_some_and(|v| v.get("model_path").is_some())
}

/// SGLang's `/v1/models` cards carry `max_model_len` as vLLM's do, and its queue
/// is a gauge. Its own state per request is not published, so none is claimed.
/// Source: sglang/srt/entrypoints/openai/protocol.py, `ModelCard`.
fn sglang_serving(base: &str, timeout: Duration, sample: &ProviderSample) -> Option<ServingDetail> {
    let context = http::get(&format!("{base}/v1/models"), timeout, &[])
        .ok()
        .filter(|r| r.status == 200)
        .and_then(|r| parse_vllm_models(&r.body));
    let queued = sample
        .gauges
        .get("num_queue_reqs")
        .map(|v| v.max(0.0) as u64);
    if context.is_none() && queued.is_none() {
        return None;
    }
    Some(ServingDetail {
        context_max: context,
        queued,
        ..Default::default()
    })
}

// ── LM Studio ────────────────────────────────────────────────────────────────

/// `GET /api/v1/models`, the v1 REST listing. The top-level key is `models`, and
/// a model is loaded when its `loaded_instances` array is non-empty. The v1 API
/// is the one the docs recommend; the listing gives no runtime counters.
/// Source: https://lmstudio.ai/docs/developer/rest/list
#[derive(Debug, Deserialize)]
struct LmsReply {
    models: Option<Vec<LmsModel>>,
}

#[derive(Debug, Deserialize)]
struct LmsModel {
    key: Option<String>,
    loaded_instances: Option<Vec<serde_json::Value>>,
}

pub fn parse_lmstudio(body: &str) -> Result<Loaded, SourceError> {
    let reply: LmsReply =
        serde_json::from_str(body).map_err(|e| SourceError::NotJson(e.to_string()))?;
    let Some(models) = reply.models else {
        return Err(SourceError::Rejected("the reply has no models list".into()));
    };
    let loaded: Vec<&LmsModel> = models
        .iter()
        .filter(|m| m.loaded_instances.as_ref().is_some_and(|v| !v.is_empty()))
        .collect();
    // Source: https://lmstudio.ai/docs/developer/rest/list — each loaded instance
    // carries `config.context_length`.
    let context = loaded
        .first()
        .and_then(|m| m.loaded_instances.as_ref()?.first())
        .and_then(|i| i.pointer("/config/context_length")?.as_u64());
    Ok(Loaded {
        count: loaded.len(),
        first: loaded.first().and_then(|m| m.key.clone()),
        context,
    })
}

fn poll_lmstudio(base: &str, timeout: Duration) -> Result<ProviderSample, SourceError> {
    let reply = http::get(&format!("{base}/api/v1/models"), timeout, &[])?;
    if reply.status != 200 {
        return Err(SourceError::Status(reply.status));
    }
    let loaded = parse_lmstudio(&reply.body)?;
    let mut gauges = BTreeMap::new();
    gauges.insert("models_loaded".to_string(), loaded.count as f64);
    Ok(ProviderSample {
        identity: Identity {
            model: loaded.first,
            backend: None,
            version: None,
            status: None,
        },
        gauges,
        serving: context_only(loaded.context),
        ..ProviderSample::default()
    })
}

// ── MLX-LM ───────────────────────────────────────────────────────────────────

/// `GET /health` answers `{"status": "ok"}` with 200 when the generation thread is
/// alive, and `{"status": "unavailable"}` with 503 otherwise. MLX-LM exposes no
/// metrics, so this is reachability only.
/// Source: https://raw.githubusercontent.com/ml-explore/mlx-lm/main/mlx_lm/server.py
#[derive(Debug, Deserialize)]
struct HealthReply {
    status: Option<String>,
}

fn poll_mlx(base: &str, timeout: Duration) -> Result<ProviderSample, SourceError> {
    let reply = http::get(&format!("{base}/health"), timeout, &[])?;
    if reply.status != 200 {
        return Err(SourceError::Status(reply.status));
    }
    let health: HealthReply =
        serde_json::from_str(&reply.body).map_err(|e| SourceError::NotJson(e.to_string()))?;
    match health.status.as_deref() {
        Some("ok") => Ok(ProviderSample {
            identity: Identity {
                status: Some("ok".into()),
                ..Identity::default()
            },
            ..ProviderSample::default()
        }),
        Some(other) => Err(SourceError::Rejected(format!(
            "its health status is \"{other}\""
        ))),
        None => Err(SourceError::Rejected(
            "the health reply has no status".into(),
        )),
    }
}

#[cfg(test)]
mod serving_tests {
    use super::*;

    #[test]
    fn a_generating_slot_gives_its_progress_and_never_its_prompt() {
        let body = r#"[{"id":0,"n_ctx":8192,"is_processing":true,"prompt":"PRIVATE TEXT",
            "params":{"n_predict":-1},
            "next_token":{"has_next_token":true,"n_remain":300,"n_decoded":200}},
            {"id":1,"n_ctx":8192,"is_processing":false}]"#;
        let d = parse_slots(body).unwrap();
        assert_eq!(d.state, Some(EngineState::Generating));
        let p = d.progress.clone().unwrap();
        assert_eq!((p.done, p.total), (200, 500));
        assert_eq!(d.context_max, Some(8192));
        assert!(!format!("{d:?}").contains("PRIVATE"));
    }

    #[test]
    fn a_slot_reading_its_prompt_has_no_generation_progress() {
        let body = r#"[{"n_ctx":4096,"is_processing":true,
            "next_token":[{"n_remain":-1,"n_decoded":0}]}]"#;
        let d = parse_slots(body).unwrap();
        assert_eq!(d.state, Some(EngineState::Reading));
        assert!(d.progress.is_none());
    }

    #[test]
    fn idle_slots_say_idle_and_a_limitless_request_has_no_total() {
        let idle = parse_slots(r#"[{"n_ctx":4096,"is_processing":false}]"#).unwrap();
        assert_eq!(idle.state, Some(EngineState::Idle));
        let open = parse_slots(
            r#"[{"is_processing":true,"params":{"n_predict":-1},
                "next_token":{"n_remain":-1,"n_decoded":9}}]"#,
        )
        .unwrap();
        assert_eq!(open.state, Some(EngineState::Generating));
        assert!(
            open.progress.is_none(),
            "no limit, so no total to count against"
        );
    }

    #[test]
    fn context_sizes_come_from_each_engines_listing() {
        assert_eq!(
            parse_vllm_models(r#"{"data":[{"id":"m","max_model_len":32768}]}"#),
            Some(32768)
        );
        assert_eq!(parse_vllm_models(r#"{"data":[{"id":"m"}]}"#), None);
        let ps = parse_ps(r#"{"models":[{"name":"m","context_length":4096}]}"#).unwrap();
        assert_eq!(ps.context, Some(4096));
        let lms = parse_lmstudio(
            r#"{"models":[{"key":"m","loaded_instances":[{"id":"m","config":{"context_length":8192}}]}]}"#,
        )
        .unwrap();
        assert_eq!(lms.context, Some(8192));
    }
}

#[cfg(test)]
mod tests {
    /// A refused key is a statement about the key, not about what answered.
    #[test]
    fn a_rejected_remote_key_is_named_as_such() {
        let text = reason(
            ProviderKind::Anthropic,
            "https://api.anthropic.com",
            &SourceError::Status(401),
        );
        assert_eq!(text, "Anthropic rejected the configured key (HTTP 401)");
    }

    use super::*;

    #[test]
    fn ollama_ps_counts_loaded_models_and_names_the_first() {
        let body = r#"{"models":[{"name":"mistral:latest","model":"mistral:latest","size":4100000000,"size_vram":4100000000},{"name":"llama3:8b"}]}"#;
        let loaded = parse_ps(body).expect("parses");
        assert_eq!(loaded.count, 2);
        assert_eq!(loaded.first.as_deref(), Some("mistral:latest"));
    }

    #[test]
    fn ollama_ps_with_no_models_is_a_real_zero() {
        let loaded = parse_ps(r#"{"models":[]}"#).expect("parses");
        assert_eq!(loaded.count, 0);
        assert_eq!(loaded.first, None);
    }

    #[test]
    fn ollama_ps_without_a_models_list_is_rejected_not_zero() {
        // A different server answering JSON with another shape.
        assert!(matches!(
            parse_ps(r#"{"data":[]}"#),
            Err(SourceError::Rejected(_))
        ));
        assert!(matches!(parse_ps("<html>"), Err(SourceError::NotJson(_))));
    }

    #[test]
    fn ollama_version_needs_a_version_string() {
        assert_eq!(
            parse_version(r#"{"version":"0.5.1"}"#).as_deref(),
            Ok("0.5.1")
        );
        assert!(parse_version(r#"{"version":" "}"#).is_err());
    }

    const LLAMA_BODY: &str = "\
# HELP llamacpp:prompt_tokens_total Number of prompt tokens processed.
# TYPE llamacpp:prompt_tokens_total counter
llamacpp:prompt_tokens_total 1200
llamacpp:tokens_predicted_total 340
llamacpp:n_decode_total 77
llamacpp:requests_processing 2
llamacpp:requests_deferred 0
";

    #[test]
    fn llamacpp_counters_and_gauges_are_read_by_name() {
        let p = parse_prometheus(
            LLAMA_BODY,
            "llamacpp:",
            LLAMACPP_COUNTERS,
            LLAMACPP_GAUGES,
            &[],
        );
        assert!(p.recognised);
        assert_eq!(p.counters["prompt_tokens_total"], 1200);
        assert_eq!(p.counters["tokens_predicted_total"], 340);
        assert_eq!(p.gauges["requests_processing"], 2.0);
        assert_eq!(p.gauges["requests_deferred"], 0.0);
    }

    #[test]
    fn a_metric_the_server_omits_is_absent_not_zero() {
        let p = parse_prometheus(
            "llamacpp:requests_processing 1\n",
            "llamacpp:",
            LLAMACPP_COUNTERS,
            LLAMACPP_GAUGES,
            LLAMACPP_SECONDS,
        );
        assert!(!p.counters.contains_key("prompt_tokens_total"));
        assert!(!p.gauges.contains_key("requests_deferred"));
    }

    #[test]
    fn vllm_counters_are_summed_across_label_sets() {
        let body = "\
vllm:request_success_total{finished_reason=\"stop\",model_name=\"llama-3\"} 10
vllm:request_success_total{finished_reason=\"length\",model_name=\"llama-3\"} 4
vllm:num_requests_running{model_name=\"llama-3\"} 3
vllm:kv_cache_usage_perc{model_name=\"llama-3\"} 0.25
vllm:request_success_created{finished_reason=\"stop\"} 1.7e9
vllm:time_to_first_token_seconds_bucket{le=\"0.1\"} 99
";
        let p = parse_prometheus(body, "vllm:", VLLM_COUNTERS, VLLM_GAUGES, VLLM_SECONDS);
        assert_eq!(p.counters["request_success_total"], 14);
        assert_eq!(p.gauges["num_requests_running"], 3.0);
        assert_eq!(p.gauges["kv_cache_usage_perc"], 0.25);
        assert_eq!(p.model.as_deref(), Some("llama-3"));
        // Buckets and _created parts are never read; a bucket is not a count.
        assert!(!p.counters.keys().any(|k| k.contains("bucket")));
        assert!(!p.counters.contains_key("time_to_first_token_seconds_count"));
        assert!(!p.counters.keys().any(|k| k.contains("created")));
    }

    #[test]
    fn a_body_without_the_prefix_is_not_this_provider() {
        let p = parse_prometheus(
            "some_other_metric 1\n",
            "vllm:",
            VLLM_COUNTERS,
            VLLM_GAUGES,
            &[],
        );
        assert!(!p.recognised);
    }

    #[test]
    fn a_non_integer_counter_is_skipped_not_guessed() {
        let p = parse_prometheus(
            "vllm:generation_tokens_total 1.5\nvllm:prompt_tokens_total NaN\n",
            "vllm:",
            VLLM_COUNTERS,
            VLLM_GAUGES,
            VLLM_SECONDS,
        );
        assert!(p.recognised);
        assert!(p.counters.is_empty());
    }

    #[test]
    fn latency_histograms_are_read_as_a_sum_and_a_count() {
        let body = "\
vllm:time_to_first_token_seconds_bucket{le=\"0.1\",model_name=\"m\"} 3
vllm:time_to_first_token_seconds_sum{model_name=\"m\"} 1.25
vllm:time_to_first_token_seconds_count{model_name=\"m\"} 5
vllm:e2e_request_latency_seconds_sum{model_name=\"m\"} 9.5
";
        let p = parse_prometheus(body, "vllm:", VLLM_COUNTERS, VLLM_GAUGES, VLLM_SECONDS);
        assert_eq!(p.seconds["time_to_first_token_seconds_sum"], 1.25);
        assert_eq!(p.counters["time_to_first_token_seconds_count"], 5);
        // A sum without its count is kept; the mean is simply not derived.
        assert_eq!(p.seconds["e2e_request_latency_seconds_sum"], 9.5);
        assert!(!p.counters.contains_key("e2e_request_latency_seconds_count"));
    }

    #[test]
    fn llamacpp_time_totals_give_seconds_per_token_over_a_window() {
        use crate::inference::InferenceSampler;
        let sample = |prompt_s: f64, prompt: u64, at: u64| {
            let body = format!(
                "llamacpp:prompt_tokens_total {prompt}\nllamacpp:prompt_seconds_total {prompt_s}\n"
            );
            let p = parse_prometheus(
                &body,
                "llamacpp:",
                LLAMACPP_COUNTERS,
                LLAMACPP_GAUGES,
                LLAMACPP_SECONDS,
            );
            ProviderSample {
                uptime_ns: Some(at),
                counters: p.counters,
                seconds: p.seconds,
                mean_defs: LLAMACPP_MEANS,
                ..ProviderSample::default()
            }
        };
        let mut s = InferenceSampler::default();
        s.update(sample(2.0, 1_000, 1_000_000_000));
        s.update(sample(2.5, 2_000, 9_000_000_000));
        // Half a second of prompt processing for a thousand tokens: 0.5 ms each,
        // however long the server sat idle in the eight-second window.
        let mean = s.derived().mean("prefill_per_token").unwrap();
        assert!((mean - 0.0005).abs() < 1e-12, "{mean}");
        assert_eq!(s.derived().mean("decode_per_token"), None);
    }

    #[test]
    fn lmstudio_counts_models_with_loaded_instances() {
        let body = r#"{"models":[
            {"type":"llm","key":"google/gemma-4-26b-a4b","loaded_instances":[{"id":"a","config":{}}]},
            {"type":"llm","key":"qwen/other","loaded_instances":[]}
        ]}"#;
        let loaded = parse_lmstudio(body).expect("parses");
        assert_eq!(loaded.count, 1);
        assert_eq!(loaded.first.as_deref(), Some("google/gemma-4-26b-a4b"));
        assert!(matches!(
            parse_lmstudio(r#"{"data":[]}"#),
            Err(SourceError::Rejected(_))
        ));
    }

    #[test]
    fn reasons_name_the_endpoint_and_separate_the_states() {
        let none = reason(
            ProviderKind::Vllm,
            "http://127.0.0.1:8000",
            &SourceError::Unreachable("connection refused".into()),
        );
        let other = reason(
            ProviderKind::Vllm,
            "http://127.0.0.1:8000",
            &SourceError::Status(404),
        );
        assert!(none.starts_with("no vLLM answered at http://127.0.0.1:8000"));
        assert!(other.contains("answered HTTP 404"));
        assert_ne!(none, other);
        let llama = reason(
            ProviderKind::LlamaCpp,
            "http://127.0.0.1:9931",
            &SourceError::Status(404),
        );
        assert!(llama.contains("--metrics"), "{llama}");
    }

    #[test]
    fn env_overrides_follow_the_ironworks_pattern() {
        assert_eq!(
            env_and_default(ProviderKind::Ollama),
            Some((OLLAMA_ENV, OLLAMA_DEFAULT))
        );
        assert_eq!(env_and_default(ProviderKind::IronWorks), None);
        assert!(OLLAMA_DEFAULT.starts_with("http://127.0.0.1"));
    }
}

#[cfg(test)]
mod poll_tests {
    use super::*;
    use crate::inference::poller_tests::Stub;
    use std::time::Instant;

    const WAIT: Duration = Duration::from_secs(2);

    /// Hand-written from the vLLM and llama.cpp metric names in their docs.
    const VLLM_BODY: &str =
        "vllm:generation_tokens_total 42\nvllm:num_requests_running{model_name=\"m\"} 1\n";

    #[test]
    fn vllm_metrics_from_a_loopback_server_are_read() {
        let stub = Stub::start(200, VLLM_BODY);
        let source = LocalSource::new(ProviderKind::Vllm, &stub.url());
        let sample = source.poll(WAIT);
        stub.stop();
        let sample = sample.expect("the metrics are read");
        assert_eq!(sample.counters["generation_tokens_total"], 42);
        assert_eq!(sample.gauges["num_requests_running"], 1.0);
        assert_eq!(sample.identity.model.as_deref(), Some("m"));
    }

    /// Hand-written from SGLang's production-metrics docs.
    const SGLANG_BODY: &str = "sglang:generation_tokens_total{model_name=\"q\"} 7\n\
sglang:num_running_reqs{model_name=\"q\"} 2\n\
sglang:num_queue_reqs{model_name=\"q\"} 3\n\
sglang:token_usage{model_name=\"q\"} 0.28\n";

    #[test]
    fn sglang_metrics_from_a_loopback_server_are_read() {
        let stub = Stub::start(200, SGLANG_BODY);
        let source = LocalSource::new(ProviderKind::Sglang, &stub.url());
        let sample = source.poll(WAIT);
        stub.stop();
        let sample = sample.expect("the metrics are read");
        assert_eq!(sample.counters["generation_tokens_total"], 7);
        assert_eq!(sample.gauges["num_running_reqs"], 2.0);
        assert_eq!(sample.gauges["token_usage"], 0.28);
        assert_eq!(sample.identity.model.as_deref(), Some("q"));
        // The stub's /v1/models is not JSON, so no context; the queue is the gauge.
        let serving = sample.serving.expect("the queue is reported");
        assert_eq!(serving.queued, Some(3));
        assert_eq!(serving.context_max, None);
    }

    /// A 404 from a server that is not SGLang is not "SGLang with metrics off".
    #[test]
    fn an_unrelated_404_is_not_sglang_with_metrics_off() {
        let stub = Stub::start(404, "<html>no such page</html>");
        let source = LocalSource::new(ProviderKind::Sglang, &stub.url());
        let result = source.poll(WAIT);
        stub.stop();
        assert_eq!(result, Err(SourceError::Status(404)));
    }

    /// Another server on the port answers 404 with no vLLM series. It is reported
    /// as an answer that is not vLLM, not as a provider with its metrics off.
    #[test]
    fn an_unrelated_404_is_a_status_not_a_provider() {
        let stub = Stub::start(404, "<html>no such page</html>");
        let source = LocalSource::new(ProviderKind::Vllm, &stub.url());
        let result = source.poll(WAIT);
        stub.stop();
        assert_eq!(result, Err(SourceError::Status(404)));
    }

    #[test]
    fn a_200_without_the_providers_series_is_rejected() {
        let stub = Stub::start(200, "some_other_metric 1\n");
        let source = LocalSource::new(ProviderKind::Vllm, &stub.url());
        let result = source.poll(WAIT);
        stub.stop();
        assert!(
            matches!(result, Err(SourceError::Rejected(_))),
            "{result:?}"
        );
    }

    #[test]
    fn ollama_ps_from_a_loopback_server_is_read() {
        let stub = Stub::start(200, r#"{"models":[{"name":"qwen:7b"}]}"#);
        let source = LocalSource::new(ProviderKind::Ollama, &stub.url());
        let sample = source.poll(WAIT);
        stub.stop();
        let sample = sample.expect("read");
        assert_eq!(sample.identity.model.as_deref(), Some("qwen:7b"));
        assert_eq!(sample.gauges["models_loaded"], 1.0);
    }

    /// A closed port is reported as unreachable, and the poll returns within its
    /// deadline. On this Windows machine a refused loopback connect takes about two
    /// seconds to report, measured with a raw socket, so the bound is the deadline
    /// plus slack rather than an instant refusal.
    #[test]
    fn a_closed_port_is_unreachable_and_bounded_by_the_deadline() {
        let stub = Stub::start(200, VLLM_BODY);
        let url = stub.url();
        stub.stop();
        let source = LocalSource::new(ProviderKind::Vllm, &url);
        let started = Instant::now();
        let result = source.poll(WAIT);
        assert!(
            started.elapsed() < WAIT + Duration::from_secs(2),
            "a closed port held the poll for {:?}",
            started.elapsed()
        );
        assert!(
            matches!(result, Err(SourceError::Unreachable(_))),
            "{result:?}"
        );
    }
}
