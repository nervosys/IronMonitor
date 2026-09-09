//! Cyber theme for IronMonitor GUI
//!
//! A dark cyberpunk-inspired theme with neon accents
//! Now with Glances-style threshold colors

use egui::{
    Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Style, TextStyle, Visuals,
};
use std::sync::Arc;

/// Cyber color palette
pub struct CyberColors;

impl CyberColors {
    // Primary colors
    pub const BACKGROUND: Color32 = Color32::from_rgb(13, 17, 23);
    pub const BACKGROUND_DARK: Color32 = Color32::from_rgb(8, 10, 15);
    #[allow(dead_code)]
    pub const BACKGROUND_LIGHT: Color32 = Color32::from_rgb(22, 27, 34);
    pub const SURFACE: Color32 = Color32::from_rgb(30, 37, 46);
    pub const SURFACE_HOVER: Color32 = Color32::from_rgb(40, 48, 58);

    // Accent colors (neon)
    pub const CYAN: Color32 = Color32::from_rgb(0, 255, 255);
    pub const CYAN_DIM: Color32 = Color32::from_rgb(0, 180, 180);
    pub const MAGENTA: Color32 = Color32::from_rgb(255, 0, 255);
    #[allow(dead_code)]
    pub const MAGENTA_DIM: Color32 = Color32::from_rgb(180, 0, 180);
    pub const NEON_GREEN: Color32 = Color32::from_rgb(57, 255, 20);
    pub const NEON_ORANGE: Color32 = Color32::from_rgb(255, 165, 0);
    pub const NEON_YELLOW: Color32 = Color32::from_rgb(255, 255, 0);
    pub const NEON_RED: Color32 = Color32::from_rgb(255, 60, 60);
    pub const NEON_BLUE: Color32 = Color32::from_rgb(0, 150, 255);
    pub const NEON_PURPLE: Color32 = Color32::from_rgb(180, 100, 255);

    // Text colors
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(230, 237, 243);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(140, 148, 158);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(100, 108, 118);

    // Status colors
    #[allow(dead_code)]
    pub const SUCCESS: Color32 = Color32::from_rgb(46, 160, 67);
    pub const WARNING: Color32 = Color32::from_rgb(210, 153, 34);
    pub const ERROR: Color32 = Color32::from_rgb(218, 54, 51);
    #[allow(dead_code)]
    pub const INFO: Color32 = Color32::from_rgb(56, 139, 253);

    // Glances-style threshold colors
    pub const THRESHOLD_OK: Color32 = Color32::from_rgb(46, 204, 113);
    pub const THRESHOLD_CAREFUL: Color32 = Color32::from_rgb(52, 211, 255);
    pub const THRESHOLD_WARNING: Color32 = Color32::from_rgb(255, 206, 86);
    pub const THRESHOLD_CRITICAL: Color32 = Color32::from_rgb(255, 99, 99);

    // Grid and borders
    #[allow(dead_code)]
    pub const GRID: Color32 = Color32::from_rgb(48, 54, 61);
    pub const BORDER: Color32 = Color32::from_rgb(48, 54, 61);
    #[allow(dead_code)]
    pub const BORDER_GLOW: Color32 = Color32::from_rgb(0, 200, 200);
}

pub fn threshold_color(percent: f32) -> Color32 {
    match percent {
        p if p >= 90.0 => CyberColors::THRESHOLD_CRITICAL,
        p if p >= 70.0 => CyberColors::THRESHOLD_WARNING,
        p if p >= 50.0 => CyberColors::THRESHOLD_CAREFUL,
        _ => CyberColors::THRESHOLD_OK,
    }
}

// ── Device-class color palettes (matches TUI conventions) ────────────────────

/// Device-class title colors (bright cyberpunk neon)
pub struct DeviceTitleColors;

