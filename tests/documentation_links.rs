//! Every relative link and image in the repository's markdown must resolve.
//!
//! The README carried six `docs/images/*.png` references for months. None of the
//! files was ever added, so the two "Screenshots" sections rendered as broken
//! images on GitHub — and, once the crate was published, on its crates.io page
//! too. Nothing caught it because a broken *image* fails silently: the markdown is
//! valid, the build is green, and the damage is only visible to a human looking at
//! the rendered page, which is exactly the thing nobody re-reads.
//!
//! A missing target is checkable, so it should be checked rather than noticed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Markdown files tracked by git, so generated and vendored trees are skipped.
///
/// Returns `None` when there is no git checkout to ask — the published `.crate`
/// tarball and vendored builds have no `.git`, and these two tests are about
/// repository hygiene rather than about anything the library does at runtime.
/// Failing there would make `cargo test` on a packaged copy fail for a reason the
/// person running it cannot act on.
fn tracked_markdown() -> Option<Vec<PathBuf>> {
    let root = repo_root();
    let output = Command::new("git")
        .args(["ls-files", "*.md"])
        .current_dir(&root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| root.join(line))
            .collect(),
    )
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Pull the target out of every `[text](target)` and `![alt](target)`.
fn link_targets(markdown: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let bytes: Vec<char> = markdown.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == ']' && i + 1 < bytes.len() && bytes[i + 1] == '(' {
            let start = i + 2;
            let mut end = start;
            while end < bytes.len() && bytes[end] != ')' && bytes[end] != '\n' {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == ')' {
                targets.push(bytes[start..end].iter().collect());
            }
            i = end;
        }
        i += 1;
    }
    targets
}

/// A target IronMonitor is responsible for: not a URL, not a bare anchor.
fn is_local(target: &str) -> bool {
    !target.starts_with("http://")
        && !target.starts_with("https://")
        && !target.starts_with("mailto:")
        && !target.starts_with('#')
        && !target.is_empty()
}

#[test]
fn every_relative_link_and_image_resolves() {
    let Some(files) = tracked_markdown() else {
        eprintln!("skipping: not a git checkout, so there is no file list to check");
        return;
    };
    // Inside a checkout the list must be non-empty, or the assertions below run
    // over nothing and report success without having examined anything.
    assert!(
        !files.is_empty(),
        "git reported no tracked markdown; the checker would pass vacuously"
    );

    let mut broken = Vec::new();
    let mut checked = 0usize;

    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        // Resolve relative to the file that contains the link, not to the working
        // directory. An earlier version of this check got that wrong and reported
        // a file that existed as missing, which is worse than no check at all —
        // it produced a confident, wrong claim that was then written down.
        let dir = file.parent().unwrap_or_else(|| Path::new("."));

        for target in link_targets(&text) {
            if !is_local(&target) {
                continue;
            }
            // Strip any `#anchor`; the file is what must exist.
            let path_part = target.split('#').next().unwrap_or(&target);
            if path_part.is_empty() {
                continue;
            }
            checked += 1;
            if !dir.join(path_part).exists() {
                broken.push(format!(
                    "{} -> {target}",
                    file.strip_prefix(repo_root()).unwrap_or(file).display()
                ));
            }
        }
    }

    assert!(
        broken.is_empty(),
        "{} of {checked} relative links do not resolve:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// Documentation must not carry identifiers belonging to whoever built it.
///
/// The TUI capture embedded in the README is real output, and real output from
/// this tool includes the machine's hostname. Sanitizing it was a manual step, and
/// manual steps are the ones that get skipped the next time someone refreshes the
/// capture.
#[test]
fn documentation_carries_no_machine_identifiers() {
    // Identifiers observed on the machine this was authored on. The point is not
    // this particular list — it is that pasting real tool output into docs is a
    // routine way to leak one, so the routine gets a check.
    const IDENTIFIERS: &[&str] = &["heimdall", "adamm"];

    let Some(files) = tracked_markdown() else {
        eprintln!("skipping: not a git checkout, so there is no file list to check");
        return;
    };
    assert!(
        !files.is_empty(),
        "git reported no tracked markdown; the checker would pass vacuously"
    );

    let mut found = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let lowered = text.to_lowercase();
        for id in IDENTIFIERS {
            if lowered.contains(id) {
                found.push(format!(
                    "{} contains {id:?}",
                    file.strip_prefix(repo_root()).unwrap_or(&file).display()
                ));
            }
        }
    }

    assert!(
        found.is_empty(),
        "documentation leaks a machine identifier:\n  {}",
        found.join("\n  ")
    );
}

