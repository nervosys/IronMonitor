//! OTLP/HTTP metric export: readings pushed to an OpenTelemetry Collector.
//!
//! `ironmon serve` pushes to `http://localhost:4318/v1/metrics` by default, the
//! port and path a local Collector listens on. That default is this machine, and
//! any other destination is one the operator names (`--otlp-endpoint` or the
//! standard `OTEL_EXPORTER_OTLP_*` variables), so this is the operator's data
//! going where they sent it — not telemetry, in the sense
//! [`crate::consent`] defines. `--no-otlp`, `OTEL_SDK_DISABLED=true`,
//! `OTEL_METRICS_EXPORTER=none`, `--offline` and `--no-telemetry` each turn it
//! off. With no collector listening it backs off and says so once.
//!
//! # What is sent
//!
//! The same readings `/api/v1/snapshot` serves, encoded as OTLP JSON
//! (`ExportMetricsServiceRequest`). Every data point carries
//! `ironmon.provenance` (`measured`, `derived` or `specification`) and
//! `ironmon.id`, the dotted id `ironmon get` takes. An unavailable reading
//! produces **no data point**: OTLP has no "unknown" value, and a zero would be a
//! claim. Non-numeric readings (names, identifiers) are not metrics and are not
//! sent; the host name becomes the `host.name` resource attribute.
//!
//! # Names
//!
//! Where the OpenTelemetry semantic conventions define a metric whose instrument
//! and unit match a reading exactly, that name is used: `system.cpu.utilization`
//! (per-core idle, `cpu.mode=idle`), `system.cpu.frequency`,
//! `system.memory.usage`, `system.memory.limit`, `system.memory.utilization`,
//! `system.network.io`, `system.uptime`, `hw.gpu.utilization`,
//! `hw.gpu.memory.usage`, `hw.gpu.memory.limit` and `hw.power`. The match is
//! checked against the reading's unit, so a reading in an unexpected unit falls
//! back rather than being mislabelled. Every other reading is
//! `ironmon.<template>` with the instance segments moved into attributes:
//! `gpu.0.thermal.temperature` is `ironmon.gpu.thermal.temperature` with
//! `ironmon.gpu.n="0"`. The GenAI conventions define histograms per request,
//! which engine counters are not, so inference readings keep `ironmon.` names.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

use crate::ontology::resolve::Reading;
use crate::ontology::{Ontology, Provenance, Unit};

/// Where a local Collector listens for OTLP/HTTP.
pub const DEFAULT_ENDPOINT: &str = "http://localhost:4318";
/// The OTLP/HTTP metrics path, appended to a base endpoint.
pub const METRICS_PATH: &str = "/v1/metrics";
/// How often to push when neither the flag nor `OTEL_METRIC_EXPORT_INTERVAL` says.
/// The SDK default is 60 s; 15 s matches a typical Prometheus scrape, so the two
/// paths give the same resolution.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(15);
/// The longest wait between attempts while no collector answers.
pub const MAX_BACKOFF: Duration = Duration::from_secs(300);

/// A resolved export configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Full URL the request is posted to, path included. May hold credentials;
    /// show it only through [`Settings::display_url`].
    pub url: String,
    /// Extra request headers, typically auth. Never printed.
    pub headers: Vec<(String, String)>,
    pub interval: Duration,
}

impl Settings {
    /// The URL as it may be shown: credentials and query string redacted.
    pub fn display_url(&self) -> String {
        crate::inference::redact_urls(&self.url)
    }
}

/// Whether to export, and if not, why not.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Export(Settings),
    Off(String),
}

/// What the command line said about OTLP.
#[derive(Debug, Clone, Default)]
pub struct Flags<'a> {
    pub endpoint: Option<&'a str>,
    pub headers_file: Option<&'a str>,
    pub interval_secs: Option<u64>,
    pub disabled: bool,
}

