//! Custom widgets for IronMonitor GUI
//!
//! Cyber-styled widgets for displaying hardware metrics
//! Now with Glances-style threshold colors and quicklook panel

use super::theme::{threshold_color, CyberColors};
use egui::epaint::PathShape;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, Widget};

/// A cyber-styled progress bar with glow effect, animated pulse, and Glances-style threshold colors
pub struct CyberProgressBar {
    progress: f32,
    color: Color32,
    label: Option<String>,
    show_percentage: bool,
    height: f32,
    animated: bool,
    use_threshold_color: bool,
    trend: Option<&'static str>,
}

impl CyberProgressBar {
    pub fn new(progress: f32) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            color: CyberColors::CYAN,
            label: None,
            show_percentage: true,
            height: 20.0,
            // Off by default. The animation is a scanline sweep and a glow pulse —
            // decoration that carries no reading — and any visible animated bar keeps
            // the whole window repainting continuously. On a 3440x1440 display that
            // cost more CPU than collecting the hardware data it was decorating. Opt
            // in with `.animated(true)` where it is worth the frames.
            animated: false,
            use_threshold_color: false,
            trend: None,
        }
    }

    pub fn color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Use Glances-style threshold colors based on percentage
    pub fn with_threshold_color(mut self) -> Self {
        self.use_threshold_color = true;
        self
    }

    /// Add a trend indicator (↑, ↓, or →)
    pub fn with_trend(mut self, trend: &'static str) -> Self {
        self.trend = Some(trend);
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[allow(dead_code)]
    pub fn show_percentage(mut self, show: bool) -> Self {
        self.show_percentage = show;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    #[allow(dead_code)]
    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }
}

impl Widget for CyberProgressBar {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = Vec2::new(ui.available_width(), self.height);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let time = ui.input(|i| i.time) as f32;

            // Use threshold color if enabled, otherwise use provided color
            let bar_color = if self.use_threshold_color {
                threshold_color(self.progress * 100.0)
            } else {
                self.color
            };

            // Background with gradient effect
            painter.rect_filled(rect, 4.0, CyberColors::BACKGROUND_DARK);

            // Subtle inner shadow
            let inner_shadow = Rect::from_min_size(
                rect.min + Vec2::new(1.0, 1.0),
                rect.size() - Vec2::new(2.0, 2.0),
            );
            painter.rect_filled(
                inner_shadow,
                3.0,
                Color32::from_rgba_unmultiplied(0, 0, 0, 40),
            );