/// Every `ironmon …` invocation shown in the documentation must be a real command.
///
/// The README's main usage block documented eight commands that do not exist —
/// `ironmon cpu`, `ironmon gpu`, `ironmon audio`, `ironmon displays` and others, all of
/// which live under `ironmon cli`. It contradicted itself ten lines later, where the
/// watch-mode examples used the correct `ironmon cli audio --watch`. `docs/UTILITIES.md`
/// had `ironmon all` the same way.
///
/// Anyone following the quick-start hit `error: unrecognized subcommand` on their
/// first command. Nothing caught it because documentation is not compiled — but
/// the binary can be asked what it accepts, so the two can be compared.
#[test]
fn every_documented_command_exists() {
    let Some(files) = tracked_markdown() else {
        eprintln!("skipping: not a git checkout, so there is no file list to check");
        return;
    };

    let catalog = Command::new(env!("CARGO_BIN_EXE_ironmon"))
        .args(["describe", "--commands", "--format", "json"])
        .output()
        .expect("ironmon describe should run");
    assert!(
        catalog.status.success(),
        "`ironmon describe --commands` failed; the catalog is the source of truth here"
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&catalog.stdout).expect("the catalog should be JSON");

    // Flatten the tree into the set of accepted paths: "cli", "cli cpu", "ai query".
    let mut valid = BTreeSet::new();
    fn walk(node: &serde_json::Value, prefix: &str, out: &mut BTreeSet<String>) {
        let Some(subs) = node.get("subcommands").and_then(|s| s.as_array()) else {
            return;
        };
        for sub in subs {
            let Some(name) = sub.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix} {name}")
            };
            out.insert(path.clone());
            walk(sub, &path, out);
        }
    }
    walk(&catalog, "", &mut valid);
    assert!(
        !valid.is_empty(),
        "the command catalog is empty; this check would pass vacuously"
    );

    let mut broken = Vec::new();
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for line in text.lines() {
            // Only lines that *are* an invocation. Prose mentioning ironmon in the
            // middle of a sentence is not a command and must not be flagged.
            let Some(rest) = line.trim_start().strip_prefix("ironmon ") else {
                continue;
            };
            // `ironmon [OPTIONS] [COMMAND]` is a synopsis, not an invocation — the
            // brackets are the giveaway, and clap prints exactly that shape.
            let tokens: Vec<&str> = rest
                .split_whitespace()
                .take_while(|t| !t.starts_with('-') && !t.starts_with('['))
                .collect();
            if tokens.is_empty() {
                continue;
            }
            // Accept the longest prefix that resolves; `ironmon cli cpu --watch`
            // is fine, and so is `ironmon get some.entity.id` where `get` resolves
            // and the rest is an argument.
            let resolves = (1..=tokens.len())
                .rev()
                .any(|n| valid.contains(&tokens[..n].join(" ")));
            if !resolves {
                broken.push(format!(
                    "{}: ironmon {}",
                    file.strip_prefix(repo_root()).unwrap_or(file).display(),
                    tokens.join(" ")
                ));
            }
        }
    }

    assert!(
        broken.is_empty(),
        "documentation shows commands IronMonitor does not accept:\n  {}",
        broken.join("\n  ")
    );
}

/// An HTTP path named in documentation must be a path the server routes.
///
/// `ironmon serve --help` said "Prometheus metrics are at /metrics/prometheus
/// (not /metrics — that route returns JSON)". Both halves were wrong:
/// `/metrics/prometheus` is a 404, and so is `/metrics`. The real path is
/// `/api/v1/metrics/prometheus`, which the server's own startup banner and
/// `grafana/README.md` both got right.
///
/// The mechanism is worth knowing, because it will recur: `routes::
/// METRICS_PROMETHEUS` is the string `"/metrics/prometheus"`, and the
/// dispatcher only compares it after stripping the `/api/v1` prefix. Reading
/// the constant and documenting it verbatim gives a path that does not exist,
/// and nothing about the constant says so.
///
/// A help string is the one place a user has no way to check a claim before
/// acting on it — they run the command it names and get nothing.
#[test]
fn documented_http_paths_match_the_route_table() {
    use ironmonlib::observability::server::routes;

    let prometheus = format!("{}{}", routes::API_V1, routes::METRICS_PROMETHEUS);

    let mut sources: Vec<PathBuf> = vec![repo_root().join("src/bin/main.rs")];
    if let Some(markdown) = tracked_markdown() {
        sources.extend(markdown);
    }

    let mut wrong = Vec::new();
    for path in &sources {
        // HANDOFF.md is a record of defects, so it quotes wrong values on
        // purpose — including this one, three times, while explaining that it
        // is wrong. It is excluded from the published crate and instructs
        // nobody, so a mention there is a citation rather than a claim.
        //
        // This exemption exists because the test failed CI on exactly that:
        // the writeup was added after the last local run, which is its own
        // lesson. HANDOFF.md is an input to the suite now, and editing it
        // after the gate invalidates the gate.
        if path.file_name().is_some_and(|n| n == "HANDOFF.md") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            // Every mention of the Prometheus endpoint must be the whole path.
            // A bare "/metrics/prometheus" is the constant without its prefix,
            // which is exactly the mistake being guarded against.
            for (idx, _) in line.match_indices("/metrics/prometheus") {
                let full = line[..idx].ends_with(routes::API_V1);
                if !full {
                    wrong.push(format!(
                        "{}:{}: names /metrics/prometheus; the served path is {}",
                        path.strip_prefix(repo_root()).unwrap_or(path).display(),
                        n + 1,
                        prometheus
                    ));
                }
            }
        }
    }

    assert!(
        wrong.is_empty(),
        "documentation names an HTTP path the server does not route:\n  {}",
        wrong.join("\n  ")
    );
}
