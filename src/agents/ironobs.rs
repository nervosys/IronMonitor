//! Running IronObserver's `ironobs` to write the recordings directory, and keeping
//! the Agents tab's state current.
//!
//! IronMonitor does not link IronObserver. When `ironobs` is on PATH, a background
//! thread runs one import cycle at a time. A cycle takes the single-instance lock,
//! then for each producer whose source folder exists runs
//! `ironobs import <producer> <source> --out <recordings> --format json`, with
//! `--modified-since` set from that producer's watermark once it has one.
//!
//! - **Deadline.** Each producer's import is killed at [`IMPORT_DEADLINE`] and
//!   reported as stopped. Its stderr is drained on its own thread and only counted,
//!   and its summary goes to a temporary file rather than a pipe, so no read can
//!   block the poller.
//! - **Schedule.** The next cycle is due [`IMPORT_GAP`] after the previous one
//!   *ended*, not on a fixed period: a long import never overlaps the next one.
//! - **Watermarks.** A producer's watermark moves only when its own import exits 0.
//!   It becomes that run's start time minus [`WATERMARK_MARGIN`]. A failed or killed
//!   run leaves it where it was.
//!
//! Nothing here runs from `snapshot` or `get`: those read the cache file and the
//! recordings directory, within a bound. The draw path copies the poller's state.
//!
//! Single instance: a cycle holds `.import.lock` in the recordings directory. A
//! second IronMonitor window that finds the lock reports the cycle as running and
//! does not start its own. A lock older than [`STALE_LOCK`] belongs to a process
//! that is gone, and is replaced.

use super::cache::{self, Cache, ImportRecord, Probe};
use super::{claude_code_dir, codex_dir, recordings_dir, scan_with, Scan, UNBOUNDED};
use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use serde::Deserialize;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

pub const BINARY: &str = "ironobs";
pub const LOCK_FILE: &str = ".import.lock";
/// The single-file output of IronMonitor's first version. Recordings are now one
/// file per run, so this file is left unread rather than counted twice.
pub const LEGACY_OUTPUT: &str = "claude-code.json";
/// The time between the end of one import cycle and the start of the next.
pub const IMPORT_GAP: Duration = Duration::from_secs(15 * 60);
/// The most time one producer's import may run. It is killed at this time.
pub const IMPORT_DEADLINE: Duration = Duration::from_secs(30 * 60);
/// How far before a run's start its watermark sits, so a session written while the
/// run was reading is selected by the next one.
pub const WATERMARK_MARGIN_SECS: i64 = 10 * 60;
/// A lock this old is taken to belong to a process that is gone. It must exceed the
/// two producer deadlines of one cycle, with margin.
pub const STALE_LOCK: Duration = Duration::from_secs(2 * 3600);
/// How long a `--version` or `--help` probe may run.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// How often the poller wakes to check whether a cycle is due and to report state.
const POLL_TICK: Duration = Duration::from_secs(5);
/// How often the poller rescans the recordings directory, reusing cached summaries.
const SCAN_EVERY: Duration = Duration::from_secs(60);
/// How often a running import is checked for exit.
const CHILD_POLL: Duration = Duration::from_millis(100);
const SUMMARY_PREFIX: &str = ".summary-";
/// The window a first import reads, in days, when no watermark exists yet.
pub const FIRST_WINDOW_DAYS: i64 = 7;

/// The reason shown when `ironobs` is not installed. It says how to get it.
pub const NOT_INSTALLED: &str = "ironobs is not on PATH. Install IronObserver so its `ironobs` \
     binary is on PATH; IronMonitor then imports Claude Code and Codex sessions in the background. \
     Recordings already on disk are still read.";
/// The reason shown when the installed binary predates `import --out`.
pub const TOO_OLD: &str = "this ironobs is too old for IronMonitor: needs import --out";
/// The reason shown when `ironobs import --help` did not answer in time.
pub const PROBE_FAILED: &str = "ironobs did not answer `import --help` within 2 s";

/// The sources IronMonitor imports, in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Importer {
    ClaudeCode,
    Codex,
}

impl Importer {
    pub const ALL: [Importer; 2] = [Importer::ClaudeCode, Importer::Codex];

