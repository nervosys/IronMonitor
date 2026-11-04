//! What the Agents tab says, as plain text, before either surface draws it.
//!
//! The TUI and the GUI both render a [`Report`], so the two cannot disagree about a
//! count, a time or a reason. Building it reads only [`AgentsState`], which is
//! already in memory: nothing here touches disk.
//!
//! Privacy: a row names a producer, a model, times, counts and token totals. It
//! never names a session, a run, a prompt or a project path. A folder is named only
//! by its final component, so no full path reaches a screen.

use super::ironobs::{AgentsState, Cycle, ProducerImport};
use super::recording::{InProgress, Session};
use super::{
    relative, EngineRequests, Scan, KNOWN_PRODUCERS, REMOTE_COST_POINTER, REMOTE_NOT_READ,
};
use chrono::{DateTime, Utc};

/// Most rows an Active or Recent section shows. The rest are counted, not listed.
pub const LIST_CAP: usize = 25;
/// Most workflows listed.
pub const WORKFLOW_CAP: usize = 10;

/// One titled block of rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: &'static str,
    pub rows: Vec<String>,
    /// How many rows were left out of `rows` by the cap, stated to the reader.
    pub not_shown: usize,
}

impl Section {
    fn new(title: &'static str) -> Self {
        Self {
            title,
            rows: Vec::new(),
            not_shown: 0,
        }
    }
}

/// The whole tab, as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// One line each, shown above the sections.
    pub summary: Vec<String>,
    pub sections: Vec<Section>,
}

/// Build the tab's text from the cached state and the Inference poller's engine figures.
pub fn build(state: &AgentsState, engine: Option<EngineRequests>, now: DateTime<Utc>) -> Report {
    let scan = state.scan_or_empty();
    let mut summary = Vec::new();
    summary.push(sessions_line(state.scan.as_ref(), &scan));
    summary.push(format!("Import: {}", cycle_text(state, now)));
    summary.push(format!(
        "Recordings: {} file(s), {} recording(s) read, {} file(s) unusable, {} recording(s) refused{}",
        scan.files,
        scan.recordings(),
        scan.corrupt_files,
        scan.refused_count(),
        if scan.files_truncated > 0 {
            format!(", {} file(s) past the read bound", scan.files_truncated)
        } else {
            String::new()
        }
    ));

    let mut sections = Vec::new();

    let mut active = Section::new("Active");
    let active_sessions: Vec<&Session> = scan
        .sessions
        .iter()
        .filter(|s| s.in_progress == InProgress::True)
        .collect();
    if scan.recordings() == 0 || !scan.is_complete() {
        active.rows.push(format!("unknown: {}", none_reason(&scan)));
    } else if active_sessions.is_empty() {
        active.rows.push("no recording shows in progress".into());
    } else {
        for s in active_sessions.iter().take(LIST_CAP) {
            active.rows.push(session_row(s, now));
        }
        active.not_shown = active_sessions.len().saturating_sub(LIST_CAP);
    }
    sections.push(active);

    let mut recent = Section::new("Recent");
    if scan.recordings() == 0 || !scan.is_complete() {
        recent.rows.push(format!("unknown: {}", none_reason(&scan)));
    } else {
        let mut tops: Vec<&Session> = scan
            .sessions
            .iter()
            .filter(|s| super::recording::is_top_level(s))
            .collect();
        tops.sort_by_key(|s| std::cmp::Reverse(s.started.or(s.ended)));
        for s in tops.iter().take(LIST_CAP) {
            recent.rows.push(session_row(s, now));
        }
        recent.not_shown = tops.len().saturating_sub(LIST_CAP);
    }
    sections.push(recent);

    let mut workflows = Section::new("Workflows");
    if scan.recordings() == 0 || !scan.is_complete() {
        workflows
            .rows
            .push(format!("unknown: {}", none_reason(&scan)));
    } else {
        let mut flows: Vec<&Session> = scan
            .sessions
            .iter()
            .filter(|s| super::recording::is_top_level(s) && s.child_count > 0)
            .collect();
        flows.sort_by_key(|s| std::cmp::Reverse(s.started.or(s.ended)));
        if flows.is_empty() {
            workflows
                .rows
                .push("no multi-agent runs in the recordings read".into());
        }
        for s in flows.iter().take(WORKFLOW_CAP) {
            workflows.rows.push(format!(
                "{} · {} subagent run(s) · started {} · {} event(s)",
                producer_label(&s.producer),
                s.child_count,
                time_or_unknown(s.started, now),
                s.event_count
            ));
        }
        workflows.not_shown = flows.len().saturating_sub(WORKFLOW_CAP);
    }
    sections.push(workflows);

    let mut engines = Section::new("Engine requests");
    match engine {
        None => engines
            .rows
            .push("unknown: the Inference tab's engine poller is not running".into()),
        Some(e) if !e.ever_answered => {
            // Nothing has ever come back, so there is no figure to be stale.
            engines
                .rows
                .push("no engine answered; no figures are shown".into());
        }
        Some(e) => {
            engines.rows.push(match e.active {
                Some(n) => format!("{n} request(s) in flight, from IronWorks"),
                None => "in flight: unknown, the engine did not report it".into(),
            });
            engines.rows.push(match e.recent {
                Some(n) => format!("{n} entr(y/ies) in the engine's recent-request buffer"),
                None => "recent requests: unknown, the engine did not report them".into(),
            });
            if !e.reachable {
                engines.rows.push(
                    "the engine did not answer its last poll; these figures may be stale".into(),
                );
            }
        }
    }
    engines
        .rows
        .push("Provider cost and usage are on the Inference tab.".into());
    sections.push(engines);

    let mut coverage = Section::new("Coverage");
    for p in KNOWN_PRODUCERS {
        let n = scan.producer_count(p.key);
        let import = state.imports.iter().find(|i| i.importer.key() == p.key);
        let row = match (p.not_read, import) {
            (Some(why), _) => format!("Not read: {} — {why}", p.label),
            (None, Some(imp)) => producer_row(p.label, n, imp, state, now),
            (None, None) => format!("Not read: {} — not imported by this IronMonitor", p.label),
        };
        coverage.rows.push(row);
    }
    for key in scan.producers() {
        if KNOWN_PRODUCERS.iter().any(|p| p.key == key) {
            continue;
        }
        coverage.rows.push(format!(
            "Read: {} — {} recording(s)",
            producer_label(&key),
            scan.producer_count(&key)
        ));
    }
    coverage.rows.push(format!("Not read: {REMOTE_NOT_READ}"));
    coverage.rows.push(format!("Note: {REMOTE_COST_POINTER}"));
    sections.push(coverage);

    Report { summary, sections }
}