            // Progress fill
            let fill_width = rect.width() * self.progress;
            if fill_width > 0.0 {
                let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_width, rect.height()));

                // Multi-layer gradient fill using bar_color
                let dark_color = bar_color.linear_multiply(0.4);
                let bright_color = bar_color.linear_multiply(1.2);

                // Base fill
                painter.rect_filled(fill_rect.shrink(1.0), 3.0, dark_color);

                // Middle gradient band
                let mid_rect = Rect::from_min_size(
                    fill_rect.min + Vec2::new(1.0, rect.height() * 0.3),
                    Vec2::new(fill_width - 2.0, rect.height() * 0.4),
                );
                painter.rect_filled(mid_rect, 0.0, bar_color.linear_multiply(0.7));

                // Top highlight (glossy effect)
                let highlight_rect = Rect::from_min_size(
                    fill_rect.min + Vec2::new(1.0, 1.0),
                    Vec2::new(fill_width - 2.0, rect.height() * 0.35),
                );
                let highlight_color = Color32::from_rgba_unmultiplied(
                    bright_color.r(),
                    bright_color.g(),
                    bright_color.b(),
                    80,
                );
                painter.rect_filled(highlight_rect, 2.0, highlight_color);

                // Animated scanline effect
                if self.animated && fill_width > 10.0 {
                    let scan_pos = ((time * 2.0).sin() * 0.5 + 0.5) * fill_width;
                    let scan_width = 20.0;
                    if scan_pos > 0.0 && scan_pos < fill_width {
                        let scan_rect = Rect::from_min_size(
                            fill_rect.min + Vec2::new(scan_pos - scan_width / 2.0, 0.0),
                            Vec2::new(
                                scan_width.min(fill_width - scan_pos + scan_width / 2.0),
                                rect.height(),
                            ),
                        );
                        let scan_color = Color32::from_rgba_unmultiplied(255, 255, 255, 25);
                        painter.rect_filled(scan_rect.shrink(1.0), 2.0, scan_color);
                    }
                }

                // Glow effect on the edge (pulsing)
                let glow_x = rect.min.x + fill_width;
                if fill_width > 2.0 {
                    let pulse = if self.animated {
                        (time * 3.0).sin() * 0.3 + 0.7
                    } else {
                        1.0
                    };

                    for i in 0..5 {
                        let alpha = ((80.0 - i as f32 * 15.0) * pulse) as u8;
                        let glow_color = Color32::from_rgba_unmultiplied(
                            bar_color.r(),
                            bar_color.g(),
                            bar_color.b(),
                            alpha,
                        );
                        painter.vline(
                            glow_x - i as f32,
                            rect.y_range(),
                            Stroke::new(1.0_f32, glow_color),
                        );
                    }

                    // Bright dot at the end
                    let dot_y = rect.center().y;
                    painter.circle_filled(
                        Pos2::new(glow_x - 1.0, dot_y),
                        3.0,
                        Color32::from_rgba_unmultiplied(255, 255, 255, (150.0 * pulse) as u8),
                    );
                }
            }

            // Outer border with subtle glow
            painter.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, CyberColors::BORDER));

            // Label and percentage with trend indicator
            let text_color = CyberColors::TEXT_PRIMARY;

            if let Some(label) = &self.label {
                // Add trend indicator to label if present
                let label_with_trend = if let Some(trend) = self.trend {
                    format!("{} {}", trend, label)
                } else {
                    label.clone()
                };

                // Text shadow
                painter.text(
                    Pos2::new(rect.min.x + 9.0, rect.center().y + 1.0),
                    egui::Align2::LEFT_CENTER,
                    &label_with_trend,
                    egui::FontId::proportional(12.0),
                    Color32::from_black_alpha(180),
                );
                painter.text(
                    Pos2::new(rect.min.x + 8.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &label_with_trend,
                    egui::FontId::proportional(12.0),
                    text_color,
                );
            }

            if self.show_percentage {
                let percent_text = format!("{:.1}%", self.progress * 100.0);
                // Text shadow
                painter.text(
                    Pos2::new(rect.max.x - 7.0, rect.center().y + 1.0),
                    egui::Align2::RIGHT_CENTER,
                    &percent_text,
                    egui::FontId::proportional(12.0),
                    Color32::from_black_alpha(180),
                );
                painter.text(
                    Pos2::new(rect.max.x - 8.0, rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    percent_text,
                    egui::FontId::proportional(12.0),
                    text_color,
                );
            }
        }

        // Drive the scanline and glow pulse at a bounded rate.
        //
        // This was a bare `request_repaint()`, which means "repaint as soon as
        // possible" — so a single visible progress bar pinned the whole app at the
        // display's refresh rate, permanently, to animate decoration. On a 3440x1440
        // screen that was the single largest consumer in the process, several times
        // the cost of actually collecting the hardware data. Decoration does not need
        // more than ~30 FPS, and egui still repaints instantly on input.
        if self.animated {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }

        response
    }
}

/// A cyber-styled metric card
pub struct MetricCard<'a> {
    title: &'a str,
    value: String,
    unit: Option<&'a str>,
    color: Color32,
    icon: Option<&'a str>,
}

impl<'a> MetricCard<'a> {
    pub fn new(title: &'a str, value: impl std::fmt::Display) -> Self {
        Self {
            title,
            value: value.to_string(),
            unit: None,
            color: CyberColors::CYAN,
            icon: None,
        }
    }

    pub fn unit(mut self, unit: &'a str) -> Self {
        self.unit = Some(unit);
        self
    }

