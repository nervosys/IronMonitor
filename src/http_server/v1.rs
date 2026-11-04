//! The `/api/v1` routes that serve the ontology: `describe`, `snapshot`, `get/<id>`,
//! and the sampler-aware `health`.
//!
//! Every body is built by the function the CLI calls, so the two cannot drift:
//! `describe` uses [`Ontology::describe_document`], `snapshot` uses
//! [`resolve::snapshot_document`], and `get` serialises the same [`Reading`].
//! The only additions are `sampled_at` and `age_ms` on each reading object.
//!
//! Rates are not differenced here. A rate needs two samples, and the sampler keeps
//! the CLI's single-sample behaviour, so rate fields come back `unavailable` with
//! the CLI's reason.

use super::sampler::Sampler;
use crate::ontology::resolve::{self, Reading};
use crate::ontology::Domain;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

/// How long a request waits for segments that have no sample yet. Cheap segments
/// answer inside this bound; expensive ones answer 503 and the client retries.
pub(crate) const SAMPLE_BOUND: Duration = Duration::from_millis(250);

/// Seconds a client should wait before retrying a 503.
const RETRY_AFTER_SECS: &str = "1";

/// A response before it is written to the socket.
#[derive(Debug)]
pub(crate) struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Reply {
    /// A JSON body, pretty-printed with a trailing newline. That is byte-for-byte what
    /// the CLI prints for the same document.
    pub(crate) fn json(status: u16, value: &Value) -> Self {
        let mut body = serde_json::to_string_pretty(value)
            .unwrap_or_else(|_| "{\"error\":\"serialisation failed\"}".to_string());
        body.push('\n');
        Self {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub(crate) fn error(status: u16, message: &str) -> Self {
        Self::json(status, &json!({ "error": message }))
    }
}

/// Whether `path` is one of the routes this module answers.
pub(crate) fn handles(path: &str) -> bool {
    matches!(
        path,
        "/api/v1/describe" | "/api/v1/snapshot" | "/api/v1/health"
    ) || path.starts_with("/api/v1/get/")
}

/// Dispatch one of the routes. The caller has already checked the key, so every
/// route answered here is answered in full. The unauthenticated liveness reply is
/// produced by the caller with [`health`] and `detailed` false.
pub(crate) fn handle(
    sampler: &Sampler,
    method: &str,
    path: &str,
    query: &HashMap<String, String>,
) -> Reply {
    if path == "/api/v1/health" {
        if method != "GET" {
            return method_not_allowed();
        }
        return health(sampler, true);
    }
    if method != "GET" {
        return method_not_allowed();
    }
    if path == "/api/v1/describe" {
        return describe();
    }
    if path == "/api/v1/snapshot" {
        return snapshot(sampler, query.get("domain").map(String::as_str));
    }
    match path.strip_prefix("/api/v1/get/") {
        Some(raw) => get(sampler, raw),
        None => Reply::error(404, "unknown route"),
    }
}

/// `GET /api/v1/health`. Adds the sampler's state to the existing `status` and
/// `timestamp`, and omits all of it when `detailed` is false.
pub(crate) fn health(sampler: &Sampler, detailed: bool) -> Reply {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if !detailed {
        return Reply::json(200, &json!({ "status": "healthy", "timestamp": timestamp }));
    }
    let status = sampler.status();
    let mut segments = serde_json::Map::new();
    for segment in status.segments {
        let value = match segment.last_sampled {
            Some((at, age)) => json!({ "last_sampled_at": at, "age_ms": age }),
            None => json!({ "last_sampled_at": null, "age_ms": null }),
        };
        segments.insert(segment.name, value);
    }
    Reply::json(
        200,
        &json!({
            "status": "healthy",
            "timestamp": timestamp,
            "sampler": {
                "state": if status.running { "running" } else { "idle" },
                "segments": segments,
            },
        }),
    )
}

fn method_not_allowed() -> Reply {
    Reply::error(405, "method not allowed; these routes accept GET only")
        .with_header("Allow", "GET")
}

fn describe() -> Reply {
    let ontology = resolve::ontology();
    let doc = ontology.describe_document(ontology.entities.values());
    Reply::json(200, &doc)
}

fn snapshot(sampler: &Sampler, domain: Option<&str>) -> Reply {
    let segments: Vec<String> = match domain {
        None => Domain::ALL.iter().map(|d| d.as_str().to_string()).collect(),
        Some(name) => match Domain::parse(name) {
            Some(parsed) => vec![parsed.as_str().to_string()],
            None => {
                let accepted: Vec<&str> = Domain::ALL.iter().map(|d| d.as_str()).collect();
                return Reply::json(
                    400,
                    &json!({
                        "error": "unknown domain",
                        "domain": name,
                        "accepted": accepted,
                    }),
                );
            }
        },
    };

    let samples = match sampler.sample_for(&segments, SAMPLE_BOUND) {
        Ok(samples) => samples,
        Err(missing) => return sampling(missing),
    };

    // Each reading carries the age of the sample it came from. Segments are sampled
    // at different times, so one envelope-wide age would make some readings look
    // fresher than they were.
    let mut rows: Vec<(Reading, String, u64)> = Vec::new();
    for (_, sample) in &samples {
        let at = sample.sampled_at_rfc3339();
        let age = sample.age_ms();
        for reading in &sample.readings {
            rows.push((reading.clone(), at.clone(), age));
        }
    }
    rows.sort_by(|a, b| a.0.id.cmp(&b.0.id));

    let readings: Vec<Reading> = rows.iter().map(|(r, _, _)| r.clone()).collect();
    let problems = resolve::validate(&readings);
    let mut doc = resolve::snapshot_document(&readings, &problems);
    if let Some(array) = doc.get_mut("readings").and_then(Value::as_array_mut) {
        for (object, (_, at, age)) in array.iter_mut().zip(rows.iter()) {
            add_sample_fields(object, at, *age);
        }
    }
    Reply::json(200, &doc)
}

/// `GET /api/v1/get/<id>`. The id arrives percent-encoded (`disk.%3Cnone%3E`).
///
/// Status codes mirror `ironmon get`: 200 for a value and for an unavailable reading
/// (an answer, not an error), 404 for an id the schema does not declare, a template,
/// or a concrete id the resolver did not produce, and 503 while its segment has no
/// sample yet.
fn get(sampler: &Sampler, raw: &str) -> Reply {
    let Some(id) = percent_decode(raw) else {
        return Reply::error(400, "id is not valid percent-encoded UTF-8");
    };
    let not_found = || Reply::json(404, &json!({ "error": "no such entity id", "id": id }));

    let Some(segment) = resolve::owning_segment(&id) else {
        return not_found();
    };
    if Domain::parse(&segment).is_none() {
        return not_found();
    }

    let samples = match sampler.sample_for(std::slice::from_ref(&segment), SAMPLE_BOUND) {
        Ok(samples) => samples,
        Err(missing) => return sampling(missing),
    };
    let Some((_, sample)) = samples.first() else {
        return sampling(vec![segment]);
    };
    let Some(reading) = sample.readings.iter().find(|r| r.id == id) else {
        return not_found();
    };

    let mut object = match serde_json::to_value(reading) {
        Ok(Value::Object(map)) => Value::Object(map),
        _ => return Reply::error(500, "reading did not serialise"),
    };
    add_sample_fields(&mut object, &sample.sampled_at_rfc3339(), sample.age_ms());
    Reply::json(200, &object)
}

fn sampling(segments: Vec<String>) -> Reply {
    Reply::json(503, &json!({ "state": "sampling", "segments": segments }))
        .with_header("Retry-After", RETRY_AFTER_SECS)
}

fn add_sample_fields(object: &mut Value, sampled_at: &str, age_ms: u64) {
    if let Some(map) = object.as_object_mut() {
        map.insert("sampled_at".into(), Value::String(sampled_at.to_string()));
        map.insert("age_ms".into(), json!(age_ms));
    }
}

/// Decode `%XX` escapes. Returns `None` for a malformed escape or bytes that are not
/// UTF-8. Nothing else is special in a path segment, so `+` is left alone.
pub(crate) fn percent_decode(input: &str) -> Option<String> {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let high = hex(*bytes.get(i + 1)?)?;
            let low = hex(*bytes.get(i + 2)?)?;
            out.push(high * 16 + low);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decoding_recovers_diagnostic_ids() {
        assert_eq!(
            percent_decode("disk.%3Cnone%3E").as_deref(),
            Some("disk.<none>")
        );
        assert_eq!(percent_decode("gpu.0.name").as_deref(), Some("gpu.0.name"));
        assert_eq!(percent_decode("a+b").as_deref(), Some("a+b"));
    }

    #[test]
    fn malformed_escapes_are_refused_not_guessed() {
        assert_eq!(percent_decode("disk.%3"), None);
        assert_eq!(percent_decode("disk.%zz"), None);
        assert_eq!(percent_decode("%ff"), None, "not UTF-8");
    }

    #[test]
    fn unknown_route_under_v1_is_not_claimed() {
        assert!(handles("/api/v1/describe"));
        assert!(handles("/api/v1/get/gpu.0.name"));
        assert!(!handles("/api/v1/metrics/prometheus"));
        assert!(!handles("/api/v1/context"));
    }
}
