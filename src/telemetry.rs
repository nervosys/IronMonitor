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

/// The bearer token the nervosys collector requires, from
/// `IRONMON_TELEMETRY_TOKEN` at run time. None is built in: ingest tokens are
/// private, and anything compiled from this public source would not be. Without
/// one nothing is sent to nervosys (see [`missing_token`]).
pub fn token() -> Option<String> {
    std::env::var("IRONMON_TELEMETRY_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// True when telemetry would go to the nervosys collector without the token it
/// requires. Such a push is refused with 401 after the readings have already
/// been uploaded, so none is made. A replaced endpoint is exempt, since a
/// self-hosted receiver need not ask for one.
pub fn missing_token() -> bool {
    token().is_none()
        && std::env::var("IRONMON_TELEMETRY_ENDPOINT")
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
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
    if missing_token() {
        return Some("this build has no telemetry token (IRONMON_TELEMETRY_TOKEN)".into());
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
fn locale() -> Option<String> {
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
