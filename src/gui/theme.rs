//! Cyber theme for IronMonitor GUI
//!
//! A dark cyberpunk-inspired theme with neon accents
//! Now with Glances-style threshold colors

use super::app::ColorTheme;
use egui::{
    Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Style, TextStyle, Visuals,
};
use std::sync::Arc;

fn theme_id() -> egui::Id {
    egui::Id::new("ironmonitor_color_theme")
}

/// Apply the selected palette without changing typography or widget geometry.
pub fn apply_theme(ctx: &egui::Context, selected: ColorTheme) {
    if selected == ColorTheme::Light {
        apply_light_theme(ctx);
    } else {
        apply_cyber_theme(ctx);
    }
    ctx.data_mut(|data| data.insert_temp(theme_id(), selected));
    ctx.style_mut(|style| {
        let visuals = &mut style.visuals;
        if selected == ColorTheme::Monochrome {
            visuals.window_fill = Color32::from_gray(8);
            visuals.panel_fill = Color32::from_gray(8);
            visuals.faint_bg_color = Color32::from_gray(15);
            visuals.extreme_bg_color = Color32::from_gray(4);
            for widget in [
                &mut visuals.widgets.noninteractive,
                &mut visuals.widgets.inactive,
                &mut visuals.widgets.hovered,
                &mut visuals.widgets.active,
                &mut visuals.widgets.open,
            ] {
                widget.bg_fill = gray(widget.bg_fill);
                widget.weak_bg_fill = gray(widget.weak_bg_fill);
                widget.bg_stroke.color = gray(widget.bg_stroke.color);
                widget.fg_stroke.color = gray(widget.fg_stroke.color);
            }
            visuals.selection.bg_fill = Color32::from_gray(52);
            visuals.selection.stroke.color = Color32::from_gray(225);
            visuals.hyperlink_color = Color32::from_gray(215);
            visuals.warn_fg_color = Color32::from_gray(210);
            visuals.error_fg_color = Color32::from_gray(240);
            visuals.text_cursor.stroke.color = Color32::from_gray(225);
        } else {
            // Adapt the editor palettes to the existing monitor surfaces.
            let palette = match selected {
                ColorTheme::Dracula => Some((0x282a36, 0x21222c, 0x343746, 0xf8f8f2)),
                ColorTheme::OneDarkPro => Some((0x282c34, 0x21252b, 0x303640, 0xabb2bf)),
                ColorTheme::TokyoNight => Some((0x1a1b26, 0x16161e, 0x24283b, 0xc0caf5)),
                ColorTheme::Nord => Some((0x2e3440, 0x242933, 0x3b4252, 0xeceff4)),
                ColorTheme::Monokai => Some((0x272822, 0x1e1f1c, 0x34352f, 0xf8f8f2)),
                ColorTheme::GitHubDark => Some((0x0d1117, 0x010409, 0x161b22, 0xe6edf3)),
                _ => None,
            };
            if let Some((background, dark, surface, foreground)) = palette {
                let rgb =
                    |hex: u32| Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
                visuals.window_fill = rgb(background);
                visuals.panel_fill = rgb(background);
                visuals.extreme_bg_color = rgb(dark);
                visuals.faint_bg_color = rgb(surface);
                for widget in [
                    &mut visuals.widgets.noninteractive,
                    &mut visuals.widgets.inactive,
                    &mut visuals.widgets.hovered,
                    &mut visuals.widgets.active,
                    &mut visuals.widgets.open,
                ] {
                    widget.bg_fill = rgb(surface);
                    widget.weak_bg_fill = rgb(surface);
                    widget.fg_stroke.color = rgb(foreground);
                }
            }
            let accent = selected.accent_color();
            visuals.widgets.hovered.fg_stroke.color = accent;
            visuals.widgets.hovered.bg_stroke.color = accent.linear_multiply(0.7);
            visuals.widgets.active.bg_stroke.color = accent;
            visuals.widgets.open.fg_stroke.color = accent;
            visuals.widgets.open.bg_stroke.color = accent.linear_multiply(0.7);
            visuals.selection.stroke.color = accent;
            visuals.selection.bg_fill = accent.linear_multiply(0.2);
            visuals.hyperlink_color = accent;
        }
    });
}

