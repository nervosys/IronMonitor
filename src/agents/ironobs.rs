//! Running IronObserver's `ironobs` to write the recordings directory.
//!
//! IronMonitor does not link IronObserver. When `ironobs` is on PATH, a background
//! thread runs `ironobs import claude-code` into the recordings directory, at most
//! once per [`MIN_INTERVAL`]. Nothing here runs on a UI thread, and nothing runs
//! from `snapshot` or `get`: those only read what is already on disk.
//!
//! Single instance: an import holds `.import.lock` in the recordings directory. A
//! second IronMonitor window that finds the lock reports the import as running
//! since the time the lock names, and does not start its own. A lock older than
//! [`STALE_LOCK`] is taken to belong to a process that died, and is replaced.
//!
//! Output: `ironobs` writes a JSON array to stdout. That is written to a temporary
//! file and renamed into place only when it holds recordings, so a failed or empty
//! run never replaces a good file. With `--modified-since` (used only when the
//! installed binary's help names it) the output is a delta, merged by run id.

use super::{claude_code_dir, recordings_dir, scan_dir, Scan, UNBOUNDED};
use chrono::{DateTime, SecondsFormat, Utc};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

pub const BINARY: &str = "ironobs";
pub const LOCK_FILE: &str = ".import.lock";
pub const OUTPUT_FILE: &str = "claude-code.json";
/// Never import more often than this, whatever interval the caller asks for.
pub const MIN_INTERVAL: Duration = Duration::from_secs(300);
/// A lock this old is taken to belong to a process that is gone. A full import
/// took about fifteen minutes on the machine it was measured on.
pub const STALE_LOCK: Duration = Duration::from_secs(2 * 3600);
/// How long a `--version` or `--help` probe may run.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// How often the poller checks whether an import is due and rescans the directory.
const TICK: Duration = Duration::from_secs(15);

/// The reason shown when `ironobs` is not installed. It says how to get it.
pub const NOT_INSTALLED: &str = "ironobs is not on PATH. Install IronObserver so its `ironobs` \
     binary is on PATH; IronMonitor then imports Claude Code sessions in the background. \
     Recordings already on disk are still read.";

/// An installed `ironobs`, as far as one bounded probe could tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ironobs {
    pub path: PathBuf,
    /// The first line of `ironobs --version`, when it printed a sensible one.
    pub version: Option<String>,
    /// Whether `ironobs import --help` names `--modified-since`.
    pub modified_since: bool,
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
    let modified_since = run_bounded(path, &["import", "--help"], PROBE_TIMEOUT)
        .is_some_and(|help| help.contains("--modified-since"));
    Ironobs {
        path: path.to_path_buf(),
        version,
        modified_since,
    }
}

fn quiet(path: &Path) -> Command {
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

/// What one import did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    /// Recordings were written. `failed_files` counts files ironobs could not read.
    Wrote {
        recordings: usize,
        failed_files: usize,
        delta: bool,
    },
    /// ironobs ran and found nothing to write. The existing file is kept.
    NothingNew { failed_files: usize },
    /// The import did not produce usable output. The reason says why.
    Failed(String),
}

impl ImportOutcome {
    pub fn describe(&self) -> String {
        match self {
            ImportOutcome::Wrote {
                recordings,
                failed_files,
                delta,
            } => {
                let kind = if *delta { "new or changed" } else { "all" };
                let base = format!("wrote {recordings} {kind} recording(s)");
                if *failed_files > 0 {
                    format!("{base}; ironobs could not read {failed_files} session file(s)")
                } else {
                    base
                }
            }
            ImportOutcome::NothingNew { failed_files } if *failed_files > 0 => format!(
                "ironobs found nothing it could read; {failed_files} session file(s) failed"
            ),
            ImportOutcome::NothingNew { .. } => "ironobs found no sessions; nothing written".into(),
            ImportOutcome::Failed(why) => format!("failed: {why}"),
        }
    }
}