impl DeviceTitleColors {
    pub const CPU: Color32 = Color32::from_rgb(0, 255, 255); // Neon Cyan
    pub const ACCEL: Color32 = Color32::from_rgb(57, 255, 20); // Neon Green
    pub const MEMORY: Color32 = Color32::from_rgb(255, 0, 255); // Neon Magenta
    pub const DISK: Color32 = Color32::from_rgb(255, 165, 0); // Neon Orange
    pub const NETWORK: Color32 = Color32::from_rgb(0, 150, 255); // Neon Blue
}

/// CPU device-class color (Neon Cyan family)
pub fn cpu_color(percent: f32) -> Color32 {
    match percent {
        p if p >= 90.0 => CyberColors::NEON_RED,
        p if p >= 70.0 => CyberColors::NEON_YELLOW,
        p if p >= 50.0 => Color32::from_rgb(0, 200, 200), // bright cyan
        _ => Color32::from_rgb(0, 255, 255),              // neon cyan
    }
}

/// Accelerator (GPU/NPU) device-class color (Neon Green family)
pub fn accel_color(percent: f32) -> Color32 {
    match percent {
        p if p >= 90.0 => CyberColors::NEON_RED,
        p if p >= 70.0 => CyberColors::NEON_YELLOW,
        p if p >= 50.0 => Color32::from_rgb(80, 255, 80), // bright green
        _ => Color32::from_rgb(57, 255, 20),              // neon green
    }
}

/// Memory device-class color (Neon Magenta family)
pub fn memory_color(percent: f32) -> Color32 {
    match percent {
        p if p >= 90.0 => CyberColors::NEON_RED,
        p if p >= 70.0 => CyberColors::NEON_YELLOW,
        p if p >= 50.0 => Color32::from_rgb(255, 100, 255), // bright magenta
        _ => Color32::from_rgb(255, 0, 255),                // neon magenta
    }
}

/// Disk device-class color (Neon Orange family)
#[allow(dead_code)]
pub fn disk_color(percent: f32) -> Color32 {
    match percent {
        p if p >= 90.0 => CyberColors::NEON_RED,
        p if p >= 70.0 => CyberColors::NEON_YELLOW,
        p if p >= 50.0 => Color32::from_rgb(255, 200, 50), // bright amber
        _ => Color32::from_rgb(255, 165, 0),               // neon orange
    }
}

/// Network device-class color (Neon Blue family)
#[allow(dead_code)]
pub fn network_color(percent: f32) -> Color32 {
    match percent {
        p if p >= 90.0 => CyberColors::NEON_RED,
        p if p >= 70.0 => CyberColors::NEON_YELLOW,
        p if p >= 50.0 => Color32::from_rgb(80, 180, 255), // bright blue
        _ => Color32::from_rgb(0, 150, 255),               // neon blue
    }
}

pub fn trend_indicator(current: f32, previous: f32) -> (&'static str, Color32) {
    let delta = current - previous;
    if delta.abs() < 0.5 {
        ("-", CyberColors::TEXT_MUTED)
    } else if delta > 0.0 {
        ("^", CyberColors::THRESHOLD_CRITICAL)
    } else {
        ("v", CyberColors::THRESHOLD_OK)
    }
}