    /// The id segment, which matches the producer name in recordings.
    pub fn key(self) -> &'static str {
        match self {
            Importer::ClaudeCode => "claude_code",
            Importer::Codex => "codex",
        }
    }

    /// The producer name `ironobs import` takes.
    pub fn arg(self) -> &'static str {
        match self {
            Importer::ClaudeCode => "claude-code",
            Importer::Codex => "codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Importer::ClaudeCode => "Claude Code",
            Importer::Codex => "Codex CLI",
        }
    }

    /// The folder `ironobs` reads for this producer.
    pub fn source_dir(self) -> PathBuf {
        match self {
            Importer::ClaudeCode => claude_code_dir(),
            Importer::Codex => codex_dir(),
        }
    }

    /// The folder's own name, which is all a display names of it.
    pub fn source_folder(self) -> &'static str {
        match self {
            Importer::ClaudeCode => "projects",
            Importer::Codex => "sessions",
        }
    }
}

/// An installed `ironobs`, as far as one bounded probe could tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ironobs {
    pub path: PathBuf,
    /// The first line of `ironobs --version`, when it printed a sensible one.
    pub version: Option<String>,
    /// Whether the help probe answered. An unprobed binary is not judged too old.
    pub probed: bool,
    /// Whether `ironobs import --help` names `--out`.
    pub out: bool,
    /// Whether it names `--format`.
    pub format_json: bool,
    /// Whether it names `--modified-since`. Without it every cycle is a full import.
    pub modified_since: bool,
}

impl Ironobs {
    /// Usable for an import cycle: it accepts `--out` and `--format json`.
    pub fn is_usable(&self) -> bool {
        self.probed && self.out && self.format_json
    }

    fn unprobed(path: PathBuf) -> Self {
        Self {
            path,
            version: None,
            probed: false,
            out: false,
            format_json: false,
            modified_since: false,
        }
    }

    fn from_cached(path: PathBuf, p: &Probe) -> Self {
        Self {
            path,
            version: p.version.clone(),
            probed: true,
            out: p.out,
            format_json: p.format_json,
            modified_since: p.modified_since,
        }
    }

    fn to_cached(&self) -> Probe {
        Probe {
            version: self.version.clone(),
            out: self.out,
            format_json: self.format_json,
            modified_since: self.modified_since,
        }
    }
}

/// Find `ironobs` on PATH by name. Nothing is executed.
pub fn locate() -> Option<PathBuf> {
    locate_in(std::env::var_os("PATH"))
}

/// [`locate`] over a given PATH value, so the lookup can be tested without changing
/// the process environment.
fn locate_in(path: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["ironobs.exe", "ironobs"]
    } else {
        &["ironobs"]
    };
    let path = path?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Probe an installed binary. Both probes are bounded by [`PROBE_TIMEOUT`].
pub fn probe(path: &Path) -> Ironobs {
    let version = run_bounded(path, &["--version"], PROBE_TIMEOUT)
        .and_then(|out| out.lines().next().map(str::trim).map(str::to_string))
        .filter(|v| {
            !v.is_empty() && v.len() <= 80 && v.chars().all(|c| c.is_ascii_graphic() || c == ' ')
        });
    let help = run_bounded(path, &["import", "--help"], PROBE_TIMEOUT);
    let names = |flag: &str| {
        help.as_deref()
            .is_some_and(|h| h.lines().any(|l| l.trim_start().starts_with(flag)))
    };
    Ironobs {
        path: path.to_path_buf(),
        version,
        probed: help.is_some(),
        out: names("--out"),
        format_json: names("--format"),
        modified_since: names("--modified-since"),
    }
}

fn quiet(path: &Path) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut cmd = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;

/// The command for an import, at low priority where that is cheap to ask for.
fn import_command(path: &Path) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new(path);
        cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
        cmd
    }
    #[cfg(not(windows))]
    {
        // `nice` is not guaranteed, so the import runs at normal priority without it.
        if Path::new("/usr/bin/nice").is_file() {
            let mut cmd = Command::new("/usr/bin/nice");
            cmd.arg("-n").arg("19").arg(path);
            cmd
        } else {
            Command::new(path)
        }
    }
}