    pub fn color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    #[allow(dead_code)]
    pub fn icon(mut self, icon: &'a str) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl Widget for MetricCard<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = Vec2::new(140.0, 70.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // Card background
            painter.rect_filled(rect, 6.0, CyberColors::SURFACE);

            // Accent border on left
            let accent_rect = Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height()));
            painter.rect_filled(
                accent_rect,
                egui::Rounding {
                    nw: 6.0,
                    sw: 6.0,
                    ne: 0.0,
                    se: 0.0,
                },
                self.color,
            );

            // Title
            painter.text(
                Pos2::new(rect.min.x + 12.0, rect.min.y + 12.0),
                egui::Align2::LEFT_TOP,
                self.title,
                egui::FontId::proportional(11.0),
                CyberColors::TEXT_SECONDARY,
            );

            // Value with unit
            let value_text = if let Some(unit) = self.unit {
                format!("{} {}", self.value, unit)
            } else {
                self.value.clone()
            };

            painter.text(
                Pos2::new(rect.min.x + 12.0, rect.max.y - 12.0),
                egui::Align2::LEFT_BOTTOM,
                value_text,
                egui::FontId::proportional(18.0),
                self.color,
            );
        }

        response
    }
}

/// Sparkline chart for historical data - sexy animated version with improved readability
pub struct SparklineChart {
    data: Vec<f32>,
    color: Color32,
    height: f32,
    show_grid: bool,
    show_glow: bool,
    show_dots: bool,
    smooth: bool,
    gradient_fill: bool,
    title: Option<String>,
    unit: Option<String>,
    show_scale: bool,
    fixed_max: Option<f32>,
    show_min_max: bool,
    line_thickness: f32,
}

impl SparklineChart {
    pub fn new(data: Vec<f32>) -> Self {
        Self {
            data,
            color: CyberColors::CYAN,
            height: 70.0,
            show_grid: true,
            show_glow: false, // Disabled - too bright
            show_dots: false, // Disabled - cluttered
            smooth: true,
            gradient_fill: false, // Disabled - cleaner look
            title: None,
            unit: None,
            show_scale: true,
            fixed_max: None,
            show_min_max: false, // Disabled by default - less clutter
            line_thickness: 2.5, // Default thicker lines
        }
    }

    pub fn color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    #[allow(dead_code)]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[allow(dead_code)]
    pub fn unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    #[allow(dead_code)]
    pub fn show_grid(mut self, show: bool) -> Self {
        self.show_grid = show;
        self
    }

    #[allow(dead_code)]
    pub fn show_glow(mut self, show: bool) -> Self {
        self.show_glow = show;
        self
    }

    #[allow(dead_code)]
    pub fn smooth(mut self, smooth: bool) -> Self {
        self.smooth = smooth;
        self
    }

    #[allow(dead_code)]
    pub fn show_scale(mut self, show: bool) -> Self {
        self.show_scale = show;
        self
    }

    /// Set a fixed maximum value for consistent scaling
    #[allow(dead_code)]
    pub fn max_value(mut self, max: f32) -> Self {
        self.fixed_max = Some(max);
        self
    }

    #[allow(dead_code)]
    pub fn show_min_max(mut self, show: bool) -> Self {
        self.show_min_max = show;
        self
    }

    /// Set the line thickness for the sparkline
    #[allow(dead_code)]
    pub fn line_thickness(mut self, thickness: f32) -> Self {
        self.line_thickness = thickness;
        self
    }
}

