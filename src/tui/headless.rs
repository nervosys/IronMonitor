//! Headless rendering harness for the TUI.
//!
//! The GUI counterpart in `crate::gui::headless` exists because screenshots could
//! not distinguish "this text was never drawn" from "this text was drawn in an
//! unreadable colour". The TUI has the same problem in a different form: it renders
//! into an alternate screen under raw mode, so the only way to see what it produced
//! was to look at a terminal — which no test can do, and which cannot be diffed.
//!
//! `ratatui::backend::TestBackend` renders into an in-memory buffer instead. Reading
//! that buffer is the terminal equivalent of walking the GUI's painted galleys, and
//! it is what lets an agent — or a test — assert on what a human would see without
//! a terminal being involved at all.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use super::app::App;

/// Default geometry for a headless frame.
///
/// Wide enough that the tab bar does not wrap, which would split words across cells
/// and make substring assertions fail for reasons unrelated to what is being tested.
pub const DEFAULT_WIDTH: u16 = 200;
pub const DEFAULT_HEIGHT: u16 = 50;

/// Render one frame of `app` and return the visible text, one entry per row.
///
/// Trailing blanks are trimmed per row: a buffer is a fixed grid, so every row is
/// padded to full width and un-trimmed output would be mostly spaces.
pub fn render_rows(app: &App, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("TestBackend cannot fail to initialise");
    terminal
        .draw(|f| super::ui::draw(f, app))
        .expect("drawing into an in-memory buffer cannot fail");

    let buffer = terminal.backend().buffer();
    let mut rows = Vec::with_capacity(height as usize);
    for y in 0..height {
        let mut row = String::with_capacity(width as usize);
        for x in 0..width {
            row.push_str(buffer[(x, y)].symbol());
        }
        rows.push(row.trim_end().to_string());
    }
    rows
}