/// Run `ironobs import claude-code` into `dir`. Blocks until ironobs exits. Call it
/// only from a background thread, holding the lock from [`try_lock`].
///
/// `since` asks for a delta, and only when the installed binary names the flag. A
/// binary that rejects the flag (exit 2, a usage error) is retried as a full import.
pub fn run_import(
    bin: &Ironobs,
    source: &Path,
    dir: &Path,
    since: Option<SystemTime>,
) -> ImportOutcome {
    let delta_since = since.filter(|_| bin.modified_since);
    match run_import_once(bin, source, dir, delta_since) {
        (_, true) if delta_since.is_some() => run_import_once(bin, source, dir, None).0,
        (outcome, _) => outcome,
    }
}

/// One import. The flag is true when ironobs exited 2, its usage-error code, so the
/// caller can retry without a flag the binary does not know.
fn run_import_once(
    bin: &Ironobs,
    source: &Path,
    dir: &Path,
    since: Option<SystemTime>,
) -> (ImportOutcome, bool) {
    let tmp = dir.join(format!("{OUTPUT_FILE}.tmp"));
    let Ok(out_file) = File::create(&tmp) else {
        return (
            ImportOutcome::Failed("could not create the temporary recordings file".into()),
            false,
        );
    };
    let mut cmd = import_command(&bin.path);
    cmd.arg("import").arg("claude-code");
    if let Some(since) = since {
        let stamp = DateTime::<Utc>::from(since).to_rfc3339_opts(SecondsFormat::Secs, true);
        cmd.arg("--modified-since").arg(stamp);
    }
    cmd.arg(source)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out_file))
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return (
                ImportOutcome::Failed(format!("could not start ironobs ({:?})", e.kind())),
                false,
            );
        }
    };
    // stderr names session files, which name projects. It is counted, never kept.
    let mut failed_files = 0usize;
    let mut other_lines = 0usize;
    if let Some(stderr) = child.stderr.take() {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if line.starts_with("could not read") {
                failed_files += 1;
            } else {
                other_lines += 1;
            }
        }
    }
    let status = match child.wait() {
        Ok(s) => s,
        Err(_) => {
            let _ = std::fs::remove_file(&tmp);
            return (
                ImportOutcome::Failed("ironobs did not report an exit status".into()),
                false,
            );
        }
    };
    let usage_error = status.code() == Some(2);
    let body = match std::fs::read(&tmp) {
        Ok(b) => b,
        Err(_) => {
            return (
                ImportOutcome::Failed("no output was written".into()),
                usage_error,
            )
        }
    };
    let _ = std::fs::remove_file(&tmp);
    let parsed: Vec<serde_json::Value> = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            let code = status
                .code()
                .map_or_else(|| "no exit code".to_string(), |c| format!("exit code {c}"));
            return (
                ImportOutcome::Failed(format!(
                    "ironobs output was not a recordings array ({code}; {other_lines} other stderr line(s), not shown)"
                )),
                usage_error,
            );
        }
    };
    // A non-zero exit with recordings in the output is a partial import: ironobs
    // writes what it could read, then exits 1 for the files it could not.
    if !status.success() && parsed.is_empty() {
        let why = match status.code() {
            Some(c) => format!("ironobs exited {c}"),
            None => "ironobs was terminated".into(),
        };
        return (ImportOutcome::Failed(why), usage_error);
    }
    if parsed.is_empty() {
        return (ImportOutcome::NothingNew { failed_files }, usage_error);
    }
    let count = parsed.len();
    let result = if since.is_some() {
        merge_delta(dir, parsed).map(|()| true)
    } else {
        write_atomic(&dir.join(OUTPUT_FILE), &serde_json::Value::Array(parsed)).map(|()| false)
    };
    match result {
        Ok(delta) => (
            ImportOutcome::Wrote {
                recordings: count,
                failed_files,
                delta,
            },
            false,
        ),
        Err(why) => (ImportOutcome::Failed(why), false),
    }
}

/// Merge a delta into the recordings file. A run in the delta replaces the run of the
/// same id; every other run is kept as it was.
fn merge_delta(dir: &Path, delta: Vec<serde_json::Value>) -> Result<(), String> {
    let path = dir.join(OUTPUT_FILE);
    let mut merged: Vec<serde_json::Value> = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "the existing recordings file is not an array; not merged".to_string())?,
        Err(_) => Vec::new(),
    };
    let incoming: std::collections::HashSet<String> = delta
        .iter()
        .filter_map(|v| v.get("run").and_then(|r| r.as_str()).map(str::to_string))
        .collect();
    merged.retain(|v| {
        v.get("run")
            .and_then(|r| r.as_str())
            .is_none_or(|r| !incoming.contains(r))
    });
    merged.extend(delta);
    write_atomic(&path, &serde_json::Value::Array(merged))
}

