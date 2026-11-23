//! System telemetry to nervosys: the [`ConsentScope::SystemTelemetry`] scope.
//!
//! `ironmon serve` sends every reading it takes to
//! `https://telemetry.nervosys.ai/v1/metrics` every [`INTERVAL`], encoded as
//! OTLP/HTTP JSON by [`crate::otlp`], with a random install id as
//! `service.instance.id`. **On by default**; [`enabled`] is consulted before
//! every send, so `ironmon privacy opt-out`, `--no-telemetry`, `--offline`,
//! `IRONMON_NO_TELEMETRY` and `DO_NOT_TRACK=1` each stop the next send. A
//! detected sandbox sends nothing. [`crate::consent`] has the disclosure.
//!
//! `IRONMON_TELEMETRY_ENDPOINT` replaces the destination, for testing or for a
//! self-hosted receiver, and `IRONMON_TELEMETRY_INTERVAL` the interval in
//! seconds (at least 10).
//!
//! [`ConsentScope::SystemTelemetry`]: crate::consent::ConsentScope::SystemTelemetry

use std::path::PathBuf;
use std::time::Duration;

use crate::consent::{ConsentManager, ConsentScope};

/// Where system telemetry goes.
pub const ENDPOINT: &str = "https://telemetry.nervosys.ai/v1/metrics";

/// The bearer token for the collector: `IRONMON_TELEMETRY_TOKEN`, else the one
/// this install was issued by [`ensure_token`]. None is built in: ingest tokens
/// are private, and anything compiled from this public source would not be.
pub fn token() -> Option<String> {
    env_nonempty("IRONMON_TELEMETRY_TOKEN").or_else(|| {
        let path = token_path()?;
        let stored = std::fs::read_to_string(path).ok()?;
        let stored = stored.trim();
        valid_token(stored).then(|| stored.to_string())
    })
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// A token as the registration endpoint issues them: printable, no whitespace,
/// short enough to be a header value.
fn valid_token(t: &str) -> bool {
    !t.is_empty() && t.len() <= 512 && t.bytes().all(|b| b.is_ascii_graphic())
}

/// Whether this install gets its token by registering: true for the nervosys
/// collector, false when a token is supplied or the endpoint is replaced (a
/// self-hosted receiver has its own arrangements).
pub fn registers() -> bool {
    env_nonempty("IRONMON_TELEMETRY_TOKEN").is_none()
        && env_nonempty("IRONMON_TELEMETRY_ENDPOINT").is_none()
}

/// The registration endpoint beside [`ENDPOINT`].
pub const REGISTER_URL: &str = "https://telemetry.nervosys.ai/v1/register";

/// What [`ensure_token`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenState {
    /// A token to send.
    Ready(String),
    /// nervosys revoked this install, or does not accept it: stop for good.
    Revoked,
    /// The request was refused as malformed (400). Retrying the same request
    /// cannot succeed, so stop for this run; the next run (often a newer
    /// version) tries again.
    Refused,
    /// Not now (offline, rate limited, server busy): try again later.
    Later,
}

/// The token to send, registering this install first if it has none, or if
/// `rotate` asks for a fresh one after the collector refused the old. The call
/// is made only after consent has been checked, so an opted-out install never
/// contacts nervosys. A revocation is recorded and honoured from then on.
#[cfg(feature = "remote-backends")]
pub fn ensure_token(rotate: bool) -> TokenState {
    if revoked() {
        return TokenState::Revoked;
    }
    if !rotate {
        if let Some(t) = token() {
            return TokenState::Ready(t);
        }
    }
    let Some(id) = install_id() else {
        return TokenState::Later;
    };
    match register(&id) {
        Registered::Token(t) => {
            store_token(&t);
            TokenState::Ready(t)
        }
        Registered::Revoked => {
            if let Some(path) = revoked_path() {
                let _ = std::fs::write(path, "revoked\n");
            }
            if let Some(path) = token_path() {
                let _ = std::fs::remove_file(path);
            }
            TokenState::Revoked
        }
        Registered::Refused => TokenState::Refused,
        Registered::Later => TokenState::Later,
    }
}

#[cfg(feature = "remote-backends")]
enum Registered {
    Token(String),
    Revoked,
    Refused,
    Later,
}

