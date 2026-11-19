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
        headers: Vec::new(),
        interval: std::env::var("IRONMON_TELEMETRY_INTERVAL")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map_or(INTERVAL, |secs| Duration::from_secs(secs.max(10))),
    }
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
    fn install_ids_are_random_and_hex() {
        let a = random_hex_128();
        let b = random_hex_128();
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