/// Decide from the flags and the environment (`env` reads one variable).
///
/// Precedence follows the OTLP exporter specification: `--otlp-endpoint`, then
/// `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` (used as given), then
/// `OTEL_EXPORTER_OTLP_ENDPOINT` (a base, `/v1/metrics` appended), then
/// [`DEFAULT_ENDPOINT`]. A flag endpoint with no path is treated as a base.
/// Configuration that cannot be honoured is an error, not a silent default: a
/// headers file that cannot be read would otherwise send unauthenticated data.
pub fn decide(flags: &Flags, env: &dyn Fn(&str) -> Option<String>) -> Result<Decision, String> {
    let set = |name: &str| env(name).filter(|v| !v.trim().is_empty());

    if flags.disabled {
        return Ok(Decision::Off("--no-otlp".into()));
    }
    if set("IRONMON_NO_TELEMETRY").is_some() || set("IRONMON_OFFLINE").is_some() {
        return Ok(Decision::Off("--offline or --no-telemetry".into()));
    }
    if set("OTEL_SDK_DISABLED").is_some_and(|v| v.trim().eq_ignore_ascii_case("true")) {
        return Ok(Decision::Off("OTEL_SDK_DISABLED=true".into()));
    }
    if set("OTEL_METRICS_EXPORTER").is_some_and(|v| v.trim().eq_ignore_ascii_case("none")) {
        return Ok(Decision::Off("OTEL_METRICS_EXPORTER=none".into()));
    }
    let protocol =
        set("OTEL_EXPORTER_OTLP_METRICS_PROTOCOL").or_else(|| set("OTEL_EXPORTER_OTLP_PROTOCOL"));
    if protocol.as_deref().is_some_and(|p| p.trim() == "grpc") && flags.endpoint.is_none() {
        return Err(
            "OTEL_EXPORTER_OTLP_PROTOCOL=grpc: ironmon exports OTLP over HTTP only; \
             point --otlp-endpoint at the collector's HTTP port (4318) or pass --no-otlp"
                .into(),
        );
    }

    let url = if let Some(flag) = flags.endpoint {
        base_or_full(flag)?
    } else if let Some(full) = set("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT") {
        let full = full.trim().to_string();
        check_scheme(&full)?;
        full
    } else if let Some(base) = set("OTEL_EXPORTER_OTLP_ENDPOINT") {
        let base = base.trim();
        check_scheme(base)?;
        format!("{}{METRICS_PATH}", base.trim_end_matches('/'))
    } else {
        format!("{DEFAULT_ENDPOINT}{METRICS_PATH}")
    };

    let from_env =
        set("OTEL_EXPORTER_OTLP_METRICS_HEADERS").or_else(|| set("OTEL_EXPORTER_OTLP_HEADERS"));
    let headers = match (flags.headers_file, from_env) {
        (Some(_), Some(_)) => {
            return Err("set --otlp-headers-file or OTEL_EXPORTER_OTLP_HEADERS, not both".into())
        }
        (Some(path), None) => {
            let raw = std::fs::read_to_string(path)
                .map_err(|e| format!("cannot read OTLP headers file {path}: {e}"))?;
            let headers = parse_headers(&raw.replace('\n', ","))?;
            if headers.is_empty() {
                return Err(format!(
                    "OTLP headers file {path} holds no headers; refusing to export without them"
                ));
            }
            headers
        }
        (None, Some(raw)) => parse_headers(&raw)?,
        (None, None) => Vec::new(),
    };

    let interval = match flags.interval_secs {
        Some(0) => return Err("--otlp-interval must be at least 1 second".into()),
        Some(secs) => Duration::from_secs(secs),
        None => match set("OTEL_METRIC_EXPORT_INTERVAL") {
            Some(ms) => {
                let ms: u64 = ms.trim().parse().map_err(|_| {
                    format!("OTEL_METRIC_EXPORT_INTERVAL is not milliseconds: {ms}")
                })?;
                Duration::from_millis(ms.max(1000))
            }
            None => DEFAULT_INTERVAL,
        },
    };

    Ok(Decision::Export(Settings {
        url,
        headers,
        interval,
    }))
}

fn check_scheme(url: &str) -> Result<(), String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(())
    } else {
        Err(format!(
            "OTLP endpoint {} must start with http:// or https://",
            crate::inference::redact_urls(url)
        ))
    }
}

