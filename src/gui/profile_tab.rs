//! Profile inspector tab — NVIDIA Profile Inspector / XTU / Ryzen Master /
//! nvme-cli style read-only enumeration of vendor driver settings.

use egui::RichText;

use super::app::IronMonitorApp;
use super::theme::CyberColors;

const AUDIT_TAIL_REFRESH_MS: u64 = 1000;
const AUDIT_TAIL_LINES: usize = 12;

impl IronMonitorApp {
    pub(super) fn draw_profiles_tab(&mut self, ui: &mut egui::Ui) {
        use crate::profile::{SettingRisk, Subsystem};

        ui.horizontal(|ui| {
            ui.heading(RichText::new("🛠 Hardware Profile Inspector").color(CyberColors::CYAN));
            ui.label(
                RichText::new(
                    "  read-only — NVIDIA Profile Inspector / Intel XTU / AMD Ryzen Master / nvme-cli",
                )
                .color(CyberColors::TEXT_SECONDARY)
                .italics(),
            );
        });

        // ── Toolbar (mutable borrows confined here) ───────────────────────
        let mut refresh_clicked = false;
        ui.horizontal(|ui| {
            refresh_clicked = ui
                .button(RichText::new("🔄 Refresh").color(CyberColors::CYAN))
                .clicked();
            ui.separator();
            ui.label("Filter:");
            ui.text_edit_singleline(&mut self.profile_filter);
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "cache: {} hit / {} miss",
                    crate::profile::cache::CACHE_STATS.hits(),
                    crate::profile::cache::CACHE_STATS.misses()
                ))
                .small()
                .color(CyberColors::TEXT_SECONDARY),
            );
            if self.profile_snapshot_loading {
                ui.separator();
                ui.spinner();
                ui.label(
                    RichText::new("Loading…")
                        .small()
                        .color(CyberColors::TEXT_SECONDARY),
                );
            }
        });

        ui.horizontal(|ui| {
            ui.label("Subsystems:");
            if ui
                .selectable_label(self.profile_subsystem_filter.is_none(), "All")
                .clicked()
            {
                self.profile_subsystem_filter = None;
            }
            for sub in Subsystem::ALL {
                let selected = self.profile_subsystem_filter == Some(*sub);
                if ui.selectable_label(selected, sub.as_str()).clicked() {
                    self.profile_subsystem_filter = if selected { None } else { Some(*sub) };
                }
            }
        });

        ui.separator();

        // ── Trigger background load on first visit / refresh ─────────────
        if refresh_clicked {
            self.start_profile_load(true);
        } else {
            self.start_profile_load(false);
        }

        // Synchronous fallback (runs at most once per refresh generation):
        // if no snapshot exists, no load is in flight, AND the background
        // load already returned nothing (e.g. it panicked or its providers
        // misbehaved off the main thread), do a one-shot blocking load on
        // the main thread so the tab always shows real data.
        if !refresh_clicked
            && self.profile_snapshot.is_none()
            && !self.profile_snapshot_loading
            && self.profile_snapshot_receiver.is_none()
            && !self.profile_sync_attempted
        {
            self.profile_sync_attempted = true;
            self.load_profile_snapshot_sync(false);
        }

        // ── Lazily refresh derived caches ────────────────────────────────
        if self.profile_deviations_cache.is_none() {
            if let Some(snapshot) = self.profile_snapshot.as_ref() {
                self.profile_deviations_cache =
                    Some(crate::profile::deviation::deviations_from_default(snapshot));
            }
        }
        let need_audit_refresh = self
            .profile_audit_last_read
            .map(|t| t.elapsed() >= std::time::Duration::from_millis(AUDIT_TAIL_REFRESH_MS))
            .unwrap_or(true);
        if need_audit_refresh {
            let audit_path = crate::profile::apply::audit_log_path();
            let audit_text = std::fs::read_to_string(&audit_path).unwrap_or_default();
            let lines: Vec<&str> = audit_text.lines().collect();
            let start = lines.len().saturating_sub(AUDIT_TAIL_LINES);
            self.profile_audit_tail_cache = lines[start..].iter().map(|s| s.to_string()).collect();
            self.profile_audit_last_read = Some(std::time::Instant::now());
        }

        // ── Borrow snapshot (and cached derivatives) immutably ───────────
        let Some(snapshot) = self.profile_snapshot.as_ref() else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading profile snapshot…");
            });
            return;
        };
        let deviations: &[crate::profile::deviation::Deviation] =
            self.profile_deviations_cache.as_deref().unwrap_or(&[]);
        let audit_tail: &[String] = self.profile_audit_tail_cache.as_slice();
        let filter_lc = self.profile_filter.to_ascii_lowercase();
        let subsystem_filter = self.profile_subsystem_filter;

        // ── Deviations panel ──────────────────────────────────────────────
        let heading = if deviations.is_empty() {
            RichText::new("Deviations from default · 0 — at stock".to_string())
                .color(egui::Color32::from_rgb(100, 220, 100))
        } else {
            RichText::new(format!("Deviations from default · {}", deviations.len()))
                .color(egui::Color32::from_rgb(240, 200, 80))
                .strong()
        };
        egui::CollapsingHeader::new(heading)
            .default_open(!deviations.is_empty())
            .id_salt("profile_deviations_panel")
            .show(ui, |ui| {
                if deviations.is_empty() {
                    ui.label(
                        RichText::new(
                            "All settings with a declared default are at their default value.",
                        )
                        .color(CyberColors::TEXT_SECONDARY),
                    );
                } else {
                    egui::Grid::new("profile_deviations_grid")
                        .striped(true)
                        .num_columns(4)
                        .show(ui, |ui| {
                            ui.label(RichText::new("risk").strong());
                            ui.label(RichText::new("setting").strong());
                            ui.label(RichText::new("default").strong());
                            ui.label(RichText::new("current").strong());
                            ui.end_row();
                            for d in deviations {
                                let risk_color = match d.risk {
                                    crate::profile::SettingRisk::Dangerous => {
                                        egui::Color32::from_rgb(255, 90, 90)
                                    }
                                    crate::profile::SettingRisk::Moderate => {
                                        egui::Color32::from_rgb(240, 200, 80)
                                    }
                                    crate::profile::SettingRisk::Safe => {
                                        egui::Color32::from_rgb(100, 220, 100)
                                    }
                                    crate::profile::SettingRisk::Informational => {
                                        CyberColors::TEXT_SECONDARY
                                    }
                                };
                                ui.label(
                                    RichText::new(format!("{:?}", d.risk))
                                        .color(risk_color)
                                        .small(),
                                );
                                ui.label(RichText::new(format!(
                                    "[{}] {} :: {}",
                                    d.subsystem.as_str(),
                                    d.device,
                                    d.display_name
                                )));
                                ui.label(
                                    RichText::new(d.default.to_string())
                                        .color(CyberColors::TEXT_SECONDARY),
                                );
                                ui.label(
                                    RichText::new(d.current.to_string())
                                        .strong()
                                        .color(risk_color),
                                );
                                ui.end_row();
                            }
                        });
                }
            });

        // ── Audit log tail panel ─────────────────────────────────────────
        let audit_path = crate::profile::apply::audit_log_path();
        let audit_heading = RichText::new(format!(
            "Apply audit log · last {} entr{}",
            audit_tail.len(),
            if audit_tail.len() == 1 { "y" } else { "ies" }
        ))
        .color(CyberColors::CYAN);
        egui::CollapsingHeader::new(audit_heading)
            .default_open(false)
            .id_salt("profile_audit_panel")
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("source: {}", audit_path.display()))
                        .color(CyberColors::TEXT_SECONDARY)
                        .italics()
                        .small(),
                );
                if audit_tail.is_empty() {
                    ui.label(
                        RichText::new("(no entries — no apply attempts have been made)")
                            .color(CyberColors::TEXT_SECONDARY),
                    );
                } else {
                    egui::Grid::new("profile_audit_grid")
                        .striped(true)
                        .num_columns(4)
                        .show(ui, |ui| {
                            ui.label(RichText::new("when").strong());
                            ui.label(RichText::new("status").strong());
                            ui.label(RichText::new("setting").strong());
                            ui.label(RichText::new("requested").strong());
                            ui.end_row();
                            for line in audit_tail {
                                if let Ok(o) = serde_json::from_str::<
                                    crate::profile::apply::ApplyOutcome,
                                >(line)
                                {
                                    let (status_str, status_color) = match o.status {
                                        crate::profile::apply::ApplyStatus::Applied => {
                                            ("applied", egui::Color32::from_rgb(100, 220, 100))
                                        }
                                        crate::profile::apply::ApplyStatus::Refused => {
                                            ("refused", egui::Color32::from_rgb(255, 90, 90))
                                        }
                                        crate::profile::apply::ApplyStatus::Failed => {
                                            ("failed", egui::Color32::from_rgb(255, 90, 90))
                                        }
                                        crate::profile::apply::ApplyStatus::NotWritable => {
                                            ("not writable", egui::Color32::from_rgb(240, 200, 80))
                                        }
                                        crate::profile::apply::ApplyStatus::NeedsConfirm => {
                                            ("needs confirm", egui::Color32::from_rgb(240, 200, 80))
                                        }
                                    };
                                    ui.label(
                                        RichText::new(o.timestamp.to_string())
                                            .color(CyberColors::TEXT_SECONDARY)
                                            .small(),
                                    );
                                    ui.label(RichText::new(status_str).color(status_color));
                                    ui.label(RichText::new(o.setting_id).monospace());
                                    ui.label(RichText::new(o.requested.to_string()).strong());
                                    ui.end_row();
                                }
                            }
                        });
                }
            });

        ui.separator();

        let mut total_groups = 0usize;
        let mut total_settings = 0usize;

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for (sub, groups) in &snapshot.providers {
                    if let Some(sel) = subsystem_filter {
                        if sel != *sub {
                            continue;
                        }
                    }
                    if groups.is_empty() {
                        continue;
                    }
                    let heading = RichText::new(sub.as_str().to_uppercase())
                        .color(CyberColors::CYAN)
                        .strong();
                    egui::CollapsingHeader::new(heading)
                        .default_open(true)
                        .id_salt(format!("profile_sub_{}", sub.as_str()))
                        .show(ui, |ui| {
                            for group in groups {
                                // Cheap header pass: count only when no filter is
                                // active; otherwise the contents-collapsed body
                                // does the work lazily when expanded.
                                if filter_lc.is_empty() {
                                    total_groups += 1;
                                    total_settings += group.settings.len();
                                } else {
                                    // With a filter, do a single fast scan that
                                    // doesn't allocate a Vec or format strings —
                                    // just count matches. The detailed filtered
                                    // list is built lazily inside the expanded
                                    // body.
                                    let matched: usize = group
                                        .settings
                                        .iter()
                                        .filter(|s| setting_matches(s, &filter_lc))
                                        .count();
                                    if matched == 0 {
                                        continue;
                                    }
                                    total_groups += 1;
                                    total_settings += matched;
                                }

                                let group_heading = RichText::new(format!(
                                    "{}  —  {}",
                                    group.device, group.display_name
                                ))
                                .strong();
                                egui::CollapsingHeader::new(group_heading)
                                    .default_open(false)
                                    .id_salt(format!(
                                        "profile_grp_{}_{}",
                                        sub.as_str(),
                                        group.device
                                    ))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(format!("source: {}", group.source))
                                                .color(CyberColors::TEXT_SECONDARY)
                                                .italics(),
                                        );
                                        let grid_id = format!(
                                            "profile_grid_{}_{}",
                                            sub.as_str(),
                                            group.device
                                        );
                                        egui::Grid::new(grid_id).striped(true).num_columns(3).show(
                                            ui,
                                            |ui| {
                                                for s in group.settings.iter().filter(|s| {
                                                    filter_lc.is_empty()
                                                        || setting_matches(s, &filter_lc)
                                                }) {
                                                    let risk_color = match s.risk {
                                                        SettingRisk::Informational => {
                                                            CyberColors::TEXT_SECONDARY
                                                        }
                                                        SettingRisk::Safe => {
                                                            egui::Color32::from_rgb(100, 220, 100)
                                                        }
                                                        SettingRisk::Moderate => {
                                                            egui::Color32::from_rgb(240, 200, 80)
                                                        }
                                                        SettingRisk::Dangerous => {
                                                            egui::Color32::from_rgb(255, 90, 90)
                                                        }
                                                    };
                                                    ui.label(
                                                        RichText::new(&s.display_name)
                                                            .color(risk_color),
                                                    );
                                                    let unit = s
                                                        .unit
                                                        .as_deref()
                                                        .map(|u| format!(" {}", u))
                                                        .unwrap_or_default();
                                                    ui.label(
                                                        RichText::new(format!(
                                                            "{}{}",
                                                            s.value, unit
                                                        ))
                                                        .strong(),
                                                    );
                                                    ui.label(
                                                        RichText::new(s.id.clone())
                                                            .color(CyberColors::TEXT_SECONDARY)
                                                            .italics()
                                                            .monospace(),
                                                    );
                                                    ui.end_row();
                                                }
                                            },
                                        );
                                        for n in &group.notes {
                                            ui.label(
                                                RichText::new(format!("• {}", n))
                                                    .color(CyberColors::TEXT_SECONDARY)
                                                    .italics(),
                                            );
                                        }
                                    });
                            }
                        });
                }
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "Showing {} group(s) · {} setting(s)",
                        total_groups, total_settings
                    ))
                    .color(CyberColors::TEXT_SECONDARY),
                );
            });
    }
}