/// One `POST /v1/register`. The body carries the install id, version and OS
/// type and nothing else; the endpoint rejects any other field. 201 and 200
/// carry a token, 403 means stop, and anything else means try later.
#[cfg(feature = "remote-backends")]
fn register(install_id: &str) -> Registered {
    use std::io::Read;
    let body = serde_json::json!({
        "service.name": "ironmon",
        "telemetry.install_id": install_id,
        "service.version": env!("CARGO_PKG_VERSION"),
        "os.type": std::env::consts::OS,
    });
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return Registered::Later;
    };
    let Ok(response) = client.post(REGISTER_URL).json(&body).send() else {
        return Registered::Later;
    };
    match response.status().as_u16() {
        200 | 201 => {
            let mut text = String::new();
            if response.take(16 * 1024).read_to_string(&mut text).is_err() {
                return Registered::Later;
            }
            serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("token")?.as_str().map(str::to_string))
                .filter(|t| valid_token(t))
                .map_or(Registered::Later, Registered::Token)
        }
        403 => Registered::Revoked,
        400 => Registered::Refused,
        _ => Registered::Later,
    }
}

/// The device-profile notice's version. Raise it whenever what the profile
/// covers widens, so existing users see the notice again rather than silently
/// inheriting the larger scope on upgrade.
pub const DEVICE_NOTICE_VERSION: u32 = 2;

fn device_notice_path() -> Option<PathBuf> {
    Some(
        ConsentManager::default_path()
            .ok()?
            .with_file_name("device_profile_notice"),
    )
}

/// Whether this install has yet to be shown the current device-profile notice.
pub fn device_notice_due() -> bool {
    let shown = device_notice_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(0);
    shown < DEVICE_NOTICE_VERSION
}

/// Record that the current notice has been shown.
pub fn mark_device_notice_shown() {
    if let Some(path) = device_notice_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, DEVICE_NOTICE_VERSION.to_string());
    }
}

fn token_path() -> Option<PathBuf> {
    Some(
        ConsentManager::default_path()
            .ok()?
            .with_file_name("telemetry_token"),
    )
}

fn revoked_path() -> Option<PathBuf> {
    Some(
        ConsentManager::default_path()
            .ok()?
            .with_file_name("telemetry_revoked"),
    )
}

/// Whether nervosys has revoked this install's telemetry.
pub fn revoked() -> bool {
    revoked_path().is_some_and(|p| p.exists())
}

/// Keep the issued token beside the consent record, readable only by its owner
/// where the platform has such a mode (the Windows profile directory is already
/// private to the user).
#[cfg(feature = "remote-backends")]
fn store_token(token: &str) {
    let Some(path) = token_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Ok(mut file) = options.open(&path) {
        use std::io::Write;
        let _ = file.write_all(token.as_bytes());
    }
}

/// How often `ironmon serve` sends it.
pub const INTERVAL: Duration = Duration::from_secs(15 * 60);

/// The destination: [`ENDPOINT`], or `IRONMON_TELEMETRY_ENDPOINT` when set.
pub fn endpoint() -> String {
    std::env::var("IRONMON_TELEMETRY_ENDPOINT")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| ENDPOINT.to_string())
}

/// Whether the device profile may be sent: [`ConsentScope::DeviceProfile`] is
/// on by default and has not been denied, with the same runtime and sandbox
/// refusals as everything else.
pub fn device_profile_enabled() -> bool {
    ConsentManager::load()
        .map(|m| m.should_collect_data(ConsentScope::DeviceProfile))
        .unwrap_or(false)
}

/// The device-profile consent state, reported on every vendor push as
/// `device.profile_consent` so the server can tell a decline from a fault: a
/// push that says `granted` but carries no `device.*` keys is a bug, not a
/// privacy-respecting user, and that is the one combination worth alerting on.
/// Carries no device data — only the state the user set. `unsupported` is never
/// emitted by this client (every platform it runs on can produce a profile).
pub fn profile_consent() -> &'static str {
    match ConsentManager::load() {
        Err(_) => "unavailable",
        Ok(m) => m.device_profile_consent_label(),
    }
}

/// Whether system telemetry may be sent right now: not opted out by flag,
/// `DO_NOT_TRACK` or a recorded denial, and not in a detected sandbox. An
/// unreadable consent file refuses, since a denial in it cannot be ruled out.
pub fn enabled() -> bool {
    ConsentManager::load()
        .map(|m| m.should_collect_data(ConsentScope::SystemTelemetry))
        .unwrap_or(false)
}

/// Why system telemetry is off, if it is, for the `ironmon serve` banner.
pub fn off_reason() -> Option<String> {
    if let Some(flag) = crate::consent::runtime_opt_out() {
        return Some(flag.to_string());
    }
    if revoked() {
        return Some("nervosys revoked this install's telemetry token".into());
    }
    match ConsentManager::load() {
        Err(_) => Some("the consent file could not be read".into()),
        Ok(m) if !m.recorded_or_default(ConsentScope::SystemTelemetry) => {
            Some("opted out with `ironmon privacy opt-out`".into())
        }
        Ok(m) if !m.should_collect_data(ConsentScope::SystemTelemetry) => {
            Some("this looks like a sandbox".into())
        }
        Ok(_) => None,
    }
}

