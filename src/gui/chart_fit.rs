//! Charts that take the room a tab has.
//!
//! A tab's charts are the part of it that can give: text and tables have the
//! height their content needs, while a chart reads the same at many heights.
//! So the charts absorb the difference between the tab and the window, growing
//! on a large monitor and shrinking on a short one, and a reader who raises
//! the UI scale for legibility gets charts fitted to the smaller room in points
//! rather than a tab that overflows.
//!
//! Each frame the tab asks for every chart's height by its natural height,
//! then reports the room it had and the height it drew. The scale for the next
//! frame steps by the shortfall over the charts' natural height. Where a chart
//! sits beside something taller, growing it moves the content less than that,
//! so the step undershoots and never overshoots: the scale settles rather than
//! oscillating.
//!
//! A tab whose other content alone is taller than the window will scroll at
//! any chart size. Shrinking its charts would cost legibility and fit nothing,
//! so they stay at their natural height until the room changes.

/// The bounds on the scale. Below 0.6 a chart's axis labels crowd its line;
/// above 2 it stops adding detail and only takes room from what follows.
pub const MIN_SCALE: f32 = 0.6;
pub const MAX_SCALE: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct State {
    scale: f32,
    /// The room in which the tab was found to scroll whatever the charts did.
    scrolls_at: Option<f32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            scale: 1.0,
            scrolls_at: None,
        }
    }
}

/// One frame of a tab's chart fitting. See the module documentation.
pub struct ChartFit {
    id: egui::Id,
    state: State,
    natural: f32,
    drawn: f32,
}

impl ChartFit {
    /// Start a frame for the tab `name`, at the scale the last frame settled on.
    pub fn begin(ctx: &egui::Context, name: &str) -> Self {
        let id = egui::Id::new(("chart_fit", name));
        let state = ctx.data(|d| d.get_temp::<State>(id)).unwrap_or_default();
        Self {
            id,
            state,
            natural: 0.0,
            drawn: 0.0,
        }
    }

    /// The scale a tab's charts are drawn at, for tests.
    #[cfg(test)]
    pub fn scale_of(ctx: &egui::Context, name: &str) -> f32 {
        let id = egui::Id::new(("chart_fit", name));
        ctx.data(|d| d.get_temp::<State>(id))
            .unwrap_or_default()
            .scale
    }

    /// The height to draw a chart whose natural height is `natural`.
    pub fn chart(&mut self, natural: f32) -> f32 {
        self.chart_at_least(natural, 0.0)
    }

    /// As `chart`, but never shorter than `floor`: for small charts such as a
    /// per-core grid, which become unreadable well before the main chart does.
    pub fn chart_at_least(&mut self, natural: f32, floor: f32) -> f32 {
        let drawn = (natural * self.state.scale).max(floor).round();
        self.natural += natural;
        self.drawn += drawn;
        drawn
    }

    /// Close the frame: the tab had `room` points of height and drew `content`.
    pub fn finish(self, ctx: &egui::Context, room: f32, content: f32) {
        if self.natural <= 0.0 || room <= 0.0 {
            return;
        }
        let mut next = self.state;
        if next.scrolls_at.is_some_and(|r| (r - room).abs() < 1.0) {
            return;
        }
        next.scrolls_at = None;
        // `content - drawn` is at most what is not chart, so when it alone
        // overflows the room no chart size makes the tab fit.
        if content - self.drawn >= room {
            next.scale = 1.0;
            next.scrolls_at = Some(room);
        } else {
            next.scale =
                (self.state.scale + (room - content) / self.natural).clamp(MIN_SCALE, MAX_SCALE);
        }
        if (next.scale - self.state.scale).abs() > 0.005 || next.scrolls_at != self.state.scrolls_at
        {
            ctx.data_mut(|d| d.insert_temp(self.id, next));
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tab whose content is `fixed + k * charts`, fitted for a few frames.
    fn settle(fixed: f32, natural: f32, k: f32, room: f32) -> f32 {
        let ctx = egui::Context::default();
        for _ in 0..40 {
            let mut fit = ChartFit::begin(&ctx, "t");
            let drawn = fit.chart(natural);
            fit.finish(&ctx, room, fixed + k * drawn);
        }
        ChartFit::scale_of(&ctx, "t")
    }

    #[test]
    fn charts_grow_into_spare_room_and_shrink_into_a_short_one() {
        // 200 of text, 200 of chart, 600 of room: the chart doubles.
        assert!((settle(200.0, 200.0, 1.0, 600.0) - 2.0).abs() < 0.01);
        // 500 of room: the chart takes the 300 left.
        assert!((settle(200.0, 200.0, 1.0, 500.0) - 1.5).abs() < 0.01);
        // 340 of room: the chart gives up 60.
        assert!((settle(200.0, 200.0, 1.0, 340.0) - 0.7).abs() < 0.01);
    }

    #[test]
    fn a_chart_beside_taller_content_settles_rather_than_oscillating() {
        // Only half of each chart point shows in the content height.
        let s = settle(200.0, 200.0, 0.5, 400.0);
        assert!(
            (200.0 + 0.5 * 200.0 * s - 400.0).abs() < 2.0,
            "settled at {s}"
        );
    }

    #[test]
    fn a_tab_that_scrolls_anyway_keeps_its_charts_at_natural_size() {
        assert_eq!(settle(900.0, 200.0, 1.0, 600.0), 1.0);
    }
}
