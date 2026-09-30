//! Profile inspector tab — NVIDIA Profile Inspector / XTU / Ryzen Master /
//! nvme-cli style read-only enumeration of vendor driver settings.

use egui::RichText;

use super::app::IronMonitorApp;
use super::theme::{self, CyberColors};

const AUDIT_TAIL_REFRESH_MS: u64 = 1000;
const AUDIT_TAIL_LINES: usize = 12;

impl IronMonitorApp {
    pub(super) fn draw_profiles_tab(&mut self, ui: &mut egui::Ui) {
        use crate::profile::Subsystem;

        ui.horizontal_wrapped(|ui| {
            ui.heading(RichText::new("🛠 Hardware Profile Inspector").color(theme::color(ui.ctx(), CyberColors::CYAN)));
            ui.label(
                RichText::new(
                    "  read-only — NVIDIA Profile Inspector / Intel XTU / AMD Ryzen Master / nvme-cli",
                )
                .color(theme::color(ui.ctx(), CyberColors::TEXT_SECONDARY))
                .italics(),
            );
        });

        // ── Toolbar (mutable borrows confined here) ───────────────────────
        let mut refresh_clicked = false;
        ui.horizontal_wrapped(|ui| {
            refresh_clicked = ui
                .button(
                    RichText::new("🔄 Refresh").color(theme::color(ui.ctx(), CyberColors::CYAN)),
                )
                .clicked();
            ui.separator();
            ui.label("Filter:");
            ui.add(egui::TextEdit::singleline(&mut self.profile_filter).desired_width(220.0));
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "cache: {} hit / {} miss",
                    crate::profile::cache::CACHE_STATS.hits(),
                    crate::profile::cache::CACHE_STATS.misses()
                ))
                .small()
                .color(theme::color(ui.ctx(), CyberColors::TEXT_SECONDARY)),
            );
            if self.profile_snapshot_loading {
                ui.separator();
                ui.spinner();
                ui.label(
                    RichText::new("Loading…")
                        .small()
                        .color(theme::color(ui.ctx(), CyberColors::TEXT_SECONDARY)),
                );
            }
        });

        ui.horizontal_wrapped(|ui| {
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
        draw_profile_contents(
            ui,
            snapshot,
            deviations,
            audit_tail,
            &filter_lc,
            subsystem_filter,
        );
    }
}

fn column_width(ui: &egui::Ui, columns: usize) -> f32 {
    ((ui.available_width() - ui.spacing().item_spacing.x * (columns - 1) as f32 - 8.0)
        / columns as f32)
        .max(20.0)
}

fn wrapped_label(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(egui::Label::new(text).wrap())
}