/// The export settings for the telemetry push.
pub fn settings() -> crate::otlp::Settings {
    crate::otlp::Settings {
        url: endpoint(),
        headers: token()
            .map(|t| vec![("Authorization".to_string(), format!("Bearer {t}"))])
            .unwrap_or_default(),
        interval: std::env::var("IRONMON_TELEMETRY_INTERVAL")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map_or(INTERVAL, |secs| Duration::from_secs(secs.max(10))),
    }
}

/// The resource attributes the nervosys collector's client contract requires
/// (`otel/SEMANTIC-CONVENTIONS.md` in nervosys_v2): `os.type`, `host.arch`,
/// `telemetry.locale` and `telemetry.install_id`, beside `service.name` and
/// `service.version`, which the encoder adds. The install id is also sent as
/// `service.instance.id`, its OpenTelemetry name. An attribute that cannot be
/// read is left out rather than guessed.
pub fn resource_attributes(install_id: Option<String>) -> Vec<(&'static str, String)> {
    let mut out = vec![
        ("os.type", os_type().to_string()),
        ("host.arch", std::env::consts::ARCH.to_string()),
    ];
    if let Some(locale) = locale() {
        out.push(("telemetry.locale", locale));
    }
    if let Some(id) = install_id {
        out.push(("telemetry.install_id", id.clone()));
        out.push(("service.instance.id", id));
    }
    out
}

/// `linux`, `windows` or `macos`, as the contract spells them.
fn os_type() -> &'static str {
    std::env::consts::OS
}

/// The user's locale in `en_US` form: `LC_ALL`, `LC_MESSAGES` or `LANG`, then
/// the Windows user default. Desktop launches on Windows and macOS rarely set
/// the variables, which is why the OS is asked; macOS without them is left out.
pub(crate) fn locale() -> Option<String> {
    let from_env = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.trim().is_empty() && v != "C" && v != "POSIX");
    #[cfg(target_os = "windows")]
    let raw = from_env.or_else(crate::os_info::windows_user_locale);
    #[cfg(not(target_os = "windows"))]
    let raw = from_env;
    raw.map(|r| normalize_locale(&r)).filter(|l| !l.is_empty())
}

/// `en-US`, `en_US.UTF-8` and `en_US@euro` all become `en_US`; at most 20
/// characters, the width of the column it lands in.
fn normalize_locale(raw: &str) -> String {
    let base = raw.trim().split(['.', '@']).next().unwrap_or("");
    base.replace('-', "_").chars().take(20).collect()
}

fn install_id_path() -> Option<PathBuf> {
    Some(
        ConsentManager::default_path()
            .ok()?
            .with_file_name("install_id"),
    )
}

/// A random id for this installation, created on first use and kept beside the
/// consent record. It is not derived from the hardware, the user or the host
/// name, and deleting the file makes a new one.
pub fn install_id() -> Option<String> {
    let path = install_id_path()?;
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim();
        if existing.len() == 32 && existing.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Some(existing.to_string());
        }
    }
    let id = random_hex_128();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, &id).ok()?;
    Some(id)
}

/// 128 random bits as hex, from the OS-seeded keys `RandomState` draws.
fn random_hex_128() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let half = || {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        h.finish()
    };
    format!("{:016x}{:016x}", half(), half())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_destination_is_nervosys() {
        assert_eq!(ENDPOINT, "https://telemetry.nervosys.ai/v1/metrics");
        assert_eq!(INTERVAL, Duration::from_secs(900));
    }

    #[test]
    fn issued_tokens_are_checked_before_use() {
        assert!(valid_token("nrvs_otlp_abc123"));
        assert!(!valid_token(""));
        assert!(!valid_token("has space"));
        assert!(!valid_token("line\nbreak"));
        assert!(!valid_token(&"x".repeat(513)));
    }

    #[test]
    fn locales_are_normalised_to_the_contract_form() {
        assert_eq!(normalize_locale("en-US"), "en_US");
        assert_eq!(normalize_locale("de_DE.UTF-8"), "de_DE");
        assert_eq!(normalize_locale("fr_FR@euro"), "fr_FR");
    }

    #[test]
    fn the_contract_resource_attributes_are_present() {
        let attrs = resource_attributes(Some("ab".repeat(16)));
        let keys: Vec<&str> = attrs.iter().map(|(k, _)| *k).collect();
        for key in [
            "os.type",
            "host.arch",
            "telemetry.install_id",
            "service.instance.id",
        ] {
            assert!(keys.contains(&key), "{key} missing from {keys:?}");
        }
        let os = &attrs.iter().find(|(k, _)| *k == "os.type").unwrap().1;
        assert!(["linux", "windows", "macos"].contains(&os.as_str()) || !os.is_empty());
    }

    #[test]
    fn install_ids_are_random_and_hex() {
        let a = random_hex_128();
        let b = random_hex_128();
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