/// A flag endpoint: a bare `scheme://host:port` is a base and gets
/// `/v1/metrics`; one with a path is taken as the full URL.
fn base_or_full(endpoint: &str) -> Result<String, String> {
    let endpoint = endpoint.trim();
    check_scheme(endpoint)?;
    let after_scheme = &endpoint[endpoint.find("://").map_or(0, |i| i + 3)..];
    let has_path = after_scheme
        .find('/')
        .is_some_and(|i| !after_scheme[i..].trim_end_matches('/').is_empty());
    if has_path {
        Ok(endpoint.to_string())
    } else {
        Ok(format!("{}{METRICS_PATH}", endpoint.trim_end_matches('/')))
    }
}

/// `key=value` pairs separated by commas, as `OTEL_EXPORTER_OTLP_HEADERS` has
/// them; values may be percent-encoded. Blank entries and `#` comment lines are
/// skipped, so a headers file can hold one pair per line.
pub fn parse_headers(raw: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() || entry.starts_with('#') {
            continue;
        }
        let Some((key, value)) = entry.split_once('=') else {
            // The entry itself may be a secret, so it is not echoed.
            return Err("an OTLP header entry has no '=' (expected key=value)".into());
        };
        let key = key.trim();
        if key.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(format!(
                "OTLP header name {key:?} is not a valid header name"
            ));
        }
        out.push((key.to_string(), percent_decode(value.trim())));
    }
    Ok(out)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A reading to encode, with the time its pass started.
pub struct Timed<'a> {
    pub reading: &'a Reading,
    pub time_unix_nano: u64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Gauge,
    /// Cumulative sum; `true` when monotonic (a counter).
    Sum(bool),
}

struct Metric {
    unit: &'static str,
    description: String,
    kind: Kind,
    points: Vec<Value>,
}

/// How one reading becomes a point: metric name, unit, kind, attributes and a
/// scale applied to the value.
struct Mapping {
    name: String,
    unit: &'static str,
    kind: Kind,
    attributes: Vec<(String, Value)>,
    scale: f64,
}

/// The OTLP JSON body (`ExportMetricsServiceRequest`) for `readings`.
///
/// `start_unix_nano` is when this exporter started: the start of every
/// cumulative sum it reports, since the counters' own origins are not known.
pub fn encode(readings: &[Timed], start_unix_nano: u64) -> Value {
    encode_with(readings, start_unix_nano, &[])
}

