//! One bounded HTTP GET for the inference adapters.
//!
//! The request runs on its own thread with its own client. A blocking client
//! creates and drops an internal runtime, which panics inside an async context,
//! and snapshots can be taken from one. Every request is bounded by its timeout,
//! and the body is read to a fixed cap, so a server cannot make a poll unbounded.

use super::SourceError;
use std::time::Duration;

/// The most body a reply may carry. Remote usage pages are bounded by their own
/// `limit` parameter; this cap only stops a runaway server.
pub const MAX_BODY_BYTES: u64 = 4 * 1024 * 1024;

/// A reply that came back: its status and its body as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub body: String,
}

/// GET `url` with `headers`, giving up after `timeout`.
///
/// Errors name the cause but never echo the request headers, so a secret sent in
/// a header cannot reach an error message through this function.
#[cfg(feature = "remote-backends")]
pub fn get(url: &str, timeout: Duration, headers: &[(&str, &str)]) -> Result<Reply, SourceError> {
    use std::io::Read;

    let url = url.to_string();
    let headers: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let client = reqwest::blocking::Client::builder()
                    .timeout(timeout)
                    .build()
                    .map_err(|_| {
                        SourceError::Unreachable("could not start an HTTP client".into())
                    })?;
                let mut request = client.get(&url).timeout(timeout);
                for (name, value) in &headers {
                    request = request.header(name.as_str(), value.as_str());
                }
                let response = request.send().map_err(|e| classify(&e, timeout))?;
                let status = response.status().as_u16();
                let mut bytes = Vec::new();
                response
                    .take(MAX_BODY_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| {
                        SourceError::Unreachable(format!("the reply was cut short ({e})"))
                    })?;
                if bytes.len() as u64 > MAX_BODY_BYTES {
                    return Err(SourceError::Rejected(format!(
                        "the reply is larger than {MAX_BODY_BYTES} bytes"
                    )));
                }
                Ok(Reply {
                    status,
                    body: String::from_utf8_lossy(&bytes).into_owned(),
                })
            })
            .join()
            .unwrap_or_else(|_| Err(SourceError::Unreachable("the request panicked".into())))
    })
}

/// Without `remote-backends` there is no HTTP client to make the request with.
#[cfg(not(feature = "remote-backends"))]
pub fn get(url: &str, timeout: Duration, headers: &[(&str, &str)]) -> Result<Reply, SourceError> {
    let _ = (url, timeout, headers);
    Err(SourceError::NotSupported(
        "this build has no HTTP client, because the remote-backends feature is off".into(),
    ))
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
