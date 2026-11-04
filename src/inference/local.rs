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
    Identity, InferenceSource, Locality, ProviderKind, ProviderSample, RatioDef, SourceError,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Override for each local server's base URL, in the `IRONMON_IRONWORKS_URL` pattern.
pub const OLLAMA_ENV: &str = "IRONMON_OLLAMA_URL";
pub const LLAMACPP_ENV: &str = "IRONMON_LLAMACPP_URL";
pub const VLLM_ENV: &str = "IRONMON_VLLM_URL";
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
        ProviderKind::LmStudio => Some((LMSTUDIO_ENV, LMSTUDIO_DEFAULT)),
        ProviderKind::MlxLm => Some((MLX_ENV, MLX_DEFAULT)),
        ProviderKind::IronWorks => None,
    }
}

/// Every local provider as a source, each at its env override or default.
pub fn all_from_env() -> Vec<Box<dyn InferenceSource>> {
    [
        ProviderKind::Ollama,
        ProviderKind::LlamaCpp,
        ProviderKind::Vllm,
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
            ProviderKind::LlamaCpp => poll_metrics(
                base,
                timeout,
                "llamacpp:",
                LLAMACPP_COUNTERS,
                LLAMACPP_GAUGES,
                NO_RATIOS,
            ),
            ProviderKind::Vllm => poll_metrics(
                base,
                timeout,
                "vllm:",
                VLLM_COUNTERS,
                VLLM_GAUGES,
                VLLM_RATIOS,
            ),
            ProviderKind::LmStudio => poll_lmstudio(base, timeout),
            ProviderKind::MlxLm => poll_mlx(base, timeout),
            ProviderKind::IronWorks => Err(SourceError::NotSupported(
                "IronWorks is read by its own adapter, not this one".into(),
            )),
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
        SourceError::Status(code) => {
            let hint = if kind == ProviderKind::LlamaCpp {
                "; llama-server serves /metrics only when started with --metrics"
            } else {
                ""
            };
            format!("{endpoint} answered HTTP {code}, not {name} data{hint}")
        }
        SourceError::MetricsNotEnabled => {
            format!("{name} answered, but its metrics are not enabled")
        }
        SourceError::NotSupported(why) => format!("{name} cannot be read in this build: {why}"),
        SourceError::NotJson(why) => format!("{endpoint} did not return JSON: {why}"),
        SourceError::Rejected(why) => {
            format!("{endpoint} answered, but not as {name}: {why}")
        }
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
    /// The `model_name` label, when a series carries one.
    pub model: Option<String>,
}

/// Parse the text exposition format, keeping only the names asked for.
///
/// Counters are summed across their label sets, which gives the engine total.
/// Histogram and summary parts (`_bucket`, `_sum`, `_count`) are never asked for,
/// so they cannot be mistaken for a counter. A value that is not a finite
/// non-negative integer for a counter, or not finite for a gauge, is skipped: an
/// unreadable value is absent, not zero.
pub fn parse_prometheus(
    body: &str,
    prefix: &str,
    counters: &[&str],
    gauges: &[&str],
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
    prefix: &str,
    counters: &[&str],
    gauges: &[&str],
    ratios: &'static [RatioDef],
) -> Result<ProviderSample, SourceError> {
    let reply = http::get(&format!("{base}/metrics"), timeout, &[])?;
    if reply.status != 200 {
        return Err(SourceError::Status(reply.status));
    }
    let parsed = parse_prometheus(&reply.body, prefix, counters, gauges);
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
        ratio_defs: ratios,
        ..ProviderSample::default()
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
    Ok(Loaded {
        count: loaded.len(),
        first: loaded.first().and_then(|m| m.key.clone()),
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
mod tests {
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
        let p = parse_prometheus(LLAMA_BODY, "llamacpp:", LLAMACPP_COUNTERS, LLAMACPP_GAUGES);
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
        let p = parse_prometheus(body, "vllm:", VLLM_COUNTERS, VLLM_GAUGES);
        assert_eq!(p.counters["request_success_total"], 14);
        assert_eq!(p.gauges["num_requests_running"], 3.0);
        assert_eq!(p.gauges["kv_cache_usage_perc"], 0.25);
        assert_eq!(p.model.as_deref(), Some("llama-3"));
        // Histogram and _created parts are never read as counters.
        assert!(!p.counters.keys().any(|k| k.contains("time_to_first")));
        assert!(!p.counters.keys().any(|k| k.contains("created")));
    }

    #[test]
    fn a_body_without_the_prefix_is_not_this_provider() {
        let p = parse_prometheus("some_other_metric 1\n", "vllm:", VLLM_COUNTERS, VLLM_GAUGES);
        assert!(!p.recognised);
    }

    #[test]
    fn a_non_integer_counter_is_skipped_not_guessed() {
        let p = parse_prometheus(
            "vllm:generation_tokens_total 1.5\nvllm:prompt_tokens_total NaN\n",
            "vllm:",
            VLLM_COUNTERS,
            VLLM_GAUGES,
        );
        assert!(p.recognised);
        assert!(p.counters.is_empty());
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
