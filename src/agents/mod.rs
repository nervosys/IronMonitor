//! Agent activity: what local coding agents, workflows and inference requests did.
//!
//! IronMonitor does not parse agent transcripts itself. The IronObserver tool
//! (`ironobs`) reads each producer's own session files and writes one recording
//! file per run into a directory, and IronMonitor reads those recordings. Two steps
//! stay separate:
//!
//! - **Reading** ([`scan_with`], [`cache`]). Recording files are read here, metadata
//!   only (see [`recording`]). A scan keeps one summary per file, reused while the
//!   file's size and modification time are unchanged, and the background poller
//!   writes that summary to a cache file. A snapshot or `get` reads that cache and
//!   parses only what it lacks, within [`QUERY_DEADLINE`].
//! - **Importing** (see [`ironobs`]). When `ironobs` is on PATH, a background thread
//!   runs `ironobs import ... --out` for each producer whose source folder exists,
//!   under a hard deadline, and schedules the next cycle after the last one ended.
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
//! - `IRONMON_CODEX_DIR` overrides the Codex CLI sessions directory that is
//!   imported. Default: `~/.codex/sessions`.

pub mod cache;
pub mod ironobs;
pub mod recording;
pub mod report;

use chrono::{DateTime, Utc};
use recording::{InProgress, Session};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const RECORDINGS_ENV: &str = "IRONMON_RECORDINGS_DIR";
pub const CLAUDE_CODE_ENV: &str = "IRONMON_CLAUDE_CODE_DIR";
pub const CODEX_ENV: &str = "IRONMON_CODEX_DIR";

/// How long a snapshot or `get` may spend reading recordings. Files not read by
/// then are counted, and the counts that depend on them are reported unavailable.
pub const QUERY_DEADLINE: Duration = Duration::from_secs(2);

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

/// The Codex CLI sessions directory `ironobs import codex` reads.
pub fn codex_dir() -> PathBuf {
    match std::env::var_os(CODEX_ENV) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => home_dir().join(".codex").join("sessions"),
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

/// A stable 64-bit key for a name (FNV-1a). Run identifiers and file names are
/// reduced to keys before they are stored, so the cache holds no raw identifier.
pub fn key_of(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
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
    /// Recording files whose summaries are in this scan: read now, or reused from
    /// the cache because they have not changed.
    pub files: usize,
    pub file_bytes: u64,
    /// Files that are not JSON of either recording shape. Each is skipped.
    pub corrupt_files: usize,
    /// Recordings refused, by reason, counted. Reasons are fixed strings.
    pub refused: BTreeMap<String, usize>,
    /// Recording files the scan did not reach before its limit or deadline. Counts
    /// that depend on them are not reported.
    pub files_truncated: usize,
    /// Every recording that was read, linked to its parents.
    pub sessions: Vec<Session>,
    /// When the newest recording file was written, from its modification time.
    pub newest_written: Option<DateTime<Utc>>,
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
            newest_written: None,
            taken_at: Utc::now(),
        }
    }

    /// Whether every recording file in the directory is in this scan.
    pub fn is_complete(&self) -> bool {
        self.files_truncated == 0
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

/// Limits for one scan. The background poller reads everything. A query reads until
/// its deadline, and says how many files it did not reach.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Most files whose summaries this scan may include.
    pub max_files: usize,
    /// Most bytes of recording files this scan may include.
    pub max_bytes: u64,
    /// Past this instant, no further file is included.
    pub deadline: Option<Instant>,
}

pub const UNBOUNDED: Limits = Limits {
    max_files: usize::MAX,
    max_bytes: u64::MAX,
    deadline: None,
};

/// Limits for a snapshot or `get`: everything the cache holds, plus whatever more
/// can be read within [`QUERY_DEADLINE`].
pub fn query_limits() -> Limits {
    Limits {
        max_files: usize::MAX,
        max_bytes: u64::MAX,
        deadline: Some(Instant::now() + QUERY_DEADLINE),
    }
}

/// Read every `.json` recording file in `dir`, within `limits`, without a cache.
pub fn scan_dir(dir: &Path, limits: Limits) -> Scan {
    scan_with(dir, limits, &mut cache::Cache::default())
}