/// Run a command and return its stdout, or `None` if it failed or ran past `limit`.
fn run_bounded(path: &Path, args: &[&str], limit: Duration) -> Option<String> {
    let mut child = quiet(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) => {
                if !status.success() {
                    return None;
                }
                let mut text = String::new();
                if let Some(out) = child.stdout.take() {
                    use std::io::Read;
                    out.take(64 * 1024).read_to_string(&mut text).ok()?;
                }
                return Some(text);
            }
            None if started.elapsed() >= limit => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Who holds the import lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockState {
    Free,
    HeldSince(DateTime<Utc>),
}

/// The lock file's state. A lock past [`STALE_LOCK`] reads as free.
pub fn lock_state(dir: &Path) -> LockState {
    let path = dir.join(LOCK_FILE);
    let Ok(meta) = std::fs::metadata(&path) else {
        return LockState::Free;
    };
    let age = meta
        .modified()
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .unwrap_or_default();
    if age > STALE_LOCK {
        return LockState::Free;
    }
    let since = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| {
            text.lines()
                .nth(1)
                .and_then(|l| DateTime::parse_from_rfc3339(l.trim()).ok())
                .map(|t| t.with_timezone(&Utc))
        })
        .unwrap_or_else(|| DateTime::<Utc>::from(meta.modified().unwrap_or(SystemTime::now())));
    LockState::HeldSince(since)
}

/// The import lock. Released when dropped.
#[derive(Debug)]
pub struct ImportLock {
    path: PathBuf,
}

impl Drop for ImportLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Take the import lock, or report who holds it.
pub fn try_lock(dir: &Path) -> Result<ImportLock, LockState> {
    std::fs::create_dir_all(dir).map_err(|_| LockState::Free)?;
    let path = dir.join(LOCK_FILE);
    for attempt in 0..2 {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
                let _ = writeln!(file, "{}\n{now}", std::process::id());
                return Ok(ImportLock { path });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt == 0 => {
                match lock_state(dir) {
                    LockState::Free => {
                        let _ = std::fs::remove_file(&path);
                    }
                    held => return Err(held),
                }
            }
            Err(_) => return Err(LockState::Free),
        }
    }
    Err(lock_state(dir))
}

/// The counts `ironobs import --format json` prints. The `files` list it also prints
/// is skipped without being built, so names derived from runs never reach memory.
#[derive(Debug, Deserialize)]
pub struct Summary {
    pub recordings_written: Option<u64>,
    pub sessions: Option<u64>,
    pub subagent_runs: Option<u64>,
    pub in_progress: Option<u64>,
    pub recordings_skipped: Option<u64>,
    pub recordings_failed: Option<u64>,
    pub lines_unparseable: Option<u64>,
    pub lines_oversized: Option<u64>,
    pub records_unknown_kind: Option<u64>,
    #[allow(dead_code)]
    files: serde::de::IgnoredAny,
}

/// How one producer's import ended.
#[derive(Debug)]
pub enum RunOutcome {
    /// Exit 0: every file found was read.
    Succeeded(Summary),
    /// Exit 1: some files could not be read. What was read is written.
    Partial(Summary),
    /// Exit 2: ironobs refused the request.
    Refused,
    /// Anything else, with the reason.
    Failed(String),
    /// Killed at the deadline, which is carried.
    Stopped(Duration),
}

impl RunOutcome {
    /// Whether the run succeeded outright.
    pub fn ok(&self) -> bool {
        matches!(self, RunOutcome::Succeeded(_))
    }

    /// Whether the run may move the watermark: it exited 0 or 1 with a summary. Exit 1
    /// means some files could not be read, or nothing matched; a session written later
    /// is selected again anyway, so the watermark may move.
    pub fn moves_watermark(&self) -> bool {
        matches!(self, RunOutcome::Succeeded(_) | RunOutcome::Partial(_))
    }

    /// What happened, in counts only: no path, no run identifier, no session text.
    pub fn describe(&self) -> String {
        match self {
            RunOutcome::Succeeded(s) => format!(
                "wrote {} recording(s): {} session(s), {} subagent run(s), {} still in progress",
                count(s.recordings_written),
                count(s.sessions),
                count(s.subagent_runs),
                count(s.in_progress)
            ),
            RunOutcome::Partial(s) => format!(
                "wrote {} recording(s); {} file(s) could not be read",
                count(s.recordings_written),
                count(s.recordings_failed)
            ),
            RunOutcome::Refused => "refused by ironobs (exit 2); nothing was written".into(),
            RunOutcome::Failed(why) => format!("failed: {why}"),
            RunOutcome::Stopped(after) => stopped_text(*after),
        }
    }
}

fn count(v: Option<u64>) -> String {
    v.map_or_else(|| "an unknown number of".to_string(), |n| n.to_string())
}

/// "stopped after 30 min", or in seconds for a deadline under a minute.
fn stopped_text(after: Duration) -> String {
    if after >= Duration::from_secs(60) {
        format!("stopped after {} min", after.as_secs() / 60)
    } else {
        format!("stopped after {} s", after.as_secs())
    }
}