/// [`encode`], with extra resource attributes (such as `service.instance.id`).
pub fn encode_with(
    readings: &[Timed],
    start_unix_nano: u64,
    resource_extra: &[(&str, String)],
) -> Value {
    let ontology = crate::ontology::resolve::ontology();
    let mut metrics: BTreeMap<String, Metric> = BTreeMap::new();
    let mut host_name: Option<String> = None;

    for timed in readings {
        let reading = timed.reading;
        if reading.id == "system.hostname" {
            host_name = reading
                .value
                .as_ref()
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if reading.provenance == Provenance::Unavailable {
            continue;
        }
        let Some(value) = reading.value.as_ref() else {
            continue;
        };
        if !value.is_number() {
            continue;
        }
        let Some(mapping) = map(ontology, reading) else {
            continue;
        };

        let mut attributes = mapping.attributes;
        attributes.push((
            "ironmon.provenance".into(),
            json!(reading.provenance.as_str()),
        ));
        attributes.push(("ironmon.id".into(), json!(reading.id)));

        let mut point = serde_json::Map::new();
        point.insert(
            "attributes".into(),
            Value::Array(attributes.into_iter().map(attr).collect()),
        );
        point.insert(
            "timeUnixNano".into(),
            json!(timed.time_unix_nano.to_string()),
        );
        if matches!(mapping.kind, Kind::Sum(_)) {
            point.insert(
                "startTimeUnixNano".into(),
                json!(start_unix_nano.to_string()),
            );
        }
        match (value.as_i64(), mapping.scale == 1.0) {
            // OTLP JSON carries 64-bit integers as decimal strings.
            (Some(int), true) => {
                point.insert("asInt".into(), json!(int.to_string()));
            }
            _ => {
                let Some(f) = value.as_f64() else { continue };
                point.insert("asDouble".into(), json!(f * mapping.scale));
            }
        }

        let description = ontology
            .template_for(&reading.id)
            .map(|e| e.description.clone())
            .unwrap_or_default();
        let entry = metrics.entry(mapping.name).or_insert_with(|| Metric {
            unit: mapping.unit,
            description,
            kind: mapping.kind,
            points: Vec::new(),
        });
        entry.points.push(Value::Object(point));
    }

    let metrics: Vec<Value> = metrics
        .into_iter()
        .map(|(name, metric)| {
            let mut m = json!({
                "name": name,
                "unit": metric.unit,
                "description": metric.description,
            });
            match metric.kind {
                Kind::Gauge => m["gauge"] = json!({ "dataPoints": metric.points }),
                Kind::Sum(monotonic) => {
                    m["sum"] = json!({
                        "dataPoints": metric.points,
                        // AGGREGATION_TEMPORALITY_CUMULATIVE
                        "aggregationTemporality": 2,
                        "isMonotonic": monotonic,
                    })
                }
            }
            m
        })
        .collect();

    let mut resource = vec![
        attr(("service.name".into(), json!("ironmon"))),
        attr(("service.version".into(), json!(env!("CARGO_PKG_VERSION")))),
    ];
    if let Some(host) = host_name {
        resource.push(attr(("host.name".into(), json!(host))));
    }
    for (key, value) in resource_extra {
        // Device-profile counts and sizes go as intValue, so every client
        // stores the same JSON type for the same field.
        let value = match value.parse::<i64>() {
            Ok(n) if crate::device_profile::is_integer_field(key) => json!(n),
            _ => json!(value),
        };
        resource.push(attr(((*key).to_string(), value)));
    }

    json!({
        "resourceMetrics": [{
            "resource": { "attributes": resource },
            "scopeMetrics": [{
                "scope": { "name": "ironmon", "version": env!("CARGO_PKG_VERSION") },
                "metrics": metrics,
            }],
        }],
    })
}

fn attr((key, value): (String, Value)) -> Value {
    let value = match value {
        Value::String(s) => json!({ "stringValue": s }),
        Value::Bool(b) => json!({ "boolValue": b }),
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({ "intValue": n.to_string() }),
        Value::Number(n) => json!({ "doubleValue": n.as_f64() }),
        other => json!({ "stringValue": other.to_string() }),
    };
    json!({ "key": key, "value": value })
}

/// UCUM, as OTLP units are written.
fn ucum(unit: Option<Unit>) -> &'static str {
    match unit {
        Some(Unit::Celsius) => "Cel",
        Some(Unit::Percent) => "%",
        Some(Unit::Bytes) => "By",
        Some(Unit::BytesPerSecond) => "By/s",
        Some(Unit::Hertz) => "Hz",
        Some(Unit::Megahertz) => "MHz",
        Some(Unit::MegatransfersPerSecond) => "MT/s",
        Some(Unit::Watts) => "W",
        Some(Unit::Milliwatts) => "mW",
        Some(Unit::Millijoules) => "mJ",
        Some(Unit::Volts) => "V",
        Some(Unit::Rpm) => "{rotation}/min",
        Some(Unit::Seconds) => "s",
        Some(Unit::Hours) => "h",
        Some(Unit::Milliseconds) => "ms",
        Some(Unit::Count) => "{count}",
        Some(Unit::Dbm) => "dBm",
        Some(Unit::MegabitsPerSecond) => "Mbit/s",
        Some(Unit::TokensPerSecond) => "{token}/s",
        Some(Unit::Amount) => "{amount}",
        Some(Unit::Identifier) | Some(Unit::Text) | None => "1",
    }
}

/// The template's placeholder values in `id`, keyed `ironmon.<segment>.<name>`
/// after the segment that precedes each placeholder.
fn instance_attributes(template: &str, id: &str) -> Vec<(String, Value)> {
    let t: Vec<&str> = template.split('.').collect();
    let c: Vec<&str> = id.split('.').collect();
    if t.len() != c.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, (tp, cp)) in t.iter().zip(&c).enumerate() {
        if let Some(name) = tp.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            let before = if i > 0 { t[i - 1] } else { "instance" };
            out.push((format!("ironmon.{before}.{name}"), json!(cp)));
        }
    }
    out
}

