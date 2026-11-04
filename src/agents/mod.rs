//! Agent activity: what local coding agents, workflows and inference requests did.
//!
//! IronMonitor does not parse agent transcripts itself. The IronObserver tool
//! (`ironobs`) reads each producer's own session files and writes recordings, and
//! IronMonitor reads those recordings. Two steps stay separate:
//!
//! - **Reading** ([`scan_dir`], [`query`]). Recording files on disk are read here,
//!   metadata only (see [`recording`]). Reads are bounded, and `snapshot` and `get`
//!   may do them.
//! - **Importing** (see [`ironobs`]). When `ironobs` is on PATH, a background
//!   thread runs `ironobs import` at a long interval, writes the recordings
//!   directory, and holds a single-instance lock. Nothing else starts an import.
//!
//! Nothing in this module reads or writes an agent's own session folder beyond
//! what `ironobs` does when it is asked to import it. Session text never reaches
//! this module's types.
//!
//! Environment:
//! - `IRONMON_RECORDINGS_DIR` overrides where recordings are read and written.
//!   Default: `<cache>/agents/recordings`, where `<cache>` is `%LOCALAPPDATA%\ironmon`
//!   on Windows and `~/.cache/ironmon` elsewhere.
//! - `IRONMON_CLAUDE_CODE_DIR` overrides the Claude Code projects directory that is
//!   imported. Default: `~/.claude/projects`.

pub mod ironobs;
pub mod recording;
pub mod report;

use chrono::{DateTime, Utc};
use recording::{InProgress, Refusal, Session};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const RECORDINGS_ENV: &str = "IRONMON_RECORDINGS_DIR";
pub const CLAUDE_CODE_ENV: &str = "IRONMON_CLAUDE_CODE_DIR";

/// The most recording files one query will read. A snapshot must stay short, so
/// files past this are counted and not read.
pub const QUERY_MAX_FILES: usize = 64;
/// The most bytes one query will read across all files.
pub const QUERY_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// The directory recordings are read from, and that `ironobs` writes into.
pub fn recordings_dir() -> PathBuf {
    match std::env::var_os(RECORDINGS_ENV) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => cache_root().join("agents").join("recordings"),
    }
}

/// The Claude Code projects directory `ironobs import claude-code` reads.
pub fn claude_code_dir() -> PathBuf {
    match std::env::var_os(CLAUDE_CODE_ENV) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => home_dir().join(".claude").join("projects"),
    }
}

fn cache_root() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join("AppData").join("Local"));
        base.join("ironmon")
    }
    #[cfg(not(windows))]
    {
        home_dir().join(".cache").join("ironmon")
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// A coverage row: a producer IronMonitor reads, or one it does not, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Producer {
    /// The id segment: `agents.producer.<key>.sessions`.
    pub key: &'static str,
    pub label: &'static str,
    /// Why it is not read, when it is not. `None` for a producer `ironobs` reads.
    pub not_read: Option<&'static str>,
}

/// The producers this tab knows about, in the order the Coverage section shows them.
/// A producer that appears in a recording but is not listed here is still read, and
/// is shown under its own name.
pub const KNOWN_PRODUCERS: &[Producer] = &[
    Producer {
        key: "claude_code",
        label: "Claude Code",
        not_read: None,
    },
    Producer {
        key: "codex",
        label: "Codex CLI",
        not_read: None,
    },
    Producer {
        key: "gemini_cli",
        label: "Gemini CLI",
        not_read: Some(
            "not covered: Gemini CLI stores no chats on disk, so there is nothing to read",
        ),
    },
];

/// Remote and hosted agent activity: there is no readable source on this machine.
pub const REMOTE_NOT_READ: &str =
    "cloud and hosted agent sessions have no source readable from this machine";

/// Provider cost and usage live on the Inference tab, not here.
pub const REMOTE_COST_POINTER: &str =
    "provider cost and usage are on the Inference tab, from the provider's admin API";

/// The state of one directory scan.
#[derive(Debug, Clone, PartialEq)]
pub struct Scan {
    pub dir: PathBuf,
    /// Whether the directory exists at all. A missing directory is not an empty one.
    pub dir_exists: bool,
    /// Recording files read (or attempted) in this scan.
    pub files: usize,
    pub file_bytes: u64,
    /// Files that are not JSON of either recording shape. Each is skipped.
    pub corrupt_files: usize,
    /// Recordings refused, by reason, counted. Reasons are fixed strings.
    pub refused: BTreeMap<String, usize>,
    /// Recording files past the scan bound, counted and not read.
    pub files_truncated: usize,
    /// Every recording that was read, linked to its parents.
    pub sessions: Vec<Session>,
    pub taken_at: DateTime<Utc>,
}