impl Widget for SparklineChart {
    fn ui(self, ui: &mut Ui) -> Response {
        // Reserve space for Y-axis labels on left
        let y_axis_width = if self.show_scale { 45.0 } else { 0.0 };
        let title_height = if self.title.is_some() { 20.0 } else { 0.0 };

        let desired_size = Vec2::new(ui.available_width(), self.height + title_height);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let time = ui.input(|i| i.time) as f32;

            // Calculate data bounds
            let data_min = self.data.iter().cloned().fold(f32::MAX, f32::min);
            let data_max = self.data.iter().cloned().fold(0.0_f32, f32::max);
            let max_val = self.fixed_max.unwrap_or_else(|| data_max.max(1.0));
            let current_val = self.data.last().cloned().unwrap_or(0.0);
            let unit_str = self.unit.as_deref().unwrap_or("");

            // Title area
            let graph_rect = if self.title.is_some() {
                let title_rect =
                    Rect::from_min_size(rect.min, Vec2::new(rect.width(), title_height));

                // Draw title
                if let Some(ref title) = self.title {
                    painter.text(
                        title_rect.left_center() + Vec2::new(y_axis_width + 5.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        title,
                        egui::FontId::proportional(13.0),
                        CyberColors::TEXT_SECONDARY,
                    );

                    // Min/max info (if enabled and data has variance)
                    if self.show_min_max && self.data.len() > 1 && data_max > data_min {
                        let min_max_text = format!("min:{:.0} max:{:.0}", data_min, data_max);
                        painter.text(
                            title_rect.center() + Vec2::new(20.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            min_max_text,
                            egui::FontId::proportional(10.0),
                            CyberColors::TEXT_MUTED,
                        );
                    }

                    // Current value on right side of title
                    let value_text = format!("{:.1}{}", current_val, unit_str);
                    painter.text(
                        title_rect.right_center() - Vec2::new(10.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        value_text,
                        egui::FontId::proportional(14.0),
                        self.color,
                    );
                }

                Rect::from_min_max(rect.min + Vec2::new(0.0, title_height), rect.max)
            } else {
                rect
            };

            // Background - slightly lighter for better contrast
            let muted_bg = Color32::from_rgba_unmultiplied(8, 12, 18, 200);
            painter.rect_filled(graph_rect, 6.0, muted_bg);

            // Very subtle inner shadow for depth
            let inner = graph_rect.shrink(1.0);
            painter.rect_filled(
                Rect::from_min_size(inner.min, Vec2::new(inner.width(), 2.0)),
                0.0,
                Color32::from_rgba_unmultiplied(0, 0, 0, 15),
            );

            // Calculate the actual chart area (with room for Y-axis labels)
            let chart_rect = Rect::from_min_max(
                graph_rect.min + Vec2::new(y_axis_width, 0.0),
                graph_rect.max,
            );

            // Draw Y-axis scale labels
            if self.show_scale {
                let scale_color = CyberColors::TEXT_MUTED;
                let font = egui::FontId::proportional(10.0);

                // Max value at top
                // Three digits or more reads fine without a decimal; below that the
                // tenth is the only thing distinguishing adjacent samples.
                let max_text = if max_val >= 100.0 {
                    format!("{:.0}{}", max_val, unit_str)
                } else {
                    format!("{:.1}{}", max_val, unit_str)
                };
                painter.text(
                    Pos2::new(
                        graph_rect.min.x + y_axis_width - 5.0,
                        chart_rect.min.y + 8.0,
                    ),
                    egui::Align2::RIGHT_CENTER,
                    max_text,
                    font.clone(),
                    scale_color,
                );

                // Mid value
                let mid_val = max_val / 2.0;
                let mid_text = if mid_val >= 100.0 {
                    format!("{:.0}", mid_val)
                } else {
                    format!("{:.1}", mid_val)
                };
                painter.text(
                    Pos2::new(graph_rect.min.x + y_axis_width - 5.0, chart_rect.center().y),
                    egui::Align2::RIGHT_CENTER,
                    mid_text,
                    font.clone(),
                    scale_color,
                );

                // Zero/min at bottom
                painter.text(
                    Pos2::new(
                        graph_rect.min.x + y_axis_width - 5.0,
                        chart_rect.max.y - 8.0,
                    ),
                    egui::Align2::RIGHT_CENTER,
                    "0",
                    font,
                    scale_color,
                );
            }

            // Simple static grid lines
            if self.show_grid {
                let grid_color = Color32::from_rgba_unmultiplied(60, 70, 80, 40);

                // Horizontal grid lines (4 lines = 5 zones for 0%, 25%, 50%, 75%, 100%)
                for i in 1..4 {
                    let y = chart_rect.min.y + chart_rect.height() * (i as f32 / 4.0);
                    painter.hline(chart_rect.x_range(), y, Stroke::new(0.5_f32, grid_color));
                }

                // Vertical grid lines (time markers) - very subtle
                let num_vlines = 6;
                for i in 1..num_vlines {
                    let x = chart_rect.min.x + chart_rect.width() * (i as f32 / num_vlines as f32);
                    painter.vline(
                        x,
                        chart_rect.y_range(),
                        Stroke::new(0.5_f32, grid_color.linear_multiply(0.5)),
                    );
                }
            }

            // Draw sparkline
            if self.data.len() >= 2 {
                let padding = 4.0;
                let plot_rect = chart_rect.shrink(padding);

                // Calculate points using the max_val calculated earlier
                let points: Vec<Pos2> = self
                    .data
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| {
                        let x = plot_rect.min.x
                            + (i as f32 / (self.data.len() - 1) as f32) * plot_rect.width();
                        let normalized = (v / max_val).clamp(0.0, 1.0);
                        let y = plot_rect.max.y - normalized * plot_rect.height() * 0.95;
                        Pos2::new(x, y)
                    })
                    .collect();

                // Smooth the curve using Catmull-Rom spline interpolation
                // Higher subdivision = smoother curves (8 for silky smooth)
                let smooth_points = if self.smooth && points.len() >= 4 {
                    catmull_rom_spline(&points, 8)
                } else if self.smooth && points.len() >= 2 {
                    // For fewer points, still apply some smoothing
                    catmull_rom_spline(&points, 4)
                } else {
                    points.clone()
                };

                // Gradient fill under the line (multiple layers for depth)
                if self.gradient_fill {
                    let mut fill_points = smooth_points.clone();
                    fill_points.push(Pos2::new(plot_rect.max.x, plot_rect.max.y));
                    fill_points.push(Pos2::new(plot_rect.min.x, plot_rect.max.y));

                    // Layer 1: Dark base
                    let fill_color_dark = Color32::from_rgba_unmultiplied(
                        self.color.r(),
                        self.color.g(),
                        self.color.b(),
                        15,
                    );
                    painter.add(egui::Shape::convex_polygon(
                        fill_points.clone(),
                        fill_color_dark,
                        Stroke::NONE,
                    ));

                    // Layer 2: Gradient bands (simulated)
                    for layer in 0..3 {
                        let _band_height = plot_rect.height() * (0.3 - layer as f32 * 0.1);
                        let alpha = 20 - layer * 5;
                        let band_points: Vec<Pos2> = smooth_points
                            .iter()
                            .map(|p| {
                                let y_offset = (plot_rect.max.y - p.y) * (0.3 - layer as f32 * 0.1);
                                Pos2::new(p.x, (p.y + y_offset).min(plot_rect.max.y))
                            })
                            .collect();

                        let mut band_fill = band_points;
                        band_fill.push(Pos2::new(plot_rect.max.x, plot_rect.max.y));
                        band_fill.push(Pos2::new(plot_rect.min.x, plot_rect.max.y));

                        let band_color = Color32::from_rgba_unmultiplied(
                            self.color.r(),
                            self.color.g(),
                            self.color.b(),
                            alpha as u8,
                        );
                        painter.add(egui::Shape::convex_polygon(
                            band_fill,
                            band_color,
                            Stroke::NONE,
                        ));
                    }
                }

                // Glow effect under the line (if enabled)
                if self.show_glow {
                    for offset in 1..=3 {
                        let glow_alpha = (40 - offset * 12) as u8;
                        let glow_color = Color32::from_rgba_unmultiplied(
                            self.color.r(),
                            self.color.g(),
                            self.color.b(),
                            glow_alpha,
                        );

                        let glow_points: Vec<Pos2> = smooth_points
                            .iter()
                            .map(|p| Pos2::new(p.x, p.y + offset as f32))
                            .collect();

                        let glow_path = PathShape::line(
                            glow_points,
                            Stroke::new(3.0 - offset as f32 * 0.5, glow_color),
                        );
                        painter.add(glow_path);
                    }
                }

                // Main line - bright color for visibility
                let line_color = Color32::from_rgba_unmultiplied(
                    (self.color.r() as f32 * 0.9) as u8,
                    (self.color.g() as f32 * 0.9) as u8,
                    (self.color.b() as f32 * 0.9) as u8,
                    240,
                );
                let main_path = PathShape::line(
                    smooth_points.clone(),
                    Stroke::new(self.line_thickness, line_color),
                );
                painter.add(main_path);

                // Data point dots (only on original points, not interpolated)
                if self.show_dots && points.len() <= 30 {
                    for (i, point) in points.iter().enumerate() {
                        let is_last = i == points.len() - 1;
                        let dot_size = if is_last { 5.0 } else { 2.5 };

                        // Outer glow for dots
                        if is_last {
                            let pulse = (time * 4.0).sin() * 0.3 + 0.7;
                            for r in (1..=3).rev() {
                                let alpha = ((60 - r * 15) as f32 * pulse) as u8;
                                painter.circle_filled(
                                    *point,
                                    dot_size + r as f32 * 2.0,
                                    Color32::from_rgba_unmultiplied(
                                        self.color.r(),
                                        self.color.g(),
                                        self.color.b(),
                                        alpha,
                                    ),
                                );
                            }
                        }

                        // Inner dot
                        painter.circle_filled(*point, dot_size, self.color);

                        // Bright center
                        if is_last {
                            painter.circle_filled(
                                *point,
                                dot_size * 0.5,
                                Color32::from_rgb(255, 255, 255),
                            );
                        }
                    }
                }
            }

            // Simple subtle border
            painter.rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(60, 70, 80, 80)),
            );
        }

