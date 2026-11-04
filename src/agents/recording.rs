//! Reads IronObserver recording files, keeping metadata only.
//!
//! A recording is one run of an agent: a session, or a subagent the session
//! delegated to. IronObserver writes them as JSON, either one recording or an
//! array of them. The fields read here are the ones the Agents tab reports.
//! Everything else in a recording -- prompt and response text, tool arguments,
//! file contents -- is parsed past and dropped, so none of it can reach a screen,
//! a log or an ontology reading.
//!
//! Formats: `ironobserver.recording/1` and `/2` are read. A recording with no
//! `format` field is version 1, as IronObserver defines it. Any other format is
//! refused with a reason and counted; it is never guessed at.

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// The formats this build reads. A later major version is refused, not guessed at.
pub const FORMATS_READ: [&str; 2] = ["ironobserver.recording/1", "ironobserver.recording/2"];

/// Whether a recording says it is still being written.
///
/// The three states are kept apart on purpose. `NotEvidenced` means the producer
/// did not say, which is not the same as `False` (the source shows the run closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InProgress {
    True,
    False,
    NotEvidenced,
}

/// One agent run, reduced to metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    /// The producer, sanitised to a lowercase id segment such as `claude_code`.
    pub producer: String,
    pub started: Option<DateTime<Utc>>,
    pub ended: Option<DateTime<Utc>>,
    pub in_progress: InProgress,
    /// The last model named by any event, when one was named.
    pub model: Option<String>,
    /// Input tokens summed over events that report usage. `None` when no event did.
    pub input_tokens: Option<u64>,
    /// Output tokens summed over events that report usage. `None` when no event did.
    pub output_tokens: Option<u64>,
    pub event_count: usize,
    /// True when this run names a parent that is also in the scanned files.
    pub is_subagent: bool,
    /// True when this run names a parent that was not in the scanned files.
    pub parent_missing: bool,
    /// Subagent runs whose parent is this run.
    pub child_count: usize,
    /// The run's identifier, reduced to a 64-bit key by [`super::key_of`]. Kept only
    /// to link children to parents while a scan is assembled, and cached in that
    /// form, so no run identifier is ever stored, displayed or reported.
    pub(crate) run_id: u64,
    /// The parent's key, for the same linking.
    pub(crate) parent_key: Option<u64>,
}

/// Why a single recording was not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The recording names a format this build does not read.
    UnknownFormat(String),
    /// The recording is missing its run identifier or has a field of the wrong type.
    Malformed,
}

impl Refusal {
    pub fn reason(&self) -> String {
        match self {
            Refusal::UnknownFormat(f) => format!(
                "format {f} is not read by this IronMonitor (it reads {} and {}); \
                 a newer IronObserver needs a newer IronMonitor",
                FORMATS_READ[0], FORMATS_READ[1]
            ),
            Refusal::Malformed => "missing a run identifier, or a field has the wrong type".into(),
        }
    }
}

/// What a file held: its recordings, each read or refused. A file that is not JSON
/// at all is returned as an `Err` from [`read_file`] and counted as corrupt.
pub type FileContents = Vec<Result<Session, Refusal>>;

#[derive(Deserialize)]
struct RawRecording {
    run: String,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    events: Vec<RawEvent>,
    #[serde(default)]
    started_at: Option<String>,
    #[serde(default)]
    ended_at: Option<String>,
    #[serde(default)]
    in_progress: Option<bool>,
    #[serde(default)]
    producer: Option<String>,
}

/// Only the metadata fields. Text fields on an event are not declared, so serde
/// skips them and they are never kept.
#[derive(Deserialize)]
struct RawEvent {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<RawUsage>,
}

#[derive(Deserialize)]
struct RawUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
}

/// Read one recording file's bytes.
///
/// `Err` means the file is not JSON, or is JSON of neither shape. The caller
/// reports the file as corrupt and carries on with the rest.
pub fn read_file(bytes: &[u8]) -> Result<FileContents, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "not JSON".to_string())?;
    let items = match value {
        serde_json::Value::Array(items) => items,
        obj @ serde_json::Value::Object(_) => vec![obj],
        _ => return Err("neither a recording nor an array of them".into()),
    };
    Ok(items.into_iter().map(read_one).collect())
}

fn read_one(value: serde_json::Value) -> Result<Session, Refusal> {
    // A recording that does not even name its format is read as version 1, but
    // one that names a format this build cannot read is refused outright.
    if let Some(format) = value.get("format").and_then(|f| f.as_str()) {
        if !FORMATS_READ.contains(&format) {
            return Err(Refusal::UnknownFormat(known_shape(format)));
        }
    } else if value.get("format").is_some_and(|f| !f.is_null()) {
        return Err(Refusal::Malformed);
    }
    let raw: RawRecording = serde_json::from_value(value).map_err(|_| Refusal::Malformed)?;
    if raw.run.trim().is_empty() {
        return Err(Refusal::Malformed);
    }
    Ok(session_from(raw))
}

