//! Remote inference providers: OpenAI and Anthropic.
//!
//! Each provider is read in two parts, and nothing is requested unless the user
//! has configured the provider: an API key, or an explicit endpoint, in its
//! environment variable.
//!
//! - Reachability: one authenticated model listing, polled about once a minute.
//!   It answers "does this key work", with the status when it does not.
//! - Cost: the current UTC calendar month to date, summed from the rows the
//!   provider's admin usage API reports. It needs an admin key in its own
//!   variable, and is fetched at most once every five minutes. A failed fetch
//!   leaves the figure unavailable with the reason; a partial sum is never given.
//!
//! Keys are held in [`Secret`], whose `Debug` output is redacted, and no error or
//! reason in this module ever includes a key or a reply body.
//!
//! Endpoints and fields, from the providers' documentation:
//! - OpenAI, `GET /v1/models`, `Authorization: Bearer`, fields `data[].id`:
//!   https://developers.openai.com/api/reference/resources/models/methods/list
//! - OpenAI, `GET /v1/organization/costs` (admin key, Bearer), `start_time` and
//!   `end_time` in Unix seconds, `bucket_width=1d`, `limit`, `page` cursor,
//!   buckets in `data[]` with `results[].amount.{value,currency}`, currency
//!   lowercase ISO-4217, `has_more` and `next_page`:
//!   https://github.com/openai/openai-openapi (openapi.yaml, `/organization/costs`)
//! - Anthropic, `GET /v1/models`, `x-api-key` and `anthropic-version`, `data[].id`:
//!   https://platform.claude.com/docs/en/api/models/list
//! - Anthropic, `GET /v1/organizations/cost_report` (admin key in `x-api-key`,
//!   `anthropic-version`), `starting_at`, `bucket_width=1d`, `limit`, `page`,
//!   buckets with `results[].amount` as a decimal string in lowest units (cents),
//!   `results[].currency`, `has_more` and `next_page`. The endpoint is marked
//!   legacy and the docs name `/v1/organizations/analytics/cost` as its
//!   replacement; this build uses the legacy endpoint, which remains supported.
//!   Priority Tier costs are not included in it:
//!   https://platform.claude.com/docs/en/api/beta/organization/cost_report/retrieve
//!
//! Cost is never estimated from token counts or a price table.

use super::http;
use super::ironworks::locality_of;
use super::{Identity, InferenceSource, Locality, ProviderKind, ProviderSample, SourceError};
use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, TimeZone, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

pub const OPENAI_BASE: &str = "https://api.openai.com";
pub const ANTHROPIC_BASE: &str = "https://api.anthropic.com";

pub const OPENAI_KEY_ENV: &str = "OPENAI_API_KEY";
pub const OPENAI_ADMIN_ENV: &str = "IRONMON_OPENAI_ADMIN_KEY";
pub const OPENAI_URL_ENV: &str = "IRONMON_OPENAI_URL";
pub const ANTHROPIC_KEY_ENV: &str = "ANTHROPIC_API_KEY";
pub const ANTHROPIC_ADMIN_ENV: &str = "IRONMON_ANTHROPIC_ADMIN_KEY";
pub const ANTHROPIC_URL_ENV: &str = "IRONMON_ANTHROPIC_URL";

/// The `anthropic-version` the cost report example in the docs sends.
const ANTHROPIC_VERSION: &str = "2026-10-21";

/// Reachability is asked about once a minute at most.
pub const REACHABILITY_INTERVAL: Duration = Duration::from_secs(60);

/// Cost is fetched at most once every five minutes.
pub const COST_INTERVAL: Duration = Duration::from_secs(300);

/// Deadline for each remote request. Remote calls are slower than loopback ones.
pub const REMOTE_TIMEOUT: Duration = Duration::from_secs(5);

/// Pages read before a cost figure is given up as unavailable.
const MAX_PAGES: usize = 20;