fn draw_profile_contents(
    ui: &mut egui::Ui,
    snapshot: &crate::profile::ProfileSnapshot,
    deviations: &[crate::profile::deviation::Deviation],
    audit_tail: &[String],
    filter_lc: &str,
    subsystem_filter: Option<crate::profile::Subsystem>,
) {
    use crate::profile::SettingRisk;
    egui::ScrollArea::vertical()
        .id_salt("profile_contents")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let heading = if deviations.is_empty() {
                RichText::new("Deviations from default · 0 — at stock".to_string())
                    .color(theme::color(ui.ctx(), CyberColors::NEON_GREEN))
            } else {
                RichText::new(format!("Deviations from default · {}", deviations.len()))
                    .color(theme::color(ui.ctx(), CyberColors::NEON_YELLOW))
                    .strong()
            };
            egui::CollapsingHeader::new(heading)
                .default_open(!deviations.is_empty())
                .id_salt("profile_deviations_panel")
                .show(ui, |ui| {
                    if deviations.is_empty() {
                        wrapped_label(
                            ui,
                            RichText::new(
                                "All settings with a declared default are at their default value.",
                            )
                            .color(theme::color(ui.ctx(), CyberColors::TEXT_SECONDARY)),
                        );
                    } else {
                        egui::Grid::new("profile_deviations_grid")
                            .striped(true)
                            .num_columns(4)
                            .min_col_width(0.0)
                            .max_col_width(column_width(ui, 4))
                            .show(ui, |ui| {
                                wrapped_label(ui, RichText::new("risk").strong());
                                wrapped_label(ui, RichText::new("setting").strong());
                                wrapped_label(ui, RichText::new("default").strong());
                                wrapped_label(ui, RichText::new("current").strong());
                                ui.end_row();
                                for d in deviations {
                                    let risk_color = match d.risk {
                                        crate::profile::SettingRisk::Dangerous => {
                                            theme::color(ui.ctx(), CyberColors::NEON_RED)
                                        }
                                        crate::profile::SettingRisk::Moderate => {
                                            theme::color(ui.ctx(), CyberColors::NEON_YELLOW)
                                        }
                                        crate::profile::SettingRisk::Safe => {
                                            theme::color(ui.ctx(), CyberColors::NEON_GREEN)
                                        }
                                        crate::profile::SettingRisk::Informational => {
                                            theme::color(ui.ctx(), CyberColors::TEXT_SECONDARY)
                                        }
                                    };
                                    wrapped_label(
                                        ui,
                                        RichText::new(format!("{:?}", d.risk))
                                            .color(risk_color)
                                            .small(),
                                    );
                                    wrapped_label(
                                        ui,
                                        RichText::new(format!(
                                            "[{}] {} :: {}",
                                            d.subsystem.as_str(),
                                            d.device,
                                            d.display_name
                                        )),
                                    );
                                    wrapped_label(
                                        ui,
                                        RichText::new(d.default.to_string()).color(theme::color(
                                            ui.ctx(),
                                            CyberColors::TEXT_SECONDARY,
                                        )),
                                    );
                                    wrapped_label(
                                        ui,
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
            .color(theme::color(ui.ctx(), CyberColors::CYAN));
            egui::CollapsingHeader::new(audit_heading)
                .default_open(false)
                .id_salt("profile_audit_panel")
                .show(ui, |ui| {
                    wrapped_label(
                        ui,
                        RichText::new(format!("source: {}", audit_path.display()))
                            .color(theme::color(ui.ctx(), CyberColors::TEXT_SECONDARY))
                            .italics()
                            .small(),
                    );
                    if audit_tail.is_empty() {
                        wrapped_label(
                            ui,
                            RichText::new("(no entries — no apply attempts have been made)")
                                .color(theme::color(ui.ctx(), CyberColors::TEXT_SECONDARY)),
                        );
                    } else {
                        egui::Grid::new("profile_audit_grid")
                            .striped(true)
                            .num_columns(4)
                            .min_col_width(0.0)
                            .max_col_width(column_width(ui, 4))
                            .show(ui, |ui| {
                                wrapped_label(ui, RichText::new("when").strong());
                                wrapped_label(ui, RichText::new("status").strong());
                                wrapped_label(ui, RichText::new("setting").strong());
                                wrapped_label(ui, RichText::new("requested").strong());
                                ui.end_row();
                                for line in audit_tail {
                                    if let Ok(o) = serde_json::from_str::<
                                        crate::profile::apply::ApplyOutcome,
                                    >(line)
                                    {
                                        let (status_str, status_color) = match o.status {
                                            crate::profile::apply::ApplyStatus::Applied => (
                                                "applied",
                                                theme::color(ui.ctx(), CyberColors::NEON_GREEN),
                                            ),
                                            crate::profile::apply::ApplyStatus::Refused => (
                                                "refused",
                                                theme::color(ui.ctx(), CyberColors::NEON_RED),
                                            ),
                                            crate::profile::apply::ApplyStatus::Failed => (
                                                "failed",
                                                theme::color(ui.ctx(), CyberColors::NEON_RED),
                                            ),
                                            crate::profile::apply::ApplyStatus::NotWritable => (
                                                "not writable",
                                                theme::color(ui.ctx(), CyberColors::NEON_YELLOW),
                                            ),
                                            crate::profile::apply::ApplyStatus::NeedsConfirm => (
                                                "needs confirm",
                                                theme::color(ui.ctx(), CyberColors::NEON_YELLOW),
                                            ),
                                        };
                                        wrapped_label(
                                            ui,
                                            RichText::new(o.timestamp.to_string())
                                                .color(theme::color(
                                                    ui.ctx(),
                                                    CyberColors::TEXT_SECONDARY,
                                                ))
                                                .small(),
                                        );
                                        wrapped_label(
                                            ui,
                                            RichText::new(status_str).color(status_color),
                                        );
                                        wrapped_label(ui, RichText::new(o.setting_id).monospace());
                                        wrapped_label(
                                            ui,
                                            RichText::new(o.requested.to_string()).strong(),
                                        );
                                        ui.end_row();
                                    }
                                }
                            });
                    }
                });

            ui.separator();

            let mut total_groups = 0usize;
            let mut total_settings = 0usize;

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
                    .color(theme::color(ui.ctx(), CyberColors::CYAN))
                    .strong();
                egui::CollapsingHeader::new(heading)
                    .default_open(true)
                    .id_salt(format!("profile_sub_{}", sub.as_str()))
                    .show(ui, |ui| {
                        for (group_index, group) in groups.iter().enumerate() {
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
                                    .filter(|s| setting_matches(s, filter_lc))
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
                            let full_heading = format!("{} / {}", group.device, group.display_name);
                            let group_heading = egui::WidgetText::from(group_heading).into_galley(
                                ui,
                                Some(egui::TextWrapMode::Truncate),
                                (ui.available_width() - 32.0).max(20.0),
                                egui::TextStyle::Button,
                            );
                            let group_response = egui::CollapsingHeader::new(group_heading)
                                .default_open(false)
                                .id_salt(("profile_group", sub, group_index))
                                .show(ui, |ui| {
                                    wrapped_label(
                                        ui,
                                        RichText::new(format!("source: {}", group.source))
                                            .color(theme::color(
                                                ui.ctx(),
                                                CyberColors::TEXT_SECONDARY,
                                            ))
                                            .italics(),
                                    );
                                    let grid_id = ("profile_grid", sub, group_index);
                                    egui::Grid::new(grid_id)
                                        .striped(true)
                                        .num_columns(3)
                                        .min_col_width(0.0)
                                        .max_col_width(column_width(ui, 3))
                                        .show(ui, |ui| {
                                            for s in group.settings.iter().filter(|s| {
                                                filter_lc.is_empty()
                                                    || setting_matches(s, filter_lc)
                                            }) {
                                                let risk_color = match s.risk {
                                                    SettingRisk::Informational => theme::color(
                                                        ui.ctx(),
                                                        CyberColors::TEXT_SECONDARY,
                                                    ),
                                                    SettingRisk::Safe => theme::color(
                                                        ui.ctx(),
                                                        CyberColors::NEON_GREEN,
                                                    ),
                                                    SettingRisk::Moderate => theme::color(
                                                        ui.ctx(),
                                                        CyberColors::NEON_YELLOW,
                                                    ),
                                                    SettingRisk::Dangerous => theme::color(
                                                        ui.ctx(),
                                                        CyberColors::NEON_RED,
                                                    ),
                                                };
                                                wrapped_label(
                                                    ui,
                                                    RichText::new(&s.display_name)
                                                        .color(risk_color),
                                                );
                                                let unit = s
                                                    .unit
                                                    .as_deref()
                                                    .map(|u| format!(" {}", u))
                                                    .unwrap_or_default();
                                                wrapped_label(
                                                    ui,
                                                    RichText::new(format!("{}{}", s.value, unit))
                                                        .strong(),
                                                );
                                                wrapped_label(
                                                    ui,
                                                    RichText::new(s.id.clone())
                                                        .color(theme::color(
                                                            ui.ctx(),
                                                            CyberColors::TEXT_SECONDARY,
                                                        ))
                                                        .italics()
                                                        .monospace(),
                                                );
                                                ui.end_row();
                                            }
                                        });
                                    for n in &group.notes {
                                        wrapped_label(
                                            ui,
                                            RichText::new(format!("• {}", n))
                                                .color(theme::color(
                                                    ui.ctx(),
                                                    CyberColors::TEXT_SECONDARY,
                                                ))
                                                .italics(),
                                        );
                                    }
                                });
                            group_response.header_response.on_hover_text(full_heading);
                        }
                    });
            }
            ui.separator();
            wrapped_label(
                ui,
                RichText::new(format!(
                    "Showing {} group(s) · {} setting(s)",
                    total_groups, total_settings
                ))
                .color(theme::color(ui.ctx(), CyberColors::TEXT_SECONDARY)),
            );
        });
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

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::gui::headless::{painted_text_rects_sized, themed_context};
    use crate::profile::{ProfileGroup, ProfileSnapshot, Setting, SettingValue, Subsystem};
    use std::collections::BTreeMap;

    #[test]
    fn same_device_profiles_expand_independently() {
        let mut first = ProfileGroup::new(Subsystem::Gpu, "Shared GPU", "First profile", "driver");
        first.push(Setting::info(
            "first_setting",
            "First setting",
            SettingValue::Bool(true),
        ));
        let mut second =
            ProfileGroup::new(Subsystem::Gpu, "Shared GPU", "Second profile", "driver");
        second.push(Setting::info(
            "second_setting",
            "Second setting",
            SettingValue::Bool(false),
        ));
        let snapshot = ProfileSnapshot {
            timestamp: 0,
            providers: BTreeMap::from([(Subsystem::Gpu, vec![first, second])]),
            errors: BTreeMap::new(),
        };
        let ctx = themed_context();
        ctx.style_mut(|style| style.animation_time = 0.0);
        let size = egui::vec2(800.0, 600.0);
        let initial = painted_text_rects_sized(&ctx, size, |ui| {
            draw_profile_contents(ui, &snapshot, &[], &[], "", None);
        });
        let position = initial
            .iter()
            .find(|(text, _)| text.contains("First profile"))
            .expect("first profile header")
            .1
            .center();
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events: vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        draw_profile_contents(ui, &snapshot, &[], &[], "", None);
                    });
                },
            );
        }
        let expanded = painted_text_rects_sized(&ctx, size, |ui| {
            draw_profile_contents(ui, &snapshot, &[], &[], "", None);
        });
        assert!(expanded.iter().any(|(text, _)| text == "first_setting"));
        assert!(!expanded.iter().any(|(text, _)| text == "second_setting"));
    }

    #[test]
    fn expanded_profile_tables_and_long_headers_fit_the_window() {
        let mut first = ProfileGroup::new(
            Subsystem::Gpu,
            "Long device name ".repeat(20),
            "First profile ".repeat(20),
            "Long source path ".repeat(25),
        );
        let mut setting = Setting::info(
            "long_identifier_".repeat(20),
            "Long setting name ".repeat(20),
            SettingValue::Text("Long current value ".repeat(20)),
        );
        setting.default = Some(SettingValue::Text("Long declared default ".repeat(20)));
        first.push(setting);
        let mut second = ProfileGroup::new(
            Subsystem::Gpu,
            first.device.clone(),
            "Second profile",
            "driver",
        );
        second.push(Setting::info(
            "second_setting",
            "Second setting",
            SettingValue::Unreadable("Driver did not supply the value".into()),
        ));
        let snapshot = ProfileSnapshot {
            timestamp: 0,
            providers: BTreeMap::from([(Subsystem::Gpu, vec![first, second])]),
            errors: BTreeMap::new(),
        };
        let deviations = crate::profile::deviation::deviations_from_default(&snapshot);
        for width in [800.0, 1100.0, 1400.0] {
            let ctx = themed_context();
            ctx.memory_mut(|memory| memory.set_everything_is_visible(true));
            let text = painted_text_rects_sized(&ctx, egui::vec2(width, 6000.0), |ui| {
                draw_profile_contents(ui, &snapshot, &deviations, &[], "", None)
            });
            assert!(text
                .iter()
                .any(|(text, _)| text.contains("Long current value")));
            assert!(text
                .iter()
                .any(|(text, _)| text.contains("Driver did not supply the value")));
            for (text, rect) in text {
                assert!(
                    rect.max.x <= width + 1.0,
                    "{width}px: {text:?} paints outside the window: {rect:?}"
                );
            }
        }
    }
}
