//! The Agents tab, end to end, over synthetic recording files.
//!
//! The recordings directory is a temporary folder of this test's own, named through
//! `IRONMON_RECORDINGS_DIR`. It holds every shape the tab has to report: a version 1
//! recording with no format field, a version 2 parent with a subagent, a recording
//! that does not say whether it is in progress, and a file that is not JSON. Each
//! run is asked for through the built `ironmon` binary, so argv, environment and
//! rendering are the surfaces an agent touches.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Text that must never reach a frame, a count or a `get`: it is what a recording's
/// events carry, and the tab reads metadata only.
const SECRET: &str = "SECRET-PROMPT-TEXT";

fn ironmon(recordings: &Path) -> Command {
    // No provider key reaches the child, as in `agentic_contract.rs`.
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironmon"));
    for key in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "IRONMON_OPENAI_ADMIN_KEY",
        "IRONMON_ANTHROPIC_ADMIN_KEY",
    ] {
        command.env_remove(key);
    }
    // Source folders that do not exist, so the background import has nothing to
    // run and this test never reads a developer's real sessions.
    command
        .env("IRONMON_RECORDINGS_DIR", recordings)
        .env(
            "IRONMON_CLAUDE_CODE_DIR",
            recordings.join("no-claude-projects"),
        )
        .env("IRONMON_CODEX_DIR", recordings.join("no-codex-sessions"));
    command
}

fn fixture_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("ironmon-agents-e2e-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the fixture directory");
    write_recordings(&dir);
    dir
}

/// The synthetic recordings. Each event carries prompt-like text that must not show.
fn write_recordings(dir: &Path) {
    // Version 1, one recording, no format field: in progress.
    std::fs::write(
        dir.join("a_single.json"),
        format!(
            r#"{{"run":"solo","producer":"claude-code","in_progress":true,
                "events":[{{"model":"model-a","request":"{SECRET}","response":"{SECRET}",
                "usage":{{"input_tokens":12,"output_tokens":3}}}}]}}"#
        ),
    )
    .unwrap();
    // Version 2: a parent that is closed, and its subagent, which is also closed.
    std::fs::write(
        dir.join("b_pair.json"),
        format!(
            r#"[{{"format":"ironobserver.recording/2","run":"root","producer":"claude-code",
                 "in_progress":false,"started_at":"2026-03-01T12:00:00Z",
                 "ended_at":"2026-03-01T12:30:00Z",
                 "events":[{{"request":"{SECRET}","response":"{SECRET}"}}]}},
                {{"format":"ironobserver.recording/2","run":"sub","parent":"root",
                 "producer":"claude-code","in_progress":false,
                 "events":[{{"request":"{SECRET}"}}]}}]"#
        ),
    )
    .unwrap();
    // A recording that does not say whether it is still being written.
    std::fs::write(
        dir.join("c_unevidenced.json"),
        r#"{"format":"ironobserver.recording/2","run":"unknown-state","producer":"codex","events":[]}"#,
    )
    .unwrap();
    // Not JSON at all: counted as unusable, and the others are still read.
    std::fs::write(dir.join("d_corrupt.json"), "{ this is not json").unwrap();
    // A file that is not a recording file is not read.
    std::fs::write(dir.join("notes.txt"), SECRET).unwrap();
}

fn run(command: &mut Command) -> Output {
    command.output().expect("run the built ironmon binary")
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Top-level sessions: `solo`, `root`, `unknown-state`. The subagent is not one.
const SESSIONS: i64 = 3;

#[test]
fn get_reads_the_session_count_from_the_recordings() {
    let dir = fixture_dir("get");
    let out = run(ironmon(&dir).args(["get", "agents.sessions.count", "--format", "json"]));
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_str(&text(&out)).expect("get --format json prints an object");
    assert_eq!(value["value"], serde_json::json!(SESSIONS), "{value}");
    assert_eq!(value["provenance"], "measured", "{value}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn get_counts_the_unusable_file_and_the_in_progress_state() {
    let dir = fixture_dir("get-states");
    let rejected =
        run(ironmon(&dir).args(["get", "agents.recordings.rejected", "--format", "json"]));
    let v: serde_json::Value = serde_json::from_str(&text(&rejected)).unwrap();
    assert_eq!(
        v["value"],
        serde_json::json!(1),
        "the corrupt file is counted: {v}"
    );
    let active = run(ironmon(&dir).args(["get", "agents.sessions.active", "--format", "json"]));
    let v: serde_json::Value = serde_json::from_str(&text(&active)).unwrap();
    assert_eq!(
        v["value"],
        serde_json::json!(1),
        "only `solo` says in progress: {v}"
    );
    let unevidenced = run(ironmon(&dir).args([
        "get",
        "agents.sessions.in_progress_unevidenced",
        "--format",
        "json",
    ]));
    let v: serde_json::Value = serde_json::from_str(&text(&unevidenced)).unwrap();
    assert_eq!(v["value"], serde_json::json!(1), "{v}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tui_agents_frame_shows_the_recordings_and_no_path_or_session_text() {
    let dir = fixture_dir("tui");
    let out = run(ironmon(&dir).args([
        "tui", "--frame", "--tab", "Agents", "--width", "160", "--height", "40",
    ]));
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let frame = text(&out);
    assert!(
        frame.contains(&format!("Sessions: {SESSIONS} ")),
        "the frame does not show the session count:\n{frame}"
    );
    assert!(frame.contains("1 subagent run(s)"), "{frame}");
    assert!(frame.contains("1 workflow(s)"), "{frame}");
    assert!(frame.contains("Coverage"), "{frame}");
    assert_no_leak(&frame);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "gui")]
#[test]
fn gui_agents_frame_shows_the_recordings_and_no_path_or_session_text() {
    let dir = fixture_dir("gui");
    let out = run(ironmon(&dir).args(["gui", "--frame", "--tab", "agents"]));
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let frame = text(&out);
    assert!(
        frame.contains(&format!("Sessions: {SESSIONS} ")),
        "the frame does not show the session count:\n{frame}"
    );
    assert_no_leak(&frame);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A rendered frame names no full path, no Claude Code folder, and no `.jsonl`
/// file, and carries none of the text the recordings' events hold.
fn assert_no_leak(frame: &str) {
    let sep_users = frame.contains("\\Users") || frame.contains("/Users/");
    assert!(
        !sep_users,
        "a path to a home folder reached the frame:\n{frame}"
    );
    assert!(
        !frame.contains(".claude"),
        "a Claude folder path reached the frame:\n{frame}"
    );
    assert!(
        !frame.contains(".jsonl"),
        "a session file name reached the frame:\n{frame}"
    );
    assert!(
        !frame.contains(SECRET),
        "session text reached the frame:\n{frame}"
    );
}
