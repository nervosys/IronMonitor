//! Every PowerShell this crate starts must skip the user's profile.
//!
//! Without `-NoProfile`, `powershell.exe` runs the user's profile script before
//! the command. On the machine this was found on that took a start from 244 ms
//! to 1,067 ms, and ten spawns paid it -- four of them inside the boot-time
//! reader that every ontology snapshot ran. It is also a correctness problem:
//! anything a profile prints lands on stdout, where the callers parse numbers
//! and JSON.
//!
//! The command-helper spawns in `core::command` already passed the flag; these
//! did not. The scan covers both `Command::new("powershell")` and calls through
//! `core::command::capture*`.

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

/// The text of the call starting at `at`: up to the point the command runs,
/// or the end of the argument list for a `capture*` helper.
fn call_text(text: &str, at: usize) -> &str {
    let rest = &text[at..];
    let end = [".output()", ".spawn()", ".status()", "])"]
        .iter()
        .filter_map(|m| rest.find(m))
        .min()
        .unwrap_or(rest.len().min(400));
    &rest[..end]
}

#[test]
fn every_powershell_spawn_skips_the_profile() {
    let mut paths = Vec::new();
    rust_sources(Path::new("src"), &mut paths);
    let mut missing = Vec::new();
    for path in paths {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for needle in [
            "Command::new(\"powershell\")",
            "Command::new(\"powershell.exe\")",
            "(\"powershell\",",
            "(\n                \"powershell\",",
        ] {
            let mut from = 0;
            while let Some(pos) = text[from..].find(needle) {
                let at = from + pos;
                from = at + needle.len();
                let line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
                // Doc comments quote the old shape on purpose.
                if text[line_start..].trim_start().starts_with("//") {
                    continue;
                }
                if !call_text(&text, at).contains("-NoProfile") {
                    let line = text[..at].matches('\n').count() + 1;
                    missing.push(format!("{}:{line}", path.display()));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "PowerShell started without -NoProfile, which runs the user's profile first: {missing:?}"
    );
}
