//! The Agents scan cache: one summary per recording file, the last `ironobs` probe,
//! and each producer's import record, in a single file beside the recordings.
//!
//! The background poller writes this file atomically after each scan and import
//! cycle. A snapshot or `get` reads it and parses only the recording files it does
//! not already have a summary for, so a read stays within its deadline. Nothing here
//! holds an event, a prompt, a raw run identifier or a file name: a file is keyed by
//! [`super::key_of`], and a run by its key.

use super::recording::{InProgress, Session};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::UNIX_EPOCH;

/// The file name. It has no `.json` extension, so the recording scan never reads it.
pub const FILE: &str = ".scan-cache";
const TMP: &str = ".scan-cache.tmp";
const FORMAT: &str = "ironmon.agents-scan/1";

/// What the last successful `ironobs --help` probe said. Written only when a probe
/// answered, so its absence means "not probed", never "too old".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Probe {
    pub version: Option<String>,
    /// `import --out` is supported.
    pub out: bool,
    /// `import --format json` is supported.
    pub format_json: bool,
    /// `import --modified-since` is supported.
    pub modified_since: bool,
}

/// One producer's import history, kept across restarts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRecord {
    /// The producer key, such as `claude_code`.
    pub producer: String,
    /// Sessions modified at or after this time are the next import's selection.
    /// Moved by an import that exited 0 or 1 with a summary.
    pub watermark: Option<DateTime<Utc>>,
    /// The start of the first import's window, fixed when that import first runs:
    /// sessions last written before it are never imported. Kept until the
    /// watermark exists, so a killed first import retries the same window.
    pub first_since: Option<DateTime<Utc>>,
    pub last_at: Option<DateTime<Utc>>,
    pub last_ok: bool,
    /// What the last attempt reported, in counts only.
    pub last_text: String,
}

/// One recording file's summary.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Entry {
    /// [`super::key_of`] of the file name.
    pub key: u64,
    pub len: u64,
    pub mtime_ms: u64,
    /// True when the file is not JSON of either recording shape.
    pub corrupt: bool,
    /// Refused recordings, counted by reason.
    pub refused: BTreeMap<String, usize>,
    pub sessions: Vec<StoredSession>,
}

/// A recording's metadata, as the cache keeps it. Linking flags are not stored: they
/// depend on the other files and are recomputed at each scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub producer: String,
    pub started: Option<DateTime<Utc>>,
    pub ended: Option<DateTime<Utc>>,
    pub in_progress: Option<bool>,
    pub model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub event_count: usize,
    pub run_id: u64,
    pub parent_key: Option<u64>,
}

impl StoredSession {
    pub fn from_session(s: &Session) -> Self {
        Self {
            producer: s.producer.clone(),
            started: s.started,
            ended: s.ended,
            in_progress: match s.in_progress {
                InProgress::True => Some(true),
                InProgress::False => Some(false),
                InProgress::NotEvidenced => None,
            },
            model: s.model.clone(),
            input_tokens: s.input_tokens,
            output_tokens: s.output_tokens,
            event_count: s.event_count,
            run_id: s.run_id,
            parent_key: s.parent_key,
        }
    }

    pub fn into_session(self) -> Session {
        Session {
            producer: self.producer,
            started: self.started,
            ended: self.ended,
            in_progress: match self.in_progress {
                Some(true) => InProgress::True,
                Some(false) => InProgress::False,
                None => InProgress::NotEvidenced,
            },
            model: self.model,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            event_count: self.event_count,
            is_subagent: false,
            parent_missing: false,
            child_count: 0,
            run_id: self.run_id,
            parent_key: self.parent_key,
        }
    }
}

/// The cache in memory.
#[derive(Debug, Default)]
pub struct Cache {
    pub probe: Option<Probe>,
    pub imports: Vec<ImportRecord>,
    /// When the next import cycle is due, once a cycle has ended.
    pub next_at: Option<DateTime<Utc>>,
    /// Summaries keyed by file key.
    pub entries: HashMap<u64, Entry>,
}

impl Cache {
    /// The import record for a producer key, if one exists.
    pub fn record(&self, producer: &str) -> Option<&ImportRecord> {
        self.imports.iter().find(|r| r.producer == producer)
    }

    /// The import record for a producer key, created empty if absent.
    pub fn record_mut(&mut self, producer: &str) -> &mut ImportRecord {
        if let Some(i) = self.imports.iter().position(|r| r.producer == producer) {
            return &mut self.imports[i];
        }
        self.imports.push(ImportRecord {
            producer: producer.to_string(),
            watermark: None,
            first_since: None,
            last_at: None,
            last_ok: false,
            last_text: String::new(),
        });
        self.imports.last_mut().expect("just pushed")
    }
}