impl Scan {
    /// A scan that has read nothing yet.
    pub fn empty(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            dir_exists: false,
            files: 0,
            file_bytes: 0,
            corrupt_files: 0,
            refused: BTreeMap::new(),
            files_truncated: 0,
            sessions: Vec::new(),
            taken_at: Utc::now(),
        }
    }

    /// Recordings read, in total.
    pub fn recordings(&self) -> usize {
        self.sessions.len()
    }

    /// Top-level sessions: recordings that are not a subagent of another.
    pub fn sessions_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| recording::is_top_level(s))
            .count()
    }

    /// Recordings that say they are still being written.
    pub fn active_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| s.in_progress == InProgress::True)
            .count()
    }

    /// Recordings that do not say whether they are still being written.
    pub fn unevidenced_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| s.in_progress == InProgress::NotEvidenced)
            .count()
    }

    pub fn subagent_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.is_subagent).count()
    }

    /// Top-level sessions that have subagent runs: the multi-agent workflows.
    pub fn workflow_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| recording::is_top_level(s) && s.child_count > 0)
            .count()
    }

    /// Recordings whose parent was not among the files read.
    pub fn orphan_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.parent_missing).count()
    }

    pub fn refused_count(&self) -> usize {
        self.refused.values().sum()
    }

    /// Sum of input tokens over recordings that report any, or `None` when none do.
    pub fn input_tokens(&self) -> Option<u64> {
        sum_opt(self.sessions.iter().map(|s| s.input_tokens))
    }

    pub fn output_tokens(&self) -> Option<u64> {
        sum_opt(self.sessions.iter().map(|s| s.output_tokens))
    }

    /// Recordings read for one producer key.
    pub fn producer_count(&self, key: &str) -> usize {
        self.sessions.iter().filter(|s| s.producer == key).count()
    }

    /// Distinct producers present, sorted.
    pub fn producers(&self) -> Vec<String> {
        let mut v: Vec<String> = self.sessions.iter().map(|s| s.producer.clone()).collect();
        v.sort();
        v.dedup();
        v
    }
}

fn sum_opt(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    values
        .flatten()
        .fold(None, |acc, v| Some(acc.unwrap_or(0).saturating_add(v)))
}

/// Limits for one scan. The background poller reads everything. A query reads a
/// bounded number of files, and says how many it did not read.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_files: usize,
    pub max_bytes: u64,
}

pub const UNBOUNDED: Limits = Limits {
    max_files: usize::MAX,
    max_bytes: u64::MAX,
};

/// Read every `.json` recording file in `dir`, within `limits`.
///
/// A corrupt file is counted and skipped; the rest are read. Files are read in name
/// order so the result is stable from one scan to the next.
pub fn scan_dir(dir: &Path, limits: Limits) -> Scan {
    let mut scan = Scan::empty(dir);
    let Ok(read) = std::fs::read_dir(dir) else {
        return scan;
    };
    scan.dir_exists = true;
    let mut paths: Vec<PathBuf> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("json"))
                && p.is_file()
        })
        .collect();
    paths.sort();

    let mut sessions = Vec::new();
    for path in paths {
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if scan.files >= limits.max_files || scan.file_bytes.saturating_add(len) > limits.max_bytes
        {
            scan.files_truncated += 1;
            continue;
        }
        scan.files += 1;
        scan.file_bytes = scan.file_bytes.saturating_add(len);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => {
                scan.corrupt_files += 1;
                continue;
            }
        };
        match recording::read_file(&bytes) {
            Ok(items) => {
                for item in items {
                    match item {
                        Ok(s) => sessions.push(s),
                        Err(refusal) => count_refusal(&mut scan.refused, &refusal),
                    }
                }
            }
            Err(_) => scan.corrupt_files += 1,
        }
    }
    recording::link(&mut sessions);
    scan.sessions = sessions;
    scan
}

fn count_refusal(refused: &mut BTreeMap<String, usize>, refusal: &Refusal) {
    *refused.entry(refusal.reason()).or_default() += 1;
}

/// Scan for a snapshot or a get: bounded, and never an import.
pub fn query(dir: &Path) -> Scan {
    scan_dir(
        dir,
        Limits {
            max_files: QUERY_MAX_FILES,
            max_bytes: QUERY_MAX_BYTES,
        },
    )
}

/// Turn a producer name into the form used in ids: `claude-code` becomes `claude_code`.
pub fn producer_key(raw: &str) -> String {
    recording::sanitise_producer(Some(raw))
}

/// Whether a session counts as active for display.
pub fn is_active(s: &Session) -> bool {
    s.in_progress == InProgress::True
}

/// "3m ago", "in 2h", or "just now". Relative to `now`, for display only.
pub fn relative(t: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - t).num_seconds();
    let (n, future) = if secs < 0 {
        (-secs, true)
    } else {
        (secs, false)
    };
    let body = if n < 45 {
        return "just now".to_string();
    } else if n < 3600 {
        format!("{}m", (n + 30) / 60)
    } else if n < 86_400 {
        format!("{}h", (n + 1800) / 3600)
    } else {
        format!("{}d", (n + 43_200) / 86_400)
    };
    if future {
        format!("in {body}")
    } else {
        format!("{body} ago")
    }
}

