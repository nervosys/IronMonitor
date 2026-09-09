//! Headless rendering harness for the GUI.
//!
//! egui draws into a `Context` that needs no window, so a test can run real widget
//! code and read back the text that was actually painted. That matters here because
//! the alternative — screenshotting a live window — proved unusable: on a
//! multi-monitor setup the window handle resolves to transient popups and the
//! captured rect is whatever happened to be on top.
//!
//! More importantly, a screenshot cannot answer the question that actually matters.
//! The Profiles tab rendered all 19 of its groups perfectly while every heading was
//! drawn in the panel colour; a human looking at the window and a naive capture both
//! reported an empty tab. Reading the painted galleys distinguishes "this text was
//! never emitted" from "this text was emitted and could not be seen", which are
//! different bugs with different fixes.

use egui::Context;

/// Every string painted by `body`, in paint order.
///
/// Walks the tessellated output rather than instrumenting the widgets, so it sees
/// what egui actually drew — including text emitted by widgets this module knows
/// nothing about.
pub fn painted_text(ctx: &Context, body: impl FnMut(&mut egui::Ui)) -> Vec<String> {
    painted_text_sized(ctx, DEFAULT_VIEWPORT, body)
}

/// Viewport used when none is given.
///
/// Large enough that a tab's content is not clipped away. This is not cosmetic:
/// `RawInput::default()` carries no `screen_rect`, and widgets that guard on
/// `Ui::is_rect_visible` — `SectionHeader` among them — paint nothing in a
/// degenerate viewport.
pub const DEFAULT_VIEWPORT: egui::Vec2 = egui::Vec2::new(1600.0, 1200.0);

/// Every string painted by `body`, at an explicit viewport size.
pub fn painted_text_sized(
    ctx: &Context,
    size: egui::Vec2,
    mut body: impl FnMut(&mut egui::Ui),
) -> Vec<String> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..Default::default()
    };

    // Two frames, and the second is the one that counts.
    //
    // egui is immediate-mode but not stateless: a `ScrollArea` does not know its
    // content size until it has laid the content out once, and on that first frame
    // it reports a viewport that makes `Ui::is_rect_visible` false for everything
    // inside it. A tab whose body is entirely wrapped in one — the CPU and System
    // tabs both are — paints nothing at all on frame one. That read as two dead
    // tabs and was an artefact of rendering a single frame, not a defect in them.
    //
    // The body therefore has to run twice, which is why this takes `FnMut`.
    let mut run_frame = || {
        ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| body(ui));
        })
    };

    let _warmup = run_frame();
    let settled = run_frame();

    let mut out = Vec::new();
    for clipped in &settled.shapes {
        collect_shape_text(&clipped.shape, &mut out);
    }
    out
}

/// Render frames until `settled` reports true or `deadline` passes, then return
/// what the last frame painted.
///
/// The two-frame warmup above handles egui's own laziness. This handles the
/// application's: four tabs load their contents on a background thread and paint
/// a spinner until it lands, and the headless path never ran the loop that
/// collects those results. A single frame therefore captured "Loading disk
/// information…" forever — which *is* painted text, so
/// `every_gui_tab_paints_text` passed while an agent reading the disk tab learned
/// nothing.
///
/// `pump` is called between frames to advance whatever the caller is waiting on.
/// Returning on a deadline rather than blocking is deliberate: a machine whose
/// disk enumeration hangs should yield a slow, honest "still loading" rather than
/// a headless command that never exits.
/// `settled` is asked about the *painted text*, not about the application's
/// internal flags. That is deliberate: a flag-based predicate makes every tab wait
/// for every loader, so tabs that render instantly — memory, network — went from
/// immediate to twelve seconds because they were waiting on a peripherals query
/// they do not draw. What the caller actually wants to know is whether *this* tab
/// still says it is loading, and the frame answers that directly.
///
/// `state` is threaded through explicitly rather than captured, because all three
/// callbacks need it and closures capturing one `&mut` cannot coexist.
pub fn painted_text_until<T>(
    ctx: &Context,
    deadline: std::time::Duration,
    state: &mut T,
    mut pump: impl FnMut(&mut T, &Context),
    mut settled: impl FnMut(&T, &[String]) -> bool,
    mut body: impl FnMut(&mut T, &mut egui::Ui),
) -> Vec<String> {
    let start = std::time::Instant::now();
    let mut last = painted_text(ctx, |ui| body(state, ui));

    while !settled(state, &last) && start.elapsed() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
        pump(state, ctx);
        last = painted_text(ctx, |ui| body(state, ui));
    }

    if settled(state, &last) {
        return last;
    }

    // One more pass after settling: the frame that observes a load finishing is
    // the frame *before* its data is available to draw.
    pump(state, ctx);
    let final_pass = painted_text(ctx, |ui| body(state, ui));

    // A tab that painted nothing on the very last pass should report whatever it
    // last managed rather than a blank, which would read as a dead tab.
    if final_pass.is_empty() {
        last
    } else {
        final_pass
    }
}

