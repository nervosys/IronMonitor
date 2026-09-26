//! A helper that returns `Option` must not have its absence thrown away.
//!
//! The commonest defect in this crate's fabricated-reading sweep had one shape:
//! a reader helper returning `Option<u32>` -- `read_sysfs_u32`, `read_file_u32`,
//! `sysctl_u64` -- called as `helper(..).unwrap_or(0)`. The helper's own author
//! had already decided the value can be missing. The caller overruled that with
//! a number, and the number reached a user as a reading: a watchdog's pre-timeout
//! reported as deliberately disabled, a cooling device reported as not
//! throttling, a GPU clock reported as 0 MHz.
//!
//! This scans `src/` for that shape. It is the lowest-noise of the sweep's
//! searches, because the return type settles whether absence was possible and
//! leaves only one question: is the default a legitimate value at this site?
//! Each site where the answer was yes is listed below with its reason and an
//! exact count, so a new discard of an allowed helper in the same file still
//! fails.
//!
//! **Adding to the allowlist is a claim about that call site.** Read the default
//! and what it does before adding one; `HANDOFF.md` records how each entry below
//! was judged.
//!
//! **What this does not see.** It matches the *direct* form, the helper's
//! closing parenthesis followed by the discard. A discard that happens after a
//! closure boundary -- `.and_then(|p| { ..; helper(p) }).unwrap_or(-1)` -- is
//! not adjacent and is missed; `dma_engine/mod.rs`'s first `numa_node` read is
//! one. Every instance the sweep actually fixed was the direct form, which is
//! why this is still worth having, but a clean run is not proof there are none.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Integer and float primitives: the types a reading is carried in.
const NUMERIC: [&str; 12] = [
    "u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize", "f32", "f64",
];

/// Names shared with standard-library methods that return `Option` because a
/// *sequence* may be empty -- `iter().max()`, `slice.last()`. That is a different
/// proposition from a failed read, and when called as a method these are almost
/// always the std one, so method-call uses of these names are not scanned.
const STD_NAMES: [&str; 8] = [
    "max", "min", "first", "last", "get", "next", "nth", "latest",
];

/// Names of every `fn` in `src/` whose return type is `Option<numeric>`.
fn option_numeric_helpers(files: &[(PathBuf, String)]) -> Vec<String> {
    let mut names = Vec::new();
    for (_, text) in files {
        let mut rest = text.as_str();
        while let Some(i) = rest.find("fn ") {
            let after = &rest[i + 3..];
            let name: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            // The signature runs to the body or to the end of a trait method.
            let sig_end = after.find(['{', ';']).unwrap_or(after.len());
            let sig: String = after[..sig_end].split_whitespace().collect();
            if !name.is_empty() {
                if let Some(ret) = sig.split("->").nth(1) {
                    if let Some(inner) = ret
                        .strip_prefix("Option<")
                        .and_then(|r| r.strip_suffix('>'))
                    {
                        if NUMERIC.contains(&inner) {
                            names.push(name);
                        }
                    }
                }
            }
            rest = &after[sig_end.min(after.len())..];
        }
    }
    names.sort();
    names.dedup();
    names
}