pub fn apply_cyber_theme(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "emoji".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoEmoji-VariableFont.ttf"
        ))),
    );

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("emoji".to_owned());

    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("emoji".to_owned());

    ctx.set_fonts(fonts);

    let mut style = Style::default();
    let mut visuals = Visuals::dark();

    visuals.window_fill = CyberColors::BACKGROUND;
    visuals.panel_fill = CyberColors::BACKGROUND;
    visuals.faint_bg_color = CyberColors::SURFACE;
    visuals.extreme_bg_color = CyberColors::BACKGROUND_DARK;

    visuals.widgets.noninteractive.bg_fill = CyberColors::SURFACE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, CyberColors::TEXT_SECONDARY);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, CyberColors::BORDER);

    visuals.widgets.inactive.bg_fill = CyberColors::SURFACE;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, CyberColors::TEXT_PRIMARY);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, CyberColors::BORDER);

    visuals.widgets.hovered.bg_fill = CyberColors::SURFACE_HOVER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, CyberColors::CYAN);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, CyberColors::CYAN_DIM);

    // `widgets.active` is not only the pressed-widget style: egui derives the global
    // strong-text colour from it (`Visuals::strong_text_color` -> `widgets.active
    // .text_color()`). This used to be a bright CYAN_DIM fill with BACKGROUND-coloured
    // text, which reads well on a pressed button and is invisible everywhere else —
    // every `RichText::strong()` in the app was drawn in rgb(13,17,23) on the
    // rgb(13,17,23) panel. That is what made the Profiles tab look empty: it rendered
    // all 19 groups, each with an unreadable heading.
    //
    // A dark fill with bright text satisfies both roles: legible when pressed, legible
    // as strong body text. The cyan identity moves to the border.
    visuals.widgets.active.bg_fill = CyberColors::SURFACE_HOVER;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, CyberColors::TEXT_PRIMARY);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, CyberColors::CYAN);

    visuals.widgets.open.bg_fill = CyberColors::SURFACE_HOVER;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, CyberColors::CYAN);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, CyberColors::CYAN_DIM);

    visuals.selection.bg_fill = CyberColors::CYAN_DIM.linear_multiply(0.3);
    visuals.selection.stroke = Stroke::new(1.0_f32, CyberColors::CYAN);

    visuals.hyperlink_color = CyberColors::CYAN;

    visuals.window_shadow.color = Color32::from_black_alpha(120);
    visuals.popup_shadow.color = Color32::from_black_alpha(100);

    visuals.window_rounding = egui::Rounding::same(8.0);
    visuals.menu_rounding = egui::Rounding::same(6.0);

    style.visuals = visuals;

    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(18.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();

    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);

    ctx.set_style(style);
}

pub fn utilization_color(percent: f32) -> Color32 {
    if percent < 50.0 {
        CyberColors::NEON_GREEN
    } else if percent < 75.0 {
        CyberColors::NEON_YELLOW
    } else if percent < 90.0 {
        CyberColors::NEON_ORANGE
    } else {
        CyberColors::NEON_RED
    }
}

pub fn temperature_color(temp: u32) -> Color32 {
    if temp < 50 {
        CyberColors::NEON_GREEN
    } else if temp < 70 {
        CyberColors::NEON_YELLOW
    } else if temp < 85 {
        CyberColors::NEON_ORANGE
    } else {
        CyberColors::NEON_RED
    }
}

#[allow(dead_code)]
pub fn neon_color_by_index(index: usize) -> Color32 {
    const COLORS: &[Color32] = &[
        CyberColors::CYAN,
        CyberColors::MAGENTA,
        CyberColors::NEON_GREEN,
        CyberColors::NEON_ORANGE,
        CyberColors::NEON_PURPLE,
        CyberColors::NEON_BLUE,
        CyberColors::NEON_YELLOW,
        CyberColors::NEON_RED,
    ];
    COLORS[index % COLORS.len()]
}

/// Light theme colors
pub struct LightColors;

impl LightColors {
    pub const BACKGROUND: Color32 = Color32::from_rgb(250, 250, 252);
    pub const SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
    pub const SURFACE_HOVER: Color32 = Color32::from_rgb(240, 242, 245);
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(30, 41, 59);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(100, 116, 139);
    pub const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);
    pub const ACCENT_DIM: Color32 = Color32::from_rgb(96, 165, 250);
    pub const BORDER: Color32 = Color32::from_rgb(226, 232, 240);
}