        response
    }
}

/// Catmull-Rom spline interpolation for smooth curves
fn catmull_rom_spline(points: &[Pos2], subdivisions: usize) -> Vec<Pos2> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let mut result = Vec::new();

    for i in 0..points.len() - 1 {
        let p0 = if i == 0 { points[0] } else { points[i - 1] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < points.len() {
            points[i + 2]
        } else {
            points[points.len() - 1]
        };

        for j in 0..=subdivisions {
            let t = j as f32 / subdivisions as f32;
            let t2 = t * t;
            let t3 = t2 * t;

            let x = 0.5
                * ((2.0 * p1.x)
                    + (-p0.x + p2.x) * t
                    + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                    + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3);

            let y = 0.5
                * ((2.0 * p1.y)
                    + (-p0.y + p2.y) * t
                    + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                    + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3);

            if j == 0 && i > 0 {
                continue; // Skip duplicate points
            }
            result.push(Pos2::new(x, y));
        }
    }

    result
}

/// Section header with cyber styling
pub struct SectionHeader<'a> {
    title: &'a str,
    icon: Option<&'a str>,
}

impl<'a> SectionHeader<'a> {
    pub fn new(title: &'a str) -> Self {
        Self { title, icon: None }
    }

    pub fn icon(mut self, icon: &'a str) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl Widget for SectionHeader<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = Vec2::new(ui.available_width(), 28.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // Title with icon
            let title_text = if let Some(icon) = self.icon {
                format!("{} {}", icon, self.title)
            } else {
                self.title.to_string()
            };

            painter.text(
                Pos2::new(rect.min.x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                title_text,
                egui::FontId::proportional(14.0),
                CyberColors::CYAN,
            );

            // Decorative line
            let line_start = rect.min.x
                + painter
                    .layout_no_wrap(
                        self.title.to_string(),
                        egui::FontId::proportional(14.0),
                        CyberColors::CYAN,
                    )
                    .rect
                    .width()
                + 20.0;

            painter.hline(
                line_start..=rect.max.x,
                rect.center().y,
                Stroke::new(1.0_f32, CyberColors::BORDER),
            );
        }

        response
    }
}

