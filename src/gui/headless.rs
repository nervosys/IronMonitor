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
use std::time::Duration;

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

/// Every painted string with the rectangle it occupies, at an explicit viewport.
///
/// `painted_text` answers "was this drawn"; this answers "drawn *where*". The
/// distinction matters because the two Dewey failures were different: one tab
/// painted nothing, and another painted everything and clipped it off the right
/// edge. Reading text alone catches the first and is blind to the second.
pub fn painted_text_rects_sized(
    ctx: &Context,
    size: egui::Vec2,
    mut body: impl FnMut(&mut egui::Ui),
) -> Vec<(String, egui::Rect)> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..Default::default()
    };
    let mut run_frame = || {
        ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| body(ui));
        })
    };
    let _warmup = run_frame();
    let settled = run_frame();

    let mut out = Vec::new();
    for clipped in &settled.shapes {
        collect_shape_text_rects(&clipped.shape, &mut out);
    }
    out
}

/// Painted strings whose box extends past `size` horizontally.
///
/// Vertical overrun is not overflow here: every tab body is inside a
/// `ScrollArea`, so content taller than the viewport is reachable. Width is not
/// scrollable in this layout, so text past the right edge is simply unreadable.
pub fn horizontal_overflow(
    ctx: &Context,
    size: egui::Vec2,
    body: impl FnMut(&mut egui::Ui),
) -> Vec<(String, f32)> {
    painted_text_rects_sized(ctx, size, body)
        .into_iter()
        .filter(|(text, rect)| !text.trim().is_empty() && rect.max.x > size.x)
        .map(|(text, rect)| (text, rect.max.x - size.x))
        .collect()
}

/// Bring an app to the state a user actually sees before measuring it.
///
/// Two things arrive late and both change what a tab paints, so a measurement
/// taken before them is of a different screen:
///
/// - **Background loaders.** Four tabs fetch their contents off-thread and paint
///   a spinner until the data lands.
/// - **The collector's first real snapshot.** The pipeline publishes a warm-up
///   generation built from an empty source set, so GPUs, processes and
///   connections are absent from it *by construction*. On this three-GPU machine
///   the Overview's accelerator cards simply are not there yet, and the tab is
///   shorter than it will be a moment later.
///
/// Returns false if either is still outstanding when the budget runs out, so a
/// caller can decline to measure rather than measure the wrong thing.
pub fn settle(app: &mut crate::gui::app::IronMonitorApp, ctx: &Context, budget: Duration) -> bool {
    let deadline = std::time::Instant::now() + budget;

    while std::time::Instant::now() < deadline {
        app.pump_background_loaders(ctx);
        app.sync_snapshot();
        if app.has_real_snapshot() && !app.has_pending_load() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// The bottom-right corner of everything `body` actually paints.
///
/// The measurement that works on this GUI, where the other two do not.
/// `Context::used_size` reports *allocated* space — `1400x4000` for a 1400x4000
/// canvas, and `-inf` in the live app, where tabs paint through panels rather
/// than the measured `Ui`. Asking a tab how wide it wants to be is circular too:
/// every widget sizes to `available_width()`, so all thirteen report wanting
/// exactly whatever canvas they are handed.
///
/// Where text lands is not circular. Rendered at 1400 wide the Overview paints
/// out to 1382x918; at 1100 wide, 1082x918. The width follows the canvas because
/// the layout is elastic, and **the height does not** — 918 px is a real
/// property of the tab, and the number a window can be fitted to.
pub fn painted_extent(
    ctx: &Context,
    canvas: egui::Vec2,
    body: impl FnMut(&mut egui::Ui),
) -> egui::Vec2 {
    let mut extent = egui::Vec2::ZERO;
    for (text, rect) in painted_text_rects_sized(ctx, canvas, body) {
        if text.trim().is_empty() {
            continue;
        }
        extent.x = extent.x.max(rect.max.x);
        extent.y = extent.y.max(rect.max.y);
    }
    extent
}

/// Every painted shape's rectangle and what kind of shape it was.
///
/// **`painted_text_rects_sized` reads galleys, so it can only see glyphs.** That
/// is the right instrument for "is this text off the edge" and the wrong one for
/// "what made the container this wide", because a chart, a frame or a progress
/// bar paints no text and is therefore invisible to it. The `ai` tab was pinned
/// as overflowing for exactly that reason: its welcome sentence was centred in a
/// box roughly 1369 px wide at an 800 px window, so the sentence was a symptom
/// and the thing that widened the box could not be named.
///
/// `Shape::visual_bounding_rect` covers every variant — rects, circles, paths,
/// meshes — so this can answer the question the text reader cannot.
///
/// The kind is carried as a string because the caller wants to *report* it. A
/// bare rectangle that is 300 px too wide tells you where to look and not what
/// to look for.
pub fn painted_shape_rects(
    ctx: &Context,
    size: egui::Vec2,
    mut body: impl FnMut(&mut egui::Ui),
) -> Vec<(&'static str, egui::Rect)> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..Default::default()
    };
    let mut run_frame = || {
        ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| body(ui));
        })
    };
    // Two frames, for the same reason the text reader needs two: a `ScrollArea`
    // does not know its content size until it has laid it out once.
    let _warmup = run_frame();
    let settled = run_frame();

    let mut out = Vec::new();
    for clipped in &settled.shapes {
        collect_shape_rects(&clipped.shape, &mut out);
    }
    out
}