/// Apply light theme to the egui context
pub fn apply_light_theme(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "emoji".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoEmoji-VariableFont.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("emoji".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("emoji".to_owned());
    ctx.set_fonts(fonts);

    let mut style = Style::default();
    let mut visuals = Visuals::light();
    visuals.window_fill = LightColors::BACKGROUND;
    visuals.panel_fill = LightColors::BACKGROUND;
    visuals.faint_bg_color = LightColors::SURFACE;
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.widgets.noninteractive.bg_fill = LightColors::SURFACE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, LightColors::TEXT_SECONDARY);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LightColors::BORDER);
    visuals.widgets.inactive.bg_fill = LightColors::SURFACE;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, LightColors::TEXT_PRIMARY);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LightColors::BORDER);
    visuals.widgets.hovered.bg_fill = LightColors::SURFACE_HOVER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, LightColors::ACCENT);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, LightColors::ACCENT_DIM);
    // Same constraint as the dark theme: this stroke doubles as the global strong-text
    // colour, so white here made every `strong()` label invisible on the near-white
    // panel. Dark text on a light fill satisfies both roles.
    visuals.widgets.active.bg_fill = LightColors::SURFACE_HOVER;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, LightColors::TEXT_PRIMARY);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, LightColors::ACCENT);
    visuals.widgets.open.bg_fill = LightColors::SURFACE_HOVER;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, LightColors::ACCENT);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, LightColors::ACCENT_DIM);
    visuals.selection.bg_fill = LightColors::ACCENT.linear_multiply(0.2);
    visuals.selection.stroke = Stroke::new(1.0_f32, LightColors::ACCENT);
    visuals.hyperlink_color = LightColors::ACCENT;
    visuals.window_shadow.color = Color32::from_black_alpha(30);
    visuals.popup_shadow.color = Color32::from_black_alpha(20);
    visuals.window_rounding = egui::Rounding::same(8.0);
    visuals.menu_rounding = egui::Rounding::same(6.0);
    style.visuals = visuals;
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(18.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Perceptual distance between two colours, as a rough sum of channel deltas.
    /// Good enough to catch "text drawn in the background colour"; not a WCAG metric.
    fn channel_distance(a: Color32, b: Color32) -> i32 {
        (a.r() as i32 - b.r() as i32).abs()
            + (a.g() as i32 - b.g() as i32).abs()
            + (a.b() as i32 - b.b() as i32).abs()
    }

    /// Every text role must be legible against the surface it is drawn on.
    ///
    /// `RichText::strong()` does not carry its own colour — egui resolves it through
    /// `Visuals::strong_text_color()`, which returns `widgets.active.text_color()`.
    /// That same stroke styles a pressed widget, so it is tempting to set it to the
    /// background colour for dark-on-accent buttons. Doing so silently paints every
    /// strong label in the app the same colour as the panel: the Profiles tab rendered
    /// all 19 of its groups with headings that could not be seen, which read as the
    /// tab being broken rather than as a contrast bug.
    #[test]
    fn strong_text_is_legible_on_the_panel_it_is_drawn_on() {
        for (name, apply) in [
            ("cyber", apply_cyber_theme as fn(&egui::Context)),
            ("light", apply_light_theme as fn(&egui::Context)),
        ] {
            let ctx = egui::Context::default();
            apply(&ctx);
            let visuals = ctx.style().visuals.clone();

            let panel = visuals.panel_fill;
            let strong = visuals.strong_text_color();
            let body = visuals.text_color();

            assert!(
                channel_distance(strong, panel) > 60,
                "{name}: strong text {strong:?} is indistinguishable from the panel \
                 fill {panel:?} — every RichText::strong() would be invisible"
            );
            assert!(
                channel_distance(body, panel) > 60,
                "{name}: body text {body:?} is indistinguishable from the panel fill \
                 {panel:?}"
            );

            // A pressed widget draws its own label on `active.bg_fill`, so that pair
            // has to hold up too — this is the constraint that motivated the original
            // background-coloured stroke.
            let active_fill = visuals.widgets.active.bg_fill;
            let active_text = visuals.widgets.active.text_color();
            assert!(
                channel_distance(active_text, active_fill) > 60,
                "{name}: pressed-widget text {active_text:?} is indistinguishable from \
                 its own fill {active_fill:?}"
            );
        }
    }
}