/// Whether a rendered frame is still showing a background load rather than data.
///
/// The tabs that load asynchronously all paint a spinner beside a line of the
/// form "Loading … information…". Matching on that is coupling to a UI string,
/// which is worth stating plainly — but the alternative, introspecting per-tab
/// loading flags, couples to more and gets the answer wrong for tabs that draw
/// none of them. `every_gui_tab_paints_text` asserts this same property from the
/// other side, so a placeholder that changes wording fails a test rather than
/// silently making the wait a no-op.
pub fn frame_is_still_loading(lines: &[String]) -> bool {
    // Matches "Loading disk information...", "Loading profile snapshot…" and the
    // bare "Loading…". An earlier version required a trailing "..." and missed
    // every placeholder using the single-character ellipsis, which is most of
    // them — the check has to be as loose as the wording actually is.
    lines.iter().any(|l| l.trim_start().starts_with("Loading"))
}

/// Text painted by `body`, joined into one haystack for substring assertions.
pub fn painted_blob(ctx: &Context, body: impl FnMut(&mut egui::Ui)) -> String {
    painted_text(ctx, body).join("\n")
}

fn collect_shape_text(shape: &egui::Shape, out: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => {
            let s = text.galley.text();
            if !s.trim().is_empty() {
                out.push(s.to_string());
            }
        }
        // Widgets compose, so text is routinely nested inside a Vec shape.
        egui::Shape::Vec(shapes) => {
            for s in shapes {
                collect_shape_text(s, out);
            }
        }
        _ => {}
    }
}

/// A context with the app's own theme applied, so tests exercise the real palette
/// rather than egui's defaults.
pub fn themed_context() -> Context {
    let ctx = Context::default();
    super::theme::apply_cyber_theme(&ctx);
    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::RichText;

    #[test]
    fn the_harness_sees_text_that_was_painted() {
        let ctx = themed_context();
        let blob = painted_blob(&ctx, |ui| {
            ui.label("a plain label");
            ui.label(RichText::new("a strong label").strong());
        });
        assert!(blob.contains("a plain label"), "got: {blob}");
        assert!(blob.contains("a strong label"), "got: {blob}");
    }

    #[test]
    fn the_harness_sees_text_nested_inside_widgets() {
        let ctx = themed_context();
        let blob = painted_blob(&ctx, |ui| {
            egui::CollapsingHeader::new("outer heading")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label("nested body text");
                });
        });
        assert!(blob.contains("outer heading"), "got: {blob}");
        assert!(blob.contains("nested body text"), "got: {blob}");
    }

    /// The harness must not report text that was never drawn, or it would mask the
    /// very regressions it exists to catch.
    #[test]
    fn the_harness_does_not_invent_text() {
        let ctx = themed_context();
        let blob = painted_blob(&ctx, |ui| {
            ui.label("only this");
        });
        assert!(!blob.contains("not drawn"), "got: {blob}");
    }

    /// Reading painted text separates "never emitted" from "emitted but invisible" —
    /// the distinction a screenshot cannot make, and the one that made the Profiles
    /// tab look dead while it was rendering correctly.
    #[test]
    fn invisible_text_is_still_painted_text() {
        let ctx = themed_context();
        let panel_fill = ctx.style().visuals.panel_fill;
        let blob = painted_blob(&ctx, |ui| {
            // Deliberately drawn in the background colour, as the old theme did.
            ui.label(RichText::new("camouflaged").color(panel_fill));
        });
        assert!(
            blob.contains("camouflaged"),
            "the harness should see text regardless of its colour, so that a \
             contrast bug is diagnosable as distinct from a missing-widget bug"
        );
    }
}

/// Tests that build the real application and render real tabs.
///
/// These are the ones that can answer "does tab X work", which the colour and
/// harness tests above cannot: they exercise the actual widget code with the
/// actual application state.
#[cfg(test)]
mod app_tab_tests {
    use super::*;
    use crate::gui::app::IronMonitorApp;

