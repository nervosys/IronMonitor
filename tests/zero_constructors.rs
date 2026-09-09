// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Pin the set of `new()` constructors that fabricate readings.
//!
//! This crate's central claim is that absent, unavailable and zero are different
//! facts. A `new()` that returns `0` for a measurement quietly converts the first
//! two into the third, and does it behind a name that reads like "construct this
//! properly".
//!
//! It has cost twice. `MemoryStats::new()` returns all zeros and `CpuStats::new()`
//! returns no cores and 100% idle; two GUI call sites shipped those numbers to
//! users as readings, found a day apart. The handoff's response was a note
//! telling the next reader to *assume any other `T::new()` may be one until
//! checked*. Nobody had checked, and a note is not a check — which is how the
//! same shape survived in `PowerStats` and `ProcessStats` without anyone
//! recording it.
//!
//! So this is the check, and it runs. Every argument-free `new()` in the crate is
//! classified, and the ones that fabricate numbers are compared against a known
//! list. Adding another fails here until it is deliberately acknowledged.
//!
//! **This test does not say these four are bugs.** All four are currently used
//! only as builder bases inside platform readers, which fill them immediately —
//! the legitimate use. What it says is that the list is exactly this long, so a
//! fifth arrival is noticed by CI rather than by a user reading 0 W off a
//! dashboard.

use std::collections::BTreeSet;
use std::path::Path;

/// Constructors known to return fabricated numbers rather than readings.
///
/// To add an entry you must be able to say why a caller cannot mistake the value
/// for a measurement. The honest fix for all of these is a rename — `empty()` or
/// `zeroed()` — which is breaking and is queued for the next major version.
const KNOWN_FABRICATORS: &[&str] = &[
    // Empty, and that is the finished state rather than a starting one.
    //
    // It held four entries until 6.0.0: CpuStats, MemoryStats, PowerStats and
    // ProcessStats all returned fabricated numbers from a method called `new()`.
    // They are now `empty()`, which says what they do, and no `new()` in this
    // crate fabricates a reading.
    //
    // The rename was not cosmetic. Going to do it turned up three live defects
    // that the misleading name had been hiding: `UnifiedMonitor::snapshot_cpu`
    // and `snapshot_memory` returned zeros from the public API, the health
    // checks computed CPU usage from 100% idle so they could never fire, and the
    // Prometheus exporter published 0% CPU on every scrape.
    //
    // Adding an entry here means a `new()` fabricates again. Do not: rename it.
];