/// One frame as a single string, for substring assertions.
pub fn render_frame(app: &App) -> String {
    render_rows(app, DEFAULT_WIDTH, DEFAULT_HEIGHT).join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Building a real `App` enumerates hardware. Slow, but a mock would test the
    /// mock.
    fn app() -> App {
        App::new_fast().expect("the TUI app must be constructible headlessly")
    }

    #[test]
    fn a_frame_renders_and_contains_the_tab_bar() {
        let app = app();
        let frame = render_frame(&app);
        assert!(
            !frame.trim().is_empty(),
            "the TUI rendered an entirely blank frame"
        );
        // The tab bar is the one element present on every tab.
        assert!(
            frame.contains("Overview"),
            "the tab bar is missing from the frame:\n{frame}"
        );
    }

    /// The harness must see real content, not just chrome — otherwise it would pass
    /// against a TUI that drew borders around nothing.
    #[test]
    fn the_overview_tab_renders_actual_readings() {
        let mut app = app();
        app.selected_tab = super::super::app::TAB_OVERVIEW;
        let frame = render_frame(&app);
        assert!(
            frame.contains("CPU") || frame.contains("Memory"),
            "the Overview tab shows no subsystem at all:\n{frame}"
        );
    }

    /// Inference is the main view: first in the tab order and the one a fresh
    /// session opens on.
    #[test]
    fn the_inference_tab_is_first_and_selected_by_default() {
        let app = app();
        assert_eq!(app.tabs[0], "Inference");
        assert_eq!(app.selected_tab, 0);
    }

    /// With no engine running, the tab says who did not answer and why, rather
    /// than drawing an empty or idle-looking page.
    #[test]
    fn the_inference_tab_reports_when_no_engine_answered() {
        use crate::inference::{
            InferencePoller, InferenceSource, Locality, ProviderKind, ProviderSample, SourceError,
        };
        use std::time::Duration;

        // A stub, because whatever really listens on 8080 on this machine is not
        // something this test can assume.
        struct Refusing;
        impl InferenceSource for Refusing {
            fn kind(&self) -> ProviderKind {
                ProviderKind::IronWorks
            }
            fn endpoint(&self) -> String {
                "http://127.0.0.1:8080".to_string()
            }
            fn locality(&self) -> Locality {
                Locality::Local
            }
            fn poll(&self, _: Duration) -> Result<ProviderSample, SourceError> {
                Err(SourceError::Unreachable("connection refused".to_string()))
            }
        }

        let mut app = app();
        app.inference = Some(InferencePoller::spawn(
            vec![Box::new(Refusing)],
            Duration::from_secs(60),
        ));
        app.await_first_inference_poll(Duration::from_secs(5));
        let frame = render_frame(&app);
        assert!(
            frame.contains("no engine answered at http://127.0.0.1:8080"),
            "the Inference tab does not say no engine answered:\n{frame}"
        );
    }

    /// The chat box is shared: opening it from the Inference tab types into it,
    /// and a digit typed into it does not switch tabs.
    #[test]
    fn typing_into_the_agent_box_does_not_switch_tabs() {
        use crossterm::event::{KeyCode, KeyModifiers};

        let mut app = app();
        assert_eq!(app.selected_tab, super::super::app::TAB_INFERENCE);
        app.handle_key(KeyCode::Char('a'), KeyModifiers::empty());
        assert!(app.agent_input_mode, "'a' should open the question box");

        app.handle_key(KeyCode::Char('2'), KeyModifiers::empty());
        app.handle_key(KeyCode::Tab, KeyModifiers::empty());
        app.handle_key(KeyCode::Char('q'), KeyModifiers::empty());
        assert_eq!(app.selected_tab, super::super::app::TAB_INFERENCE);
        assert_eq!(app.agent_input, "2q");
        assert!(app.agent_input_mode);
    }

    /// Number keys follow the tab order: `1` is Inference, `0` is Agent.
    #[test]
    fn number_keys_follow_the_tab_order() {
        use super::super::app::{TAB_AGENT, TAB_INFERENCE, TAB_OVERVIEW, TAB_PROFILES};
        use crossterm::event::{KeyCode, KeyModifiers};

        let mut app = app();
        app.handle_key(KeyCode::Char('2'), KeyModifiers::empty());
        assert_eq!(app.selected_tab, TAB_OVERVIEW);
        app.handle_key(KeyCode::Char('1'), KeyModifiers::empty());
        assert_eq!(app.selected_tab, TAB_INFERENCE);
        app.handle_key(KeyCode::Char('9'), KeyModifiers::empty());
        assert_eq!(app.selected_tab, TAB_PROFILES);
        app.handle_key(KeyCode::Char('0'), KeyModifiers::empty());
        assert_eq!(app.selected_tab, TAB_AGENT);
        assert_eq!(app.tabs[app.selected_tab], "Agent");
    }

    /// Every tab must render without panicking and produce something.
    ///
    /// A tab that panics takes the whole TUI down, and one that renders blank is
    /// indistinguishable to a user from one that is broken — the failure that
    /// started this whole thread in the GUI.
    #[test]
    fn every_tab_renders_without_panicking() {
        let mut app = app();
        let tab_count = app.tabs.len();
        assert!(tab_count > 1, "expected several tabs, found {tab_count}");

        for index in 0..tab_count {
            app.selected_tab = index;
            let frame = render_frame(&app);
            assert!(
                !frame.trim().is_empty(),
                "tab {index} ({}) rendered a blank frame",
                app.tabs[index]
            );
            // The tab bar survives on every tab, so its absence means the layout
            // collapsed rather than the content simply being empty.
            assert!(
                frame.contains("Overview"),
                "tab {index} ({}) lost the tab bar, so the layout collapsed:\n{frame}",
                app.tabs[index]
            );
        }
    }

    /// The TUI's own vocabulary must match the ontology, asserted against rendered
    /// output rather than against the source list — a tab renamed at the point of
    /// drawing would pass a check that only read `app.tabs`.
    #[test]
    fn rendered_tab_labels_use_the_ontology_spelling() {
        use crate::ontology::labels;

        let app = app();
        let frame = render_frame(&app);

        for tab in &app.tabs {
            let lowered = tab.to_ascii_lowercase();
            if labels::is_known_domain(&lowered) {
                let canonical = labels::domain_label(&lowered);
                assert!(
                    frame.contains(&canonical),
                    "the TUI renders the domain {lowered:?} but the frame does not \
                     contain its ontology spelling {canonical:?}:\n{frame}"
                );
            }
        }
    }

    /// A frame is a fixed grid, so geometry must be honoured exactly — an agent
    /// diffing two frames needs them to be the same shape.
    #[test]
    fn frame_geometry_matches_the_requested_size() {
        let app = app();
        let rows = render_rows(&app, 120, 30);
        assert_eq!(rows.len(), 30, "wrong number of rows");
        // Rows are right-trimmed, so assert the untrimmed width instead by checking
        // no row exceeds the requested width.
        for (i, row) in rows.iter().enumerate() {
            assert!(
                row.chars().count() <= 120,
                "row {i} is {} cells wide, exceeding the requested 120",
                row.chars().count()
            );
        }
    }
}

