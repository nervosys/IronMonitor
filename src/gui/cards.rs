//! A dashboard look for the GUI: rounded cards with a soft shadow, large figures
//! with small units, line-and-area sparklines, pill chips, a ring gauge and thin
//! bars. Colours come from the active theme, so the cards read on Light and on
//! the dark themes alike.
//!
//! Every drawing keeps the provenance rule the rest of the GUI follows: with no
//! reading there is no bar, ring arc or line, only the reason as text.

use eframe::egui::{
    self, Color32, CornerRadius, Margin, Pos2, Rect, RichText, Sense, Shadow, Stroke, Vec2,
};

use super::theme::{self, CyberColors};

/// Corner radius of a card.
const RADIUS: u8 = 12;
/// Padding inside a card, and the gap between cards. Every tab uses these, so
/// the tabs read as one design.
pub const MARGIN: i8 = 14;
pub const GAP: f32 = 12.0;

/// The card surface for the active theme: white on Light, a lifted panel on dark.
fn surface(ctx: &egui::Context) -> Color32 {
    ctx.style().visuals.faint_bg_color
}

/// A faint line for borders and bar tracks.
fn hairline(ctx: &egui::Context) -> Color32 {
    let v = &ctx.style().visuals;
    v.widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6)
}

pub fn muted(ctx: &egui::Context) -> Color32 {
    theme::text_color(ctx, CyberColors::TEXT_SECONDARY)
}

pub fn strong(ctx: &egui::Context) -> Color32 {
    theme::text_color(ctx, CyberColors::TEXT_PRIMARY)
}

/// The accent for a healthy reading.
pub fn good(ctx: &egui::Context) -> Color32 {
    theme::color(ctx, CyberColors::SUCCESS)
}

/// The accent for temperatures.
pub fn warm(ctx: &egui::Context) -> Color32 {
    theme::color(ctx, CyberColors::NEON_ORANGE)
}

/// The accent for transfer rates.
pub fn info(ctx: &egui::Context) -> Color32 {
    theme::color(ctx, CyberColors::INFO)
}

pub fn bad(ctx: &egui::Context) -> Color32 {
    theme::color(ctx, CyberColors::ERROR)
}

pub fn caution(ctx: &egui::Context) -> Color32 {
    theme::color(ctx, CyberColors::WARNING)
}

/// A card holding a section: its header, then its body, then the gap to the
/// next card. The header is a `SectionHeader` so the heading-spelling scan in
/// widgets.rs still sees each literal title.
pub fn section<R>(
    ui: &mut egui::Ui,
    header: super::widgets::SectionHeader<'_>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let out = card(ui, |ui| {
        ui.add(header);
        add(ui)
    });
    ui.add_space(GAP);
    out
}

/// The card's frame, for panes that manage their own width and contents.
pub fn frame(ctx: &egui::Context) -> egui::Frame {
    let dark = ctx.style().visuals.dark_mode;
    egui::Frame::new()
        .fill(surface(ctx))
        .stroke(Stroke::new(1.0_f32, hairline(ctx)))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::same(MARGIN))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(if dark { 60 } else { 18 }),
        })
}

/// A rounded card filling the available width.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    frame(&ctx)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A card heading: bold, in the strong text colour.
pub fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .strong()
            .size(17.0)
            .color(strong(ui.ctx())),
    );
}

/// A tile's small title, with an optional glyph before it.
pub fn title(ui: &mut egui::Ui, glyph: &str, text: &str) {
    let c = muted(ui.ctx());
    ui.horizontal(|ui| {
        if !glyph.is_empty() {
            ui.label(RichText::new(glyph).size(14.0).color(c));
        }
        ui.label(RichText::new(text).size(14.0).color(c));
    });
}

/// A large figure with its unit set small beside it; a dash when there is none.
pub fn figure(ui: &mut egui::Ui, value: Option<&str>, unit: &str) {
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        match value {
            Some(v) => {
                ui.label(
                    RichText::new(v)
                        .font(theme::data_font(&ctx, 30.0))
                        .color(strong(&ctx)),
                );
                if !unit.is_empty() {
                    ui.label(RichText::new(unit).size(14.0).color(muted(&ctx)));
                }
            }
            None => {
                ui.label(RichText::new("—").size(30.0).color(muted(&ctx)));
            }
        }
    });
}

