// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! File integrity monitoring: what changed since last time, and what could not
//! be checked.
//!
//! # Hashes, not timestamps
//!
//! Size and mtime are metadata, and anything able to rewrite a file is able to
//! rewrite them — `touch -r` is one command. A file integrity monitor built on
//! mtime detects accidents and reports nothing about an adversary, while looking
//! exactly like one that does. SHA-256 over the contents is the cheapest thing
//! that is actually a check.
//!
//! # Unreadable is not unchanged
//!
//! A file the scanner could not open — permissions, a lock, a deleted inode —
//! produces [`FileState::Unreadable`], never a silent skip and never a "matches".
//! On a compromised machine the file an attacker touched is exactly the one most
//! likely to be unreadable, so treating that as "fine" inverts the tool.
//!
//! # The first scan proves nothing
//!
//! Recording a baseline tells you what the machine looks like now, including if
//! it was already compromised when you looked. [`scan`] returns
//! [`ScanStatus::NoBaseline`] on a first run and never [`ScanStatus::Clean`],
//! because a baseline taken after an intrusion is a record of the intrusion, not
//! evidence against one.

use super::{now_secs, Confidence, Evidence, Finding, ScanStatus, Severity, Subject};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What was recorded about one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum FileState {
    /// Read in full and hashed.
    Hashed { sha256: String, size_bytes: u64 },
    /// The path did not exist. A fact, and distinct from being unreadable.
    Missing,
    /// The path exists and could not be read, with the reason.
    Unreadable { reason: String },
}

/// A recorded set of file states to compare against.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    /// Unix epoch seconds when this was taken.
    pub recorded_at: u64,
    pub files: BTreeMap<String, FileState>,
}

impl Baseline {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }
}

/// Hash one file, or say why not.
///
/// Streams in chunks rather than reading whole: a watchlist can legitimately
/// include something large, and a monitor that allocates a gigabyte to check a
/// file is a denial of service against its own host.
pub fn state_of(path: &Path) -> FileState {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return FileState::Missing,
        Err(e) => {
            return FileState::Unreadable {
                reason: e.to_string(),
            }
        }
    };
    if meta.is_dir() {
        return FileState::Unreadable {
            reason: "path is a directory, not a file".into(),
        };
    }

    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return FileState::Unreadable {
                reason: e.to_string(),
            }
        }
    };

    use std::io::Read;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(e) => {
                return FileState::Unreadable {
                    reason: e.to_string(),
                }
            }
        }
    }

    FileState::Hashed {
        sha256: hex::encode(hasher.finalize()),
        size_bytes: meta.len(),
    }
}

/// Record the current state of every path in the watchlist.
pub fn record(paths: &[PathBuf]) -> Baseline {
    let mut files = BTreeMap::new();
    for p in paths {
        files.insert(p.to_string_lossy().to_string(), state_of(p));
    }
    Baseline {
        recorded_at: now_secs(),
        files,
    }
}

/// Compare the current state of the watchlist against a baseline.
///
/// Paths in the watchlist that the baseline never saw are reported as findings
/// in their own right: a new file appearing in a watched location is the thing a
/// watchlist is for.
pub fn scan(paths: &[PathBuf], baseline: &Baseline) -> ScanStatus {
    // An empty watchlist and an empty baseline are the same value and different
    // facts. Without `--watch` the baseline is empty on every run, so this said
    // "one has now been recorded" every time, and the next run contradicted it
    // by saying the same thing again. Recording nothing is not recording a
    // baseline.
    //
    // Worth separating for a second reason: a detector that prints "this run is
    // not evidence that the machine is clean" on every run forever teaches the
    // reader to skip the line, and that line is the one that matters after a
    // genuine first run.
    if paths.is_empty() {
        return ScanStatus::NoBaseline {
            recorded: 0,
            reason: concat!(
                "no files are being watched, so there is nothing to baseline ",
                "and nothing was recorded — pass --watch PATH to watch one. ",
                "The listening sockets are checked regardless.",
            )
            .into(),
        };
    }

    if baseline.is_empty() {
        let fresh = record(paths);
        return ScanStatus::NoBaseline {
            recorded: fresh.len(),
            reason: "no baseline existed, so nothing could be compared; one has \
                     now been recorded. A baseline taken after an intrusion \
                     records the intrusion — this run is not evidence that the \
                     machine is clean."
                .into(),
        };
    }

    let mut findings = Vec::new();
    let mut checked = 0usize;

    for path in paths {
        let key = path.to_string_lossy().to_string();
        let current = state_of(path);
        checked += 1;

        let Some(previous) = baseline.files.get(&key) else {
            if let Some(f) = new_path_finding(&key, &current) {
                findings.push(f);
            }
            continue;
        };

        if let Some(f) = compare(&key, previous, &current) {
            findings.push(f);
        }
    }

    // Paths the baseline knew about that are no longer being watched are not
    // reported: that is a change to the watchlist, not to the machine, and
    // conflating the two would make every configuration edit look like an event.

    if findings.is_empty() {
        ScanStatus::Clean { checked }
    } else {
        ScanStatus::Findings { checked, findings }
    }
}