/// Glances-style QuickLook summary panel
/// Shows CPU, MEM, SWAP, LOAD in a compact horizontal bar format
pub struct QuickLookPanel {
    cpu_percent: f32,
    mem_percent: f32,
    /// Swap usage, or `None` where the pagefile was not read.
    ///
    /// A summary panel showing `SWAP 0%` for a machine whose swap could not be
    /// read is the most reassuring possible rendering of an unknown.
    swap_percent: Option<f32>,
    load_1m: f32,
    cpu_trend: Option<&'static str>,
    mem_trend: Option<&'static str>,
}

impl QuickLookPanel {
    pub fn new(cpu: f32, mem: f32, swap: Option<f32>, load: f32) -> Self {
        Self {
            cpu_percent: cpu.clamp(0.0, 100.0),
            mem_percent: mem.clamp(0.0, 100.0),
            swap_percent: swap.map(|s| s.clamp(0.0, 100.0)),
            load_1m: load,
            cpu_trend: None,
            mem_trend: None,
        }
    }

    pub fn with_trends(mut self, cpu_trend: &'static str, mem_trend: &'static str) -> Self {
        self.cpu_trend = Some(cpu_trend);
        self.mem_trend = Some(mem_trend);
        self
    }
}

impl Widget for QuickLookPanel {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = Vec2::new(ui.available_width(), 32.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // Background
            painter.rect_filled(rect, 4.0, CyberColors::SURFACE);

            // Calculate bar widths (4 equal sections)
            let section_width = rect.width() / 4.0 - 8.0;
            let bar_height = 16.0;
            let y_center = rect.center().y;

            // Draw each metric
            let metrics = [
                ("CPU", Some(self.cpu_percent), self.cpu_trend),
                ("MEM", Some(self.mem_percent), self.mem_trend),
                ("SWAP", self.swap_percent, None),
                // Scale load to 0-100
                ("LOAD", Some((self.load_1m * 10.0).min(100.0)), None),
            ];

            for (i, (label, percent, trend)) in metrics.iter().enumerate() {
                // A metric with no reading draws its label and a dash, and no
                // bar at all -- an empty bar is indistinguishable from a bar at
                // zero, and only one of those is something that was measured.
                let percent = *percent;
                let x_start = rect.min.x + 4.0 + i as f32 * (section_width + 8.0);

                // Label with trend
                let label_text = if let Some(t) = trend {
                    format!("{} {}", t, label)
                } else {
                    label.to_string()
                };

                painter.text(
                    Pos2::new(x_start, y_center - 6.0),
                    egui::Align2::LEFT_CENTER,
                    &label_text,
                    egui::FontId::proportional(10.0),
                    CyberColors::TEXT_SECONDARY,
                );

                // Mini bar
                let bar_rect = Rect::from_min_size(
                    Pos2::new(x_start, y_center + 2.0),
                    Vec2::new(section_width * 0.6, bar_height * 0.5),
                );
                painter.rect_filled(bar_rect, 2.0, CyberColors::BACKGROUND_DARK);

                if let Some(pct) = percent {
                    let fill_width = bar_rect.width() * (pct / 100.0);
                    if fill_width > 0.0 {
                        let fill_rect = Rect::from_min_size(
                            bar_rect.min,
                            Vec2::new(fill_width, bar_rect.height()),
                        );
                        painter.rect_filled(fill_rect, 2.0, threshold_color(pct));
                    }
                }

                // Percentage text
                let percent_text = match (percent, *label == "LOAD") {
                    (_, true) => format!("{:.2}", self.load_1m),
                    (Some(pct), false) => format!("{pct:.0}%"),
                    (None, false) => "-".to_string(),
                };

                painter.text(
                    Pos2::new(x_start + section_width * 0.65, y_center + 2.0),
                    egui::Align2::LEFT_CENTER,
                    percent_text,
                    egui::FontId::proportional(11.0),
                    threshold_color(percent.unwrap_or(0.0)),
                );
            }

            // Border
            painter.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, CyberColors::BORDER));
        }

        response
    }
}

