//! A rough guess at whether this machine sits in the EU/EEA, used only to make
//! the device profile opt-in there (ePrivacy Art. 5(3) requires consent before
//! a device fingerprint is stored or read).
//!
//! It is a heuristic, not a determination of law or residence: it reads the
//! system locale's region and the configured time zone, and either placing the
//! machine in the EEA is enough. It therefore errs toward asking — a false
//! positive only shows a consent prompt that elsewhere would be skipped. It
//! never decides telemetry by itself; it only flips the Device Profile default
//! from granted to not-granted where consent is required, and an explicit
//! recorded answer (either way) overrides it.

/// EU 27 plus the three non-EU EEA states: Iceland, Liechtenstein, Norway.
/// Two-letter ISO-3166 alpha-2, uppercase.
const EEA_REGIONS: &[&str] = &[
    // EU 27
    "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IT", "LV",
    "LT", "LU", "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE", // EEA non-EU
    "IS", "LI", "NO",
];

/// IANA time-zone names whose country is in the EEA. Kept explicit rather than
/// matching all of `Europe/*`, so UK and Swiss zones (not in the rule as the
/// owner decided) are not swept in; the few EEA zones outside the `Europe/`
/// prefix (the Atlantic islands, Canaries, Ceuta) are listed too.
const EEA_TZ: &[&str] = &[
    "Europe/Vienna",      // AT
    "Europe/Brussels",    // BE
    "Europe/Sofia",       // BG
    "Europe/Zagreb",      // HR
    "Europe/Nicosia",     // CY
    "Asia/Nicosia",       // CY (older tzdata alias)
    "Europe/Prague",      // CZ
    "Europe/Copenhagen",  // DK
    "Europe/Tallinn",     // EE
    "Europe/Helsinki",    // FI
    "Europe/Paris",       // FR
    "Europe/Berlin",      // DE
    "Europe/Busingen",    // DE
    "Europe/Athens",      // GR
    "Europe/Budapest",    // HU
    "Europe/Dublin",      // IE
    "Europe/Rome",        // IT
    "Europe/Riga",        // LV
    "Europe/Vilnius",     // LT
    "Europe/Luxembourg",  // LU
    "Europe/Malta",       // MT
    "Europe/Amsterdam",   // NL
    "Europe/Warsaw",      // PL
    "Europe/Lisbon",      // PT
    "Atlantic/Azores",    // PT
    "Atlantic/Madeira",   // PT
    "Europe/Bucharest",   // RO
    "Europe/Bratislava",  // SK
    "Europe/Ljubljana",   // SI
    "Europe/Madrid",      // ES
    "Atlantic/Canary",    // ES
    "Africa/Ceuta",       // ES
    "Europe/Stockholm",   // SE
    "Atlantic/Reykjavik", // IS
    "Europe/Vaduz",       // LI
    "Europe/Oslo",        // NO
];

/// Windows time-zone registry keys that denote an EEA country. Matched as a
/// prefix of the key, e.g. `"W. Europe Standard Time"`. A few Windows keys span
/// EEA and non-EEA countries (`FLE` includes Ukraine, `GTB` includes Türkiye);
/// those are kept because over-inclusion only shows an extra prompt, and the UK
/// is left out — its `"GMT Standard Time"` is shared with EEA Ireland and
/// Portugal, so the locale region resolves those more precisely.
const EEA_WINDOWS_TZ: &[&str] = &[
    "W. Europe Standard Time",        // DE, AT, NL, IT, SE, ...
    "Central Europe Standard Time",   // PL, CZ, HU, SK, SI
    "Central European Standard Time", // HR, ...
    "Romance Standard Time",          // FR, ES, BE
    "E. Europe Standard Time",        // RO, ...
    "GTB Standard Time",              // GR, RO (also TR)
    "FLE Standard Time",              // FI, EE, LV, LT, BG (also UA)
];

/// Whether the machine's locale or time zone places it in the EU/EEA.
pub fn in_eea() -> bool {
    locale_region().is_some_and(|r| EEA_REGIONS.contains(&r.as_str())) || timezone_in_eea()
}

