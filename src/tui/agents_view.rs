//! The Agents tab's panel.
//!
//! Draws the [`Report`] that `crate::agents::report` builds from cached state. It
//! reads nothing from disk and starts nothing: the background poller owns both.
//!
//! Laid out like the Inference monitor: a bold summary, then each section under a
//! heading, with session lists as aligned columns rather than run-on prose.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
    Frame,
};

use crate::agents::report::{Report, Section};

use super::style::{accent, dim, warm};
/// Columns between table cells.
const GAP: usize = 3;

/// Draw the report, starting `scroll` lines down.
pub fn draw(f: &mut Frame, area: Rect, report: &Report, scroll: u16) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(dim()))
        .padding(Padding::horizontal(1))
        .title(Span::styled(" Agents ", heading()));
    let inner = block.inner(area).width as usize;
    let paragraph = Paragraph::new(to_lines_width(report, inner))
        .block(block)
        .scroll((scroll, 0));
    f.render_widget(paragraph, area);
}

/// The report as lines, at no particular width.
#[cfg(test)]
pub fn to_lines(report: &Report) -> Vec<Line<'static>> {
    to_lines_width(report, usize::MAX)
}

/// The report as lines: summary first, then each section with its table and notes.
/// Lines are cut at `width`, so a narrow terminal truncates rather than wraps a row
/// into the one below it.
pub fn to_lines_width(report: &Report, width: usize) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    for (i, s) in report.summary.iter().enumerate() {
        let style = if i == 0 {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(dim())
        };
        out.push(Line::from(Span::styled(s.clone(), style)));
    }
    for section in &report.sections {
        out.push(Line::default());
        out.push(Line::from(Span::styled(
            section.title.to_uppercase(),
            heading(),
        )));
        table_lines(section, width, &mut out);
        for row in &section.rows {
            let style = if row.starts_with("Not read") || row.starts_with("unknown") {
                Style::default().fg(dim())
            } else {
                Style::default()
            };
            out.push(Line::from(Span::styled(clip(row, width), style)));
        }
        if section.not_shown > 0 {
            out.push(Line::from(Span::styled(
                format!(
                    "… {} more not shown (the list is capped)",
                    section.not_shown
                ),
                Style::default().fg(dim()),
            )));
        }
    }
    out
}

fn table_lines(section: &Section, width: usize, out: &mut Vec<Line<'static>>) {
    if section.columns.is_empty() {
        return;
    }
    let widths: Vec<usize> = section
        .columns
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            section
                .table
                .iter()
                .filter_map(|r| r.get(i))
                .map(|c| c.chars().count())
                .chain(std::iter::once(name.chars().count()))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let pad = |text: &str, i: usize| {
        let (_, right) = section.columns[i];
        let w = widths[i];
        if right {
            format!("{text:>w$}")
        } else {
            format!("{text:<w$}")
        }
    };
    let gap = " ".repeat(GAP);
    let mut header = Vec::new();
    for (i, (name, _)) in section.columns.iter().enumerate() {
        if i > 0 {
            header.push(Span::raw(gap.clone()));
        }
        header.push(Span::styled(
            pad(name, i),
            Style::default().fg(dim()).add_modifier(Modifier::BOLD),
        ));
    }
    out.push(clip_line(header, width));
    for row in &section.table {
        let mut spans = Vec::new();
        for (i, cell) in row.iter().enumerate().take(widths.len()) {
            if i > 0 {
                spans.push(Span::raw(gap.clone()));
            }
            let style = match (section.columns[i].0, cell.as_str()) {
                ("STATE", "in progress") => Style::default().fg(accent()),
                ("STATE", "not stated") => Style::default().fg(warm()),
                ("STATE", _) => Style::default().fg(dim()),
                (_, "unknown") => Style::default().fg(dim()),
                ("AGENT", _) => Style::default().add_modifier(Modifier::BOLD),
                _ => Style::default(),
            };
            spans.push(Span::styled(pad(cell, i), style));
        }
        out.push(clip_line(spans, width));
    }
}

fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut s: String = text.chars().take(width.saturating_sub(1)).collect();
    s.push('…');
    s
}

fn clip_line(spans: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let mut left = width;
    let mut out = Vec::new();
    for span in spans {
        if left == 0 {
            break;
        }
        let n = span.content.chars().count();
        if n <= left {
            left -= n;
            out.push(span);
        } else {
            out.push(Span::styled(clip(&span.content, left), span.style));
            left = 0;
        }
    }
    Line::from(out)
}

fn heading() -> Style {
    Style::default().fg(accent()).add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::report::Section;

    fn text(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.to_string()).collect())
            .collect()
    }

    #[test]
    fn every_section_title_and_row_is_drawn_and_caps_are_stated() {
        let report = Report {
            summary: vec!["Sessions: 2".into()],
            sections: vec![Section {
                title: "Active",
                columns: vec![("AGENT", false), ("EVENTS", true)],
                table: vec![
                    vec!["Claude Code".into(), "7".into()],
                    vec!["Codex CLI".into(), "1,234".into()],
                ],
                rows: vec!["row one".into()],
                not_shown: 4,
            }],
        };
        let text = text(&to_lines(&report));
        assert!(text.contains(&"Sessions: 2".to_string()));
        assert!(text.contains(&"ACTIVE".to_string()));
        assert!(text.contains(&"row one".to_string()));
        assert!(text.iter().any(|l| l.contains("4 more not shown")));
        // Columns line up: counts right-aligned under their heading.
        let header = text.iter().position(|l| l.starts_with("AGENT")).unwrap();
        let ends: Vec<usize> = text[header..header + 3]
            .iter()
            .map(|l| l.trim_end().chars().count())
            .collect();
        assert!(ends.iter().all(|e| *e == ends[0]), "{:?}", &text[header..]);
    }

    #[test]
    fn a_narrow_panel_cuts_rows_rather_than_wrapping_them() {
        let report = Report {
            summary: vec![],
            sections: vec![Section {
                title: "Recent",
                columns: vec![("AGENT", false), ("MODEL", false)],
                table: vec![vec!["Claude Code".into(), "a-very-long-model-name".into()]],
                rows: vec![],
                not_shown: 0,
            }],
        };
        for line in text(&to_lines_width(&report, 20)) {
            assert!(line.chars().count() <= 20, "{line}");
        }
    }
}