    /// Constructing the app enumerates GPUs and spawns collectors. That is slow but
    /// real, and a mock would defeat the purpose of the test.
    fn app_and_ctx() -> (IronMonitorApp, egui::Context) {
        let ctx = egui::Context::default();
        let app = IronMonitorApp::with_context(&ctx);
        (app, ctx)
    }

    /// The reported bug was "AI backends do not work". The CLI path answered a
    /// question correctly against Ollama, which located the fault in the GUI — but
    /// screenshots could never confirm what the tab actually drew.
    ///
    /// This renders the real tab and asserts it emits its own furniture. It does not
    /// assert a backend is reachable: that depends on whether a model server happens
    /// to be running, which is not a property of the code.
    #[test]
    fn the_ai_tab_renders_its_controls() {
        let (mut app, ctx) = app_and_ctx();

        // First frame lands inside the detection window and paints a spinner. That
        // is a working state, but asserting on it proves nothing about the tab the
        // user actually sits in front of — an accept-either assertion here passed
        // while never once exercising the post-detection UI.
        let first = painted_blob(&ctx, |ui| {
            app.draw_ai_assistant_tab(ui);
        });
        assert!(
            first.contains("Detecting AI backends"),
            "expected the detection notice on the first frame, got: {first}"
        );

        // The tab gives detection a three-second budget and then shows the controls
        // regardless. Waiting it out is slower than mocking the clock but exercises
        // the real branch.
        std::thread::sleep(std::time::Duration::from_millis(3200));

        let settled = painted_blob(&ctx, |ui| {
            app.draw_ai_assistant_tab(ui);
        });
        assert!(
            settled.contains("AI System Assistant"),
            "past the detection budget the AI tab should paint its header and \
             controls, got: {settled}"
        );
        // The controls themselves, not just the header — a tab that painted a title
        // over an empty body is the failure being guarded against.
        assert!(
            settled.contains("Model:") || settled.contains("Select model"),
            "the AI tab painted its header but not its model selector: {settled}"
        );
        // A concurrent "still probing" note is *not* a failure and must not be
        // asserted against: backend discovery outlives the three-second spinner
        // budget by design, and saying so while the controls are usable is the
        // honest state. An earlier version of this test forbade it and failed
        // against a tab that was working correctly.
        assert!(
            settled
                .lines()
                .any(|l| l.contains("Ollama") || l.contains("IronWorks") || l.contains("backend")),
            "the AI tab offered no backend at all: {settled}"
        );
    }

    /// The Overview ask bar shares state with the AI tab, so it has to render even
    /// when no model has been selected — and say which condition is unmet rather
    /// than reporting the backend dead.
    #[test]
    fn the_overview_ask_bar_renders_and_explains_itself() {
        let (mut app, ctx) = app_and_ctx();
        let blob = painted_blob(&ctx, |ui| {
            app.draw_overview_chat_bar(ui);
        });

        assert!(
            blob.contains("Ask"),
            "the ask bar is missing its label: {blob}"
        );
        // When no model is chosen the bar must name that specific condition. The
        // backend is usually reachable and merely has nothing selected, which is why
        // this says "no model" rather than "unavailable".
        if !app.agent_can_answer() {
            assert!(
                blob.contains("no model selected"),
                "with no model available the bar should say so, got: {blob}"
            );
        }
    }

    /// The failure this whole thread began with: a tab that rendered every one of
    /// its rows while all of them were invisible. Asserted against the real tab.
    #[test]
    fn the_profiles_tab_paints_readable_headings() {
        let (mut app, ctx) = app_and_ctx();
        let blob = painted_blob(&ctx, |ui| {
            app.draw_profiles_tab(ui);
        });

        assert!(
            blob.contains("Hardware Profile Inspector"),
            "the Profiles tab did not paint its header: {blob}"
        );

        // Painted is necessary but not sufficient — the original bug painted
        // everything. Legibility is the other half.
        let visuals = ctx.style().visuals.clone();
        let strong = visuals.strong_text_color();
        let panel = visuals.panel_fill;
        let distance = (strong.r() as i32 - panel.r() as i32).abs()
            + (strong.g() as i32 - panel.g() as i32).abs()
            + (strong.b() as i32 - panel.b() as i32).abs();
        assert!(
            distance > 60,
            "Profiles headings are painted in {strong:?} on a {panel:?} panel — the \
             original failure, where all 19 groups rendered and none could be read"
        );
    }