/// One producer's coverage row: what was read, and the state of its import.
fn producer_row(
    label: &str,
    n: usize,
    imp: &ProducerImport,
    state: &AgentsState,
    now: DateTime<Utc>,
) -> String {
    let head = if !imp.source_exists && n == 0 {
        format!(
            "Not read: {label} — no `{}` folder to import",
            imp.importer.source_folder()
        )
    } else if n > 0 {
        format!("Read: {label} — {n} recording(s)")
    } else {
        format!("Read when ironobs imports it: {label} — none in the recordings read yet")
    };
    let mut notes = Vec::new();
    if imp.running {
        notes.push("importing now".to_string());
    }
    if let Cycle::Blocked(why) = &state.cycle {
        notes.push(why.clone());
    }
    if let Some(last) = &imp.last {
        notes.push(format!(
            "last import {}: {}",
            relative(last.at, now),
            last.text
        ));
    }
    if imp.source_exists {
        notes.push(window_note(imp));
    }
    if notes.is_empty() {
        head
    } else {
        format!("{head} · {}", notes.join(" · "))
    }
}

/// The first import's selection, stated on every row that has a source folder: the window
/// before the first import is the only history that is ever read.
fn window_note(imp: &ProducerImport) -> String {
    match (imp.watermark, imp.first_since) {
        (Some(w), _) => format!(
            "sessions modified since {} are selected; sessions not written in the 7 days before the first import are not imported",
            relative(w, Utc::now())
        ),
        (None, Some(t)) => format!(
            "sessions not written in the 7 days before the first import (from {}) are not imported",
            t.to_rfc3339()
        ),
        (None, None) => {
            "the first import selects sessions written in the 7 days before it runs; older sessions are not imported".into()
        }
    }
}