/// Calls that do not count as reading anything: containers, wrappers, conversions.
const INERT_CALLS: &[&str] = &[
    "Ok",
    "Some",
    "None",
    "Self",
    "new",
    "default",
    "Default",
    "String",
    "Vec",
    "HashMap",
    "BTreeMap",
    "HashSet",
    "BTreeSet",
    "to_string",
    "into",
    "from",
    "unwrap_or_else",
    "unwrap_or_default",
    "with_capacity",
    "clone",
    "PhantomData",
];

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Remove comments and string bodies so their contents cannot be mistaken for
/// code. Without this, a doc comment mentioning `read_cpu_stats()` makes an inert
/// constructor look like it reads something.
fn strip_noise(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let rest_two = (bytes[i], bytes.get(i + 1).copied().unwrap_or('\0'));
        match rest_two {
            ('/', '/') => {
                while i < bytes.len() && bytes[i] != '\n' {
                    i += 1;
                }
            }
            ('/', '*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == '*' && bytes[i + 1] == '/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            ('"', _) => {
                out.push_str("\"\"");
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == '\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == '"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            _ => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    out
}

/// The `{...}` block starting at or after `from`.
fn brace_body(src: &str, from: usize) -> Option<String> {
    let bytes: Vec<char> = src.chars().collect();
    let mut i = src[..from].chars().count();
    while i < bytes.len() && bytes[i] != '{' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let start = i;
    let mut depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(bytes[start..=i].iter().collect());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Does this body assign a bare number or `false` to a field?
///
/// An empty `Vec` or `HashMap` is a container with nothing in it, which is
/// honest. `power: 0` is a claim that the draw was measured at zero.
///
/// **Known false negatives, so nobody reads a pass here as a proof.** A literal
/// is required, so `idle: DEFAULT_IDLE` escapes even though a named constant
/// fabricates exactly as effectively as `100.0` does. `true` is not flagged
/// either — only `false` — because a struct of flags set true is usually a
/// builder rather than a fabricated reading, and flagging every one of them
/// would bury the signal.
///
/// This narrows 150 constructors to a list a person can read. It does not
/// certify the remainder, and the four it does find were confirmed by reading
/// them.
fn fabricates_a_number(body: &str) -> bool {
    for (i, c) in body.char_indices() {
        if c != ':' {
            continue;
        }
        // Skip `::` paths.
        if body[i + 1..].starts_with(':') {
            continue;
        }
        let rest = body[i + 1..].trim_start();
        let value: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || c.is_alphabetic() || *c == '_')
            .collect();
        if value.is_empty() {
            continue;
        }
        let terminated = rest[value.len()..].trim_start();
        if !(terminated.starts_with(',') || terminated.starts_with('}')) {
            continue;
        }
        let numeric = value.chars().next().is_some_and(|c| c.is_ascii_digit());
        if numeric || value == "false" {
            return true;
        }
    }
    false
}

fn calls_something_real(body: &str) -> bool {
    let inert: BTreeSet<&str> = INERT_CALLS.iter().copied().collect();
    let chars: Vec<char> = body.chars().collect();
    for i in 0..chars.len() {
        if chars[i] != '(' {
            continue;
        }
        // Walk back over the identifier immediately before the paren.
        let mut j = i;
        while j > 0 && (chars[j - 1].is_alphanumeric() || chars[j - 1] == '_') {
            j -= 1;
        }
        if j < i {
            let name: String = chars[j..i].iter().collect();
            if !inert.contains(name.as_str()) {
                return true;
            }
        }
    }
    false
}

#[test]
fn no_new_constructor_fabricates_readings_without_being_on_the_list() {
    let mut files = Vec::new();
    rust_files(Path::new("src"), &mut files);
    assert!(
        files.len() > 50,
        "the walk found only {} files; it is not looking where it thinks it is",
        files.len()
    );

    let mut found: BTreeSet<String> = BTreeSet::new();

    for path in &files {
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        let src = strip_noise(&raw);
        let mut search = 0usize;
        while let Some(rel) = src[search..].find("pub fn new(") {
            let at = search + rel;
            search = at + 11;
            // Argument-free only: a constructor handed data is a different thing.
            let after = &src[at + 11..];
            let Some(close) = after.find(')') else {
                continue;
            };
            if !after[..close].trim().is_empty() {
                continue;
            }
            let Some(body) = brace_body(&src, at + 11 + close) else {
                continue;
            };
            if calls_something_real(&body) {
                continue;
            }
            if fabricates_a_number(&body) {
                found.insert(path.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    let known: BTreeSet<String> = KNOWN_FABRICATORS.iter().map(|s| s.to_string()).collect();

    let unexpected: Vec<&String> = found.difference(&known).collect();
    assert!(
        unexpected.is_empty(),
        "new `new()` constructors return fabricated numbers: {unexpected:?}\n\
         \n\
         A `new()` that returns 0 for a measurement turns 'not read' into 'read \
         as zero', behind a name that reads like a proper constructor. That has \
         already shipped two defects in this crate.\n\
         \n\
         If the value genuinely cannot reach a caller as a reading — because it \
         is only ever a builder base filled immediately by a platform reader — \
         add the file to KNOWN_FABRICATORS with a comment saying so. If it can \
         reach a caller, that is the bug, and the list is not where it goes."
    );

    let vanished: Vec<&String> = known.difference(&found).collect();
    assert!(
        vanished.is_empty(),
        "these are on the fabricator list but no longer detected: {vanished:?}\n\
         If they were fixed, delete them from KNOWN_FABRICATORS. If the detector \
         stopped seeing them, it is broken and is now certifying nothing."
    );
}