/// Echo a format name only if it has the shape IronObserver uses. Anything else is
/// shown as a placeholder, so a malformed file cannot put arbitrary text on screen.
fn known_shape(format: &str) -> String {
    let well_formed = format.len() <= 64
        && format.starts_with("ironobserver.recording/")
        && format[23..].chars().all(|c| c.is_ascii_digit())
        && format.len() > 23;
    if well_formed {
        format.to_string()
    } else {
        "(unrecognised)".to_string()
    }
}

fn session_from(raw: RawRecording) -> Session {
    let mut model = None;
    let mut input: Option<u64> = None;
    let mut output: Option<u64> = None;
    for event in &raw.events {
        if let Some(m) = event.model.as_deref().filter(|m| !m.trim().is_empty()) {
            model = Some(m.to_string());
        }
        if let Some(u) = &event.usage {
            if let Some(i) = u.input_tokens {
                input = Some(input.unwrap_or(0).saturating_add(i));
            }
            if let Some(o) = u.output_tokens {
                output = Some(output.unwrap_or(0).saturating_add(o));
            }
        }
    }
    let in_progress = match raw.in_progress {
        Some(true) => InProgress::True,
        Some(false) => InProgress::False,
        None => InProgress::NotEvidenced,
    };
    Session {
        producer: sanitise_producer(raw.producer.as_deref()),
        started: parse_time(raw.started_at.as_deref()),
        ended: parse_time(raw.ended_at.as_deref()),
        in_progress,
        model,
        input_tokens: input,
        output_tokens: output,
        event_count: raw.events.len(),
        is_subagent: false,
        parent_missing: false,
        child_count: 0,
        run_id: super::key_of(&raw.run),
        parent_key: raw
            .parent
            .filter(|p| !p.trim().is_empty())
            .map(|p| super::key_of(&p)),
    }
}

/// A producer name as a single id segment. `claude-code` becomes `claude_code`.
/// Absent or empty is `unknown`, which is itself a fact worth showing.
pub fn sanitise_producer(raw: Option<&str>) -> String {
    let cleaned: String = raw
        .unwrap_or("")
        .trim()
        .chars()
        .take(32)
        .map(|c| match c.to_ascii_lowercase() {
            c @ ('a'..='z' | '0'..='9') => c,
            _ => '_',
        })
        .collect();
    if cleaned.trim_matches('_').is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

fn parse_time(text: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text?)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Link subagent runs to the parents they name, within one scan.
///
/// A child whose parent is in the set is marked a subagent and counted against the
/// parent. A child whose parent is absent keeps its own row and is marked
/// `parent_missing`: it is not promoted silently, and it is not hidden.
pub fn link(sessions: &mut [Session]) {
    use std::collections::HashMap;
    // Linking is recomputed from scratch, so a cached summary can be linked again
    // in a different set of files without carrying an old link forward.
    for s in sessions.iter_mut() {
        s.is_subagent = false;
        s.parent_missing = false;
        s.child_count = 0;
    }
    let index: HashMap<u64, usize> = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| (s.run_id, i))
        .collect();
    let mut child_counts: HashMap<usize, usize> = HashMap::new();
    for s in sessions.iter_mut() {
        let Some(parent) = s.parent_key else {
            continue;
        };
        if parent == s.run_id {
            continue;
        }
        match index.get(&parent) {
            Some(&p) => {
                s.is_subagent = true;
                *child_counts.entry(p).or_default() += 1;
            }
            None => s.parent_missing = true,
        }
    }
    for (p, n) in child_counts {
        sessions[p].child_count = n;
    }
}

/// The recordings that are sessions rather than subagents of another run.
pub fn is_top_level(s: &Session) -> bool {
    !s.is_subagent
}

#[cfg(test)]
mod tests {
    use super::*;

