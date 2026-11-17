//! Agents tab: the agent sessions and multi-agent runs IronObserver recorded, the
//! import's status, the engine's requests, and what is not read.
//!
//! The text comes from `crate::agents::report`, the same report the TUI draws, so
//! the two surfaces cannot disagree. This file only lays it out. Nothing here reads
//! disk: the background poller in `crate::agents` owns every read, and the draw path
//! copies its cached state.

use std::time::Duration;

use egui::RichText;

use crate::agents::report::{self, Report};

use super::app::IronMonitorApp;
use super::theme::{self, CyberColors};
use super::widgets::SectionHeader;

/// How often the Agents tab repaints so relative times stay current. The recordings
/// themselves are rescanned by the poller at its own interval.
pub(super) const REPAINT_EVERY: Duration = Duration::from_secs(10);

impl IronMonitorApp {
    /// Wait, up to `timeout`, for the Agents tab's first scan to finish. Only the
    /// one-shot headless paths call this, so a frame shows the recordings.
    pub fn await_first_agents_scan(&self, timeout: Duration) {
        self.agents.wait_for_scan(timeout);
    }

    /// Build the report from cached state. Cheap, and it never touches disk.
    pub(super) fn agents_report(&self) -> Report {
        let state = self.agents.state();
        let engine = crate::agents::engine_requests(&self.inference.states());
        report::build(&state, engine, chrono::Utc::now())
    }

    /// The Agents tab: a summary, then each section with its rows.
    pub(super) fn draw_agents_tab(&mut self, ui: &mut egui::Ui) {
        ui.ctx().request_repaint_after(REPAINT_EVERY);
        let report = self.agents_report();
        let primary = theme::text_color(ui.ctx(), CyberColors::TEXT_PRIMARY);
        let secondary = theme::text_color(ui.ctx(), CyberColors::TEXT_SECONDARY);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, line) in report.summary.iter().enumerate() {
                    let text = RichText::new(line);
                    ui.label(if i == 0 {
                        text.color(primary).strong()
                    } else {
                        text.color(secondary)
                    });
                }
                for (n, section) in report.sections.iter().enumerate() {
                    ui.add_space(10.0);
                    ui.add(SectionHeader::new(section.title));
                    if !section.columns.is_empty() {
                        egui::Grid::new(("agents_table", n))
                            .striped(true)
                            .spacing([18.0, 4.0])
                            .show(ui, |ui| {
                                for (name, _) in &section.columns {
                                    ui.label(RichText::new(*name).color(secondary).strong());
                                }
                                ui.end_row();
                                for row in &section.table {
                                    for (i, cell) in row.iter().enumerate() {
                                        let right = section.columns.get(i).is_some_and(|(_, r)| *r);
                                        let colour = if cell == "unknown" {
                                            secondary
                                        } else {
                                            primary
                                        };
                                        let label = RichText::new(cell).color(colour);
                                        if right {
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| ui.label(label),
                                            );
                                        } else {
                                            ui.label(label);
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    }
                    for row in &section.rows {
                        let colour = if row.starts_with("Not read") || row.starts_with("unknown") {
                            secondary
                        } else {
                            primary
                        };
                        ui.label(RichText::new(row).color(colour));
                    }
                    if section.not_shown > 0 {
                        ui.label(
                            RichText::new(format!(
                                "... {} more not shown (the list is capped)",
                                section.not_shown
                            ))
                            .color(secondary),
                        );
                    }
                }
            });
    }
}
