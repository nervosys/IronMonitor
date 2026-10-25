//! No reader starts `sudo`.
//!
//! AGENTS.md promises the library never prompts and never elevates itself.
//! A reader that spawns `sudo` breaks both halves at once: without cached
//! credentials it asks for a password on whatever terminal it inherited —
//! inside the TUI, or under an agent's tool call with nobody to answer — and
//! with them it runs as root on the strength of a grant the user made for
//! something else.
//!
//! The Apple Silicon monitor had one, `sudo powermetrics`, unreachable only
//! because nothing called it. `src/utils` is excluded: those are the
//! `jetson-utils` administrative commands (swap, clocks, power mode), a user
//! types each one, and SECURITY.md documents them as unsafe.

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

#[test]
fn no_reader_spawns_sudo() {
    let mut paths = Vec::new();
    rust_sources(Path::new("src"), &mut paths);
    assert!(paths.len() > 100, "the scan found {} sources", paths.len());

    let utils = Path::new("src").join("utils");
    let mut found = Vec::new();
    for path in paths.iter().filter(|p| !p.starts_with(&utils)) {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for needle in [
            "Command::new(\"sudo\")",
            "capture(\"sudo\"",
            "capture_with_timeout(\"sudo\"",
        ] {
            for (at, _) in text.match_indices(needle) {
                let line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
                if text[line_start..].trim_start().starts_with("//") {
                    continue;
                }
                let line = text[..at].matches('\n').count() + 1;
                found.push(format!("{}:{line}", path.display()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "a reader starts sudo, which can prompt and elevates the process: {found:?}"
    );
}

/// The control: the scan does see a `sudo` spawn where one exists.
///
/// The utilities reach sudo through `sudo_command()` in `security.rs`, the one
/// place that spawns it, so that is the file that still holds the literal.
#[test]
fn the_scan_sees_the_utils_it_excludes() {
    let text = std::fs::read_to_string("src/utils/security.rs").expect("sudo helper exists");
    assert!(text.contains("Command::new(\"sudo\")"));
}