    const V2: &str = r#"[
      {"format":"ironobserver.recording/2","run":"r1","producer":"claude-code",
       "started_at":"2026-03-01T12:00:00Z","ended_at":"2026-03-01T12:30:00Z",
       "in_progress":false,
       "events":[
         {"seq":0,"request":"SECRET PROMPT","response":"SECRET ANSWER","at":"2026-03-01T12:01:00Z",
          "model":"model-a","usage":{"input_tokens":10,"output_tokens":4}},
         {"seq":1,"request":"x","response":"y","usage":{"input_tokens":5}}
       ],
       "future_field":{"anything":true}}
    ]"#;

    #[test]
    fn version_two_reads_metadata_and_drops_text() {
        let file = read_file(V2.as_bytes()).unwrap();
        let s = file[0].as_ref().unwrap();
        assert_eq!(s.producer, "claude_code");
        assert_eq!(s.in_progress, InProgress::False);
        assert_eq!(s.model.as_deref(), Some("model-a"));
        assert_eq!(s.input_tokens, Some(15));
        assert_eq!(s.output_tokens, Some(4));
        assert_eq!(s.event_count, 2);
        assert!(s.started.is_some() && s.ended.is_some());
        let debug = format!("{s:?}");
        assert!(
            !debug.contains("SECRET"),
            "text leaked into a session: {debug}"
        );
    }

    #[test]
    fn version_one_reads_without_format_field() {
        let v1 = r#"{"run":"old","events":[]}"#;
        let file = read_file(v1.as_bytes()).unwrap();
        let s = file[0].as_ref().unwrap();
        assert_eq!(s.in_progress, InProgress::NotEvidenced);
        assert_eq!(s.producer, "unknown");
        assert_eq!(s.input_tokens, None);
    }

    #[test]
    fn version_one_explicit_is_read() {
        let v1 = r#"{"format":"ironobserver.recording/1","run":"old","events":[]}"#;
        assert!(read_file(v1.as_bytes()).unwrap()[0].is_ok());
    }

    #[test]
    fn unknown_major_format_is_refused_with_a_reason() {
        let v3 = r#"[{"format":"ironobserver.recording/3","run":"new","events":[]},
                     {"format":"ironobserver.recording/2","run":"ok","events":[]}]"#;
        let file = read_file(v3.as_bytes()).unwrap();
        match &file[0] {
            Err(r @ Refusal::UnknownFormat(f)) => {
                assert_eq!(f, "ironobserver.recording/3");
                assert!(r.reason().contains("newer IronMonitor"));
            }
            other => panic!("expected refusal, got {other:?}"),
        }
        assert!(
            file[1].is_ok(),
            "the readable recording beside it is still read"
        );
    }

    #[test]
    fn odd_format_strings_are_not_echoed() {
        let weird = r#"{"format":"<script>alert(1)</script>","run":"x","events":[]}"#;
        let file = read_file(weird.as_bytes()).unwrap();
        match &file[0] {
            Err(Refusal::UnknownFormat(f)) => assert_eq!(f, "(unrecognised)"),
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn in_progress_has_three_states() {
        let t = r#"[{"run":"a","in_progress":true,"events":[]},
                    {"run":"b","in_progress":false,"events":[]},
                    {"run":"c","events":[]}]"#;
        let file = read_file(t.as_bytes()).unwrap();
        let states: Vec<InProgress> = file
            .iter()
            .map(|r| r.as_ref().unwrap().in_progress)
            .collect();
        assert_eq!(
            states,
            vec![
                InProgress::True,
                InProgress::False,
                InProgress::NotEvidenced
            ]
        );
    }

    #[test]
    fn a_non_string_in_progress_is_malformed_not_false() {
        let t = r#"[{"run":"a","in_progress":"yes","events":[]}]"#;
        let file = read_file(t.as_bytes()).unwrap();
        assert_eq!(file[0], Err(Refusal::Malformed));
    }

    #[test]
    fn a_file_that_is_not_json_is_an_error_for_the_caller() {
        assert!(read_file(b"not json at all").is_err());
        assert!(read_file(b"42").is_err());
    }

    #[test]
    fn subagents_link_to_parents_and_orphans_are_flagged() {
        let t = r#"[
          {"run":"root","events":[]},
          {"run":"child1","parent":"root","events":[]},
          {"run":"child2","parent":"root","events":[]},
          {"run":"orphan","parent":"gone","events":[]}
        ]"#;
        let mut sessions: Vec<Session> = read_file(t.as_bytes())
            .unwrap()
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        link(&mut sessions);
        assert_eq!(sessions[0].child_count, 2);
        assert!(!sessions[0].is_subagent);
        assert!(sessions[1].is_subagent && sessions[2].is_subagent);
        assert!(!sessions[3].is_subagent && sessions[3].parent_missing);
        assert_eq!(sessions.iter().filter(|s| is_top_level(s)).count(), 2);
    }

    #[test]
    fn producer_names_are_segment_safe() {
        assert_eq!(sanitise_producer(Some("Claude Code")), "claude_code");
        assert_eq!(sanitise_producer(Some("a.b/c")), "a_b_c");
        assert_eq!(sanitise_producer(Some("   ")), "unknown");
        assert_eq!(sanitise_producer(None), "unknown");
    }
}