/// Cheap, allocation-free substring match across the fields a user would
/// search by. `filter_lc` MUST already be lowercased by the caller.
fn setting_matches(s: &crate::profile::Setting, filter_lc: &str) -> bool {
    if filter_lc.is_empty() {
        return true;
    }
    fn contains_ci(hay: &str, needle_lc: &str) -> bool {
        // Cheap path for ASCII haystacks: scan byte-by-byte against the
        // already-lowercased needle without allocating a new String.
        if hay.is_ascii() {
            let hay = hay.as_bytes();
            let needle = needle_lc.as_bytes();
            if needle.is_empty() || hay.len() < needle.len() {
                return needle.is_empty();
            }
            'outer: for i in 0..=hay.len() - needle.len() {
                for j in 0..needle.len() {
                    let hc = hay[i + j];
                    let hc_lc = if hc.is_ascii_uppercase() { hc + 32 } else { hc };
                    if hc_lc != needle[j] {
                        continue 'outer;
                    }
                }
                return true;
            }
            false
        } else {
            // Rare unicode path: fall back to allocating lowercase.
            hay.to_ascii_lowercase().contains(needle_lc)
        }
    }
    if contains_ci(&s.id, filter_lc) || contains_ci(&s.display_name, filter_lc) {
        return true;
    }
    if let Some(desc) = s.description.as_deref() {
        if contains_ci(desc, filter_lc) {
            return true;
        }
    }
    // value: stringify only when nothing else hit. SettingValue's Display impl
    // is cheap (no nested allocations beyond a small format buffer).
    let value_str = s.value.to_string();
    contains_ci(&value_str, filter_lc)
}
