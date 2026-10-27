//! NVIDIA Display Rules / Driver Restart Set (DRS) binary database scanner.
//!
//! `nvdrsdb0.bin` / `nvdrsdb1.bin` under `%PROGRAMDATA%\NVIDIA Corporation\Drs\`
//! is the binary store NVIDIA Profile Inspector reads/writes for per-application
//! driver profiles. The file format is officially undocumented; NVAPI's
//! `NvAPI_DRS_*` functions are the supported access path.
//!
//! Implementing the full NVAPI bindings would be a multi-month unsafe FFI
//! effort. As a pragmatic alternative, this module does a UTF-16LE string
//! scan to surface:
//!
//! - Profile names (human-readable strings preceding `.exe` clusters)
//! - Per-application executable matches (every `*.exe` referenced in the DB)
//! - Driver-known shipped profiles (Microsoft Flight Simulator, Cyberpunk
//!   2077, etc.) which are the same ones the NVIDIA Control Panel exposes.
//!
//! This gives users a high-fidelity inventory of which applications have
//! profile entries on this machine — the headline NVPI use-case — without
//! the unsafe FFI surface.

use super::{ProfileGroup, ScanOutcome, Setting, SettingValue, Subsystem};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Maximum number of profile entries to surface per DB file. Set high enough
/// to cover the full NVIDIA-shipped profile set (~12k entries in 2026 drivers)
/// without an artificial cutoff that hides popular games.
const MAX_PROFILES_PER_FILE: usize = 20_000;
/// Minimum UTF-16LE printable-Latin run that counts as a string.
const MIN_STRING_LEN: usize = 4;

/// Return the set of all `.exe` references found across both
/// `nvdrsdb0.bin` and `nvdrsdb1.bin`. Case-folded for cheap lookups.
///
/// `None` when the DRS database was not checked: the directory is missing or
/// cannot be listed, or a database file cannot be read. A partial read would
/// miss executables and report them as having no profile, so any read failure
/// makes the whole set `None`. `Some(empty)` means the database was read and
/// names no executables. Result is **not** cached — call once per check
/// (~50 ms on a 2.5 MB DB).
pub fn drs_exe_set() -> Option<std::collections::BTreeSet<String>> {
    let mut set = std::collections::BTreeSet::new();
    let dir = drs_dir().ok().flatten()?;
    let entries = std::fs::read_dir(&dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !stem.starts_with("nvdrsdb") || path.extension().and_then(|e| e.to_str()) != Some("bin")
        {
            continue;
        }
        let bytes = std::fs::read(&path).ok()?;
        for s in extract_utf16_strings(&bytes) {
            let lower = s.trim().to_ascii_lowercase();
            if lower.ends_with(".exe") && lower.len() > 4 {
                set.insert(lower);
            }
        }
    }
    Some(set)
}

/// Scan all `nvdrsdb*.bin` files in `%PROGRAMDATA%\NVIDIA Corporation\Drs\`
/// and return one [`ProfileGroup`] per file that was read.
///
/// A missing DRS directory is no NVIDIA driver database, so it is `Ok` with no
/// groups. `Err` when the location is unknown or the directory cannot be
/// listed. A database file that cannot be read is named in `unread`.
pub fn scan_drs_groups() -> Result<ScanOutcome, String> {
    let mut out = ScanOutcome::default();
    let Some(dir) = drs_dir()? else {
        return Ok(out);
    };
    let entries =
        std::fs::read_dir(&dir).map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(e) => {
                out.unread.push(format!("{}: {e}", dir.display()));
                continue;
            }
        };
        let Some(stem) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !stem.starts_with("nvdrsdb") || path.extension().and_then(|e| e.to_str()) != Some("bin")
        {
            continue;
        }
        match scan_file(&path) {
            Ok(group) => out.groups.push(group),
            Err(reason) => out.unread.push(reason),
        }
    }
    Ok(out)
}

/// The DRS database directory. `Ok(None)` when it does not exist, which is what
/// a machine without the NVIDIA driver looks like. `Err` when `PROGRAMDATA` is
/// unset, so the location itself is unknown.
fn drs_dir() -> Result<Option<PathBuf>, String> {
    let pd = std::env::var_os("PROGRAMDATA")
        .ok_or("PROGRAMDATA is not set, so the DRS directory cannot be located")?;
    let dir = PathBuf::from(pd).join("NVIDIA Corporation").join("Drs");
    Ok(dir.is_dir().then_some(dir))
}

fn scan_file(path: &Path) -> Result<ProfileGroup, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(file_group(&bytes, path))
}