fn write_atomic(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let text =
        serde_json::to_vec(value).map_err(|_| "could not serialise recordings".to_string())?;
    std::fs::write(&tmp, text).map_err(|_| "could not write the recordings file".to_string())?;
    std::fs::rename(&tmp, path).map_err(|_| {
        let _ = std::fs::remove_file(&tmp);
        "could not replace the recordings file".to_string()
    })
}

/// What the import is doing, for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportState {
    /// `ironobs` is not on PATH.
    NotInstalled,
    /// `ironobs` is installed, but there is no Claude Code projects directory.
    NoSource,
    /// `ironobs` is installed, and no import has run in this process yet.
    Pending,
    /// No import is running. `last` is the last attempt this process made, if any.
    Idle {
        last: Option<(DateTime<Utc>, String)>,
    },
    /// An import is running. `by_this_window` is false when another IronMonitor holds the lock.
    Running {
        since: DateTime<Utc>,
        by_this_window: bool,
    },
}

/// Everything the Agents tab and the ontology need, as one copyable value.
#[derive(Debug, Clone)]
pub struct AgentsState {
    pub recordings_dir: PathBuf,
    pub source_dir: PathBuf,
    pub ironobs: Option<Ironobs>,
    pub import: ImportState,
    /// `None` until the first scan has finished.
    pub scan: Option<Scan>,
    /// When the recordings file was last written by an import.
    pub last_written: Option<DateTime<Utc>>,
    pub interval: Duration,
}

impl AgentsState {
    fn new(interval: Duration) -> Self {
        Self {
            recordings_dir: recordings_dir(),
            source_dir: claude_code_dir(),
            ironobs: None,
            import: ImportState::Pending,
            scan: None,
            last_written: None,
            interval,
        }
    }

    /// The state a one-shot read can report. It probes the binary and reads the
    /// directory and the lock. It starts no import.
    pub fn query() -> Self {
        let mut st = Self::new(MIN_INTERVAL);
        st.ironobs = locate().map(|p| probe(&p));
        st.scan = Some(scan_dir(
            &st.recordings_dir,
            super::Limits {
                max_files: super::QUERY_MAX_FILES,
                max_bytes: super::QUERY_MAX_BYTES,
            },
        ));
        st.last_written = written_at(&st.recordings_dir);
        st.import = match (&st.ironobs, lock_state(&st.recordings_dir)) {
            (None, _) => ImportState::NotInstalled,
            (Some(_), LockState::HeldSince(since)) => ImportState::Running {
                since,
                by_this_window: false,
            },
            (Some(_), LockState::Free) if !st.source_dir.is_dir() => ImportState::NoSource,
            // A one-shot read does not know what the background import last did.
            (Some(_), LockState::Free) => ImportState::Idle { last: None },
        };
        st
    }

    /// The Scan, or an empty one when no scan has finished yet.
    pub fn scan_or_empty(&self) -> Scan {
        self.scan
            .clone()
            .unwrap_or_else(|| Scan::empty(&self.recordings_dir))
    }
}

fn written_at(dir: &Path) -> Option<DateTime<Utc>> {
    std::fs::metadata(dir.join(OUTPUT_FILE))
        .ok()?
        .modified()
        .ok()
        .map(DateTime::<Utc>::from)
}

/// Keeps the Agents state current on a background thread.
///
/// Dropping the poller stops the thread at its next tick. An import already running
/// finishes first, and the drop does not wait for it, so it never blocks a UI.
pub struct AgentsPoller {
    shared: Arc<Mutex<AgentsState>>,
    _stop: Sender<()>,
}