/// Glances-style threshold legend
pub struct ThresholdLegend;

impl Widget for ThresholdLegend {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = Vec2::new(ui.available_width(), 20.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let y = rect.center().y;

            let items = [
                ("OK", CyberColors::THRESHOLD_OK, "0-50%"),
                ("CAREFUL", CyberColors::THRESHOLD_CAREFUL, "50-70%"),
                ("WARNING", CyberColors::THRESHOLD_WARNING, "70-90%"),
                ("CRITICAL", CyberColors::THRESHOLD_CRITICAL, "90%+"),
            ];

            let mut x = rect.min.x;
            for (label, color, range) in items {
                // Color dot
                painter.circle_filled(Pos2::new(x + 6.0, y), 4.0, color);

                // Label
                painter.text(
                    Pos2::new(x + 14.0, y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(10.0),
                    color,
                );

                // Range
                let label_width = painter
                    .layout_no_wrap(label.to_string(), egui::FontId::proportional(10.0), color)
                    .rect
                    .width();

                painter.text(
                    Pos2::new(x + 16.0 + label_width, y),
                    egui::Align2::LEFT_CENTER,
                    range,
                    egui::FontId::proportional(9.0),
                    CyberColors::TEXT_MUTED,
                );

                x += 90.0;
            }
        }

        response
    }
}

