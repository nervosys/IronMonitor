//! The Agents tab's panel.
//!
//! Draws the [`Report`] that `crate::agents::report` builds from cached state. It
//! reads nothing from disk and starts nothing: the background poller owns both.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::agents::report::Report;

/// Draw the report, starting `scroll` lines down.
pub fn draw(f: &mut Frame, area: Rect, report: &Report, scroll: u16) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Agents")
        .title_style(title_style());
    let lines = to_lines(report);
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(paragraph, area);
}

/// The report as lines: summary first, then each section with its rows.
pub fn to_lines(report: &Report) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = report
        .summary
        .iter()
        .map(|s| Line::from(s.clone()))
        .collect();
    for section in &report.sections {
        out.push(Line::from(""));
        out.push(Line::from(Span::styled(
            section.title.to_string(),
            title_style(),
        )));
        for row in &section.rows {
            out.push(Line::from(format!("  {row}")));
        }
        if section.not_shown > 0 {
            out.push(Line::from(format!(
                "  ... {} more not shown (the list is capped)",
                section.not_shown
            )));
        }
    }
    out
}

fn title_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::report::Section;

    #[test]
    fn every_section_title_and_row_is_drawn_and_caps_are_stated() {
        let report = Report {
            summary: vec!["Sessions: 2".into()],
            sections: vec![Section {
                title: "Active",
                rows: vec!["row one".into()],
                not_shown: 4,
            }],
        };
        let text: Vec<String> = to_lines(&report)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.to_string()).collect())
            .collect();
        assert!(text.contains(&"Sessions: 2".to_string()));
        assert!(text.contains(&"Active".to_string()));
        assert!(text.contains(&"  row one".to_string()));
        assert!(text.iter().any(|l| l.contains("4 more not shown")));
    }
}