/// What the engine request figures say, read from the Inference poller's IronWorks
/// sample. Absent is `None`, never zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineRequests {
    /// Requests the engine reports in flight now.
    pub active: Option<u64>,
    /// Requests in the engine's recent-request buffer.
    pub recent: Option<usize>,
    /// Whether the IronWorks engine answered its last poll at all.
    pub reachable: bool,
}

/// Pick the IronWorks engine's request figures out of the Inference poller's states.
/// The Agents tab reuses those samples; it does not poll the engine a second time.
pub fn engine_requests(states: &[crate::inference::SourceState]) -> Option<EngineRequests> {
    let st = states
        .iter()
        .find(|s| s.kind == crate::inference::ProviderKind::IronWorks)?;
    let section = st.latest.as_ref().and_then(|s| s.ironworks.as_ref());
    Some(EngineRequests {
        active: section.and_then(|s| s.requests_active),
        recent: section.and_then(|s| s.recent.as_ref().map(|r| r.len())),
        reachable: st.reachable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    fn tmp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ironmon-agents-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_directory_is_not_an_empty_one() {
        let dir = std::env::temp_dir().join("ironmon-agents-definitely-absent-dir");
        let scan = query(&dir);
        assert!(!scan.dir_exists);
        assert_eq!(scan.recordings(), 0);
    }

    #[test]
    fn an_empty_directory_exists_and_holds_nothing() {
        let dir = tmp("empty");
        let scan = query(&dir);
        assert!(scan.dir_exists);
        assert_eq!(scan.recordings(), 0);
        assert_eq!(scan.files, 0);
    }

    #[test]
    fn a_corrupt_file_is_reported_and_the_rest_are_read() {
        let dir = tmp("corrupt");
        write(
            &dir,
            "a_good.json",
            r#"[{"run":"a","in_progress":true,"events":[]}]"#,
        );
        write(&dir, "b_bad.json", "{ this is not json");
        write(
            &dir,
            "c_good.json",
            r#"{"run":"c","in_progress":false,"events":[]}"#,
        );
        write(&dir, "notes.txt", "ignored, not a recording file");
        let scan = query(&dir);
        assert_eq!(scan.files, 3);
        assert_eq!(scan.corrupt_files, 1);
        assert_eq!(scan.recordings(), 2);
        assert_eq!(scan.active_count(), 1);
    }

    #[test]
    fn unknown_format_is_refused_and_counted_beside_good_ones() {
        let dir = tmp("refused");
        write(
            &dir,
            "f.json",
            r#"[{"format":"ironobserver.recording/3","run":"n","events":[]},{"run":"o","events":[]}]"#,
        );
        let scan = query(&dir);
        assert_eq!(scan.recordings(), 1);
        assert_eq!(scan.refused_count(), 1);
    }

    #[test]
    fn subagents_are_not_counted_as_sessions() {
        let dir = tmp("workflow");
        write(
            &dir,
            "w.json",
            r#"[{"run":"root","producer":"claude-code","events":[]},
                {"run":"sub","parent":"root","producer":"claude-code","events":[]}]"#,
        );
        let scan = query(&dir);
        assert_eq!(scan.recordings(), 2);
        assert_eq!(scan.sessions_count(), 1);
        assert_eq!(scan.subagent_count(), 1);
        assert_eq!(scan.workflow_count(), 1);
        assert_eq!(scan.producer_count("claude_code"), 2);
    }

    #[test]
    fn the_query_bound_counts_files_it_did_not_read() {
        let dir = tmp("bounded");
        for i in 0..5 {
            write(&dir, &format!("{i}.json"), r#"{"run":"x","events":[]}"#);
        }
        let scan = scan_dir(
            &dir,
            Limits {
                max_files: 2,
                max_bytes: u64::MAX,
            },
        );
        assert_eq!(scan.files, 2);
        assert_eq!(scan.files_truncated, 3);
    }

    #[test]
    fn counts_of_zero_recordings_are_distinguishable_from_absence() {
        let dir = tmp("zero");
        let scan = query(&dir);
        // The caller turns this into "unavailable: no recordings", not 0.
        assert_eq!(scan.recordings(), 0);
        assert!(scan.dir_exists);
        assert_eq!(scan.input_tokens(), None);
    }

    #[test]
    fn relative_times_read_naturally() {
        let now = Utc::now();
        assert_eq!(relative(now, now), "just now");
        assert_eq!(relative(now - chrono::Duration::minutes(3), now), "3m ago");
        assert_eq!(relative(now - chrono::Duration::hours(2), now), "2h ago");
        assert_eq!(relative(now + chrono::Duration::hours(5), now), "in 5h");
    }

    #[test]
    fn producer_keys_match_the_id_form() {
        assert_eq!(producer_key("claude-code"), "claude_code");
        assert_eq!(producer_key("Gemini CLI"), "gemini_cli");
    }
}