impl AgentsPoller {
    /// Start the poller. `interval` is raised to [`MIN_INTERVAL`] if it is shorter.
    pub fn spawn(interval: Duration) -> Self {
        let interval = interval.max(MIN_INTERVAL);
        let shared = Arc::new(Mutex::new(AgentsState::new(interval)));
        let (stop_tx, stop_rx) = mpsc::channel();
        let thread_shared = Arc::clone(&shared);
        // A failed spawn leaves the state as Pending forever, which is the truth: no
        // import runs. The UI shows it as pending rather than hiding it.
        let _ = std::thread::Builder::new()
            .name("agents-refresh".into())
            .spawn(move || refresh_loop(interval, thread_shared, stop_rx));
        Self {
            shared,
            _stop: stop_tx,
        }
    }

    /// A copy of the current state. Cheap; never touches disk.
    pub fn state(&self) -> AgentsState {
        lock(&self.shared).clone()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn refresh_loop(interval: Duration, shared: Arc<Mutex<AgentsState>>, stop: Receiver<()>) {
    let mut last_import: Option<Instant> = None;
    let mut last_since: Option<SystemTime> = None;
    loop {
        let (dir, source) = {
            let st = lock(&shared);
            (st.recordings_dir.clone(), st.source_dir.clone())
        };
        let found = locate();
        let bin = {
            let mut st = lock(&shared);
            let need_probe = match (&st.ironobs, &found) {
                (Some(current), Some(path)) => &current.path != path,
                (None, Some(_)) | (Some(_), None) => true,
                (None, None) => false,
            };
            if need_probe {
                st.ironobs = found.as_deref().map(probe);
            }
            if found.is_none() {
                st.ironobs = None;
            }
            st.ironobs.clone()
        };
        let scan = scan_dir(&dir, UNBOUNDED);
        {
            let mut st = lock(&shared);
            st.scan = Some(scan);
            st.last_written = written_at(&dir);
        }

        let due = last_import.is_none_or(|t| t.elapsed() >= interval);
        if due {
            match bin {
                None => lock(&shared).import = ImportState::NotInstalled,
                Some(_) if !source.is_dir() => lock(&shared).import = ImportState::NoSource,
                Some(bin) => match try_lock(&dir) {
                    Err(LockState::HeldSince(since)) => {
                        // Another window is importing. Wait a full interval after it
                        // rather than starting a second import as soon as it ends.
                        last_import = Some(Instant::now());
                        lock(&shared).import = ImportState::Running {
                            since,
                            by_this_window: false,
                        };
                    }
                    Err(LockState::Free) => {
                        lock(&shared).import = ImportState::Idle {
                            last: Some((Utc::now(), "could not create the import lock".into())),
                        };
                    }
                    Ok(guard) => {
                        let started = Utc::now();
                        lock(&shared).import = ImportState::Running {
                            since: started,
                            by_this_window: true,
                        };
                        let started_sys = SystemTime::now();
                        let outcome = run_import(&bin, &source, &dir, last_since);
                        drop(guard);
                        if matches!(outcome, ImportOutcome::Wrote { .. }) {
                            last_since = Some(started_sys);
                        }
                        last_import = Some(Instant::now());
                        let scan = scan_dir(&dir, UNBOUNDED);
                        let mut st = lock(&shared);
                        st.scan = Some(scan);
                        st.last_written = written_at(&dir);
                        st.import = ImportState::Idle {
                            last: Some((Utc::now(), outcome.describe())),
                        };
                    }
                },
            }
        }

        match stop.recv_timeout(TICK) {
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            _ => return,
        }
    }
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
    fn a_delta_replaces_runs_it_names_and_keeps_the_rest() {
        let d = dir("merge");
        std::fs::write(
            d.join(OUTPUT_FILE),
            r#"[{"run":"a","events":[]},{"run":"b","events":[]}]"#,
        )
        .unwrap();
        let delta = vec![
            serde_json::json!({"run":"b","events":[1]}),
            serde_json::json!({"run":"c","events":[]}),
        ];
        merge_delta(&d, delta).unwrap();
        let merged: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(d.join(OUTPUT_FILE)).unwrap()).unwrap();
        let runs: Vec<&str> = merged.iter().map(|v| v["run"].as_str().unwrap()).collect();
        assert_eq!(runs, vec!["a", "b", "c"]);
        assert_eq!(
            merged[1]["events"].as_array().unwrap().len(),
            1,
            "b was replaced"
        );
    }

    #[test]
    fn the_minimum_interval_is_five_minutes() {
        assert!(MIN_INTERVAL >= Duration::from_secs(300));
    }
}