/// A credential. Its `Debug` output never shows the value, so a struct that holds
/// one can be printed without leaking it.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: &str) -> Self {
        Self(value.to_string())
    }

    /// The value, for the one place that puts it on the wire.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// A month-to-date cost, or the reason there is none. Amounts keep the scale the
/// provider reports them in: OpenAI in currency units, Anthropic in cents.
#[derive(Debug, Clone, PartialEq)]
pub enum CostReading {
    Known {
        amount: f64,
        currency: String,
        /// `units` (currency units) or `cents` (lowest units), as reported.
        scale: &'static str,
        period_start: String,
        period_end: String,
        fetched_at: Instant,
    },
    Unavailable {
        reason: String,
        fetched_at: Instant,
    },
}

impl CostReading {
    /// When this reading was taken, so the tabs can show its age.
    pub fn fetched_at(&self) -> Instant {
        match self {
            Self::Known { fetched_at, .. } | Self::Unavailable { fetched_at, .. } => *fetched_at,
        }
    }
}

/// One remote provider, configured from its environment variables.
pub struct RemoteSource {
    kind: ProviderKind,
    base: String,
    explicit_endpoint: bool,
    key: Option<Secret>,
    admin_key: Option<Secret>,
    cost: Mutex<Option<CostReading>>,
}

impl fmt::Debug for RemoteSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Only whether each credential is set is shown, never its value.
        f.debug_struct("RemoteSource")
            .field("kind", &self.kind)
            .field("base", &self.base)
            .field("key_set", &self.key.is_some())
            .field("admin_key_set", &self.admin_key.is_some())
            .finish()
    }
}

impl RemoteSource {
    /// A source at `base`, or the provider's public endpoint when `None`.
    pub fn new(
        kind: ProviderKind,
        base: Option<&str>,
        key: Option<&str>,
        admin_key: Option<&str>,
    ) -> Self {
        let default = match kind {
            ProviderKind::Anthropic => ANTHROPIC_BASE,
            _ => OPENAI_BASE,
        };
        Self {
            kind,
            base: base
                .map(|b| b.trim().trim_end_matches('/').to_string())
                .filter(|b| !b.is_empty())
                .unwrap_or_else(|| default.to_string()),
            explicit_endpoint: base.is_some_and(|b| !b.trim().is_empty()),
            key: key.map(Secret::new),
            admin_key: admin_key.map(Secret::new),
            cost: Mutex::new(None),
        }
    }

    /// The source for OpenAI or Anthropic from the environment, or `None` for any
    /// other provider.
    pub fn from_env(kind: ProviderKind) -> Option<Self> {
        let (key_env, admin_env, url_env) = match kind {
            ProviderKind::OpenAi => (OPENAI_KEY_ENV, OPENAI_ADMIN_ENV, OPENAI_URL_ENV),
            ProviderKind::Anthropic => (ANTHROPIC_KEY_ENV, ANTHROPIC_ADMIN_ENV, ANTHROPIC_URL_ENV),
            _ => return None,
        };
        let key = env_value(key_env);
        let admin = env_value(admin_env);
        let url = env_value(url_env);
        Some(Self::new(
            kind,
            url.as_deref(),
            key.as_deref(),
            admin.as_deref(),
        ))
    }

    /// Whether the user has configured this provider. An unconfigured source
    /// makes no request at all.
    pub fn configured(&self) -> bool {
        self.key.is_some() || self.admin_key.is_some() || self.explicit_endpoint
    }

    fn setup_hint(&self) -> String {
        let (key, url) = match self.kind {
            ProviderKind::Anthropic => (ANTHROPIC_KEY_ENV, ANTHROPIC_URL_ENV),
            _ => (OPENAI_KEY_ENV, OPENAI_URL_ENV),
        };
        format!(
            "set {key}, or {url} for an explicit endpoint; nothing is requested until one is set"
        )
    }