fn gray(color: Color32) -> Color32 {
    // Work in premultiplied channels so translucent chart fills retain alpha.
    let value = ((u32::from(color.r()) * 54
        + u32::from(color.g()) * 183
        + u32::from(color.b()) * 19)
        / 256) as u8;
    Color32::from_rgba_premultiplied(value, value, value, color.a())
}

/// Resolve shared Overview colors for text, charts, and custom-painted widgets.
pub fn color(ctx: &egui::Context, base: Color32) -> Color32 {
    let selected = ctx
        .data(|data| data.get_temp::<ColorTheme>(theme_id()))
        .unwrap_or_default();
    if selected == ColorTheme::Monochrome {
        return gray(base);
    }
    if selected == ColorTheme::Cyber {
        return base;
    }
    let visuals = &ctx.style().visuals;
    match base {
        CyberColors::BACKGROUND => visuals.panel_fill,
        CyberColors::BACKGROUND_DARK => visuals.extreme_bg_color,
        CyberColors::SURFACE | CyberColors::BACKGROUND_LIGHT => visuals.faint_bg_color,
        CyberColors::SURFACE_HOVER => visuals.widgets.hovered.bg_fill,
        CyberColors::TEXT_PRIMARY => visuals.strong_text_color(),
        CyberColors::TEXT_SECONDARY | CyberColors::TEXT_MUTED => visuals.text_color(),
        CyberColors::BORDER => visuals.widgets.noninteractive.bg_stroke.color,
        CyberColors::CYAN | CyberColors::CYAN_DIM | CyberColors::BORDER_GLOW => {
            selected.accent_color()
        }
        CyberColors::MAGENTA | CyberColors::MAGENTA_DIM | CyberColors::NEON_PURPLE => {
            selected.secondary_color()
        }
        _ => base,
    }
}

/// Cyber color palette
pub struct CyberColors;

impl CyberColors {
    // Primary colors
    pub const BACKGROUND: Color32 = Color32::from_rgb(6, 8, 12);
    pub const BACKGROUND_DARK: Color32 = Color32::from_rgb(3, 5, 8);
    #[allow(dead_code)]
    pub const BACKGROUND_LIGHT: Color32 = Color32::from_rgb(10, 14, 20);
    pub const SURFACE: Color32 = Color32::from_rgb(12, 16, 23);
    pub const SURFACE_HOVER: Color32 = Color32::from_rgb(20, 27, 37);

    // Accent colors (neon)
    pub const CYAN: Color32 = Color32::from_rgb(64, 232, 216);
    pub const CYAN_DIM: Color32 = Color32::from_rgb(40, 159, 151);
    pub const MAGENTA: Color32 = Color32::from_rgb(184, 140, 255);
    #[allow(dead_code)]
    pub const MAGENTA_DIM: Color32 = Color32::from_rgb(122, 88, 181);
    pub const NEON_GREEN: Color32 = Color32::from_rgb(87, 235, 148);
    pub const NEON_ORANGE: Color32 = Color32::from_rgb(255, 164, 80);
    pub const NEON_YELLOW: Color32 = Color32::from_rgb(245, 218, 102);
    pub const NEON_RED: Color32 = Color32::from_rgb(255, 100, 119);
    pub const NEON_BLUE: Color32 = Color32::from_rgb(78, 169, 255);
    pub const NEON_PURPLE: Color32 = Color32::from_rgb(173, 128, 255);

    // Text colors
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(232, 237, 245);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(163, 175, 194);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(122, 138, 160);

    // Status colors
    #[allow(dead_code)]
    pub const SUCCESS: Color32 = Self::NEON_GREEN;
    pub const WARNING: Color32 = Self::NEON_YELLOW;
    pub const ERROR: Color32 = Self::NEON_RED;
    #[allow(dead_code)]
    pub const INFO: Color32 = Self::NEON_BLUE;

    // Glances-style threshold colors
    pub const THRESHOLD_OK: Color32 = Self::NEON_GREEN;
    pub const THRESHOLD_CAREFUL: Color32 = Self::CYAN;
    pub const THRESHOLD_WARNING: Color32 = Self::NEON_YELLOW;
    pub const THRESHOLD_CRITICAL: Color32 = Self::NEON_RED;