/// Read every `.json` recording file in `dir`, within `limits`, reusing and
/// updating the per-file summaries in `cache`.
///
/// A file whose size and modification time match its cached summary is not read
/// again. A corrupt file is counted and skipped; the rest are read. Files are taken
/// in name order so the result is stable from one scan to the next.
pub fn scan_with(dir: &Path, limits: Limits, cache: &mut cache::Cache) -> Scan {
    let mut scan = Scan::empty(dir);
    let Ok(read) = std::fs::read_dir(dir) else {
        return scan;
    };
    scan.dir_exists = true;
    let mut files: Vec<(String, PathBuf)> = read
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            let is_json = path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("json"));
            (is_json && path.is_file())
                .then(|| (e.file_name().to_string_lossy().into_owned(), path))
        })
        .filter(|(name, _)| name != ironobs::LEGACY_OUTPUT)
        .collect();
    files.sort();

    let mut live = HashSet::with_capacity(files.len());
    let mut sessions = Vec::new();
    for (name, path) in &files {
        let key = key_of(name);
        live.insert(key);
        let past_deadline = limits.deadline.is_some_and(|d| Instant::now() >= d);
        if past_deadline || scan.files >= limits.max_files {
            scan.files_truncated += 1;
            continue;
        }
        let Ok(meta) = std::fs::metadata(path) else {
            scan.files += 1;
            scan.corrupt_files += 1;
            continue;
        };
        let len = meta.len();
        if scan.file_bytes.saturating_add(len) > limits.max_bytes {
            scan.files_truncated += 1;
            continue;
        }
        let mtime = cache::mtime_ms(&meta);
        let entry = match cache.entries.get(&key) {
            Some(e) if e.len == len && e.mtime_ms == mtime => e.clone(),
            _ => {
                let parsed = cache::parse_file(path, key, len, mtime);
                cache.entries.insert(key, parsed.clone());
                parsed
            }
        };
        scan.files += 1;
        scan.file_bytes = scan.file_bytes.saturating_add(len);
        if let Some(t) = cache::datetime_of_ms(mtime) {
            scan.newest_written = Some(scan.newest_written.map_or(t, |n| n.max(t)));
        }
        if entry.corrupt {
            scan.corrupt_files += 1;
            continue;
        }
        for (reason, n) in &entry.refused {
            *scan.refused.entry(reason.clone()).or_default() += n;
        }
        sessions.extend(
            entry
                .sessions
                .into_iter()
                .map(cache::StoredSession::into_session),
        );
    }
    recording::link(&mut sessions);
    cache.entries.retain(|k, _| live.contains(k));
    scan.sessions = sessions;
    scan
}

/// Scan for a snapshot or a get: bounded by [`QUERY_DEADLINE`], and never an import.
/// The cache is read but not written here; only the background poller writes it.
pub fn query(dir: &Path) -> Scan {
    let mut cache = cache::load(dir);
    scan_with(dir, query_limits(), &mut cache)
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
    /// Whether the engine has ever answered. When it has not, no figure is shown.
    pub ever_answered: bool,
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
        ever_answered: st.latest.is_some(),
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
    fn a_subagent_in_another_file_still_links_to_its_parent() {
        let dir = tmp("cross-file");
        write(&dir, "a.json", r#"{"run":"root","events":[]}"#);
        write(
            &dir,
            "b.json",
            r#"{"run":"child","parent":"root","events":[]}"#,
        );
        let scan = query(&dir);
        assert_eq!(scan.sessions_count(), 1);
        assert_eq!(scan.subagent_count(), 1);
        assert_eq!(scan.workflow_count(), 1);
    }

    #[test]
    fn the_bound_counts_files_it_did_not_read() {
        let dir = tmp("bounded");
        for i in 0..5 {
            write(&dir, &format!("{i}.json"), r#"{"run":"x","events":[]}"#);
        }
        let scan = scan_dir(
            &dir,
            Limits {
                max_files: 2,
                max_bytes: u64::MAX,
                deadline: None,
            },
        );
        assert_eq!(scan.files, 2);
        assert_eq!(scan.files_truncated, 3);
        assert!(!scan.is_complete());
    }

    #[test]
    fn a_passed_deadline_reads_nothing_and_says_so() {
        let dir = tmp("deadline");
        write(&dir, "a.json", r#"{"run":"x","events":[]}"#);
        let scan = scan_dir(
            &dir,
            Limits {
                max_files: usize::MAX,
                max_bytes: u64::MAX,
                deadline: Some(Instant::now()),
            },
        );
        assert_eq!(scan.files, 0);
        assert_eq!(scan.files_truncated, 1);
    }

    #[test]
    fn the_cache_reuses_a_summary_while_the_file_is_unchanged() {
        let dir = tmp("cache-reuse");
        write(&dir, "a.json", r#"{"run":"x","events":[]}"#);
        let mut cache = cache::Cache::default();
        let first = scan_with(&dir, UNBOUNDED, &mut cache);
        assert_eq!(first.recordings(), 1);
        // Corrupt the file's bytes without changing its length or mtime: the cached
        // summary is used, which proves the file was not read again.
        let path = dir.join("a.json");
        let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        let mut body = std::fs::read(&path).unwrap();
        body[0] = b'!';
        std::fs::write(&path, &body).unwrap();
        let f = std::fs::File::options().write(true).open(&path).unwrap();
        f.set_modified(mtime).unwrap();
        drop(f);
        let second = scan_with(&dir, UNBOUNDED, &mut cache);
        assert_eq!(second.recordings(), 1, "the unchanged file was read again");
        assert_eq!(second.corrupt_files, 0);
    }

    #[test]
    fn a_changed_file_is_read_again() {
        let dir = tmp("cache-changed");
        write(&dir, "a.json", r#"{"run":"x","events":[]}"#);
        let mut cache = cache::Cache::default();
        assert_eq!(scan_with(&dir, UNBOUNDED, &mut cache).recordings(), 1);
        write(
            &dir,
            "a.json",
            r#"[{"run":"x","events":[]},{"run":"y","events":[]}]"#,
        );
        assert_eq!(scan_with(&dir, UNBOUNDED, &mut cache).recordings(), 2);
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

    #[test]
    fn keys_are_stable_and_distinguish_names() {
        assert_eq!(key_of("a.json"), key_of("a.json"));
        assert_ne!(key_of("a.json"), key_of("b.json"));
    }
}