    fn admin_env(&self) -> &'static str {
        match self.kind {
            ProviderKind::Anthropic => ANTHROPIC_ADMIN_ENV,
            _ => OPENAI_ADMIN_ENV,
        }
    }

    /// The headers that authenticate a request with `key`. Only this function
    /// puts a credential into a request.
    fn auth_headers(&self, key: Option<&Secret>) -> Vec<(String, String)> {
        match (self.kind, key) {
            (ProviderKind::Anthropic, key) => {
                let mut headers = vec![("anthropic-version".to_string(), ANTHROPIC_VERSION.into())];
                if let Some(k) = key {
                    headers.push(("x-api-key".to_string(), k.expose().to_string()));
                }
                headers
            }
            (_, Some(k)) => vec![(
                "Authorization".to_string(),
                format!("Bearer {}", k.expose()),
            )],
            (_, None) => Vec::new(),
        }
    }

    fn list_models(&self, timeout: Duration) -> Result<usize, SourceError> {
        let headers = self.auth_headers(self.key.as_ref());
        let url = format!("{}/v1/models", self.base);
        let reply = http::get(&url, timeout, &borrow(&headers))?;
        if reply.status != 200 {
            return Err(SourceError::Status(reply.status));
        }
        let parsed: ModelsReply =
            serde_json::from_str(&reply.body).map_err(|e| SourceError::NotJson(e.to_string()))?;
        parsed
            .data
            .map(|models| models.len())
            .ok_or_else(|| SourceError::Rejected("the reply has no data list".into()))
    }

    /// The cost reading, from the cache while it is fresh, otherwise fetched now.
    /// The cache holds failures too, so a refused admin key is not retried each poll.
    fn cost_reading(&self, timeout: Duration) -> CostReading {
        let mut cache = lock(&self.cost);
        if let Some(cached) = cache.as_ref() {
            if cached.fetched_at().elapsed() < COST_INTERVAL {
                return cached.clone();
            }
        }
        let fetched_at = Instant::now();
        let fresh = match &self.admin_key {
            None => CostReading::Unavailable {
                reason: format!(
                    "set {} to read organization costs; the usage API needs an admin key",
                    self.admin_env()
                ),
                fetched_at,
            },
            Some(admin) => match self.fetch_cost(admin, Utc::now(), timeout, fetched_at) {
                Ok(reading) => reading,
                Err(error) => CostReading::Unavailable {
                    reason: self.cost_reason(&error),
                    fetched_at,
                },
            },
        };
        *cache = Some(fresh.clone());
        fresh
    }

    fn fetch_cost(
        &self,
        admin: &Secret,
        now: DateTime<Utc>,
        timeout: Duration,
        fetched_at: Instant,
    ) -> Result<CostReading, SourceError> {
        let start = month_start(now);
        let headers = self.auth_headers(Some(admin));
        let mut total = 0.0_f64;
        let mut currency: Option<String> = None;
        let mut rows = 0usize;
        let mut page: Option<String> = None;
        let scale = match self.kind {
            ProviderKind::Anthropic => "cents",
            _ => "units",
        };

        for _ in 0..MAX_PAGES {
            let mut url = match self.kind {
                ProviderKind::Anthropic => format!(
                    "{}/v1/organizations/cost_report?starting_at={}&bucket_width=1d&limit=31",
                    self.base,
                    encode(&rfc3339(start))
                ),
                _ => format!(
                    "{}/v1/organization/costs?start_time={}&bucket_width=1d&limit=31",
                    self.base,
                    start.timestamp()
                ),
            };
            if let Some(cursor) = &page {
                url.push_str("&page=");
                url.push_str(&encode(cursor));
            }
            let reply = http::get(&url, timeout, &borrow(&headers))?;
            if reply.status != 200 {
                return Err(SourceError::Status(reply.status));
            }
            let (rows_here, more, next) = match self.kind {
                ProviderKind::Anthropic => {
                    let body: AnthropicCost = parse(&reply.body)?;
                    collect_anthropic(body, &mut total, &mut currency)?
                }
                _ => {
                    let body: OpenAiCosts = parse(&reply.body)?;
                    collect_openai(body, &mut total, &mut currency)?
                }
            };
            rows += rows_here;
            match (more, next) {
                (false, _) => {
                    return match currency {
                        Some(currency) if rows > 0 => Ok(CostReading::Known {
                            amount: total,
                            currency,
                            scale,
                            period_start: rfc3339(start),
                            period_end: rfc3339(now),
                            fetched_at,
                        }),
                        _ => Err(SourceError::Rejected(
                            "the reply has no cost rows, so no currency is known".into(),
                        )),
                    };
                }
                (true, Some(cursor)) => page = Some(cursor),
                (true, None) => {
                    return Err(SourceError::Rejected(
                        "the reply says more pages follow but gives no cursor".into(),
                    ))
                }
            }
        }
        Err(SourceError::Rejected(format!(
            "more than {MAX_PAGES} pages of cost rows; the total is not given"
        )))
    }

    /// What a failed cost fetch says, without any credential or reply body.
    fn cost_reason(&self, error: &SourceError) -> String {
        match (self.kind, error) {
            (ProviderKind::Anthropic, SourceError::Status(code)) => format!(
                "the cost report was refused (HTTP {code}); an individual account has no Admin API, so an admin key is required"
            ),
            (_, SourceError::Status(code)) => {
                format!("the costs request was refused (HTTP {code}); check the admin key")
            }
            (_, SourceError::Unreachable(why)) => format!("no reply from {}: {why}", self.base),
            (_, other) => other.to_string(),
        }
    }
}