fn shape_kind(shape: &egui::Shape) -> &'static str {
    match shape {
        egui::Shape::Noop => "noop",
        egui::Shape::Vec(_) => "vec",
        egui::Shape::Circle(_) => "circle",
        egui::Shape::Ellipse(_) => "ellipse",
        egui::Shape::LineSegment { .. } => "line",
        egui::Shape::Path(_) => "path",
        egui::Shape::Rect(_) => "rect",
        egui::Shape::Text(_) => "text",
        egui::Shape::Mesh(_) => "mesh",
        egui::Shape::QuadraticBezier(_) => "quadratic",
        egui::Shape::CubicBezier(_) => "cubic",
        egui::Shape::Callback(_) => "callback",
    }
}

fn collect_shape_rects(shape: &egui::Shape, out: &mut Vec<(&'static str, egui::Rect)>) {
    // A `Vec` is a container rather than a mark: recording its own bounds would
    // report the union of its children as though it were one wide widget, which
    // is the opposite of naming the culprit.
    if let egui::Shape::Vec(shapes) = shape {
        for s in shapes {
            collect_shape_rects(s, out);
        }
        return;
    }

    let rect = shape.visual_bounding_rect();
    // egui returns `NOTHING` — an inverted infinite rect — for shapes that paint
    // no pixels. Unioning one of those poisons every maximum downstream.
    if rect.is_finite() && rect.is_positive() {
        out.push((shape_kind(shape), rect));
    }
}

/// What paints furthest past the right edge, widest first.
///
/// Reports *any* shape, so the answer can be a chart or a frame rather than a
/// label. Use this when a tab overflows and the text reader has nothing to say
/// about why.
pub fn widest_overflowing_shapes(
    ctx: &Context,
    size: egui::Vec2,
    body: impl FnMut(&mut egui::Ui),
) -> Vec<(&'static str, egui::Rect, f32)> {
    let mut over: Vec<(&'static str, egui::Rect, f32)> = painted_shape_rects(ctx, size, body)
        .into_iter()
        .filter(|(_, rect)| rect.max.x > size.x)
        .map(|(kind, rect)| (kind, rect, rect.max.x - size.x))
        .collect();
    over.sort_by(|a, b| b.2.total_cmp(&a.2));
    over
}

fn collect_shape_text_rects(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
    match shape {
        egui::Shape::Text(text) => {
            let s = text.galley.text();
            if !s.is_empty() {
                // `shape.visual_bounding_rect()`, not `from_min_size(pos, size)`.
                //
                // **`TextShape::pos` is an anchor, not a left edge.** For a
                // galley with a centred or right horizontal alignment the text
                // extends away from `pos`, so composing a rect from it reports a
                // box the right size in the wrong place. The AI tab's welcome
                // sentence painted its pixels at 114..693 inside an 800 px
                // window while this reader claimed 404..983 — a 183 px overrun
                // that was not on the screen, and which this guard pinned as a
                // defect for weeks.
                //
                // The bounding rect is computed by egui from the galley's own
                // mesh, so it is where the glyphs actually are. That is the only
                // thing a reader can see, and the only thing worth asserting.
                out.push((s.to_string(), shape.visual_bounding_rect()));
            }
        }
        egui::Shape::Vec(shapes) => {
            for s in shapes {
                collect_shape_text_rects(s, out);
            }
        }
        _ => {}
    }
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

#[cfg(test)]
mod overflow_tests {
    use super::*;

    /// Tabs in the order `select_tab_by_name` accepts them.
    const TABS: [&str; 13] = [
        "overview",
        "cpu",
        "accelerators",
        "memory",
        "disk",
        "processes",
        "network",
        "tools",
        "connections",
        "system",
        "peripherals",
        "profiles",
        "ai",
    ];

    /// Tabs that still paint past the right edge, pinned so the list cannot grow
    /// quietly and cannot shrink without someone noticing.
    ///
    /// Both are the same construct: a `right_to_left` layout nested inside an
    /// already-advanced `horizontal`. In this egui version that starts its cursor
    /// at the row's right edge and runs outward instead of aligning back from it,
    /// so the content lands entirely outside the window. `accelerators` puts the
    /// clock and power readings there; `memory` a "(used/buffers/cache/free)"
    /// legend, and `ai` an unwrapped welcome sentence that runs 183 px past the
    /// 800 px minimum width. The `ai` one only appears in the empty state, which
    /// is why it surfaced intermittently rather than on every run.
    ///
    /// `.wrap()` does not fix the `ai` case, which is the tell that all three are
    /// one defect: the container does not bound its child's width, so there is
    /// nothing for wrapping to wrap against.
    ///
    /// Not fixed here because neither obvious remedy works: an explicit
    /// `allocate_ui_with_layout` with a bounded width leaves the offset unchanged
    /// to the pixel. What does work is inlining the content, which moves it from
    /// right-aligned to inline — a visual decision rather than a bug fix, and not
    /// one to make silently.
    /// Tabs known to paint past the right edge. **Empty, and that is the
    /// finding.**
    ///
    /// This list held four entries. Three were fixed. The fourth, `ai`, was
    /// never a defect at all: it was this guard measuring the wrong rectangle.
    ///
    /// `painted_text_rects_sized` used to build a text's rect as
    /// `Rect::from_min_size(shape.pos, galley.size())`, which assumes `pos` is
    /// the top-left corner. **For a galley with a centred or right horizontal
    /// alignment it is an anchor and the text extends away from it**, so the
    /// rect came out the right size in the wrong place. The AI tab's welcome
    /// sentence painted at `114..693` inside an 800 px window while this guard
    /// reported `404..983` and pinned a 183 px overrun that was never on screen.
    ///
    /// It reads `Shape::visual_bounding_rect` now, which egui computes from the
    /// galley's mesh — where the glyphs actually are, which is the only thing a
    /// reader can see.
    ///
    /// **The false positive was ~70% reproducible, not 100%**, which is what
    /// made it look like a real intermittent defect rather than a broken
    /// instrument. Whether the empty state rendered at all varied between
    /// process runs; when it rendered, the bad measurement followed
    /// deterministically.
    const KNOWN_OVERFLOWING: [&str; 0] = [];

    /// No tab may paint text past the right edge of a default window.
    ///
    /// The GUI had two distinct failure modes during the Dewey port and
    /// `painted_text` sees only one: a tab that drew nothing, and a tab that drew
    /// everything and clipped it off the right edge. Reading galley *text* catches
    /// the first and is blind to the second. This reads their rectangles.
    ///
    /// It caught a real one on Overview: the chat bar reserved a hardcoded 190 px
    /// for a Send button plus a 172 px status label, so the row ran 29 px past the
    /// window at every width. The text field absorbed the slack, which is why
    /// widening the window moved the overrun rather than removing it, and why
    /// everything else inside that tab's `ScrollArea` inherited the wider content
    /// box.
    ///
    /// Widths are the shipped default (`with_inner_size([1400, 900])`) and the
    /// shipped minimum (`with_min_inner_size([800, 600])`). Height is not checked:
    /// every tab body sits in a vertical `ScrollArea`, so tall content is
    /// reachable, while width in this layout is not scrollable.
    #[test]
    fn no_tab_paints_text_past_the_right_edge() {
        let ctx = themed_context();
        let mut app = crate::gui::app::IronMonitorApp::with_context(&ctx);
        let mut offenders = Vec::new();
        let mut unexpectedly_clean = Vec::new();
        let mut still_loading = Vec::new();

        for tab in TABS {
            if app.select_tab_by_name(tab).is_err() {
                continue;
            }

            // Settle the tab before measuring it — both halves.
            //
            // Four tabs fetch their contents off-thread and paint a spinner until
            // the data lands, so what this measures depends on whether the loader
            // won the race — and under the full parallel suite it sometimes does
            // and sometimes does not. That made this guard fail once and pass on
            // rerun, which is the same instrument as no guard at all.
            //
            // The loaders were once all this waited for, and that was not enough:
            // the collector's warm-up snapshot is built from an empty `Sources`,
            // so Accelerators was being measured on a machine that had no GPUs
            // yet. `settle` waits for both, and a tab that never settles is
            // skipped rather than measured mid-flight.
            if !settle(&mut app, &ctx, Duration::from_secs(30)) {
                still_loading.push(tab);
                continue;
            }

            let pinned = KNOWN_OVERFLOWING.contains(&tab);
            let mut any = false;

            for width in [1400.0_f32, 800.0] {
                let size = egui::Vec2::new(width, 900.0);
                for (text, past) in horizontal_overflow(&ctx, size, |ui| app.draw_current_tab(ui)) {
                    any = true;
                    if !pinned {
                        offenders.push(format!(
                            "{tab} @ {width:.0}px wide: {text:?} runs {past:.0}px past the edge"
                        ));
                    }
                }
            }

            // A pinned tab that has started fitting means the pin is now a lie.
            if pinned && !any {
                unexpectedly_clean.push(tab);
            }
        }

        assert!(
            offenders.is_empty(),
            "text painted outside the window, which a reader sees as clipped or missing:
  {}",
            offenders.join(
                "
  "
            )
        );

        // Reported rather than asserted: a loader that does not finish inside the
        // budget is a slow machine, not a layout defect, and failing on it would
        // make this guard about the runner instead of the GUI.
        if !still_loading.is_empty() {
            eprintln!("skipped, still loading after 5s: {still_loading:?}");
        }

        assert!(
            unexpectedly_clean.is_empty(),
            "pinned as overflowing but no longer overflowing: {unexpectedly_clean:?}. Remove them from KNOWN_OVERFLOWING so the guard covers them."
        );
    }
}

#[cfg(test)]
mod overview_extent {
    use super::*;

    /// The fit must wait for the Overview to finish loading.
    ///
    /// **This is the whole defect.** Two things arrive after the first frame and
    /// both change how tall the tab is: the background loaders, and the
    /// collector's first real snapshot. The pipeline publishes a warm-up
    /// generation built from an empty `Sources`, so it describes no GPUs *by
    /// construction* — fit against it and a three-card desktop is measured as a
    /// machine with no accelerators at all. A freshly constructed app has applied
    /// no snapshot yet, so it is in exactly that state, and the fit must decline.
    ///
    /// **An earlier note here has been withdrawn.** It recorded, as an
    /// unexplained curiosity, that the loaded Overview measured *shorter* than
    /// the unloaded one — 899.5 px against 918.5 — and guessed at a placeholder
    /// taller than its content. Both numbers came from the galley reader this
    /// harness used before it was corrected to measure painted ink. Measured
    /// properly the settled Overview is 917.5 px, which is the unloaded figure
    /// to within a pixel, and there is no curiosity to explain.
    ///
    /// Waiting is still right, for the reason it always was: the warm-up
    /// snapshot describes no GPUs by construction, so a tab measured before it
    /// lands is a different tab. That argument never depended on the numbers.
    #[test]
    fn the_fit_waits_for_the_overview_to_finish_loading() {
        let ctx = themed_context();
        let mut app = crate::gui::app::IronMonitorApp::with_context(&ctx);
        app.select_tab_by_name("overview")
            .expect("overview is a tab");

        assert!(
            !app.has_real_snapshot(),
            "a freshly constructed app has applied no snapshot, warm-up or otherwise"
        );
        assert!(
            !app.autofit_to_overview_once(&ctx),
            "the window must not be fitted to an Overview that has not loaded its data"
        );

        assert!(
            settle(&mut app, &ctx, Duration::from_secs(30)),
            "the Overview did not finish loading, so there is nothing trustworthy to measure"
        );

        // The fit measures what the tab drew, so a loaded app that has not drawn
        // yet still has nothing to go on.
        assert!(
            !app.autofit_to_overview_once(&ctx),
            "loaded but never drawn: there is no shortfall recorded to fit to"
        );
        let _ = painted_extent(&ctx, egui::Vec2::new(1400.0, 900.0), |ui| {
            app.draw_current_tab(ui)
        });

        assert!(
            app.autofit_to_overview_once(&ctx),
            "once everything has loaded and been drawn, the fit must run"
        );
        assert!(
            !app.autofit_to_overview_once(&ctx),
            "and it must run only once, or every frame resizes the user's window"
        );
    }

    /// What the fit measures is the height of the painted content, not the
    /// canvas it was given.
    ///
    /// `Context::used_size` cannot answer this: it reports *allocated* space,
    /// which headlessly is the whole 4000 px canvas, and in the live app is
    /// `-inf`, because tabs paint through panels rather than the measured `Ui`.
    /// An earlier version of this fit used it and silently never ran.
    #[test]
    fn the_fit_measures_paint_rather_than_canvas() {
        let ctx = themed_context();
        let mut app = crate::gui::app::IronMonitorApp::with_context(&ctx);
        app.select_tab_by_name("overview")
            .expect("overview is a tab");
        assert!(settle(&mut app, &ctx, Duration::from_secs(30)));

        let extent = painted_extent(&ctx, egui::Vec2::new(1400.0, 4000.0), |ui| {
            app.draw_current_tab(ui)
        });
        assert!(
            extent.y > 300.0 && extent.y < 3000.0,
            "the Overview should paint a windowful, not nothing and not the whole              4000 px canvas; got {:.1}",
            extent.y
        );
    }

    /// Width is not fittable, asserted with numbers rather than a comment.
    #[test]
    fn the_overview_takes_whatever_width_it_is_given() {
        let ctx = themed_context();
        let mut app = crate::gui::app::IronMonitorApp::with_context(&ctx);
        app.select_tab_by_name("overview")
            .expect("overview is a tab");

        let wide = painted_extent(&ctx, egui::Vec2::new(1400.0, 4000.0), |ui| {
            app.draw_current_tab(ui)
        });
        let narrow = painted_extent(&ctx, egui::Vec2::new(1100.0, 4000.0), |ui| {
            app.draw_current_tab(ui)
        });

        assert!(
            wide.x > narrow.x + 200.0,
            "elastic layout: a 300 px wider canvas should paint wider, got {:.0} vs {:.0}",
            wide.x,
            narrow.x
        );
        assert!(
            (wide.y - narrow.y).abs() < 2.0,
            "height must not depend on width: {:.1} at 1400 vs {:.1} at 1100",
            wide.y,
            narrow.y
        );
    }
}