    // Grid and borders
    #[allow(dead_code)]
    pub const GRID: Color32 = Color32::from_rgb(52, 74, 91);
    pub const BORDER: Color32 = Color32::from_rgb(52, 74, 91);
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
    pub const CPU: Color32 = CyberColors::CYAN; // Neon Cyan
    pub const ACCEL: Color32 = CyberColors::NEON_GREEN; // Neon Green
    pub const MEMORY: Color32 = CyberColors::MAGENTA; // Neon Magenta
    pub const DISK: Color32 = CyberColors::NEON_ORANGE; // Neon Orange
    pub const NETWORK: Color32 = CyberColors::NEON_BLUE; // Neon Blue
}

/// CPU device-class color (Neon Cyan family)
pub fn cpu_color(percent: f32) -> Color32 {
    match percent {
        p if p >= 90.0 => CyberColors::NEON_RED,
        p if p >= 70.0 => CyberColors::NEON_YELLOW,
        p if p >= 50.0 => CyberColors::CYAN, // shared Overview accent
        _ => Color32::from_rgb(64, 232, 216), // neon cyan
    }
}

/// Accelerator (GPU/NPU) device-class color (Neon Green family)
pub fn accel_color(percent: f32) -> Color32 {
    match percent {
        p if p >= 90.0 => CyberColors::NEON_RED,
        p if p >= 70.0 => CyberColors::NEON_YELLOW,
        p if p >= 50.0 => CyberColors::NEON_GREEN, // shared Overview accent
        _ => Color32::from_rgb(87, 235, 148),      // neon green
    }
}

/// Memory device-class color (Neon Magenta family)
pub fn memory_color(percent: f32) -> Color32 {
    match percent {
        p if p >= 90.0 => CyberColors::NEON_RED,
        p if p >= 70.0 => CyberColors::NEON_YELLOW,
        p if p >= 50.0 => CyberColors::MAGENTA, // shared Overview accent
        _ => Color32::from_rgb(184, 140, 255),  // neon magenta
    }
}

/// Disk device-class color (Neon Orange family)
#[allow(dead_code)]
pub fn disk_color(percent: f32) -> Color32 {
    match percent {
        p if p >= 90.0 => CyberColors::NEON_RED,
        p if p >= 70.0 => CyberColors::NEON_YELLOW,
        p if p >= 50.0 => CyberColors::NEON_ORANGE, // shared Overview accent
        _ => Color32::from_rgb(255, 164, 80),       // neon orange
    }
}

/// Network device-class color (Neon Blue family)
#[allow(dead_code)]
pub fn network_color(percent: f32) -> Color32 {
    match percent {
        p if p >= 90.0 => CyberColors::NEON_RED,
        p if p >= 70.0 => CyberColors::NEON_YELLOW,
        p if p >= 50.0 => CyberColors::NEON_BLUE, // shared Overview accent
        _ => Color32::from_rgb(78, 169, 255),     // neon blue
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
    ctx.data_mut(|data| data.insert_temp(theme_id(), ColorTheme::Cyber));
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

    visuals.window_corner_radius = egui::CornerRadius::same(8);
    visuals.menu_corner_radius = egui::CornerRadius::same(6);

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
    style.spacing.window_margin = egui::Margin::same(12);
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
    ctx.data_mut(|data| data.insert_temp(theme_id(), ColorTheme::Light));
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
    visuals.window_corner_radius = egui::CornerRadius::same(8);
    visuals.menu_corner_radius = egui::CornerRadius::same(6);
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
    style.spacing.window_margin = egui::Margin::same(12);
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
            ("dark monochrome", |ctx: &egui::Context| {
                apply_theme(ctx, ColorTheme::Monochrome)
            }),
            ("dracula", |ctx: &egui::Context| {
                apply_theme(ctx, ColorTheme::Dracula)
            }),
            ("one dark pro", |ctx: &egui::Context| {
                apply_theme(ctx, ColorTheme::OneDarkPro)
            }),
            ("tokyo night", |ctx: &egui::Context| {
                apply_theme(ctx, ColorTheme::TokyoNight)
            }),
            ("nord", |ctx: &egui::Context| {
                apply_theme(ctx, ColorTheme::Nord)
            }),
            ("monokai", |ctx: &egui::Context| {
                apply_theme(ctx, ColorTheme::Monokai)
            }),
            ("github dark", |ctx: &egui::Context| {
                apply_theme(ctx, ColorTheme::GitHubDark)
            }),
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
