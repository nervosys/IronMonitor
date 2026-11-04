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

/// How often the poller may run an import. It rescans the recordings directory more
/// often than this, on its own tick.
pub(super) const IMPORT_INTERVAL: Duration = crate::agents::ironobs::MIN_INTERVAL;

impl IronMonitorApp {
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
                for line in &report.summary {
                    ui.label(RichText::new(line).color(primary));
                }
                for section in &report.sections {
                    ui.add_space(10.0);
                    ui.add(SectionHeader::new(section.title));
                    for row in &section.rows {
                        ui.label(RichText::new(row).color(primary));
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