/// The uppercase two-letter region from the system locale, if it names one.
fn locale_region() -> Option<String> {
    region_of_locale(&crate::telemetry::locale()?)
}

/// `en_US` / `en-US` / `de_DE.UTF-8` -> `US` / `US` / `DE`; `None` without a
/// well-formed two-letter region.
fn region_of_locale(locale: &str) -> Option<String> {
    let head = locale.split(['.', '@']).next().unwrap_or(locale);
    let region = head.split(['-', '_']).nth(1)?.trim();
    (region.len() == 2 && region.chars().all(|c| c.is_ascii_alphabetic()))
        .then(|| region.to_ascii_uppercase())
}

fn timezone_in_eea() -> bool {
    #[cfg(target_os = "windows")]
    if let Some(key) = crate::os_info::windows_timezone_key() {
        if EEA_WINDOWS_TZ.iter().any(|k| key.starts_with(k)) {
            return true;
        }
    }
    iana_timezone().is_some_and(|tz| EEA_TZ.contains(&tz.as_str()))
}

/// The IANA time-zone name from the environment or the system, on the platforms
/// that expose one that way (Linux, macOS). Windows uses the registry key above.
#[cfg(not(target_os = "windows"))]
fn iana_timezone() -> Option<String> {
    if let Ok(tz) = std::env::var("TZ") {
        let tz = tz.trim().trim_start_matches(':');
        if tz.contains('/') {
            return Some(tz.to_string());
        }
    }
    // /etc/timezone holds the name directly (Debian family); elsewhere
    // /etc/localtime is a symlink into the zoneinfo tree.
    if let Ok(name) = std::fs::read_to_string("/etc/timezone") {
        let name = name.trim();
        if name.contains('/') {
            return Some(name.to_string());
        }
    }
    let target = std::fs::read_link("/etc/localtime").ok()?;
    let s = target.to_string_lossy();
    let idx = s.find("/zoneinfo/")?;
    Some(s[idx + "/zoneinfo/".len()..].to_string())
}

#[cfg(target_os = "windows")]
fn iana_timezone() -> Option<String> {
    std::env::var("TZ").ok().and_then(|tz| {
        let tz = tz.trim().trim_start_matches(':').to_string();
        tz.contains('/').then_some(tz)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_region_pulls_the_country() {
        assert_eq!(region_of_locale("de_DE"), Some("DE".into()));
        assert_eq!(region_of_locale("en-US"), Some("US".into()));
        assert_eq!(region_of_locale("fr_FR.UTF-8"), Some("FR".into()));
        assert_eq!(region_of_locale("en_IE@euro"), Some("IE".into()));
    }

    #[test]
    fn locale_region_rejects_malformed() {
        assert_eq!(region_of_locale("en"), None); // no region
        assert_eq!(region_of_locale("C"), None);
        assert_eq!(region_of_locale("en_USA"), None); // three letters
        assert_eq!(region_of_locale("en_U2"), None); // not all alphabetic
    }

    #[test]
    fn eea_membership_is_by_table() {
        assert!(EEA_REGIONS.contains(&"DE"));
        assert!(EEA_REGIONS.contains(&"IE"));
        assert!(EEA_REGIONS.contains(&"NO")); // EEA non-EU
        assert!(!EEA_REGIONS.contains(&"US"));
        assert!(!EEA_REGIONS.contains(&"GB")); // UK left the EU
        assert!(!EEA_REGIONS.contains(&"CH")); // Switzerland not in EEA
    }

    #[test]
    fn eea_tables_hold_the_expected_zones() {
        assert!(EEA_TZ.contains(&"Europe/Berlin"));
        assert!(EEA_TZ.contains(&"Atlantic/Reykjavik"));
        assert!(!EEA_TZ.contains(&"Europe/London")); // UK
        assert!(!EEA_TZ.contains(&"Europe/Zurich")); // CH
        assert!(EEA_WINDOWS_TZ
            .iter()
            .any(|k| "W. Europe Standard Time".starts_with(k)));
    }
}