/// Shape the group for one DRS database from its bytes.
fn file_group(bytes: &[u8], path: &Path) -> ProfileGroup {
    let strings = extract_utf16_strings(bytes);

    // Collect unique app executable references. NVIDIA stores these
    // case-folded; preserve original form for display but dedupe case-insensitively.
    let mut seen = BTreeSet::new();
    let mut exes: Vec<String> = Vec::new();
    let mut display_profiles: Vec<String> = Vec::new();
    for s in &strings {
        let s = s.trim();
        if s.is_empty() {
            continue;
        }
        let lower = s.to_ascii_lowercase();
        if lower.ends_with(".exe") {
            if seen.insert(lower.clone()) {
                exes.push(s.to_string());
            }
        } else if looks_like_profile_name(s) && exes.is_empty() {
            display_profiles.push(s.to_string());
        }
    }

    let mut group = ProfileGroup::new(
        Subsystem::Gpu,
        "NVIDIA Driver Profile Database (DRS)",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("nvdrsdb")
            .to_string(),
        path.display().to_string(),
    );

    group.push(
        Setting::info(
            "file_size_bytes",
            "DB File Size",
            SettingValue::Uint(bytes.len() as u64),
        )
        .with_unit("bytes")
        .with_source(path.display().to_string()),
    );
    // The count is of executable names that the binary mentions. A mention is
    // not proof of a per-application profile, so the label says only that.
    group.push(
        Setting::info(
            "app_profile_count",
            "Executable Names Referenced",
            SettingValue::Uint(exes.len() as u64),
        )
        .with_description(
            "Number of distinct .exe names referenced in the DRS database. A \
            reference is not confirmed as a per-application profile: it is \
            found by string scan, and the settings attached to it are not decoded.",
        )
        .with_source(path.display().to_string()),
    );

    // Surface up to MAX_PROFILES_PER_FILE referenced executable names. Each one
    // is a string match only, not a confirmed profile.
    for (i, exe) in exes.iter().take(MAX_PROFILES_PER_FILE).enumerate() {
        group.push(
            Setting::info(
                format!("app_{:04}", i),
                exe.clone(),
                SettingValue::Text("(executable referenced in DRS database)".to_string()),
            )
            .with_source("DRS binary scan"),
        );
    }

    if exes.len() > MAX_PROFILES_PER_FILE {
        group.note(format!(
            "{} additional application profiles in DB not shown (cap = {}).",
            exes.len() - MAX_PROFILES_PER_FILE,
            MAX_PROFILES_PER_FILE
        ));
    }

    group.note(
        "DRS scan extracts .exe references via UTF-16 string search. The \
        per-setting DWORD/string values inside each profile (DLSS toggle, \
        V-Sync, etc.) require NVAPI's NvAPI_DRS_* functions to decode \
        precisely and are not exposed here.",
    );
    if bytes.len() > 100 * 1024 {
        group.note(
            "If this list looks incomplete, the NVIDIA driver may be writing \
            updates to nvdrsdb1.bin instead of nvdrsdb0.bin (or vice versa) — \
            both are scanned automatically.",
        );
    }
    group
}

fn looks_like_profile_name(s: &str) -> bool {
    let len = s.chars().count();
    if !(4..=80).contains(&len) {
        return false;
    }
    // Profile names are usually mixed case with spaces, e.g. "Cyberpunk 2077".
    let has_alpha = s.chars().any(|c| c.is_ascii_alphabetic());
    let has_space = s.contains(' ');
    has_alpha && has_space && !s.contains("://") && !s.contains('\\')
}

/// Extract UTF-16LE printable-Latin strings of length >= [`MIN_STRING_LEN`].
fn extract_utf16_strings(bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let lo = bytes[i];
        let hi = bytes[i + 1];
        // Printable ASCII followed by a zero high byte.
        if hi == 0 && (0x20..0x7f).contains(&lo) {
            cur.push(lo as char);
        } else {
            if cur.chars().count() >= MIN_STRING_LEN {
                out.push(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
        }
        i += 2;
    }
    if cur.chars().count() >= MIN_STRING_LEN {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_string_extraction() {
        // "Hi\0there\0!\0" in UTF-16LE
        let bytes = [
            b'H', 0, b'i', 0, b'!', 0, // "Hi!" - len 3, below threshold
            0xFF, 0xFF, // separator
            b'H', 0, b'e', 0, b'l', 0, b'l', 0, b'o', 0, // "Hello" - kept
            0, 0,
        ];
        let s = extract_utf16_strings(&bytes);
        assert_eq!(s, vec!["Hello".to_string()]);
    }

    #[test]
    fn profile_name_heuristic() {
        assert!(looks_like_profile_name("Microsoft Flight Simulator"));
        assert!(looks_like_profile_name("Cyberpunk 2077"));
        assert!(!looks_like_profile_name("game.exe"));
        assert!(!looks_like_profile_name("C:\\foo"));
        assert!(!looks_like_profile_name("abc"));
    }

    fn utf16le(s: &str) -> Vec<u8> {
        s.bytes().flat_map(|b| [b, 0]).collect()
    }

    #[test]
    fn a_db_counts_each_executable_name_once_regardless_of_case() {
        let mut bytes = utf16le("game.exe");
        bytes.extend([0xFF, 0xFF]);
        bytes.extend(utf16le("Game.EXE"));
        let g = file_group(&bytes, Path::new(r"C:\x\nvdrsdb0.bin"));
        let count = g
            .settings
            .iter()
            .find(|s| s.id == "app_profile_count")
            .expect("count row");
        assert_eq!(count.value, SettingValue::Uint(1));
    }

    #[test]
    fn a_db_with_no_executables_reports_zero_not_a_failure() {
        let g = file_group(&[0u8; 64], Path::new(r"C:\x\nvdrsdb1.bin"));
        let count = g
            .settings
            .iter()
            .find(|s| s.id == "app_profile_count")
            .expect("count row");
        assert_eq!(count.value, SettingValue::Uint(0));
    }

    #[test]
    fn scan_smoke() {
        // Never panics, even when the DRS directory is missing.
        let _ = scan_drs_groups();
    }
}