    /// Tofu regression: the geometric-shape triangles the bundled emoji font cannot
    /// cover must not come back into headings that already draw their own arrow.
    #[test]
    fn tabs_do_not_paint_glyphs_the_bundled_fonts_lack() {
        let (mut app, ctx) = app_and_ctx();
        let blob = painted_blob(&ctx, |ui| {
            app.draw_profiles_tab(ui);
        });

        for glyph in ['\u{25BE}', '\u{25B8}'] {
            assert!(
                !blob.contains(glyph),
                "U+{:04X} is back in the Profiles tab; NotoEmoji does not cover \
                 Geometric Shapes, so it renders as a tofu box",
                glyph as u32
            );
        }
    }
}

#[cfg(test)]
mod ontology_binding_tests {
    use super::*;
    use crate::gui::widgets::{domain_section_title, SectionHeader};
    use crate::ontology::labels;

    /// The GUI must paint the ontology's spelling of a domain, not its own.
    ///
    /// Asserted through the harness rather than by calling the helper directly:
    /// a helper that returns the right string but is never reached would pass a
    /// unit test and leave the screen unchanged.
    #[test]
    fn section_headings_paint_the_ontology_domain_spelling() {
        let ctx = themed_context();
        let title = domain_section_title("gpu", "Utilization");
        let blob = painted_blob(&ctx, |ui| {
            ui.add(SectionHeader::new(&title));
        });
        assert!(
            blob.contains("GPU Utilization"),
            "expected the ontology spelling in painted output, got: {blob}"
        );
        // And the domain word is one an agent can actually query.
        assert!(labels::is_known_domain("gpu"));
    }

    /// Text painted by the GUI must map back to ids, so an agent handed a screenshot
    /// description by a user can turn it into a query.
    #[test]
    fn painted_labels_resolve_to_entity_ids() {
        let ctx = themed_context();
        let label = labels::short_label("memory.total");
        let blob = painted_blob(&ctx, |ui| {
            ui.label(&label);
        });
        assert!(blob.contains("Total"), "got: {blob}");

        let ids = labels::ids_for_label(&label);
        assert!(
            ids.iter().any(|id| id == "memory.total"),
            "the label the GUI painted does not map back to memory.total: {ids:?}"
        );
    }

    /// The regression that made Profiles look dead, caught at the level it occurred:
    /// strong text is emitted *and* distinguishable from the surface behind it.
    #[test]
    fn strong_headings_are_both_painted_and_legible() {
        let ctx = themed_context();
        let blob = painted_blob(&ctx, |ui| {
            ui.label(egui::RichText::new("Group Heading").strong());
        });
        assert!(
            blob.contains("Group Heading"),
            "strong text was not painted at all: {blob}"
        );

        let visuals = ctx.style().visuals.clone();
        let strong = visuals.strong_text_color();
        let panel = visuals.panel_fill;
        let distance = (strong.r() as i32 - panel.r() as i32).abs()
            + (strong.g() as i32 - panel.g() as i32).abs()
            + (strong.b() as i32 - panel.b() as i32).abs();
        assert!(
            distance > 60,
            "strong text {strong:?} is painted but indistinguishable from the panel \
             {panel:?} — the Profiles failure mode"
        );
    }
}

// ── Scripted inspection ──────────────────────────────────────────────────────

/// One step in a GUI script.
///
/// Deliberately smaller than the TUI's step set. The TUI needs `key` because its
/// navigation is key-driven and stateful — you press keys to get somewhere. A GUI
/// tab is addressable directly, so `goto` covers navigation entirely and there is
/// no keystroke state to drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Select a tab by name.
    Goto(String),
    /// Record the text currently painted.
    Capture,
    /// Fail unless the current frame contains this text.
    Assert(String),
    /// Fail if the current frame contains this text.
    Refute(String),
}

/// Outcome of running a GUI script.
#[derive(Debug, Default)]
pub struct ScriptResult {
    pub captures: Vec<String>,
    pub failures: Vec<String>,
}

/// Parse a GUI script. One step per line; `#` comments; blank lines ignored.
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
            "capture" => Step::Capture,
            "assert" if !rest.is_empty() => Step::Assert(rest.to_string()),
            "refute" if !rest.is_empty() => Step::Refute(rest.to_string()),
            other => {
                return Err(format!(
                    "line {}: unknown or incomplete step {other:?}. Steps: goto <tab>, \
                     capture, assert <text>, refute <text>. The GUI has no `key` step \
                     — tabs are addressable by name, so there is no navigation state \
                     to drive.",
                    n + 1
                ))
            }
        };
        steps.push(step);
    }
    Ok(steps)
}

/// How long a headless read waits for a tab's background loaders.
///
/// The system and peripherals loaders run several PowerShell CIM queries back to
/// back and were measured at over ten seconds. Bounded rather than unbounded so a
/// wedged reader gives a slow, honest answer instead of never returning.
pub const SETTLE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