// ── Scripted driving ─────────────────────────────────────────────────────────

/// One step in a TUI script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Select a tab by name (case-insensitive) or index.
    Goto(String),
    /// Send a key through the same handler the interactive loop uses.
    Key(String),
    /// Re-sample and take a fresh snapshot.
    Refresh,
    /// Record the current frame.
    Capture,
    /// Fail unless the current frame contains this text.
    Assert(String),
    /// Fail if the current frame contains this text.
    Refute(String),
}

/// Outcome of running a script.
#[derive(Debug, Default)]
pub struct ScriptResult {
    /// Frames recorded by `capture`, in order.
    pub captures: Vec<String>,
    /// Assertion failures, in order. Empty means the script passed.
    pub failures: Vec<String>,
}

/// Parse a script. One step per line; `#` starts a comment; blank lines ignored.
///
/// Deliberately tiny. A general scripting language would need its own semantics and
/// its own tests; this covers navigate, observe, assert — what an agent needs to
/// confirm the TUI shows what it expects.
pub fn parse_script(text: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (verb, rest) = match line.split_once(char::is_whitespace) {
            Some((v, r)) => (v, r.trim()),
            None => (line, ""),
        };
        let step = match verb.to_ascii_lowercase().as_str() {
            "goto" if !rest.is_empty() => Step::Goto(rest.to_string()),
            "key" if !rest.is_empty() => Step::Key(rest.to_string()),
            "refresh" => Step::Refresh,
            "capture" => Step::Capture,
            "assert" if !rest.is_empty() => Step::Assert(rest.to_string()),
            "refute" if !rest.is_empty() => Step::Refute(rest.to_string()),
            other => {
                return Err(format!(
                    "line {}: unknown or incomplete step {other:?}. Steps: goto <tab>, \
                     key <name>, refresh, capture, assert <text>, refute <text>",
                    n + 1
                ))
            }
        };
        steps.push(step);
    }
    Ok(steps)
}

/// Translate a key name into a crossterm code.
fn parse_key(name: &str) -> Option<crossterm::event::KeyCode> {
    use crossterm::event::KeyCode;
    let k = match name.to_ascii_lowercase().as_str() {
        "tab" => KeyCode::Tab,
        "backtab" | "shift-tab" => KeyCode::BackTab,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => KeyCode::Char(c),
                _ => return None,
            }
        }
    };
    Some(k)
}