/// Section heading whose domain word comes from the ontology rather than a literal.
///
/// The three surfaces used to spell the same domain three ways — the CLI said
/// `cpu`, the GUI said `CPU`, the JSON said `Cpu` — which is what stops an agent
/// correlating a heading a user is looking at with an id it can query. Routing the
/// domain word through [`crate::ontology::labels::domain_label`] makes the ontology
/// the single naming authority, so a heading cannot drift from the id space.
///
/// `domain` must be a domain the ontology knows; unknown domains are returned
/// title-cased rather than silently dropped, because a blank heading is worse than
/// an imperfect one.
pub fn domain_section_title(domain: &str, suffix: &str) -> String {
    let label = crate::ontology::labels::domain_label(domain);
    if suffix.is_empty() {
        label
    } else {
        format!("{label} {suffix}")
    }
}

#[cfg(test)]
mod section_title_tests {
    use super::*;

    #[test]
    fn domain_titles_use_the_ontology_spelling() {
        assert_eq!(
            domain_section_title("gpu", "Utilization"),
            "GPU Utilization"
        );
        assert_eq!(domain_section_title("cpu", "Overview"), "CPU Overview");
        assert_eq!(domain_section_title("network", "I/O"), "Network I/O");
        assert_eq!(domain_section_title("memory", ""), "Memory");
    }

    /// Whatever the surfaces render, the domain word has to be one the ontology
    /// recognises — otherwise the heading names something unqueryable.
    #[test]
    fn every_title_domain_is_queryable() {
        for domain in ["cpu", "gpu", "memory", "network", "disk", "thermal"] {
            assert!(
                crate::ontology::labels::is_known_domain(domain),
                "{domain} is used in a section title but is not an ontology domain"
            );
        }
    }
}

#[cfg(test)]
mod section_header_source_tests {
    use crate::ontology::labels;

    /// The GUI's own source, scanned for hardcoded section headings.
    ///
    /// Scanning the source is unusual in a test, but the alternative is worse: the
    /// headings are string literals scattered across five thousand lines, and there
    /// is no runtime registry to enumerate. Converting each by hand and trusting
    /// future edits to follow suit is exactly how the three surfaces drifted apart
    /// in the first place — the CLI saying `cpu`, the GUI `CPU`, the JSON `Cpu`.
    const GUI_SOURCE: &str = include_str!("app.rs");

    fn literal_headings() -> Vec<String> {
        let mut out = Vec::new();
        let needle = "SectionHeader::new(\"";
        let mut rest = GUI_SOURCE;
        while let Some(start) = rest.find(needle) {
            rest = &rest[start + needle.len()..];
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
            }
        }
        out
    }

    /// Any heading beginning with a word the ontology owns must spell it the
    /// ontology's way.
    ///
    /// Headings that are not entity namespaces — "Physical Memory", "Installed
    /// Drivers", "Nmap-Style Scan Results" — are deliberately untouched. Forcing
    /// those through the ontology would assert a correspondence that does not
    /// exist, which is its own kind of dishonesty.
    #[test]
    fn hardcoded_headings_do_not_contradict_the_ontology() {
        for heading in literal_headings() {
            // Strip a leading emoji or symbol so the first *word* is examined.
            let first_word = heading
                .split_whitespace()
                .find(|w| w.chars().any(|c| c.is_ascii_alphabetic()))
                .unwrap_or("");
            let lowered = first_word.to_ascii_lowercase();

            if labels::is_known_domain(&lowered) {
                assert_eq!(
                    first_word,
                    labels::domain_label(&lowered),
                    "the heading {heading:?} starts with the ontology domain \
                     {lowered:?} but spells it {first_word:?}; route it through \
                     `domain_section_title` so the surfaces cannot drift"
                );
            }
        }
    }

    /// The scan must actually find headings, or the assertion above is vacuous —
    /// a refactor that renamed the constructor would silently disable this test.
    #[test]
    fn the_source_scan_finds_headings() {
        let found = literal_headings();
        assert!(
            found.len() > 10,
            "expected the GUI to contain many section headings; the scan found \
             {} — has `SectionHeader::new` been renamed?",
            found.len()
        );
    }
}