/// Run `ironobs import` for one producer into `dir`, killing it at `deadline`.
///
/// The caller holds the lock. `since` selects sessions modified at or after it, and
/// is ignored when the binary does not name `--modified-since`. A binary without
/// `--out` is reported as too old and is not run.
pub fn run_producer(
    bin: &Ironobs,
    importer: Importer,
    source: &Path,
    dir: &Path,
    since: Option<DateTime<Utc>>,
    deadline: Duration,
) -> RunOutcome {
    if !bin.is_usable() {
        return RunOutcome::Failed(TOO_OLD.into());
    }
    let summary_path = dir.join(format!("{SUMMARY_PREFIX}{}.tmp", importer.key()));
    let Ok(stdout) = File::create(&summary_path) else {
        return RunOutcome::Failed(
            "could not create a temporary file for ironobs's summary".into(),
        );
    };
    let mut cmd = import_command(&bin.path);
    cmd.arg("import")
        .arg(importer.arg())
        .arg("--out")
        .arg(dir)
        .arg("--format")
        .arg("json");
    if let (Some(since), true) = (since, bin.modified_since) {
        cmd.arg("--modified-since")
            .arg(since.to_rfc3339_opts(SecondsFormat::Secs, true));
    }
    cmd.arg(source)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_file(&summary_path);
            return RunOutcome::Failed(format!("could not start ironobs ({:?})", e.kind()));
        }
    };
    let stderr_lines = Arc::new(AtomicUsize::new(0));
    if let Some(err) = child.stderr.take() {
        let counter = Arc::clone(&stderr_lines);
        let _ = std::thread::Builder::new()
            .name("ironobs-stderr".into())
            .spawn(move || drain_lines(BufReader::new(err), &counter));
    }
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&summary_path);
                return RunOutcome::Stopped(deadline);
            }
            Ok(None) => std::thread::sleep(CHILD_POLL),
            Err(_) => break None,
        }
    };
    let parsed = File::open(&summary_path)
        .ok()
        .and_then(|f| serde_json::from_reader::<_, Summary>(BufReader::new(f)).ok());
    let _ = std::fs::remove_file(&summary_path);
    let stderr_count = stderr_lines.load(Ordering::Relaxed);
    let Some(status) = status else {
        return RunOutcome::Failed("ironobs's exit status could not be read".into());
    };
    match (status.code(), parsed) {
        (Some(0), Some(s)) => RunOutcome::Succeeded(s),
        (Some(1), Some(s)) => RunOutcome::Partial(s),
        (Some(2), _) => RunOutcome::Refused,
        (Some(code), _) => RunOutcome::Failed(format!(
            "ironobs exited {code} without a usable summary ({stderr_count} stderr line(s), not shown)"
        )),
        (None, _) => RunOutcome::Failed("ironobs was terminated".into()),
    }
}

/// Count lines from a stream and keep none of them. Used for stderr, which names
/// session files, and so names projects.
fn drain_lines(mut reader: impl BufRead, counter: &AtomicUsize) {
    loop {
        match reader.skip_until(b'\n') {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Move a producer's watermark, for a run that moved it (see `moves_watermark`). The
/// new value is the run's start time minus the margin, and it never moves backwards.
pub fn advance_watermark(rec: &mut ImportRecord, started: DateTime<Utc>, outcome: &RunOutcome) {
    if !outcome.moves_watermark() {
        return;
    }
    let next = started - ChronoDuration::seconds(WATERMARK_MARGIN_SECS);
    rec.watermark = Some(rec.watermark.map_or(next, |old| old.max(next)));
}

/// What a producer's import looks like to a reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastRun {
    pub at: DateTime<Utc>,
    pub ok: bool,
    pub text: String,
}

/// One producer, as the state reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducerImport {
    pub importer: Importer,
    /// The folder ironobs reads. Displays show only its final name.
    pub source: PathBuf,
    pub source_exists: bool,
    /// Sessions modified at or after this time are the next import's selection.
    /// `None` means the next import reads all history.
    pub watermark: Option<DateTime<Utc>>,
    pub last: Option<LastRun>,
    /// Whether this producer is being imported now.
    pub running: bool,
    /// The start of the first import's window, once that import has begun.
    pub first_since: Option<DateTime<Utc>>,
}

/// What the import cycle is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cycle {
    /// Nothing has been read yet, so whether an import can run is not known.
    Pending,
    /// A cycle is running. `by_this_window` is false when another IronMonitor holds
    /// the lock.
    Running {
        since: DateTime<Utc>,
        by_this_window: bool,
    },
    /// No cycle is running. `last_ended` is when the last one ended, if one has.
    Waiting { last_ended: Option<DateTime<Utc>> },
    /// No import can run, and the reason says why.
    Blocked(String),
}