fn sessions_line(raw: Option<&Scan>, scan: &Scan) -> String {
    if raw.is_none() {
        return "Sessions: unknown — the first scan has not finished".into();
    }
    if scan.recordings() == 0 || !scan.is_complete() {
        return format!("Sessions: unknown — {}", none_reason(scan));
    }
    format!(
        "Sessions: {} · {} recording(s) show in progress · {} subagent run(s) · {} workflow(s)",
        scan.sessions_count(),
        scan.active_count(),
        scan.subagent_count(),
        scan.workflow_count()
    )
}

/// The final component of a folder, for display. A full path is never shown.
pub fn folder_name(dir: &std::path::Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "(root)".to_string())
}

/// Why no recording is shown, in the same words the ontology uses.
pub fn none_reason(scan: &Scan) -> String {
    if !scan.dir_exists {
        return format!(
            "the recordings folder '{}' does not exist yet; ironobs writes recordings when it imports",
            folder_name(&scan.dir)
        );
    }
    if !scan.is_complete() {
        return format!(
            "read {} of {} recording file(s) before the read bound; the rest are not counted",
            scan.files,
            scan.files + scan.files_truncated
        );
    }
    if scan.files == 0 {
        return "no recording files yet; ironobs writes them when it imports".into();
    }
    format!(
        "{} file(s) read, none usable ({} corrupt, {} refused)",
        scan.files,
        scan.corrupt_files,
        scan.refused_count()
    )
}

/// What the import cycle is doing, as one line. Shared by the tab and the resolver.
pub fn cycle_text(state: &AgentsState, now: DateTime<Utc>) -> String {
    match &state.cycle {
        Cycle::Pending => "pending: the first import has not started".into(),
        Cycle::Blocked(why) => format!("not run: {why}"),
        Cycle::Running {
            since,
            by_this_window,
        } => format!(
            "running since {}{}; each producer is stopped at 30 min",
            relative(*since, now),
            if *by_this_window {
                ""
            } else {
                " (started by another IronMonitor window)"
            }
        ),
        Cycle::Waiting { last_ended: None } => match state.next_at {
            Some(t) => format!(
                "pending: the first import has not finished; next cycle {}",
                relative(t, now)
            ),
            None => "pending: the first import has not started".into(),
        },
        Cycle::Waiting {
            last_ended: Some(t),
        } => match state.next_at {
            Some(next) => format!(
                "idle; last cycle ended {}; next cycle {}",
                relative(*t, now),
                relative(next, now)
            ),
            None => format!("idle; last cycle ended {}", relative(*t, now)),
        },
    }
}

fn session_row(s: &Session, now: DateTime<Utc>) -> String {
    let status = match s.in_progress {
        InProgress::True => "in progress",
        InProgress::False => "closed",
        InProgress::NotEvidenced => "in progress not evidenced",
    };
    let tokens = match (s.input_tokens, s.output_tokens) {
        (None, None) => "tokens unknown".to_string(),
        (i, o) => format!(
            "tokens in {} / out {}",
            i.map_or("unknown".to_string(), |v| v.to_string()),
            o.map_or("unknown".to_string(), |v| v.to_string())
        ),
    };
    let child = if s.is_subagent {
        " · subagent"
    } else if s.parent_missing {
        " · parent not read"
    } else {
        ""
    };
    format!(
        "{}{child} · {} · started {} · ended {} · {} event(s) · {tokens} · {status}",
        producer_label(&s.producer),
        s.model.as_deref().unwrap_or("model unknown"),
        time_or_unknown(s.started, now),
        time_or_unknown(s.ended, now),
        s.event_count,
    )
}

/// The label for a producer key: its known name, or the key itself.
pub fn producer_label(key: &str) -> String {
    KNOWN_PRODUCERS
        .iter()
        .find(|p| p.key == key)
        .map_or_else(|| key.to_string(), |p| p.label.to_string())
}