fn map(ontology: &Ontology, reading: &Reading) -> Option<Mapping> {
    // Diagnostic rows (`disk.<none>`) are about the reading process, not the machine.
    if reading.id.contains('<') {
        return None;
    }
    let template = ontology
        .template_for(&reading.id)
        .map(|e| e.id.as_str())
        .unwrap_or(reading.id.as_str());
    let parts: Vec<&str> = reading.id.split('.').collect();
    let unit = reading.unit;
    let semconv = |name: &str,
                   unit: &'static str,
                   kind: Kind,
                   attributes: Vec<(String, Value)>,
                   scale: f64| {
        Some(Mapping {
            name: name.to_string(),
            unit,
            kind,
            attributes,
            scale,
        })
    };
    let int = |s: &str| {
        s.parse::<i64>()
            .map(|n| json!(n))
            .unwrap_or_else(|_| json!(s))
    };
    let gpu_id = |n: &str| ("hw.id".to_string(), json!(format!("gpu.{n}")));
    let gpu_type = || ("hw.type".to_string(), json!("gpu"));
    const UPDOWN: Kind = Kind::Sum(false);

    let known = match (template, unit) {
        ("cpu.core.{n}.idle", Some(Unit::Percent)) => semconv(
            "system.cpu.utilization",
            "1",
            Kind::Gauge,
            vec![
                ("cpu.logical_number".into(), int(parts[2])),
                ("cpu.mode".into(), json!("idle")),
            ],
            0.01,
        ),
        ("cpu.core.{n}.frequency", Some(Unit::Megahertz)) => semconv(
            "system.cpu.frequency",
            "Hz",
            Kind::Gauge,
            vec![("cpu.logical_number".into(), int(parts[2]))],
            1e6,
        ),
        ("memory.used", Some(Unit::Bytes)) => semconv(
            "system.memory.usage",
            "By",
            UPDOWN,
            vec![("system.memory.state".into(), json!("used"))],
            1.0,
        ),
        ("memory.total", Some(Unit::Bytes)) => {
            semconv("system.memory.limit", "By", UPDOWN, vec![], 1.0)
        }
        ("memory.utilization", Some(Unit::Percent)) => semconv(
            "system.memory.utilization",
            "1",
            Kind::Gauge,
            vec![("system.memory.state".into(), json!("used"))],
            0.01,
        ),
        ("network.{iface}.rx_bytes" | "network.{iface}.tx_bytes", Some(Unit::Bytes)) => semconv(
            "system.network.io",
            "By",
            Kind::Sum(true),
            vec![
                ("network.interface.name".into(), json!(parts[1])),
                (
                    "network.io.direction".into(),
                    json!(if parts[2] == "rx_bytes" {
                        "receive"
                    } else {
                        "transmit"
                    }),
                ),
            ],
            1.0,
        ),
        ("system.uptime", Some(Unit::Seconds)) => {
            semconv("system.uptime", "s", Kind::Gauge, vec![], 1.0)
        }
        ("gpu.{n}.utilization", Some(Unit::Percent)) => semconv(
            "hw.gpu.utilization",
            "1",
            Kind::Gauge,
            vec![gpu_id(parts[1]), gpu_type()],
            0.01,
        ),
        ("gpu.{n}.memory.used", Some(Unit::Bytes)) => semconv(
            "hw.gpu.memory.usage",
            "By",
            UPDOWN,
            vec![gpu_id(parts[1]), gpu_type()],
            1.0,
        ),
        ("gpu.{n}.memory.total", Some(Unit::Bytes)) => semconv(
            "hw.gpu.memory.limit",
            "By",
            UPDOWN,
            vec![gpu_id(parts[1]), gpu_type()],
            1.0,
        ),
        ("gpu.{n}.power.draw", Some(Unit::Watts)) => semconv(
            "hw.power",
            "W",
            Kind::Gauge,
            vec![gpu_id(parts[1]), gpu_type()],
            1.0,
        ),
        ("gpu.{n}.power.draw", Some(Unit::Milliwatts)) => semconv(
            "hw.power",
            "W",
            Kind::Gauge,
            vec![gpu_id(parts[1]), gpu_type()],
            1e-3,
        ),
        _ => None,
    };
    if known.is_some() {
        return known;
    }

    let name: Vec<&str> = template
        .split('.')
        .filter(|p| !p.starts_with('{'))
        .collect();
    let kind = if template.ends_with("_total") {
        Kind::Sum(true)
    } else {
        Kind::Gauge
    };
    Some(Mapping {
        name: format!("ironmon.{}", name.join(".")),
        unit: ucum(unit),
        kind,
        attributes: instance_attributes(template, &reading.id),
        scale: 1.0,
    })
}