#[derive(Deserialize)]
struct DiskIn {
    format: String,
    #[serde(default)]
    probe: Option<Probe>,
    #[serde(default)]
    imports: Vec<ImportRecord>,
    #[serde(default)]
    next_at: Option<DateTime<Utc>>,
    #[serde(default)]
    entries: Vec<Entry>,
}

#[derive(Serialize)]
struct DiskOut<'a> {
    format: &'static str,
    probe: &'a Option<Probe>,
    imports: &'a [ImportRecord],
    next_at: Option<DateTime<Utc>>,
    entries: Vec<&'a Entry>,
}

/// Read the cache beside the recordings. A missing, unreadable or foreign file is
/// an empty cache: it costs time, never correctness.
pub fn load(dir: &Path) -> Cache {
    let Ok(bytes) = std::fs::read(dir.join(FILE)) else {
        return Cache::default();
    };
    let Ok(disk) = serde_json::from_slice::<DiskIn>(&bytes) else {
        return Cache::default();
    };
    if disk.format != FORMAT {
        return Cache::default();
    }
    Cache {
        probe: disk.probe,
        imports: disk.imports,
        next_at: disk.next_at,
        entries: disk.entries.into_iter().map(|e| (e.key, e)).collect(),
    }
}

/// Write the cache atomically: a reader sees the old file or the new one, never half.
pub fn save(dir: &Path, cache: &Cache) -> Result<(), String> {
    let mut entries: Vec<&Entry> = cache.entries.values().collect();
    entries.sort_by_key(|e| e.key);
    let out = DiskOut {
        format: FORMAT,
        probe: &cache.probe,
        imports: &cache.imports,
        next_at: cache.next_at,
        entries,
    };
    let bytes =
        serde_json::to_vec(&out).map_err(|_| "could not serialise the scan cache".to_string())?;
    let tmp = dir.join(TMP);
    std::fs::write(&tmp, bytes).map_err(|_| "could not write the scan cache".to_string())?;
    std::fs::rename(&tmp, dir.join(FILE)).map_err(|_| {
        let _ = std::fs::remove_file(&tmp);
        "could not replace the scan cache".to_string()
    })
}

/// A file's modification time in milliseconds since the epoch, or 0 when unknown.
pub fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

/// The instant for a millisecond timestamp, if it is representable.
pub fn datetime_of_ms(ms: u64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(i64::try_from(ms).ok()?).single()
}

/// Read and summarise one recording file. An unreadable or unparseable file is a
/// corrupt entry, so it is not read again until it changes.
pub fn parse_file(path: &Path, key: u64, len: u64, mtime_ms: u64) -> Entry {
    let mut entry = Entry {
        key,
        len,
        mtime_ms,
        ..Entry::default()
    };
    let Ok(bytes) = std::fs::read(path) else {
        entry.corrupt = true;
        return entry;
    };
    match super::recording::read_file(&bytes) {
        Err(_) => entry.corrupt = true,
        Ok(items) => {
            for item in items {
                match item {
                    Ok(s) => entry.sessions.push(StoredSession::from_session(&s)),
                    Err(refusal) => {
                        *entry.refused.entry(refusal.reason()).or_default() += 1;
                    }
                }
            }
        }
    }
    entry
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ironmon-cache-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_saved_cache_loads_back_with_its_import_record() {
        let d = dir("roundtrip");
        let at = Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).single().unwrap();
        let mut c = Cache {
            probe: Some(Probe {
                version: Some("ironobs 0.1.0".into()),
                out: true,
                format_json: true,
                modified_since: true,
            }),
            next_at: Some(at),
            ..Cache::default()
        };
        c.record_mut("claude_code").watermark = Some(at);
        save(&d, &c).unwrap();
        let back = load(&d);
        assert_eq!(back.probe, c.probe);
        assert_eq!(back.record("claude_code").unwrap().watermark, Some(at));
        assert_eq!(back.next_at, Some(at));
    }

    #[test]
    fn a_foreign_or_damaged_cache_is_an_empty_one() {
        let d = dir("damaged");
        std::fs::write(d.join(FILE), b"{ not the cache").unwrap();
        assert!(load(&d).entries.is_empty());
        std::fs::write(d.join(FILE), br#"{"format":"other/9","entries":[]}"#).unwrap();
        assert!(load(&d).probe.is_none());
    }

    #[test]
    fn a_record_for_a_producer_is_created_once() {
        let mut c = Cache::default();
        c.record_mut("codex").last_ok = true;
        c.record_mut("codex").last_text = "x".into();
        assert_eq!(c.imports.len(), 1);
        assert!(c.record("codex").unwrap().last_ok);
    }
}
