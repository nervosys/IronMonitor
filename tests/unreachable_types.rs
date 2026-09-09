// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Every public type this crate declares must be mentioned by code somewhere.
//!
//! A type nobody constructs is a type nobody has audited. That is not a style
//! observation — it is where two of this crate's fabrications were still sitting
//! after the sweeps that removed every other one.
//!
//! `NetworkState` kept the bare `f64` rate fields that `e425908` took off every
//! other network rate in the crate, because the sweep looked for code that runs
//! and nothing constructed a `NetworkState`. When `ed4ec44` finally populated
//! `FullSystemState::network` — a public field, serialised by `ironmon cli all
//! --format json`, that had come back `[]` on a host with twenty interfaces —
//! the fabrication came with it and had to be fixed on the way through.
//!
//! The same audit found 391 of the 619 lines of `ai_api::types` unreachable: a
//! whole `*Details` family, published through `pub use types::*`, that no
//! function returns and whose shape matches nothing the tools emit. Its fields
//! were flat and non-`Option` — per-core clocks, power draw, temperatures, SMART
//! attributes — declaring as always-present exactly the readings this crate has
//! spent a hundred commits establishing are usually absent. Anyone wiring them
//! up would have imported the fabrication wholesale.
//!
//! So: no allowlist. A public `struct` or `enum` that appears nowhere but its own
//! declaration is either dead and should be deleted, or is a promise the crate
//! has not kept and should be wired up. Both are the author's decision to make
//! deliberately, which is what failing here asks for.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
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

/// Blank out comments so a mention in prose cannot pass for a use.
///
/// This crate has already written one guard that its own explanatory comment
/// satisfied. Every doc comment here names the type it documents, so without
/// this every type in the crate would look referenced.
fn strip_comments(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        match (chars[i], chars.get(i + 1).copied().unwrap_or('\0')) {
            ('/', '/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            ('/', '*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i = (i + 2).min(chars.len());
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Whole-word occurrences of `name` in `hay`.
fn mentions(hay: &str, name: &str) -> usize {
    let boundary = |c: Option<char>| !matches!(c, Some(c) if c.is_alphanumeric() || c == '_');
    let bytes = hay.as_bytes();
    let mut count = 0;
    let mut from = 0;
    while let Some(rel) = hay[from..].find(name) {
        let at = from + rel;
        let before = hay[..at].chars().next_back();
        let after = hay[at + name.len()..].chars().next();
        if boundary(before) && boundary(after) {
            count += 1;
        }
        from = at + name.len();
        if from >= bytes.len() {
            break;
        }
    }
    count
}

#[test]
fn every_public_type_is_referenced_by_something() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut sources = Vec::new();
    rust_files(&root.join("src"), &mut sources);
    let declaring = sources.len();
    for extra in ["tests", "examples", "benches"] {
        rust_files(&root.join(extra), &mut sources);
    }

    let texts: Vec<(PathBuf, String)> = sources
        .iter()
        .map(|p| {
            (
                p.clone(),
                strip_comments(&std::fs::read_to_string(p).unwrap_or_default()),
            )
        })
        .collect();

    // Declarations, from `src` only.
    let mut declared: BTreeMap<String, PathBuf> = BTreeMap::new();
    for (path, text) in texts.iter().take(declaring) {
        for line in text.lines() {
            let line = line.trim_start();
            for kind in ["pub struct ", "pub enum "] {
                if let Some(rest) = line.strip_prefix(kind) {
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !name.is_empty() {
                        declared.insert(name, path.clone());
                    }
                }
            }
        }
    }

    assert!(
        declared.len() > 500,
        "found only {} public types; the scan is broken, not the crate",
        declared.len()
    );

    let mut unreachable = Vec::new();
    for (name, home) in &declared {
        // Every mention anywhere, less the one declaration itself.
        let total: usize = texts
            .iter()
            .map(|(path, text)| {
                let n = mentions(text, name);
                if path == home {
                    n.saturating_sub(1)
                } else {
                    n
                }
            })
            .sum();
        if total == 0 {
            unreachable.push(format!(
                "  {} ({})",
                name,
                home.strip_prefix(root).unwrap_or(home).display()
            ));
        }
    }

    assert!(
        unreachable.is_empty(),
        "{} public type(s) appear nowhere but their own declaration.\n{}\n\n\
         Nothing constructs these, so nothing has ever checked their field types \
         against what the machine can actually read. Delete them, or wire them up \
         and audit them as you do.",
        unreachable.len(),
        unreachable.join("\n")
    );
}