/// Everything the Agents tab and the ontology need, as one copyable value.
#[derive(Debug, Clone)]
pub struct AgentsState {
    pub recordings_dir: PathBuf,
    pub ironobs: Option<Ironobs>,
    pub cycle: Cycle,
    /// When the next cycle is due, once one has ended.
    pub next_at: Option<DateTime<Utc>>,
    pub imports: Vec<ProducerImport>,
    /// `None` until the first scan has finished.
    pub scan: Option<Scan>,
}

impl AgentsState {
    fn new() -> Self {
        let mut st = Self {
            recordings_dir: recordings_dir(),
            ironobs: None,
            cycle: Cycle::Pending,
            next_at: None,
            imports: Vec::new(),
            scan: None,
        };
        set_imports(&mut st, &Cache::default(), None);
        st
    }

    /// The state a one-shot read can report. It reads the cache file and the
    /// recordings directory within the query deadline, and checks whether an
    /// `ironobs` is present. It starts no import and runs no probe.
    pub fn query() -> Self {
        let dir = recordings_dir();
        let mut cache = cache::load(&dir);
        let scan = scan_with(&dir, super::query_limits(), &mut cache);
        let ironobs = locate().map(|path| match &cache.probe {
            Some(p) => Ironobs::from_cached(path, p),
            None => Ironobs::unprobed(path),
        });
        let cycle = match lock_state(&dir) {
            LockState::HeldSince(since) => Cycle::Running {
                since,
                by_this_window: false,
            },
            LockState::Free => match blocked_reason(ironobs.as_ref()) {
                Some(why) => Cycle::Blocked(why),
                None if ironobs.as_ref().is_some_and(|b| !b.probed) => Cycle::Pending,
                None => Cycle::Waiting {
                    last_ended: last_cycle_end(&cache),
                },
            },
        };
        let mut st = Self {
            recordings_dir: dir,
            ironobs,
            cycle,
            next_at: cache.next_at,
            imports: Vec::new(),
            scan: Some(scan),
        };
        set_imports(&mut st, &cache, None);
        st
    }

    /// The Scan, or an empty one when no scan has finished yet.
    pub fn scan_or_empty(&self) -> Scan {
        self.scan
            .clone()
            .unwrap_or_else(|| Scan::empty(&self.recordings_dir))
    }

    /// The import record for a producer, from the state's copy of the cache.
    pub fn import_for(&self, importer: Importer) -> Option<&ProducerImport> {
        self.imports.iter().find(|p| p.importer == importer)
    }
}

/// Why no import can run, if one cannot: the binary is missing or too old.
fn blocked_reason(bin: Option<&Ironobs>) -> Option<String> {
    match bin {
        None => Some(NOT_INSTALLED.into()),
        Some(b) if b.probed && !b.is_usable() => Some(TOO_OLD.into()),
        Some(_) => None,
    }
}

fn last_cycle_end(cache: &Cache) -> Option<DateTime<Utc>> {
    cache.imports.iter().filter_map(|r| r.last_at).max()
}

/// Rebuild the per-producer view from the cache, with one producer marked running.
fn set_imports(st: &mut AgentsState, cache: &Cache, running: Option<Importer>) {
    st.imports = Importer::ALL
        .iter()
        .map(|&importer| {
            let source = importer.source_dir();
            let record = cache.record(importer.key());
            ProducerImport {
                importer,
                source_exists: source.is_dir(),
                source,
                watermark: record.and_then(|r| r.watermark),
                last: record.and_then(|r| {
                    Some(LastRun {
                        at: r.last_at?,
                        ok: r.last_ok,
                        text: r.last_text.clone(),
                    })
                }),
                running: running == Some(importer),
                first_since: record.and_then(|r| r.first_since),
            }
        })
        .collect();
    st.next_at = cache.next_at;
}

/// Keeps the Agents state current on a background thread.
///
/// Dropping the poller stops the thread at its next tick. A cycle already running
/// finishes first, and the drop does not wait for it, so it never blocks a UI.
pub struct AgentsPoller {
    shared: Arc<Mutex<AgentsState>>,
    _stop: Sender<()>,
}