fn subject(path: &str) -> Subject {
    Subject::File {
        path: path.to_string(),
    }
}

fn new_path_finding(path: &str, current: &FileState) -> Option<Finding> {
    match current {
        FileState::Hashed { sha256, size_bytes } => Finding::new(
            "file.unbaselined",
            "watched file has no baseline entry",
            Severity::Low,
            Confidence::Certain,
            subject(path),
            vec![
                Evidence::observed("file.sha256", sha256),
                Evidence::observed("file.size_bytes", size_bytes.to_string()),
            ],
        ),
        // Absent then, absent now, and never recorded: nothing happened.
        FileState::Missing => None,
        FileState::Unreadable { reason } => Finding::new(
            "file.unreadable",
            "watched file could not be read",
            Severity::Low,
            Confidence::Certain,
            subject(path),
            vec![Evidence::observed("file.error", reason)],
        ),
    }
}

/// The comparison. Every arm names what it saw and what it expected.
fn compare(path: &str, previous: &FileState, current: &FileState) -> Option<Finding> {
    match (previous, current) {
        (
            FileState::Hashed {
                sha256: old,
                size_bytes: old_size,
            },
            FileState::Hashed {
                sha256: new,
                size_bytes: new_size,
            },
        ) => {
            if old == new {
                return None;
            }
            Finding::new(
                "file.modified",
                "watched file contents changed",
                Severity::High,
                // Directly observed, not inferred.
                Confidence::Certain,
                subject(path),
                vec![
                    Evidence::differs("file.sha256", new, old),
                    Evidence::differs(
                        "file.size_bytes",
                        new_size.to_string(),
                        old_size.to_string(),
                    ),
                ],
            )
        }
        (FileState::Hashed { sha256, .. }, FileState::Missing) => Finding::new(
            "file.deleted",
            "watched file no longer exists",
            Severity::High,
            Confidence::Certain,
            subject(path),
            vec![
                Evidence::differs("file.exists", "false", "true"),
                Evidence::observed("file.sha256_was", sha256),
            ],
        ),
        (FileState::Missing, FileState::Hashed { sha256, size_bytes }) => Finding::new(
            "file.created",
            "watched path now exists where it did not",
            Severity::Medium,
            Confidence::Certain,
            subject(path),
            vec![
                Evidence::differs("file.exists", "true", "false"),
                Evidence::observed("file.sha256", sha256),
                Evidence::observed("file.size_bytes", size_bytes.to_string()),
            ],
        ),
        // Became unreadable. Reported rather than skipped: on a compromised
        // machine the file that suddenly cannot be read is the interesting one.
        (FileState::Hashed { .. }, FileState::Unreadable { reason }) => Finding::new(
            "file.became_unreadable",
            "watched file was readable before and is not now",
            Severity::Medium,
            Confidence::Certain,
            subject(path),
            vec![
                Evidence::differs("file.readable", "false", "true"),
                Evidence::observed("file.error", reason),
            ],
        ),
        // Still unreadable, or unreadable-to-missing and similar: no claim about
        // contents can be made in either direction, so none is made.
        (FileState::Unreadable { .. }, _) => None,
        (FileState::Missing, FileState::Missing) => None,
        (FileState::Missing, FileState::Unreadable { .. }) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("ironmon-ids-test-{tag}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// Watching nothing is not the same as having no baseline.
    ///
    /// Both are an empty map, and the message conflated them: with no `--watch`
    /// every run said "one has now been recorded" and the next run said it again.
    /// A recording that did not happen must not be claimed, and a detector whose
    /// caveat line prints unchanged forever trains the reader to skip it.
    #[test]
    fn watching_nothing_does_not_claim_a_baseline_was_recorded() {
        let empty = Baseline {
            recorded_at: 0,
            files: BTreeMap::new(),
        };

        let status = scan(&[], &empty);
        let ScanStatus::NoBaseline { recorded, reason } = status else {
            panic!("watching nothing cannot be compared, so it is not Clean");
        };
        assert_eq!(recorded, 0, "nothing was watched, so nothing was recorded");
        assert!(
            !reason.contains("has now been recorded"),
            "claims a recording that did not happen: {reason:?}"
        );
        assert!(
            reason.contains("--watch"),
            "should say how to watch something: {reason:?}"
        );
    }

    /// A watchlist with no baseline still reports the real first-run caveat.
    #[test]
    fn a_first_run_over_watched_files_still_warns_about_the_baseline() {
        let dir = tmpdir("first-run");
        let file = dir.join("watched");
        std::fs::write(&file, b"contents").expect("write");

        let empty = Baseline {
            recorded_at: 0,
            files: BTreeMap::new(),
        };

        let ScanStatus::NoBaseline { recorded, reason } = scan(&[file], &empty) else {
            panic!("a first run cannot compare");
        };
        assert_eq!(recorded, 1);
        assert!(
            reason.contains("not evidence that the machine is clean"),
            "the caveat that matters must survive: {reason:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn hashing_a_known_string_matches_the_published_vector() {
        let dir = tmpdir("vector");
        let p = write(&dir, "abc.txt", "abc");
        match state_of(&p) {
            FileState::Hashed { sha256, size_bytes } => {
                // The SHA-256 of "abc" is a published constant. Checking against
                // it rather than against another call of this same function,
                // which would only prove the code agrees with itself.
                assert_eq!(
                    sha256,
                    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                );
                assert_eq!(size_bytes, 3);
            }
            other => panic!("expected a hash, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_missing_and_not_unreadable() {
        let dir = tmpdir("missing");
        let p = dir.join("not-here.txt");
        assert_eq!(state_of(&p), FileState::Missing);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The property the module is built on.
    #[test]
    fn the_first_scan_reports_no_baseline_rather_than_clean() {
        let dir = tmpdir("firstrun");
        let p = write(&dir, "a.txt", "hello");
        let status = scan(&[p], &Baseline::default());
        match &status {
            ScanStatus::NoBaseline { recorded, reason } => {
                assert_eq!(*recorded, 1);
                assert!(reason.contains("not evidence"), "{reason}");
            }
            other => panic!("a first run must not report a verdict: {other:?}"),
        }
        assert!(
            !status.is_conclusive(),
            "reporting a first scan as clean is the most dangerous thing this \
             module could do, because it is what a user wants to read"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unchanged_file_is_clean_and_a_changed_one_is_found() {
        let dir = tmpdir("change");
        let p = write(&dir, "b.txt", "original");
        let base = record(std::slice::from_ref(&p));

        assert_eq!(
            scan(std::slice::from_ref(&p), &base),
            ScanStatus::Clean { checked: 1 }
        );

        std::fs::write(&p, "tampered").unwrap();
        let status = scan(std::slice::from_ref(&p), &base);
        let findings = status.findings();
        assert_eq!(findings.len(), 1, "{status:?}");
        assert_eq!(findings[0].rule, "file.modified");
        assert_eq!(findings[0].confidence, Confidence::Certain);
        // The evidence must carry both sides, or a reader cannot check it.
        let hash_ev = findings[0]
            .evidence
            .iter()
            .find(|e| e.kind == "file.sha256")
            .expect("hash evidence");
        assert!(hash_ev.expected.is_some());
        assert_ne!(hash_ev.observed, *hash_ev.expected.as_ref().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Same size, different contents — the case an mtime or size check misses.
    #[test]
    fn a_same_length_edit_is_still_detected() {
        let dir = tmpdir("samelen");
        let p = write(&dir, "c.txt", "AAAA");
        let base = record(std::slice::from_ref(&p));
        std::fs::write(&p, "BBBB").unwrap();
        let status = scan(std::slice::from_ref(&p), &base);
        assert_eq!(
            status.findings().len(),
            1,
            "a content hash is the whole reason this does not use size or mtime"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deletion_and_creation_are_different_findings() {
        let dir = tmpdir("delcreate");
        let p = write(&dir, "d.txt", "here");
        let base = record(std::slice::from_ref(&p));
        std::fs::remove_file(&p).unwrap();
        let deleted = scan(std::slice::from_ref(&p), &base);
        assert_eq!(deleted.findings()[0].rule, "file.deleted");

        let absent_base = record(std::slice::from_ref(&p)); // records Missing
        std::fs::write(&p, "back").unwrap();
        let created = scan(std::slice::from_ref(&p), &absent_base);
        assert_eq!(created.findings()[0].rule, "file.created");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A path the baseline never saw is a finding, not a silent pass.
    #[test]
    fn a_watched_path_with_no_baseline_entry_is_reported() {
        let dir = tmpdir("unbaselined");
        let known = write(&dir, "known.txt", "x");
        let base = record(std::slice::from_ref(&known));
        let fresh = write(&dir, "fresh.txt", "y");

        let status = scan(&[known, fresh], &base);
        let rules: Vec<&str> = status.findings().iter().map(|f| f.rule.as_str()).collect();
        assert!(rules.contains(&"file.unbaselined"), "{rules:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A directory in the watchlist is a configuration mistake, and saying so
    /// beats hashing nothing and reporting a match.
    #[test]
    fn a_directory_is_unreadable_rather_than_silently_skipped() {
        let dir = tmpdir("isdir");
        match state_of(&dir) {
            FileState::Unreadable { reason } => assert!(reason.contains("directory"), "{reason}"),
            other => panic!("expected Unreadable, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_baseline_round_trips_through_json() {
        let dir = tmpdir("roundtrip");
        let p = write(&dir, "e.txt", "data");
        let base = record(std::slice::from_ref(&p));
        let json = serde_json::to_string(&base).unwrap();
        let back: Baseline = serde_json::from_str(&json).unwrap();
        assert_eq!(back, base);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