/// The current tab's text, once its background loaders have settled.
///
/// Both `--frame` and `--script` go through this, so an agent asserting on a tab
/// sees the same thing either way. Before it existed, `--script` captured the
/// first frame — which for four tabs was their "Loading …" placeholder, forever.
pub fn settled_tab_text(app: &mut super::app::IronMonitorApp, ctx: &Context) -> String {
    painted_text_until(
        ctx,
        SETTLE_DEADLINE,
        app,
        |app, ctx| app.pump_background_loaders(ctx),
        |app, lines| !frame_is_still_loading(lines) && !app.has_pending_load(),
        |app, ui| app.draw_current_tab(ui),
    )
    .join("\n")
}

/// Run a GUI script against `app`.
pub fn run_script(
    app: &mut super::app::IronMonitorApp,
    ctx: &Context,
    steps: &[Step],
) -> ScriptResult {
    let mut result = ScriptResult::default();
    let mut frame = settled_tab_text(app, ctx);

    for (i, step) in steps.iter().enumerate() {
        match step {
            Step::Goto(target) => match app.select_tab_by_name(target) {
                Ok(()) => {
                    frame = settled_tab_text(app, ctx);
                }
                Err(available) => result.failures.push(format!(
                    "step {}: unknown tab {target:?}; available: {available:?}",
                    i + 1
                )),
            },
            Step::Capture => result.captures.push(frame.clone()),
            Step::Assert(needle) => {
                if !frame.contains(needle.as_str()) {
                    result.failures.push(format!(
                        "step {}: expected {needle:?} in the painted text, not found",
                        i + 1
                    ));
                }
            }
            Step::Refute(needle) => {
                if frame.contains(needle.as_str()) {
                    result.failures.push(format!(
                        "step {}: {needle:?} should not be painted, but is",
                        i + 1
                    ));
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod script_tests {
    use super::*;
    use crate::gui::app::IronMonitorApp;

    fn app_and_ctx() -> (IronMonitorApp, Context) {
        let ctx = themed_context();
        let app = IronMonitorApp::with_context(&ctx);
        (app, ctx)
    }

    #[test]
    fn scripts_parse_with_comments_and_blank_lines() {
        let steps = parse_script("# go\ngoto profiles\n\ncapture  # snap\nassert Inspector\n")
            .expect("should parse");
        assert_eq!(
            steps,
            vec![
                Step::Goto("profiles".into()),
                Step::Capture,
                Step::Assert("Inspector".into()),
            ]
        );
    }

    /// The rejection must explain why there is no `key` step, or the omission reads
    /// as an oversight rather than a decision.
    #[test]
    fn a_key_step_is_rejected_with_the_reason() {
        let err = parse_script("key 3").expect_err("the GUI has no key step");
        assert!(err.contains("key"), "got: {err}");
        assert!(
            err.contains("addressable by name"),
            "the error should say why, got: {err}"
        );
    }

    #[test]
    fn goto_changes_what_is_painted() {
        let (mut app, ctx) = app_and_ctx();
        let steps = parse_script("goto profiles\nassert Hardware Profile Inspector").unwrap();
        let result = run_script(&mut app, &ctx, &steps);
        assert!(result.failures.is_empty(), "{:?}", result.failures);
    }

    #[test]
    fn assertions_and_refutations_report_rather_than_panic() {
        let (mut app, ctx) = app_and_ctx();
        let steps =
            parse_script("goto profiles\nassert not-painted-anywhere\nrefute Inspector").unwrap();
        let result = run_script(&mut app, &ctx, &steps);
        assert_eq!(
            result.failures.len(),
            2,
            "both the failed assert and the failed refute should report: {:?}",
            result.failures
        );
    }

    #[test]
    fn an_unknown_tab_lists_the_available_ones() {
        let (mut app, ctx) = app_and_ctx();
        let steps = parse_script("goto nonsense").unwrap();
        let result = run_script(&mut app, &ctx, &steps);
        assert_eq!(result.failures.len(), 1);
        assert!(
            result.failures[0].contains("overview"),
            "should name the alternatives: {:?}",
            result.failures
        );
    }

    #[test]
    fn capture_records_the_frame_after_navigation() {
        let (mut app, ctx) = app_and_ctx();
        let steps = parse_script("goto profiles\ncapture").unwrap();
        let result = run_script(&mut app, &ctx, &steps);
        assert_eq!(result.captures.len(), 1);
        assert!(result.captures[0].contains("Hardware Profile Inspector"));
    }
}