/// Why a push failed.
#[derive(Debug, Clone, PartialEq)]
pub enum SendError {
    /// Nothing answered: no collector is listening, or it is unreachable.
    Unreachable(String),
    /// The collector answered with an error status.
    Status(u16),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(why) => write!(f, "{}", crate::inference::redact_urls(why)),
            Self::Status(code) => write!(f, "the collector answered HTTP {code}"),
        }
    }
}

/// What a successful push reported back.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Accepted {
    /// Points the collector says it rejected (`partialSuccess.rejectedDataPoints`).
    pub rejected: u64,
    pub message: Option<String>,
}

/// POST one export request.
#[cfg(feature = "remote-backends")]
pub fn send(settings: &Settings, body: &Value) -> Result<Accepted, SendError> {
    use std::io::Read;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        // A redirect would carry the auth headers to a host nobody named.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| SendError::Unreachable("could not start an HTTP client".into()))?;
    let mut request = client
        .post(&settings.url)
        .header("Content-Type", "application/json")
        .body(body.to_string());
    for (name, value) in &settings.headers {
        request = request.header(name.as_str(), value.as_str());
    }
    let response = request.send().map_err(|e| {
        // reqwest names the URL in its message; Display redacts it.
        let why = if e.is_timeout() {
            "timed out after 10 s".to_string()
        } else if e.is_connect() {
            "nothing is listening there".to_string()
        } else {
            e.to_string()
        };
        SendError::Unreachable(why)
    })?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(SendError::Status(status));
    }
    let mut bytes = Vec::new();
    let _ = response.take(64 * 1024).read_to_end(&mut bytes);
    let reply: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let partial = &reply["partialSuccess"];
    Ok(Accepted {
        rejected: partial["rejectedDataPoints"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .or_else(|| partial["rejectedDataPoints"].as_u64())
            .unwrap_or(0),
        message: partial["errorMessage"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    })
}

/// How long to wait before the next attempt after `failures` consecutive
/// failures: the interval, doubling per failure, capped at [`MAX_BACKOFF`].
pub fn backoff(interval: Duration, failures: u32) -> Duration {
    if failures == 0 {
        return interval;
    }
    interval
        .saturating_mul(1u32 << failures.min(8))
        .min(MAX_BACKOFF.max(interval))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: std::collections::HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    fn settings(d: Decision) -> Settings {
        match d {
            Decision::Export(s) => s,
            Decision::Off(why) => panic!("expected export, got off: {why}"),
        }
    }

    #[test]
    fn on_by_default_to_the_local_collector() {
        let s = settings(decide(&Flags::default(), &env_of(&[])).unwrap());
        assert_eq!(s.url, "http://localhost:4318/v1/metrics");
        assert!(s.headers.is_empty());
        assert_eq!(s.interval, DEFAULT_INTERVAL);
    }

    #[test]
    fn every_off_switch_turns_it_off() {
        let off = |flags: Flags, env: &[(&str, &str)]| {
            matches!(decide(&flags, &env_of(env)).unwrap(), Decision::Off(_))
        };
        assert!(off(
            Flags {
                disabled: true,
                ..Default::default()
            },
            &[]
        ));
        assert!(off(Flags::default(), &[("IRONMON_NO_TELEMETRY", "1")]));
        assert!(off(Flags::default(), &[("IRONMON_OFFLINE", "1")]));
        assert!(off(Flags::default(), &[("OTEL_SDK_DISABLED", "TRUE")]));
        assert!(off(Flags::default(), &[("OTEL_METRICS_EXPORTER", "none")]));
        // Only "true" disables the SDK, per the specification.
        assert!(!off(Flags::default(), &[("OTEL_SDK_DISABLED", "1")]));
    }

    #[test]
    fn endpoints_follow_the_exporter_specification() {
        let url = |flags: Flags, env: &[(&str, &str)]| {
            settings(decide(&flags, &env_of(env)).unwrap()).url
        };
        assert_eq!(
            url(
                Flags::default(),
                &[("OTEL_EXPORTER_OTLP_ENDPOINT", "http://c:4318/base/")]
            ),
            "http://c:4318/base/v1/metrics"
        );
        assert_eq!(
            url(
                Flags::default(),
                &[
                    ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://ignored:4318"),
                    ("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT", "https://c/custom"),
                ]
            ),
            "https://c/custom"
        );
        let flag = Flags {
            endpoint: Some("http://box:4318"),
            ..Default::default()
        };
        assert_eq!(
            url(flag, &[("OTEL_EXPORTER_OTLP_ENDPOINT", "http://x")]),
            "http://box:4318/v1/metrics"
        );
        let flag = Flags {
            endpoint: Some("http://box:4318/otlp/v1/metrics"),
            ..Default::default()
        };
        assert_eq!(url(flag, &[]), "http://box:4318/otlp/v1/metrics");
        let bad = Flags {
            endpoint: Some("box:4318"),
            ..Default::default()
        };
        assert!(decide(&bad, &env_of(&[])).is_err());
    }

    #[test]
    fn headers_come_from_one_source_and_are_never_echoed() {
        let s = settings(
            decide(
                &Flags::default(),
                &env_of(&[(
                    "OTEL_EXPORTER_OTLP_HEADERS",
                    "Authorization=Bearer%20s3cret, x-tenant=a",
                )]),
            )
            .unwrap(),
        );
        assert_eq!(
            s.headers,
            vec![
                ("Authorization".to_string(), "Bearer s3cret".to_string()),
                ("x-tenant".to_string(), "a".to_string()),
            ]
        );
        let err = parse_headers("Authorization Bearer s3cret").unwrap_err();
        assert!(!err.contains("s3cret"), "{err}");

        let dir = std::env::temp_dir().join(format!("ironmon-otlp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("headers");
        std::fs::write(&file, "# auth\nAuthorization=Bearer k\n").unwrap();
        let path = file.to_string_lossy().to_string();
        let flags = Flags {
            headers_file: Some(&path),
            ..Default::default()
        };
        let s = settings(decide(&flags, &env_of(&[])).unwrap());
        assert_eq!(s.headers, vec![("Authorization".into(), "Bearer k".into())]);
        assert!(decide(&flags, &env_of(&[("OTEL_EXPORTER_OTLP_HEADERS", "a=b")])).is_err());
        std::fs::write(&file, "# nothing\n").unwrap();
        assert!(
            decide(&flags, &env_of(&[])).is_err(),
            "an empty headers file must not export"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_display_url_hides_credentials() {
        let s = Settings {
            url: "https://user:pw@c:4318/v1/metrics?token=t".into(),
            headers: vec![],
            interval: DEFAULT_INTERVAL,
        };
        let shown = s.display_url();
        assert!(
            !shown.contains("pw") && !shown.contains("token=t"),
            "{shown}"
        );
    }

    #[test]
    fn backoff_doubles_and_caps() {
        let i = Duration::from_secs(15);
        assert_eq!(backoff(i, 0), i);
        assert_eq!(backoff(i, 1), Duration::from_secs(30));
        assert_eq!(backoff(i, 2), Duration::from_secs(60));
        assert_eq!(backoff(i, 30), MAX_BACKOFF);
    }

    fn reading(
        id: &str,
        value: Option<Value>,
        provenance: Provenance,
        unit: Option<Unit>,
    ) -> Reading {
        Reading {
            id: id.into(),
            value,
            provenance,
            unit,
            note: None,
        }
    }

    fn metric<'a>(body: &'a Value, name: &str) -> Option<&'a Value> {
        body["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
            .as_array()?
            .iter()
            .find(|m| m["name"] == name)
    }

    fn attrs(point: &Value) -> BTreeMap<String, Value> {
        point["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                let v = &a["value"];
                let v = v.get("stringValue").or(v.get("intValue")).cloned().unwrap();
                (a["key"].as_str().unwrap().to_string(), v)
            })
            .collect()
    }

    #[test]
    fn unavailable_and_textual_readings_produce_no_points() {
        let rows = [
            reading(
                "gpu.0.utilization",
                None,
                Provenance::Unavailable,
                Some(Unit::Percent),
            ),
            reading(
                "gpu.0.name",
                Some(json!("RTX")),
                Provenance::Measured,
                Some(Unit::Text),
            ),
            reading("disk.<none>", None, Provenance::Unavailable, None),
            reading(
                "system.hostname",
                Some(json!("box")),
                Provenance::Measured,
                Some(Unit::Text),
            ),
        ];
        let timed: Vec<Timed> = rows
            .iter()
            .map(|r| Timed {
                reading: r,
                time_unix_nano: 1,
            })
            .collect();
        let body = encode(&timed, 0);
        assert_eq!(
            body["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
                .as_array()
                .unwrap()
                .len(),
            0,
            "{body:#}"
        );
        let resource = &body["resourceMetrics"][0]["resource"]["attributes"];
        assert!(resource.to_string().contains("\"host.name\""), "{resource}");
    }

    #[test]
    fn every_point_carries_provenance_and_semantic_names_are_used() {
        let rows = [
            reading(
                "gpu.0.utilization",
                Some(json!(42)),
                Provenance::Measured,
                Some(Unit::Percent),
            ),
            reading(
                "gpu.1.utilization",
                Some(json!(7.5)),
                Provenance::Measured,
                Some(Unit::Percent),
            ),
            reading(
                "network.eth0.rx_bytes",
                Some(json!(1000)),
                Provenance::Measured,
                Some(Unit::Bytes),
            ),
            reading(
                "network.eth0.tx_bytes",
                Some(json!(10)),
                Provenance::Measured,
                Some(Unit::Bytes),
            ),
            reading(
                "gpu.0.thermal.temperature",
                Some(json!(61)),
                Provenance::Measured,
                Some(Unit::Celsius),
            ),
            reading(
                "cpu.core.3.frequency",
                Some(json!(3600)),
                Provenance::Measured,
                Some(Unit::Megahertz),
            ),
        ];
        let timed: Vec<Timed> = rows
            .iter()
            .map(|r| Timed {
                reading: r,
                time_unix_nano: 5,
            })
            .collect();
        let body = encode(&timed, 1);

        let gpu = metric(&body, "hw.gpu.utilization").expect("hw.gpu.utilization");
        assert_eq!(gpu["unit"], "1");
        let points = gpu["gauge"]["dataPoints"].as_array().unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0]["asDouble"], json!(0.42));
        assert_eq!(attrs(&points[0])["hw.id"], json!("gpu.0"));

        let net = metric(&body, "system.network.io").expect("system.network.io");
        assert_eq!(net["sum"]["isMonotonic"], json!(true));
        assert_eq!(net["sum"]["aggregationTemporality"], json!(2));
        let p = &net["sum"]["dataPoints"][0];
        assert_eq!(p["asInt"], json!("1000"));
        assert_eq!(p["startTimeUnixNano"], json!("1"));
        assert_eq!(p["timeUnixNano"], json!("5"));

        let temp = metric(&body, "ironmon.gpu.thermal.temperature").expect("generic name");
        assert_eq!(temp["unit"], "Cel");
        assert_eq!(
            attrs(&temp["gauge"]["dataPoints"][0])["ironmon.gpu.n"],
            json!("0")
        );

        let freq = metric(&body, "system.cpu.frequency").expect("system.cpu.frequency");
        assert_eq!(freq["gauge"]["dataPoints"][0]["asDouble"], json!(3.6e9));
        assert_eq!(
            attrs(&freq["gauge"]["dataPoints"][0])["cpu.logical_number"],
            json!("3")
        );

        for m in body["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
            .as_array()
            .unwrap()
        {
            let points = m["gauge"]["dataPoints"]
                .as_array()
                .or(m["sum"]["dataPoints"].as_array())
                .unwrap();
            for p in points {
                let a = attrs(p);
                assert_eq!(a["ironmon.provenance"], json!("measured"), "{m}");
                assert!(a.contains_key("ironmon.id"), "{m}");
            }
        }
    }
}