fn time_or_unknown(t: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    t.map_or("unknown".to_string(), |t| relative(t, now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::ironobs::{Importer, ProducerImport};
    use crate::agents::recording::read_file;
    use crate::agents::Scan;
    use std::path::PathBuf;

    fn state_with(json: &str) -> AgentsState {
        let mut sessions: Vec<Session> = read_file(json.as_bytes())
            .unwrap()
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        crate::agents::recording::link(&mut sessions);
        let scan = Scan {
            dir: PathBuf::from("/synthetic/recordings"),
            dir_exists: true,
            files: 1,
            file_bytes: 0,
            corrupt_files: 0,
            refused: Default::default(),
            files_truncated: 0,
            sessions,
            newest_written: None,
            taken_at: Utc::now(),
        };
        AgentsState {
            recordings_dir: PathBuf::from("/synthetic/recordings"),
            ironobs: None,
            cycle: Cycle::Waiting { last_ended: None },
            next_at: None,
            imports: [Importer::ClaudeCode, Importer::Codex]
                .into_iter()
                .map(|importer| ProducerImport {
                    importer,
                    source: PathBuf::from("/synthetic/source"),
                    source_exists: false,
                    watermark: None,
                    last: None,
                    running: false,
                    first_since: None,
                })
                .collect(),
            scan: Some(scan),
        }
    }

    fn section<'a>(r: &'a Report, title: &str) -> &'a Section {
        r.sections.iter().find(|s| s.title == title).unwrap()
    }

    fn engine(answered: bool, reachable: bool) -> EngineRequests {
        EngineRequests {
            active: None,
            recent: None,
            reachable,
            ever_answered: answered,
        }
    }

    #[test]
    fn a_report_with_no_recordings_says_unknown_not_zero() {
        let mut st = state_with("[]");
        st.scan.as_mut().unwrap().files = 0;
        let r = build(&st, None, Utc::now());
        assert!(
            r.summary[0].starts_with("Sessions: unknown"),
            "{:?}",
            r.summary
        );
        assert!(!r.summary[0].contains(": 0"));
    }

    #[test]
    fn before_the_first_scan_the_tab_says_so_and_does_not_claim_a_missing_folder() {
        let mut st = state_with("[]");
        st.scan = None;
        let r = build(&st, None, Utc::now());
        assert_eq!(
            r.summary[0],
            "Sessions: unknown — the first scan has not finished"
        );
    }

    #[test]
    fn a_truncated_scan_reports_unknown_counts_not_partial_sums() {
        let mut st = state_with(r#"[{"run":"a","in_progress":true,"events":[]}]"#);
        st.scan.as_mut().unwrap().files_truncated = 4;
        let r = build(&st, None, Utc::now());
        assert!(
            r.summary[0].starts_with("Sessions: unknown"),
            "{:?}",
            r.summary
        );
        assert!(r.summary[0].contains("read 1 of 5"), "{:?}", r.summary);
    }

    #[test]
    fn the_three_in_progress_states_are_worded_apart() {
        let st = state_with(
            r#"[{"run":"a","in_progress":true,"events":[]},
                {"run":"b","in_progress":false,"events":[]},
                {"run":"c","events":[]}]"#,
        );
        let r = build(&st, None, Utc::now());
        let active = section(&r, "Active");
        assert_eq!(active.rows.len(), 1);
        assert!(active.rows[0].ends_with("in progress"));
        let recent = section(&r, "Recent").rows.join("\n");
        assert!(recent.contains("closed"));
        assert!(recent.contains("in progress not evidenced"));
    }

    #[test]
    fn lists_are_capped_and_the_cut_is_stated() {
        let rows: Vec<String> = (0..(LIST_CAP + 7))
            .map(|i| format!(r#"{{"run":"r{i}","in_progress":true,"events":[]}}"#))
            .collect();
        let st = state_with(&format!("[{}]", rows.join(",")));
        let r = build(&st, None, Utc::now());
        let active = section(&r, "Active");
        assert_eq!(active.rows.len(), LIST_CAP);
        assert_eq!(active.not_shown, 7);
    }

    #[test]
    fn absent_times_and_models_render_as_unknown() {
        let st = state_with(r#"[{"run":"a","events":[]}]"#);
        let r = build(&st, None, Utc::now());
        let row = &section(&r, "Recent").rows[0];
        assert!(row.contains("started unknown"), "{row}");
        assert!(row.contains("model unknown"), "{row}");
        assert!(row.contains("tokens unknown"), "{row}");
    }

    #[test]
    fn subagents_show_under_their_parents_workflow() {
        let st = state_with(
            r#"[{"run":"root","producer":"claude-code","events":[]},
                {"run":"s1","parent":"root","producer":"claude-code","events":[]}]"#,
        );
        let r = build(&st, None, Utc::now());
        let wf = section(&r, "Workflows");
        assert_eq!(wf.rows.len(), 1);
        assert!(wf.rows[0].contains("1 subagent run(s)"), "{:?}", wf.rows);
        assert!(wf.rows[0].starts_with("Claude Code"));
    }

    #[test]
    fn no_run_identifier_or_session_text_reaches_the_report() {
        let st = state_with(
            r#"[{"run":"SECRET-RUN-ID","producer":"claude-code","in_progress":true,
                 "events":[{"model":"m","request":"SECRET PROMPT","response":"SECRET ANSWER"}]}]"#,
        );
        let r = build(&st, None, Utc::now());
        let all = format!("{:?}", r);
        assert!(!all.contains("SECRET"), "{all}");
    }

    #[test]
    fn no_full_path_reaches_the_report() {
        let st = state_with("[]");
        let mut st = st;
        st.scan.as_mut().unwrap().files = 0;
        st.scan.as_mut().unwrap().dir_exists = false;
        let r = build(&st, None, Utc::now());
        let all = format!("{r:?}");
        assert!(
            !all.contains("/synthetic"),
            "a full path reached the report: {all}"
        );
        assert!(
            all.contains("'recordings'"),
            "the final folder name is shown: {all}"
        );
    }

    #[test]
    fn gemini_is_listed_as_not_read_with_the_reason() {
        let st = state_with("[]");
        let r = build(&st, None, Utc::now());
        let cov = section(&r, "Coverage").rows.join("\n");
        assert!(cov.contains("Not read: Gemini CLI"), "{cov}");
        assert!(cov.contains("stores no chats on disk"), "{cov}");
        assert!(cov.contains("Not read: cloud"), "{cov}");
    }

    #[test]
    fn a_missing_source_folder_is_a_reason_not_a_failure() {
        let st = state_with("[]");
        let r = build(&st, None, Utc::now());
        let cov = section(&r, "Coverage").rows.join("\n");
        assert!(cov.contains("no `sessions` folder to import"), "{cov}");
    }

    #[test]
    fn engine_requests_are_unknown_without_the_poller_and_never_zero() {
        let st = state_with("[]");
        let r = build(&st, None, Utc::now());
        let eng = section(&r, "Engine requests").rows.join("\n");
        assert!(eng.contains("unknown"), "{eng}");
        let r2 = build(
            &st,
            Some(EngineRequests {
                active: None,
                recent: Some(3),
                reachable: true,
                ever_answered: true,
            }),
            Utc::now(),
        );
        let eng2 = section(&r2, "Engine requests").rows.join("\n");
        assert!(eng2.contains("in flight: unknown"), "{eng2}");
    }

    #[test]
    fn an_engine_that_never_answered_gets_one_line_and_no_figures() {
        let st = state_with("[]");
        let r = build(&st, Some(engine(false, false)), Utc::now());
        let rows = &section(&r, "Engine requests").rows;
        assert_eq!(rows[0], "no engine answered; no figures are shown");
        assert!(
            !rows.iter().any(|row| row.contains("stale")),
            "stale wording with no sample: {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.contains("in flight")),
            "{rows:?}"
        );
    }

    #[test]
    fn stale_wording_is_used_only_after_a_real_sample() {
        let st = state_with("[]");
        let r = build(&st, Some(engine(true, false)), Utc::now());
        let rows = &section(&r, "Engine requests").rows;
        assert!(
            rows.iter().any(|row| row.contains("may be stale")),
            "{rows:?}"
        );
    }
}
