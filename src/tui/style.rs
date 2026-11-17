//! The look every TUI tab shares, taken from the Inference monitor: rounded panels
//! with dim borders, green headings, value-first tiles with bar sparklines, and
//! shaded bars that are left out, not drawn empty, when there is no reading.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
    Frame,
};

use crate::inference::monitor::Tile;

/// Headings, lit states and bars.
pub const ACCENT: Color = Color::Rgb(46, 204, 145);
/// Temperatures and anything asking for attention.
pub const WARM: Color = Color::Rgb(240, 190, 80);
/// Something past its limit.
pub const HOT: Color = Color::Rgb(235, 95, 95);
/// Borders, labels and figures that are not there.
pub const DIM: Color = Color::Rgb(110, 118, 129);
/// The unfilled part of a bar.
pub const TRACK: Color = Color::Rgb(45, 50, 58);

pub const TILE_COLUMNS: usize = 4;
/// Border, value, detail, two rows of bars, border.
pub const TILE_HEIGHT: u16 = 6;

pub fn heading(text: impl Into<String>) -> Span<'static> {
    Span::styled(
        text.into(),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )
}

/// A panel: rounded, dim border, green title, text one column in from the border.
pub fn block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .padding(Padding::horizontal(1))
        .title_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
}

/// A panel with a title, padded the way the tiles are.
pub fn titled(title: impl Into<String>) -> Block<'static> {
    block().title(Span::styled(
        format!(" {} ", title.into()),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    ))
}

/// A bar across the width: filled in `color`, the rest shaded.
pub fn gauge_bar(ratio: f64, width: usize, color: Color) -> Line<'static> {
    let filled = (ratio.clamp(0.0, 1.0) * width as f64).round() as usize;
    Line::from(vec![
        Span::styled("█".repeat(filled), Style::default().fg(color)),
        Span::styled(
            "░".repeat(width.saturating_sub(filled)),
            Style::default().fg(TRACK),
        ),
    ])
}

/// Green to amber to red as a share of a limit rises.
pub fn load_color(percent: f64) -> Color {
    if percent >= 90.0 {
        HOT
    } else if percent >= 70.0 {
        WARM
    } else {
        ACCENT
    }
}