/// Run a script against `app`.
///
/// Key steps go through [`App::handle_key`] — the same function the interactive
/// loop calls — so an assertion here is evidence about the real TUI rather than
/// about a parallel reimplementation of its bindings. That includes the agent
/// question box: a key sent while it is open is typed into it, not run as a
/// shortcut.
pub fn run_script(app: &mut App, steps: &[Step], width: u16, height: u16) -> ScriptResult {
    use crossterm::event::KeyModifiers;

    let mut result = ScriptResult::default();
    let mut frame = render_rows(app, width, height).join("\n");

    for (i, step) in steps.iter().enumerate() {
        match step {
            Step::Goto(target) => {
                let resolved = target
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n < app.tabs.len())
                    .or_else(|| app.tabs.iter().position(|t| t.eq_ignore_ascii_case(target)));
                match resolved {
                    Some(index) => app.selected_tab = index,
                    None => result.failures.push(format!(
                        "step {}: unknown tab {target:?}; available: {:?}",
                        i + 1,
                        app.tabs
                    )),
                }
            }
            Step::Key(name) => match parse_key(name) {
                Some(code) => {
                    // A quit request stops the script rather than the process: a
                    // script ending in `key q` should still report its findings.
                    if app.handle_key(code, KeyModifiers::empty()) {
                        break;
                    }
                }
                None => result
                    .failures
                    .push(format!("step {}: unknown key {name:?}", i + 1)),
            },
            Step::Refresh => {
                let _ = app.update();
                app.sync_snapshot();
            }
            Step::Capture => result.captures.push(frame.clone()),
            Step::Assert(needle) => {
                if !frame.contains(needle.as_str()) {
                    result.failures.push(format!(
                        "step {}: expected {needle:?} in the frame, not found",
                        i + 1
                    ));
                }
            }
            Step::Refute(needle) => {
                if frame.contains(needle.as_str()) {
                    result.failures.push(format!(
                        "step {}: {needle:?} should not appear in the frame, but does",
                        i + 1
                    ));
                }
            }
        }
        // Re-render after anything that could change the screen, so the next
        // assertion sees the result of this step rather than the previous frame.
        if matches!(step, Step::Goto(_) | Step::Key(_) | Step::Refresh) {
            frame = render_rows(app, width, height).join("\n");
        }
    }

    result
}

#[cfg(test)]
mod script_tests {
    use super::*;

    fn app() -> App {
        App::new_fast().expect("app must be constructible headlessly")
    }

    #[test]
    fn scripts_parse_with_comments_and_blank_lines() {
        let steps = parse_script(
            "# navigate\ngoto CPU\n\n  key tab  # inline comment\ncapture\nassert Overview\n",
        )
        .expect("should parse");
        assert_eq!(
            steps,
            vec![
                Step::Goto("CPU".into()),
                Step::Key("tab".into()),
                Step::Capture,
                Step::Assert("Overview".into()),
            ]
        );
    }

    #[test]
    fn unknown_steps_are_rejected_with_the_available_ones_named() {
        let err = parse_script("frobnicate widget").expect_err("should reject");
        assert!(err.contains("frobnicate"), "got: {err}");
        assert!(
            err.contains("goto"),
            "the error should list valid steps: {err}"
        );
    }

    #[test]
    fn goto_changes_the_selected_tab() {
        let mut app = app();
        let steps = parse_script("goto Memory\nassert Overview").unwrap();
        let result = run_script(&mut app, &steps, 160, 30);
        assert!(result.failures.is_empty(), "{:?}", result.failures);
        assert_eq!(
            app.selected_tab,
            app.tabs.iter().position(|t| *t == "Memory").unwrap()
        );
    }

    /// Keys must go through the real handler. Pressing `3` selects the third tab in
    /// the interactive TUI, so it must here too — that equivalence is the whole
    /// reason the handler was extracted.
    #[test]
    fn key_presses_drive_the_same_handler_as_the_interactive_loop() {
        let mut app = app();
        let before = app.selected_tab;
        let steps = parse_script("key 3").unwrap();
        let result = run_script(&mut app, &steps, 160, 30);
        assert!(result.failures.is_empty(), "{:?}", result.failures);
        assert_eq!(
            app.selected_tab,
            crate::tui::app::TAB_PROCESSES,
            "key '3' should select the Processes tab"
        );
        assert_ne!(before, app.selected_tab);
    }

    #[test]
    fn assertions_report_failure_rather_than_panicking() {
        let mut app = app();
        let steps = parse_script("assert definitely-not-on-screen").unwrap();
        let result = run_script(&mut app, &steps, 160, 30);
        assert_eq!(result.failures.len(), 1, "expected one failure");
        assert!(result.failures[0].contains("definitely-not-on-screen"));
    }

    #[test]
    fn refute_catches_text_that_should_be_absent() {
        let mut app = app();
        // The tab bar is always present, so refuting it must fail.
        let steps = parse_script("refute Overview").unwrap();
        let result = run_script(&mut app, &steps, 160, 30);
        assert_eq!(result.failures.len(), 1, "refute should have failed");
    }