impl AgentsPoller {
    /// Start the poller. A failed spawn leaves the state pending forever, which is
    /// the truth: no import runs, and the tab shows that rather than hiding it.
    pub fn spawn() -> Self {
        let shared = Arc::new(Mutex::new(AgentsState::new()));
        let (stop_tx, stop_rx) = mpsc::channel();
        let thread_shared = Arc::clone(&shared);
        let _ = std::thread::Builder::new()
            .name("agents-refresh".into())
            .spawn(move || refresh_loop(thread_shared, stop_rx));
        Self {
            shared,
            _stop: stop_tx,
        }
    }

    /// A copy of the current state. Cheap; never touches disk.
    pub fn state(&self) -> AgentsState {
        lock(&self.shared).clone()
    }

    /// Wait, up to `timeout`, for the first scan to finish. Only the one-shot
    /// headless paths call this, so a frame shows the recordings rather than
    /// "not scanned yet".
    pub fn wait_for_scan(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if lock(&self.shared).scan.is_some() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// When the next cycle is due, as an instant from now. A cycle that has never ended
/// is due at once.
fn resume_at(next_at: Option<DateTime<Utc>>) -> Instant {
    let wait = next_at
        .and_then(|t| (t - Utc::now()).to_std().ok())
        .unwrap_or(Duration::ZERO);
    Instant::now() + wait
}

/// Put the calling thread at the lowest scheduling priority that Windows allows, so
/// a scan of a large recordings directory does not compete with the UI thread.
#[cfg(windows)]
fn lower_thread_priority() {
    use windows::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_LOWEST,
    };
    // SAFETY: GetCurrentThread returns a pseudo-handle for the calling thread, and
    // SetThreadPriority only changes that thread's scheduling priority. Failure is
    // harmless: the thread keeps its normal priority.
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST);
    }
}

#[cfg(not(windows))]
fn lower_thread_priority() {}

fn refresh_loop(shared: Arc<Mutex<AgentsState>>, stop: Receiver<()>) {
    lower_thread_priority();
    let dir = lock(&shared).recordings_dir.clone();
    let mut cache = cache::load(&dir);
    {
        let mut st = lock(&shared);
        set_imports(&mut st, &cache, None);
    }
    let mut found: Option<PathBuf> = None;
    let mut ironobs: Option<Ironobs> = None;
    let mut next_scan = Instant::now();
    let mut next_import = Some(resume_at(cache.next_at));
    loop {
        let now_found = locate();
        if now_found != found {
            found = now_found;
            ironobs = found.as_deref().map(probe);
            cache.probe = ironobs
                .as_ref()
                .filter(|b| b.probed)
                .map(Ironobs::to_cached);
        }
        if Instant::now() >= next_scan {
            let scan = scan_with(&dir, UNBOUNDED, &mut cache);
            let _ = cache::save(&dir, &cache);
            lock(&shared).scan = Some(scan);
            next_scan = Instant::now() + SCAN_EVERY;
        }
        {
            let held_elsewhere = matches!(lock_state(&dir), LockState::HeldSince(_));
            let blocked = match ironobs.as_ref() {
                Some(b) if !b.probed => Some(PROBE_FAILED.to_string()),
                other => blocked_reason(other),
            };
            let mut st = lock(&shared);
            st.ironobs = ironobs.clone();
            let running_elsewhere = matches!(st.cycle, Cycle::Running { .. }) && held_elsewhere;
            st.cycle = match blocked {
                Some(why) => Cycle::Blocked(why),
                None if running_elsewhere => st.cycle.clone(),
                None => Cycle::Waiting {
                    last_ended: last_cycle_end(&cache),
                },
            };
        }
        if let Some(bin) = ironobs.as_ref().filter(|b| b.is_usable()) {
            if next_import.is_some_and(|due| Instant::now() >= due) {
                next_import = Some(run_cycle(bin, &dir, &shared, &mut cache));
                next_scan = Instant::now();
            }
        }
        match stop.recv_timeout(POLL_TICK) {
            Err(RecvTimeoutError::Timeout) => {}
            _ => return,
        }
    }
}