/// A small coloured dot followed by text, in a soft pill of the same hue.
///
/// `lit` fills the pill more strongly and keeps the text at full colour; an
/// unlit chip is a pale outline of the same hue, so the five states read as one
/// row with one of them on.
pub fn chip(ui: &mut egui::Ui, text: &str, hue: Color32, lit: bool) {
    let ctx = ui.ctx().clone();
    let font = egui::FontId::proportional(14.0);
    let fg = if lit {
        theme::legible(&ctx, hue)
    } else {
        hue.gamma_multiply(0.75)
    };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, fg);
    let pad = Vec2::new(12.0, 5.0);
    let dot = 7.0;
    let size = Vec2::new(
        pad.x * 2.0 + dot + 6.0 + galley.size().x,
        galley.size().y + pad.y * 2.0,
    );
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let fill = hue.gamma_multiply(if lit { 0.22 } else { 0.08 });
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(255), fill);
    if lit {
        painter.rect_stroke(
            rect,
            CornerRadius::same(255),
            Stroke::new(1.0_f32, hue.gamma_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
    }
    let dot_c = Pos2::new(rect.left() + pad.x + dot / 2.0, rect.center().y);
    painter.circle_filled(dot_c, dot / 2.0, fg);
    painter.galley(
        Pos2::new(
            dot_c.x + dot / 2.0 + 6.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        fg,
    );
}

/// A neutral pill for a status word in a table: grey with a dark dot.
pub fn status_pill(ui: &mut egui::Ui, text: &str) {
    let ctx = ui.ctx().clone();
    let hue = if text.contains("rror") || text.contains("bort") {
        bad(&ctx)
    } else {
        muted(&ctx)
    };
    let font = egui::FontId::proportional(13.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, strong(&ctx));
    let pad = Vec2::new(10.0, 4.0);
    let size = Vec2::new(
        pad.x * 2.0 + 12.0 + galley.size().x,
        galley.size().y + pad.y * 2.0,
    );
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(255), hue.gamma_multiply(0.14));
    painter.circle_filled(
        Pos2::new(rect.left() + pad.x + 3.0, rect.center().y),
        3.0,
        hue,
    );
    painter.galley(
        Pos2::new(
            rect.left() + pad.x + 12.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        strong(&ctx),
    );
}

/// A line with a translucent area under it, across the full width.
///
/// Scaled to `max` when the series has a fixed top (a percentage), otherwise to
/// its own peak. Fewer than two points draw nothing: one point is not a line.
pub fn area_spark(ui: &mut egui::Ui, points: &[f64], max: Option<f64>, hue: Color32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    if points.len() < 2 {
        return;
    }
    let peak = points.iter().copied().fold(0.0_f64, f64::max);
    let top = max.unwrap_or(peak);
    if top <= 0.0 {
        // A flat zero series is still a reading: a line along the floor.
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(2.0_f32, hue),
        );
        return;
    }
    let n = points.len() - 1;
    let pts: Vec<Pos2> = points
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + rect.width() * i as f32 / n as f32;
            let y = rect.bottom() - rect.height() * (v / top).clamp(0.0, 1.0) as f32;
            Pos2::new(x, y)
        })
        .collect();
    let fill = hue.gamma_multiply(0.15);
    let mut mesh = egui::Mesh::default();
    for w in pts.windows(2) {
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(w[0], fill);
        mesh.colored_vertex(w[1], fill);
        mesh.colored_vertex(Pos2::new(w[1].x, rect.bottom()), fill);
        mesh.colored_vertex(Pos2::new(w[0].x, rect.bottom()), fill);
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    let painter = ui.painter();
    painter.add(egui::Shape::mesh(mesh));
    painter.add(egui::Shape::line(pts, Stroke::new(2.0_f32, hue)));
}

/// A thin rounded bar of `ratio` over a pale track.
pub fn thin_bar(ui: &mut egui::Ui, ratio: f64, hue: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 7.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(4),
        hairline(ui.ctx()).gamma_multiply(0.7),
    );
    let w = rect.width() * ratio.clamp(0.0, 1.0) as f32;
    if w > 0.0 {
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(w.max(7.0), rect.height())),
            CornerRadius::same(4),
            hue,
        );
    }
}

/// A 270° ring with the figure in its middle. With no ratio only the track is
/// drawn, so an unknown fill never looks like an empty one.
pub fn ring(
    ui: &mut egui::Ui,
    ratio: Option<f64>,
    big: &str,
    small: &str,
    hue: Color32,
    size: f32,
) {
    let ctx = ui.ctx().clone();
    let size = size.min(ui.available_width());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), size), Sense::hover());
    let center = Pos2::new(rect.center().x, rect.top() + size / 2.0);
    let radius = size / 2.0 - 10.0;
    // From 135° (lower left) clockwise through the top to 405° (lower right).
    let arc = |from: f32, to: f32| -> Vec<Pos2> {
        let steps = 64;
        (0..=steps)
            .map(|i| {
                let t = from + (to - from) * i as f32 / steps as f32;
                let a = t.to_radians();
                Pos2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
            })
            .collect()
    };
    let painter = ui.painter();
    painter.add(egui::Shape::line(
        arc(135.0, 405.0),
        Stroke::new(14.0_f32, hairline(&ctx).gamma_multiply(0.7)),
    ));
    if let Some(r) = ratio {
        let r = r.clamp(0.0, 1.0) as f32;
        if r > 0.0 {
            let end = 135.0 + 270.0 * r;
            painter.add(egui::Shape::line(
                arc(135.0, end),
                Stroke::new(14.0_f32, hue),
            ));
            for p in [arc(135.0, 135.0)[0], arc(end, end)[0]] {
                painter.circle_filled(p, 7.0, hue);
            }
        }
    }
    painter.text(
        center - Vec2::new(0.0, 8.0),
        egui::Align2::CENTER_CENTER,
        big,
        theme::data_font(&ctx, 28.0),
        strong(&ctx),
    );
    painter.text(
        center + Vec2::new(0.0, 18.0),
        egui::Align2::CENTER_CENTER,
        small,
        egui::FontId::proportional(13.0),
        muted(&ctx),
    );
}