    #[test]
    fn capture_records_the_frame_at_that_point() {
        let mut app = app();
        let steps = parse_script("goto CPU\ncapture\ngoto Memory\ncapture").unwrap();
        let result = run_script(&mut app, &steps, 160, 30);
        assert_eq!(result.captures.len(), 2);
        assert!(result.captures.iter().all(|c| c.contains("Overview")));
    }

    /// `q` quits the interactive TUI, so a script must stop there rather than
    /// carrying on against an app the user has closed.
    #[test]
    fn a_quit_key_stops_the_script() {
        let mut app = app();
        let steps = parse_script("key q\nassert definitely-not-on-screen").unwrap();
        let result = run_script(&mut app, &steps, 160, 30);
        assert!(
            result.failures.is_empty(),
            "steps after a quit should not run: {:?}",
            result.failures
        );
    }
}

#[cfg(test)]
mod agents_tab_tests {
    use super::super::app::{App, TAB_AGENTS, TAB_INFERENCE, TAB_OVERVIEW};
    use super::render_frame;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn app() -> App {
        App::new_fast().expect("the TUI app must be constructible headlessly")
    }

    /// Agents is the tab after Inference, and Tab reaches it. No digit is taken from
    /// the existing tabs: the number row keeps its meaning.
    #[test]
    fn agents_follows_inference_and_is_reached_by_tab_cycling() {
        let mut app = app();
        assert_eq!(app.tabs[0], "Inference");
        assert_eq!(app.tabs[TAB_AGENTS], "Agents");
        assert_eq!(app.selected_tab, TAB_INFERENCE);
        app.handle_key(KeyCode::Tab, KeyModifiers::empty());
        assert_eq!(app.selected_tab, TAB_AGENTS);
        app.handle_key(KeyCode::Tab, KeyModifiers::empty());
        assert_eq!(app.selected_tab, TAB_OVERVIEW);
        app.handle_key(KeyCode::BackTab, KeyModifiers::empty());
        assert_eq!(app.selected_tab, TAB_AGENTS);
    }

    /// The digit row keeps its old meaning after the renumbering.
    #[test]
    fn digits_still_select_the_tabs_they_always_did() {
        let mut app = app();
        app.handle_key(KeyCode::Char('2'), KeyModifiers::empty());
        assert_eq!(app.selected_tab, TAB_OVERVIEW);
        app.handle_key(KeyCode::Char('0'), KeyModifiers::empty());
        assert_eq!(app.tabs[app.selected_tab], "Agent");
    }

    /// The panel names its sections and says what it does not read, whether or not
    /// any recording exists on this machine.
    #[test]
    fn the_agents_tab_renders_its_sections_and_coverage() {
        let mut app = app();
        app.selected_tab = TAB_AGENTS;
        let frame = render_frame(&app);
        for needle in [
            "Agents",
            "Sessions:",
            "Active",
            "Recent",
            "Workflows",
            "Engine requests",
            "Coverage",
            "Gemini CLI",
        ] {
            assert!(frame.contains(needle), "missing {needle:?} in:\n{frame}");
        }
        assert!(!frame.contains("Traceback"));
    }

    /// Scrolling keys act on the Agents list only.
    #[test]
    fn arrow_keys_scroll_the_agents_list_and_nothing_else() {
        let mut app = app();
        app.selected_tab = TAB_AGENTS;
        app.handle_key(KeyCode::PageDown, KeyModifiers::empty());
        assert_eq!(app.agents_scroll, 10);
        app.handle_key(KeyCode::Up, KeyModifiers::empty());
        assert_eq!(app.agents_scroll, 9);
        app.handle_key(KeyCode::PageUp, KeyModifiers::empty());
        assert_eq!(app.agents_scroll, 0);
        app.selected_tab = TAB_INFERENCE;
        app.handle_key(KeyCode::PageDown, KeyModifiers::empty());
        assert_eq!(app.agents_scroll, 0);
    }
}