/// One import cycle: every producer whose source folder exists, under the lock.
/// Returns when the next cycle is due, measured from the end of this one.
fn run_cycle(bin: &Ironobs, dir: &Path, shared: &Mutex<AgentsState>, cache: &mut Cache) -> Instant {
    let guard = match try_lock(dir) {
        Ok(g) => g,
        Err(LockState::HeldSince(since)) => {
            lock(shared).cycle = Cycle::Running {
                since,
                by_this_window: false,
            };
            return Instant::now() + IMPORT_GAP;
        }
        Err(LockState::Free) => {
            lock(shared).cycle = Cycle::Blocked("could not create the import lock".into());
            return Instant::now() + IMPORT_GAP;
        }
    };
    let cycle_start = Utc::now();
    for importer in Importer::ALL {
        let source = importer.source_dir();
        if !source.is_dir() {
            continue;
        }
        {
            let mut st = lock(shared);
            st.cycle = Cycle::Running {
                since: cycle_start,
                by_this_window: true,
            };
            set_imports(&mut st, cache, Some(importer));
        }
        let started = Utc::now();
        // With no watermark yet, the first import selects sessions written in the window
        // before it began. The window start is kept, so a killed first run retries it.
        let since = {
            let rec = cache.record_mut(importer.key());
            match rec.watermark {
                Some(w) => w,
                None => *rec
                    .first_since
                    .get_or_insert(started - ChronoDuration::days(FIRST_WINDOW_DAYS)),
            }
        };
        let _ = cache::save(dir, cache);
        let outcome = run_producer(bin, importer, &source, dir, Some(since), IMPORT_DEADLINE);
        let rec = cache.record_mut(importer.key());
        advance_watermark(rec, started, &outcome);
        rec.last_at = Some(Utc::now());
        rec.last_ok = outcome.ok();
        rec.last_text = outcome.describe();
        let _ = cache::save(dir, cache);
        set_imports(&mut lock(shared), cache, None);
    }
    let ended = Utc::now();
    cache.next_at = Some(ended + ChronoDuration::seconds(IMPORT_GAP.as_secs() as i64));
    let _ = cache::save(dir, cache);
    {
        let mut st = lock(shared);
        st.cycle = Cycle::Waiting {
            last_ended: Some(ended),
        };
        set_imports(&mut st, cache, None);
    }
    drop(guard);
    Instant::now() + IMPORT_GAP
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ironmon-ironobs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn summary(json: &str) -> Summary {
        serde_json::from_str(json).expect("a summary")
    }

    #[test]
    fn a_missing_ironobs_is_not_installed_and_nothing_runs() {
        // An empty PATH, and a PATH of directories that hold no ironobs, both find
        // nothing. The lookup is by name only, so nothing is executed either way.
        assert!(locate_in(None).is_none());
        assert!(locate_in(Some(std::ffi::OsString::new())).is_none());
        let empty = dir("empty-path");
        assert!(locate_in(Some(empty.clone().into_os_string())).is_none());
        assert!(NOT_INSTALLED.contains("Install IronObserver"));
    }

    #[test]
    fn an_ironobs_on_path_is_found_by_name_without_being_run() {
        let d = dir("found");
        let name = if cfg!(windows) {
            "ironobs.exe"
        } else {
            "ironobs"
        };
        std::fs::write(d.join(name), b"not a program").unwrap();
        assert_eq!(
            locate_in(Some(d.clone().into_os_string())),
            Some(d.join(name))
        );
    }

    #[test]
    fn the_lock_admits_one_holder_and_releases_on_drop() {
        let d = dir("lock");
        let first = try_lock(&d).expect("first lock");
        match try_lock(&d) {
            Err(LockState::HeldSince(_)) => {}
            other => panic!("second lock should be refused, got {other:?}"),
        }
        drop(first);
        assert!(
            try_lock(&d).is_ok(),
            "the lock is free after the holder drops it"
        );
    }

    #[test]
    fn a_stale_lock_is_replaced() {
        let d = dir("stale");
        std::fs::write(d.join(LOCK_FILE), "999\n2000-01-01T00:00:00Z\n").unwrap();
        let old = SystemTime::now() - STALE_LOCK - Duration::from_secs(60);
        let f = File::options().write(true).open(d.join(LOCK_FILE)).unwrap();
        f.set_modified(old).unwrap();
        drop(f);
        assert_eq!(lock_state(&d), LockState::Free);
        assert!(try_lock(&d).is_ok());
    }

    #[test]
    fn a_fresh_foreign_lock_reports_when_it_started() {
        let d = dir("foreign");
        std::fs::write(d.join(LOCK_FILE), "4242\n2026-03-01T12:00:00Z\n").unwrap();
        match lock_state(&d) {
            LockState::HeldSince(t) => assert_eq!(t.to_rfc3339(), "2026-03-01T12:00:00+00:00"),
            other => panic!("expected held, got {other:?}"),
        }
    }

    #[test]
    fn the_schedule_and_the_deadline_are_the_agreed_ones() {
        assert_eq!(IMPORT_GAP, Duration::from_secs(15 * 60));
        assert_eq!(IMPORT_DEADLINE, Duration::from_secs(30 * 60));
        // A stale lock must outlast a whole cycle of two producer deadlines.
        assert!(STALE_LOCK > IMPORT_DEADLINE * 2);
    }

    #[test]
    fn the_summary_reads_counts_and_skips_the_file_list() {
        let s = summary(
            r#"{"producer":"claude-code","out":"/x","recordings_written":3,"sessions":2,
                "subagent_runs":1,"in_progress":1,"recordings_skipped":9,"recordings_failed":0,
                "lines_unparseable":0,"lines_oversized":0,"records_unknown_kind":null,
                "files":["one.json","two.json"]}"#,
        );
        assert_eq!(s.recordings_written, Some(3));
        assert_eq!(s.in_progress, Some(1));
        assert_eq!(s.records_unknown_kind, None, "null is unknown, not zero");
    }

    #[test]
    fn a_binary_without_out_is_too_old_and_nothing_is_run() {
        let bin = Ironobs {
            path: PathBuf::from("/definitely/not/a/real/ironobs"),
            version: None,
            probed: true,
            out: false,
            format_json: false,
            modified_since: false,
        };
        let d = dir("too-old");
        let outcome = run_producer(&bin, Importer::Codex, &d, &d, None, Duration::from_secs(5));
        assert_eq!(
            outcome.describe(),
            "failed: this ironobs is too old for IronMonitor: needs import --out"
        );
    }

    #[test]
    fn the_watermark_moves_only_on_a_successful_run() {
        let started = Utc::now();
        let mut rec = ImportRecord {
            producer: "codex".into(),
            watermark: None,
            first_since: None,
            last_at: None,
            last_ok: false,
            last_text: String::new(),
        };
        advance_watermark(
            &mut rec,
            started,
            &RunOutcome::Stopped(Duration::from_secs(1)),
        );
        advance_watermark(&mut rec, started, &RunOutcome::Failed("x".into()));
        assert_eq!(rec.watermark, None, "a failed or killed run moves nothing");
        let partial = RunOutcome::Partial(summary(r#"{"files":[]}"#));
        advance_watermark(&mut rec, started, &partial);
        assert_eq!(
            rec.watermark,
            Some(started - ChronoDuration::seconds(WATERMARK_MARGIN_SECS)),
            "exit 1 with a summary moves the watermark"
        );
        let later = started + ChronoDuration::hours(1);
        let ok = RunOutcome::Succeeded(summary(r#"{"files":[]}"#));
        advance_watermark(&mut rec, later, &ok);
        assert_eq!(
            rec.watermark,
            Some(later - ChronoDuration::seconds(WATERMARK_MARGIN_SECS))
        );
    }

    /// A stand-in `ironobs` that never finishes: a shell script on Unix, a batch file
    /// on Windows. Both ignore their arguments and sleep for longer than the test's
    /// deadline, so only the kill can end the run.
    fn write_hanging_stub(d: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            let p = d.join("ironobs-stub.cmd");
            std::fs::write(&p, "@echo off\r\nping -n 31 127.0.0.1 > nul\r\n").unwrap();
            p
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let p = d.join("ironobs-stub.sh");
            std::fs::write(&p, "#!/bin/sh\nsleep 30\n").unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        }
    }

    #[test]
    fn a_hung_import_is_stopped_at_its_deadline() {
        let d = dir("hang");
        let bin = Ironobs {
            path: write_hanging_stub(&d),
            version: None,
            probed: true,
            out: true,
            format_json: true,
            modified_since: true,
        };
        let started = Instant::now();
        let outcome = run_producer(
            &bin,
            Importer::ClaudeCode,
            &d,
            &d,
            None,
            Duration::from_secs(1),
        );
        assert!(
            matches!(outcome, RunOutcome::Stopped(_)),
            "expected Stopped, got {outcome:?}"
        );
        assert_eq!(outcome.describe(), "stopped after 1 s");
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the stub ran {:?}; the deadline did not kill it",
            started.elapsed()
        );
        assert!(
            !d.join(format!("{SUMMARY_PREFIX}claude_code.tmp")).exists(),
            "the summary temporary file is removed"
        );
    }
}