impl InferenceSource for RemoteSource {
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
        if !self.configured() {
            return Err(SourceError::NotConfigured(self.setup_hint()));
        }
        let models = self.list_models(timeout)?;
        let mut gauges = BTreeMap::new();
        gauges.insert("models_listed".to_string(), models as f64);
        Ok(ProviderSample {
            identity: Identity::default(),
            gauges,
            cost: Some(self.cost_reading(timeout)),
            ..ProviderSample::default()
        })
    }

    fn poll_timeout(&self) -> Duration {
        REMOTE_TIMEOUT
    }

    fn min_interval(&self) -> Duration {
        REACHABILITY_INTERVAL
    }

    fn backoff_cap(&self) -> Duration {
        REACHABILITY_INTERVAL
    }
}

/// Unit tests get none: a developer's real key must not reach a real host because
/// a test opened the Inference tab.
#[cfg(test)]
pub fn all_from_env() -> Vec<Box<dyn InferenceSource>> {
    Vec::new()
}

/// Every remote provider as a source, each configured from its environment.
#[cfg(not(test))]
pub fn all_from_env() -> Vec<Box<dyn InferenceSource>> {
    [ProviderKind::OpenAi, ProviderKind::Anthropic]
        .into_iter()
        .filter_map(RemoteSource::from_env)
        .map(|s| Box::new(s) as Box<dyn InferenceSource>)
        .collect()
}