/// The index just past the `)` matching the `(` at `open`, or `None`.
///
/// Balanced rather than "anything up to the next `;`": that looser pattern ran
/// across struct literals, which contain no semicolons, and matched an unrelated
/// `.unwrap_or_default()` several fields away.
fn close_paren(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (k, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `text` at `from`, after whitespace, discards an absence: an
/// `.unwrap_or(` whose argument is a literal, or `.unwrap_or_default()`.
fn discards_absence(text: &str, from: usize) -> bool {
    let tail = text[from..].trim_start();
    if tail.starts_with(".unwrap_or_default()") {
        return true;
    }
    let Some(arg) = tail.strip_prefix(".unwrap_or(") else {
        return false;
    };
    let arg = arg.trim_start();
    arg.starts_with(|c: char| c.is_ascii_digit() || c == '-')
}

/// `(file, helper) -> number of discarding call sites`.
fn discarded_absences() -> BTreeMap<(String, String), usize> {
    let mut paths = Vec::new();
    rust_sources(Path::new("src"), &mut paths);
    let files: Vec<(PathBuf, String)> = paths
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|t| (p, t)))
        .collect();
    let helpers = option_numeric_helpers(&files);

    let mut found = BTreeMap::new();
    for (path, text) in &files {
        let file = path.to_string_lossy().replace('\\', "/");
        let bytes = text.as_bytes();
        for helper in &helpers {
            let needle = format!("{helper}(");
            let mut from = 0;
            while let Some(pos) = text[from..].find(&needle) {
                let at = from + pos;
                from = at + needle.len();

                // A whole identifier, not the tail of a longer one.
                if at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_') {
                    continue;
                }
                let line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
                let line = text[line_start..].trim_start();
                // Neither a comment nor a definition can be a discarded call.
                if line.starts_with("//") || text[..at].ends_with("fn ") {
                    continue;
                }
                if at > 0 && bytes[at - 1] == b'.' && STD_NAMES.contains(&helper.as_str()) {
                    continue;
                }
                let open = at + helper.len();
                if let Some(end) = close_paren(bytes, open) {
                    if discards_absence(text, end) {
                        *found.entry((file.clone(), helper.clone())).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    found
}

/// Sites where discarding the absence was judged correct, with the reason.
/// Keyed by file and helper, with the exact number of such calls there.
const ALLOWED: &[(&str, &str, usize, &str)] = &[
    (
        "src/core/memory.rs",
        "swap_usage_percent",
        1,
        "`swap_usage_percent_or_zero` is named for its choice, so every caller opts in",
    ),
    (
        "src/disk/windows.rs",
        "pending_sectors",
        1,
        "feeds a downgrade-only check beside a Healthy verdict that rests on the drive's own read prediction",
    ),
    (
        "src/disk/windows.rs",
        "uncorrectable_sectors",
        1,
        "same downgrade-only check as pending_sectors",
    ),
    (
        "src/dma_engine/mod.rs",
        "read_sysfs_i32",
        1,
        "numa_node -1 is the kernel's own 'no affinity'; library API only, not in the ontology (a second read, behind a closure, is not seen by this scan)",
    ),
    (
        "src/gpu_topology/mod.rs",
        "read_sysfs_i32",
        1,
        "numa_node -1, as in dma_engine; library API only",
    ),
    (
        "src/pcie.rs",
        "read_hex_file",
        3,
        "PCI IDs; nothing outside pcie.rs consumes the module (fix if it gains a consumer: class 0 decodes as 'unclassified')",
    ),
    (
        "src/silicon/apple.rs",
        "get_sysctl_value",
        2,
        "zero feeds (0..0), giving an empty core list rather than a reported count; documented at the site",
    ),
    (
        "src/silicon/apple.rs",
        "get_gpu_cores",
        1,
        "feeds a private field that is written once and read nowhere",
    ),
    (
        "src/stats.rs",
        "sysctl_u64",
        1,
        "hw.logicalcpu exists on every Mac; zero would give an empty core list, not a count",
    ),
];

#[test]
fn no_option_returning_helper_has_its_absence_discarded() {
    let found = discarded_absences();
    let allowed: BTreeMap<(String, String), (usize, &str)> = ALLOWED
        .iter()
        .map(|(f, h, n, why)| ((f.to_string(), h.to_string()), (*n, *why)))
        .collect();

    let mut problems = Vec::new();
    for ((file, helper), n) in &found {
        match allowed.get(&(file.clone(), helper.clone())) {
            None => problems.push(format!(
                concat!(
                    "{}: `{}(..)` returns Option and {} call(s) discard it with a default. ",
                    "Carry the Option through, or -- if the default is right at that site -- ",
                    "add an entry to ALLOWED with the reason."
                ),
                file, helper, n
            )),
            Some((allowed_n, _)) if n > allowed_n => problems.push(format!(
                concat!(
                    "{}: `{}(..)` is discarded {} time(s); {} were judged correct. ",
                    "The new one needs its own judgement."
                ),
                file, helper, n, allowed_n
            )),
            _ => {}
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// An allowlist entry for a call that no longer exists is a claim about code
/// that is gone, and it would quietly admit the next discard of that helper.
#[test]
fn every_allowlist_entry_still_matches_its_call_sites() {
    let found = discarded_absences();
    let mut stale = Vec::new();
    for (file, helper, n, _) in ALLOWED {
        let have = found
            .get(&(file.to_string(), helper.to_string()))
            .copied()
            .unwrap_or(0);
        if have != *n {
            stale.push(format!(
                "{file}: `{helper}` is allowed {n} time(s) but found {have}; update ALLOWED"
            ));
        }
    }
    assert!(stale.is_empty(), "{}", stale.join("\n"));
}