/// One panel holding a headline figure, its detail, and a bar of `ratio`.
///
/// With no ratio there is no bar: a bar at zero would read as a reading of zero.
pub fn metric_bar(
    f: &mut Frame,
    area: Rect,
    title: impl Into<String>,
    value: &str,
    detail: &str,
    ratio: Option<f64>,
) {
    let panel = titled(title);
    let inner = panel.inner(area);
    f.render_widget(panel, area);
    if inner.height == 0 {
        return;
    }
    let value_w = value.chars().count().max(6) + 2;
    let bar_w = (inner.width as usize / 4).clamp(10, 40);
    let detail_w = (inner.width as usize).saturating_sub(value_w + bar_w + 2);
    let detail: String = if detail.chars().count() > detail_w {
        let mut d: String = detail.chars().take(detail_w.saturating_sub(1)).collect();
        d.push('…');
        d
    } else {
        detail.to_string()
    };
    let mut spans = vec![
        Span::styled(
            format!("{value:<value_w$}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{detail:<detail_w$}  "),
            Style::default().fg(Color::Gray),
        ),
    ];
    match ratio {
        Some(r) => spans.extend(gauge_bar(r, bar_w, load_color(r * 100.0)).spans),
        None => spans.push(Span::styled(
            format!("{:>bar_w$}", "no reading"),
            Style::default().fg(DIM),
        )),
    }
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

/// The tile's history scaled 0-100 against its fixed top, or its own peak.
fn spark_points(tile: &Tile) -> Vec<u64> {
    let peak = tile.spark.iter().copied().fold(0.0_f64, f64::max);
    let top = tile.spark_max.unwrap_or(peak);
    if top <= 0.0 {
        return Vec::new();
    }
    tile.spark
        .iter()
        .map(|v| ((v / top) * 100.0).clamp(0.0, 100.0).round() as u64)
        .collect()
}

/// A tile: title in the border, the value in bold, a grey line of detail, and a
/// bar sparkline of its history.
pub fn draw_tile(f: &mut Frame, area: Rect, tile: &Tile) {
    let block = block()
        .title_style(Style::default())
        .title(Span::raw(format!(" {} ", tile.title)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let headline = match &tile.value {
        Some(v) => Line::from(Span::styled(
            format!("{v} {}", tile.unit).trim_end().to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        None => Line::from(Span::styled("—", Style::default().fg(DIM))),
    };
    let detail = Line::from(Span::styled(
        tile.detail.clone(),
        Style::default().fg(Color::Gray),
    ));
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);
    f.render_widget(Paragraph::new(headline), parts[0]);
    f.render_widget(Paragraph::new(detail), parts[1]);
    if parts[2].height == 0 {
        return;
    }
    let color = if tile.title.contains("temp") {
        WARM
    } else {
        ACCENT
    };
    let rows = spark_rows(
        &spark_points(tile),
        parts[2].width as usize,
        parts[2].height as usize,
    );
    let lines: Vec<Line> = rows
        .into_iter()
        .map(|row| {
            Line::from(
                row.into_iter()
                    .map(|(ch, present)| {
                        Span::styled(
                            ch.to_string(),
                            Style::default().fg(if present { color } else { TRACK }),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    f.render_widget(Paragraph::new(lines), parts[2]);
}

const EIGHTHS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Bars of `points` (0-100) in a box exactly `width` by `height`, top row first.
///
/// Every tile's graph is the same shape whatever its history holds: the newest
/// sample sits at the right edge, and a column with no sample yet is a dim
/// baseline marked absent, so a short history is not stretched or shrunk.
fn spark_rows(points: &[u64], width: usize, height: usize) -> Vec<Vec<(char, bool)>> {
    let shown = &points[points.len().saturating_sub(width)..];
    let pad = width - shown.len();
    let mut rows = vec![Vec::with_capacity(width); height];
    for col in 0..width {
        let sample = col.checked_sub(pad).map(|i| shown[i]);
        let eighths = sample.map(|v| (v.min(100) as usize * height * 8 + 50) / 100);
        for (r, row) in rows.iter_mut().enumerate() {
            // Row 0 is the top; the bottom row fills first.
            let floor = (height - 1 - r) * 8;
            let cell = match eighths {
                None if r == height - 1 => ('▁', false),
                None => (' ', false),
                // A reading of zero still shows, as the thinnest bar.
                Some(0) if r == height - 1 => ('▁', true),
                Some(e) => (EIGHTHS[e.saturating_sub(floor).min(8)], true),
            };
            row.push(cell);
        }
    }
    rows
}

/// Rows of tiles, `TILE_COLUMNS` to a row, two columns apart.
pub fn tile_grid(f: &mut Frame, area: Rect, tiles: &[Tile]) {
    let rows = tiles.len().div_ceil(TILE_COLUMNS);
    let bands = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(TILE_HEIGHT); rows])
        .split(area);
    for (r, chunk) in tiles.chunks(TILE_COLUMNS).enumerate() {
        let cells = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Ratio(1, TILE_COLUMNS as u32);
                TILE_COLUMNS
            ])
            .spacing(2)
            .split(bands[r]);
        for (c, tile) in chunk.iter().enumerate() {
            draw_tile(f, cells[c], tile);
        }
    }
}

/// The height `tile_grid` needs for `n` tiles.
pub fn grid_height(n: usize) -> u16 {
    n.div_ceil(TILE_COLUMNS) as u16 * TILE_HEIGHT
}

#[cfg(test)]
mod tests {
    use super::spark_rows;

    fn shape(rows: &[Vec<(char, bool)>]) -> (usize, usize) {
        (rows[0].len(), rows.len())
    }

    #[test]
    fn every_graph_is_the_same_box() {
        assert_eq!(shape(&spark_rows(&[], 30, 2)), (30, 2));
        assert_eq!(shape(&spark_rows(&[50], 30, 2)), (30, 2));
        assert_eq!(shape(&spark_rows(&[10; 200], 30, 2)), (30, 2));
    }

    #[test]
    fn newest_sample_is_at_the_right_and_missing_columns_are_absent() {
        let rows = spark_rows(&[100], 4, 2);
        assert_eq!(rows[1][3], ('█', true));
        assert_eq!(rows[0][3], ('█', true));
        assert_eq!(rows[1][0], ('▁', false));
        assert_eq!(rows[0][0], (' ', false));
    }

    #[test]
    fn half_fills_the_bottom_row_only() {
        let rows = spark_rows(&[50], 1, 2);
        assert_eq!(rows[1][0], ('█', true));
        assert_eq!(rows[0][0], (' ', true));
    }
}