// ── Replies ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ModelsReply {
    data: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiCosts {
    data: Option<Vec<OpenAiBucket>>,
    has_more: Option<bool>,
    next_page: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiBucket {
    results: Option<Vec<OpenAiResult>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResult {
    amount: Option<OpenAiAmount>,
}

#[derive(Debug, Deserialize)]
struct OpenAiAmount {
    value: Option<f64>,
    currency: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicCost {
    data: Option<Vec<AnthropicBucket>>,
    has_more: Option<bool>,
    next_page: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicBucket {
    results: Option<Vec<AnthropicResult>>,
}

#[derive(Debug, Deserialize)]
struct AnthropicResult {
    amount: Option<String>,
    currency: Option<String>,
}

fn parse<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, SourceError> {
    serde_json::from_str(body).map_err(|e| SourceError::NotJson(e.to_string()))
}

/// Add one page of OpenAI rows. Every row must name a currency, and all rows must
/// agree: rows in two currencies are never added together.
fn collect_openai(
    body: OpenAiCosts,
    total: &mut f64,
    currency: &mut Option<String>,
) -> Result<(usize, bool, Option<String>), SourceError> {
    let mut rows = 0;
    for bucket in body.data.unwrap_or_default() {
        for result in bucket.results.unwrap_or_default() {
            let amount = result
                .amount
                .ok_or_else(|| SourceError::Rejected("a cost row has no amount".into()))?;
            let value = amount
                .value
                .ok_or_else(|| SourceError::Rejected("a cost row has no value".into()))?;
            let name = amount
                .currency
                .ok_or_else(|| SourceError::Rejected("a cost row has no currency".into()))?
                .to_ascii_lowercase();
            add_row(total, currency, value, name)?;
            rows += 1;
        }
    }
    Ok((rows, body.has_more.unwrap_or(false), body.next_page))
}

/// Add one page of Anthropic rows, whose amounts are decimal strings in cents.
fn collect_anthropic(
    body: AnthropicCost,
    total: &mut f64,
    currency: &mut Option<String>,
) -> Result<(usize, bool, Option<String>), SourceError> {
    let mut rows = 0;
    for bucket in body.data.unwrap_or_default() {
        for result in bucket.results.unwrap_or_default() {
            let raw = result
                .amount
                .ok_or_else(|| SourceError::Rejected("a cost row has no amount".into()))?;
            let value: f64 = raw
                .parse()
                .map_err(|_| SourceError::Rejected("a cost amount is not a number".into()))?;
            let name = result
                .currency
                .ok_or_else(|| SourceError::Rejected("a cost row has no currency".into()))?
                .to_ascii_lowercase();
            add_row(total, currency, value, name)?;
            rows += 1;
        }
    }
    Ok((rows, body.has_more.unwrap_or(false), body.next_page))
}

fn add_row(
    total: &mut f64,
    currency: &mut Option<String>,
    value: f64,
    name: String,
) -> Result<(), SourceError> {
    if !value.is_finite() {
        return Err(SourceError::Rejected("a cost amount is not finite".into()));
    }
    match currency {
        None => *currency = Some(name),
        Some(first) if *first == name => {}
        Some(_) => {
            return Err(SourceError::Rejected(
                "the rows name more than one currency, so no single total is given".into(),
            ))
        }
    }
    *total += value;
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn env_value(var: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn borrow(headers: &[(String, String)]) -> Vec<(&str, &str)> {
    headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 00:00 UTC on the first day of `now`'s month.
fn month_start(now: DateTime<Utc>) -> DateTime<Utc> {
    NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|naive| Utc.from_utc_datetime(&naive))
        .unwrap_or(now)
}

fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Percent-encode everything outside the URL unreserved set.
fn encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key no real account has, so a leak is easy to spot in any output.
    const SENTINEL: &str = "sk-sentinel-DO-NOT-LEAK-7f3a";

    #[test]
    fn secret_debug_is_redacted() {
        let s = Secret::new(SENTINEL);
        assert_eq!(format!("{s:?}"), "Secret(<redacted>)");
        assert!(!format!("{s:?}").contains(SENTINEL));
    }

    #[test]
    fn source_debug_shows_whether_a_key_is_set_not_the_key() {
        let source = RemoteSource::new(ProviderKind::OpenAi, None, Some(SENTINEL), Some(SENTINEL));
        let shown = format!("{source:?}");
        assert!(shown.contains("key_set: true"), "{shown}");
        assert!(!shown.contains(SENTINEL), "{shown}");
    }

    #[test]
    fn an_unconfigured_source_makes_no_request_and_says_how_to_configure() {
        let source = RemoteSource::new(ProviderKind::OpenAi, None, None, None);
        assert!(!source.configured());
        let started = Instant::now();
        let result = source.poll(REMOTE_TIMEOUT);
        // A request to api.openai.com would take far longer than this, and could
        // never return NotConfigured; the early return is what makes it quick.
        assert!(started.elapsed() < Duration::from_millis(100));
        match result {
            Err(SourceError::NotConfigured(hint)) => {
                assert!(hint.contains(OPENAI_KEY_ENV), "{hint}");
            }
            other => panic!("expected NotConfigured, got {other:?}"),
        }
    }

    #[test]
    fn month_start_is_the_first_day_at_midnight_utc() {
        let now = Utc.with_ymd_and_hms(2026, 11, 3, 17, 44, 0).unwrap();
        assert_eq!(rfc3339(month_start(now)), "2026-11-01T00:00:00Z");
    }

    #[test]
    fn cursors_are_percent_encoded() {
        assert_eq!(encode("page_MjAy=="), "page_MjAy%3D%3D");
        assert_eq!(encode("2026-11-01T00:00:00Z"), "2026-11-01T00%3A00%3A00Z");
    }

    #[test]
    fn rows_in_two_currencies_are_never_added_together() {
        let mut total = 0.0;
        let mut currency = None;
        add_row(&mut total, &mut currency, 1.0, "usd".into()).expect("first row");
        let err = add_row(&mut total, &mut currency, 2.0, "eur".into()).unwrap_err();
        assert!(err.to_string().contains("more than one currency"), "{err}");
        assert_eq!(total, 1.0);
    }

    /// Hand-written from the OpenAI costs example (a bucket with
    /// `results[].amount.{value,currency}`), and the Anthropic cost report example
    /// (amounts as decimal strings in cents).
    #[test]
    fn openai_rows_sum_across_buckets_and_keep_their_currency() {
        let body: OpenAiCosts = parse(
            r#"{"object":"page","data":[
                {"object":"bucket","results":[{"amount":{"value":1.5,"currency":"usd"}}]},
                {"object":"bucket","results":[{"amount":{"value":2.25,"currency":"usd"}}]}
            ],"has_more":false,"next_page":null}"#,
        )
        .expect("parses");
        let mut total = 0.0;
        let mut currency = None;
        let (rows, more, next) = collect_openai(body, &mut total, &mut currency).expect("collects");
        assert_eq!((rows, more, next), (2, false, None));
        assert_eq!(total, 3.75);
        assert_eq!(currency.as_deref(), Some("usd"));
    }

    #[test]
    fn anthropic_amounts_are_cents_as_decimal_strings() {
        let body: AnthropicCost = parse(
            r#"{"data":[{"starting_at":"2026-11-01T00:00:00Z","results":[
                {"amount":"123.45","currency":"USD"},{"amount":"10.00","currency":"USD"}
            ]}],"has_more":false,"next_page":null}"#,
        )
        .expect("parses");
        let mut total = 0.0;
        let mut currency = None;
        collect_anthropic(body, &mut total, &mut currency).expect("collects");
        assert!((total - 133.45).abs() < 1e-9);
        assert_eq!(currency.as_deref(), Some("usd"));
    }

    // ── Loopback routes ──────────────────────────────────────────────────────

    /// A loopback HTTP server that answers by request target. A route matches when
    /// the target contains its needle, so more specific routes go first. Every
    /// request is recorded by its path and by whether it carried a credential, and
    /// never by its header values.
    struct Routes {
        port: u16,
        hits: std::sync::Arc<Mutex<Vec<String>>>,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Routes {
        fn start(routes: Vec<(&'static str, u16, &'static str)>) -> Routes {
            use std::io::{Read, Write};
            use std::net::TcpListener;
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::Arc;

            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            listener.set_nonblocking(true).expect("nonblocking");
            let port = listener.local_addr().expect("addr").port();
            let hits = Arc::new(Mutex::new(Vec::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let (h, s) = (Arc::clone(&hits), Arc::clone(&stop));
            let thread = std::thread::spawn(move || {
                while !s.load(Ordering::SeqCst) {
                    let Ok((mut conn, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    };
                    let _ = conn.set_nonblocking(false);
                    let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut buf = vec![0u8; 8192];
                    let n = conn.read(&mut buf).unwrap_or(0);
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    let target = text
                        .lines()
                        .next()
                        .and_then(|l| l.split_whitespace().nth(1))
                        .unwrap_or("")
                        .to_string();
                    let lower = text.to_ascii_lowercase();
                    let credential =
                        lower.contains("\nauthorization:") || lower.contains("\nx-api-key:");
                    h.lock().expect("hits").push(format!(
                        "{target} credential={}",
                        if credential { "sent" } else { "none" }
                    ));
                    let (status, body) = routes
                        .iter()
                        .find(|(needle, _, _)| target.contains(needle))
                        .map(|(_, status, body)| (*status, *body))
                        .unwrap_or((404, "{\"error\":\"no route\"}"));
                    let reason = if status == 200 { "OK" } else { "Error" };
                    let response = format!(
                        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = conn.write_all(response.as_bytes());
                }
            });
            Routes {
                port,
                hits,
                stop,
                thread: Some(thread),
            }
        }

        fn url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }

        fn hits(&self) -> Vec<String> {
            self.hits.lock().expect("hits").clone()
        }

        fn count(&self, path: &str) -> usize {
            self.hits().iter().filter(|h| h.starts_with(path)).count()
        }
    }

    impl Drop for Routes {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    const OPENAI_MODELS: &str = r#"{"object":"list","data":[{"id":"gpt-a"},{"id":"gpt-b"}]}"#;
    const OPENAI_PAGE_ONE: &str = r#"{"object":"page","data":[{"object":"bucket","results":[{"amount":{"value":1.5,"currency":"usd"}}]}],"has_more":true,"next_page":"cur2"}"#;
    const OPENAI_PAGE_TWO: &str = r#"{"object":"page","data":[{"object":"bucket","results":[{"amount":{"value":2.25,"currency":"usd"}}]}],"has_more":false,"next_page":null}"#;

    #[test]
    fn openai_reachability_and_paged_cost_come_from_a_loopback_server() {
        // The cursor route is listed first, so the second page request gets page two.
        let routes = Routes::start(vec![
            ("/v1/models", 200, OPENAI_MODELS),
            ("&page=cur2", 200, OPENAI_PAGE_TWO),
            ("/v1/organization/costs", 200, OPENAI_PAGE_ONE),
        ]);
        let source = RemoteSource::new(
            ProviderKind::OpenAi,
            Some(&routes.url()),
            Some(SENTINEL),
            Some(SENTINEL),
        );
        let sample = source.poll(Duration::from_secs(3)).expect("reachable");
        assert_eq!(sample.gauges["models_listed"], 2.0);
        match sample.cost.expect("cost is reported") {
            CostReading::Known {
                amount,
                currency,
                scale,
                ..
            } => {
                assert_eq!(currency, "usd");
                assert_eq!(scale, "units");
                // Page one (1.50) and page two (2.25), summed.
                assert!((amount - 3.75).abs() < 1e-9, "{amount}");
            }
            other => panic!("expected a known cost, got {other:?}"),
        }
        assert_eq!(routes.count("/v1/organization/costs"), 2);
        assert!(routes.hits().iter().all(|h| h.contains("credential=sent")));
    }

    /// The sentinel key appears in no error, no reason and no debug text, even
    /// when the server rejects it.
    #[test]
    fn a_rejected_key_is_reported_by_status_and_never_echoes_the_key() {
        let routes = Routes::start(vec![
            ("/v1/models", 401, "{\"error\":\"invalid key\"}"),
            ("/v1/organization/costs", 403, "{\"error\":\"forbidden\"}"),
        ]);
        let source = RemoteSource::new(
            ProviderKind::OpenAi,
            Some(&routes.url()),
            Some(SENTINEL),
            Some(SENTINEL),
        );
        let err = source.poll(Duration::from_secs(3)).unwrap_err();
        assert_eq!(err, SourceError::Status(401));
        let texts = [
            err.to_string(),
            format!("{err:?}"),
            format!("{source:?}"),
            super::super::local::reason(ProviderKind::OpenAi, &source.endpoint(), &err),
        ];
        for text in &texts {
            assert!(!text.contains(SENTINEL), "a key leaked into: {text}");
        }
    }

    #[test]
    fn a_refused_admin_key_leaves_cost_unavailable_with_the_status() {
        let routes = Routes::start(vec![
            ("/v1/models", 200, OPENAI_MODELS),
            ("/v1/organization/costs", 403, "{\"error\":\"forbidden\"}"),
        ]);
        let source = RemoteSource::new(
            ProviderKind::OpenAi,
            Some(&routes.url()),
            Some(SENTINEL),
            Some(SENTINEL),
        );
        let sample = source.poll(Duration::from_secs(3)).expect("reachable");
        match sample.cost.expect("cost reported") {
            CostReading::Unavailable { reason, .. } => {
                assert!(reason.contains("HTTP 403"), "{reason}");
                assert!(!reason.contains(SENTINEL), "{reason}");
            }
            other => panic!("expected unavailable, got {other:?}"),
        }
    }

    #[test]
    fn no_admin_key_means_cost_is_unavailable_and_no_cost_request_is_made() {
        let routes = Routes::start(vec![("/v1/models", 200, OPENAI_MODELS)]);
        let source = RemoteSource::new(
            ProviderKind::OpenAi,
            Some(&routes.url()),
            Some(SENTINEL),
            None,
        );
        let sample = source.poll(Duration::from_secs(3)).expect("reachable");
        match sample.cost.expect("cost reported") {
            CostReading::Unavailable { reason, .. } => {
                assert!(reason.contains(OPENAI_ADMIN_ENV), "{reason}");
            }
            other => panic!("expected unavailable, got {other:?}"),
        }
        assert_eq!(routes.count("/v1/organization/costs"), 0);
    }

    #[test]
    fn cost_is_fetched_once_per_interval_not_on_every_poll() {
        let routes = Routes::start(vec![
            ("/v1/models", 200, OPENAI_MODELS),
            ("/v1/organization/costs", 200, OPENAI_PAGE_TWO),
        ]);
        let source = RemoteSource::new(
            ProviderKind::OpenAi,
            Some(&routes.url()),
            Some(SENTINEL),
            Some(SENTINEL),
        );
        source.poll(Duration::from_secs(3)).expect("first");
        source.poll(Duration::from_secs(3)).expect("second");
        assert_eq!(routes.count("/v1/organization/costs"), 1);
        assert_eq!(routes.count("/v1/models"), 2);
    }

    #[test]
    fn anthropic_admin_refusal_names_the_individual_account_case() {
        let routes = Routes::start(vec![
            ("/v1/models", 200, "{\"data\":[{\"id\":\"claude-x\"}]}"),
            (
                "/v1/organizations/cost_report",
                403,
                "{\"error\":\"forbidden\"}",
            ),
        ]);
        let source = RemoteSource::new(
            ProviderKind::Anthropic,
            Some(&routes.url()),
            Some(SENTINEL),
            Some(SENTINEL),
        );
        let sample = source.poll(Duration::from_secs(3)).expect("reachable");
        match sample.cost.expect("cost reported") {
            CostReading::Unavailable { reason, .. } => {
                assert!(reason.contains("HTTP 403"), "{reason}");
                assert!(reason.contains("individual account"), "{reason}");
            }
            other => panic!("expected unavailable, got {other:?}"),
        }
    }

    #[test]
    fn a_closed_remote_endpoint_is_unreachable_not_zero() {
        let routes = Routes::start(vec![]);
        let url = routes.url();
        drop(routes);
        let source = RemoteSource::new(ProviderKind::Anthropic, Some(&url), Some(SENTINEL), None);
        let err = source.poll(Duration::from_secs(1)).unwrap_err();
        assert!(matches!(err, SourceError::Unreachable(_)), "{err:?}");
    }
}
