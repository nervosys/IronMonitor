//! Compact system monitoring summary backed by the shared collector.

use super::theme::{self, CyberColors};
use super::widgets::{domain_section_title, SectionHeader};
use crate::pipeline::Snapshot;
use eframe::egui::{self, RichText};

fn rows(snapshot: &Snapshot) -> [(&'static str, String); 7] {
    let unavailable = |reason: &str| format!("Unavailable — {reason}");
    let cpu = snapshot.cpu.as_ref().filter(|cpu| !cpu.cores.is_empty());
    let memory = snapshot
        .memory
        .as_ref()
        .filter(|memory| memory.ram.total > 0 && memory.ram.used <= memory.ram.total);
    let stats = snapshot.system_stats.as_ref();
    let uptime = stats
        .filter(|stats| stats.uptime_seconds.is_some())
        .map(|stats| stats.uptime_string())
        .unwrap_or_else(|| unavailable("OS uptime was not reported"));
    let processor = cpu
        .and_then(|cpu| {
            let model = &cpu.cores.first()?.model;
            (!model.trim().is_empty())
                .then(|| format!("{model} · {} logical CPUs", cpu.cores.len()))
        })
        .unwrap_or_else(|| unavailable("CPU identity was not reported"));
    let utilization = cpu
        .map(|cpu| cpu.total.idle)
        .filter(|idle| idle.is_finite() && (0.0..=100.0).contains(idle))
        .map(|idle| format!("{:.1}%", 100.0 - idle))
        .unwrap_or_else(|| unavailable("CPU utilization was not reported"));
    // MemoryStats uses KiB; converting to GiB divides by 1024 twice.
    let ram = memory
        .map(|memory| {
            format!(
                "{:.2} / {:.2} GiB ({:.1}%)",
                memory.ram.used as f64 / 1_048_576.0,
                memory.ram.total as f64 / 1_048_576.0,
                memory.ram_usage_percent()
            )
        })
        .unwrap_or_else(|| unavailable("RAM usage was not reported"));
    let swap = snapshot
        .memory
        .as_ref()
        .and_then(|memory| Some((memory.swap.used?, memory.swap.total?)))
        .filter(|(used, total)| used <= total)
        .map(|(used, total)| {
            if total == 0 {
                "Disabled (OS reports no swap capacity)".to_string()
            } else {
                format!(
                    "{:.2} / {:.2} GiB",
                    used as f64 / 1_048_576.0,
                    total as f64 / 1_048_576.0
                )
            }
        })
        .unwrap_or_else(|| unavailable("swap usage was not reported"));
    let processes = stats
        .and_then(|stats| stats.total_processes)
        .map(|count| count.to_string())
        .unwrap_or_else(|| unavailable("OS process count was not reported"));
    let accelerator_names: Vec<&str> = snapshot
        .gpu_static
        .iter()
        .map(|info| info.name.as_str())
        .filter(|name| !name.trim().is_empty())
        .collect();
    let accelerators = if accelerator_names.is_empty() {
        unavailable("collector reported no accelerator identities")
    } else {
        accelerator_names.join("\n")
    };
    let cpu_label = if cfg!(any(target_os = "linux", target_os = "macos")) {
        "CPU utilization (since boot):"
    } else {
        "CPU utilization:"
    };
    [
        ("OS uptime:", uptime),
        ("CPU:", processor),
        ("Accelerators:", accelerators),
        (cpu_label, utilization),
        ("RAM usage:", ram),
        ("Swap usage:", swap),
        ("Processes:", processes),
    ]
}

pub(super) fn draw(ui: &mut egui::Ui, snapshot: &Snapshot) {
    ui.add(SectionHeader::new(&domain_section_title(
        "system",
        "Live Summary",
    )));
    let label_color = theme::color(ui.ctx(), CyberColors::TEXT_MUTED);
    let value_color = theme::color(ui.ctx(), CyberColors::CYAN);
    egui::Grid::new("system_live_summary")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .max_col_width(((ui.available_width() - 16.0) / 2.0).max(1.0))
        .show(ui, |ui| {
            for (label, value) in rows(snapshot) {
                ui.add(egui::Label::new(RichText::new(label).color(label_color)).wrap());
                ui.add(egui::Label::new(RichText::new(value).color(value_color)).wrap());
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cpu::{CpuCore, CpuStats, CpuTotal};
    use crate::core::memory::MemoryStats;
    use crate::gui::headless::painted_blob;
    use crate::system_stats::SystemStats;

    #[test]
    fn system_summary_paints_os_uptime_and_observed_memory_units() {
        let mut memory = MemoryStats::empty();
        memory.ram.total = 16 * 1_048_576;
        memory.ram.used = 4 * 1_048_576;
        memory.swap.total = Some(0);
        memory.swap.used = Some(0);
        let snapshot = Snapshot {
            memory: Some(memory),
            cpu: Some(CpuStats {
                cores: vec![CpuCore {
                    id: 0,
                    online: true,
                    governor: String::new(),
                    frequency: None,
                    user: Some(20.0),
                    nice: Some(0.0),
                    system: Some(5.0),
                    idle: Some(75.0),
                    model: "Example CPU".into(),
                }],
                total: CpuTotal {
                    user: 20.0,
                    nice: 0.0,
                    system: 5.0,
                    idle: 75.0,
                },
            }),
            system_stats: Some(SystemStats {
                load_average: None,
                uptime_seconds: Some(90_060),
                idle_seconds: None,
                boot_time: None,
                num_cpus: 1,
                total_processes: Some(499),
                running_processes: None,
                cpu_time: None,
                vm_stats: None,
                hostname: None,
                kernel_version: None,
            }),
            ..Snapshot::default()
        };
        let ctx = egui::Context::default();
        let text = painted_blob(&ctx, |ui| draw(ui, &snapshot));
        for expected in [
            "OS uptime:",
            "1 days, 01:01",
            "Example CPU",
            "25.0%",
            "4.00 / 16.00 GiB (25.0%)",
            "499",
            "Disabled (OS reports no swap capacity)",
        ] {
            assert!(text.contains(expected), "Missing {expected:?}: {text}");
        }
    }

    #[test]
    fn empty_readings_and_placeholder_structs_are_unavailable() {
        for snapshot in [
            Snapshot::default(),
            Snapshot {
                cpu: Some(CpuStats::empty()),
                memory: Some(MemoryStats::empty()),
                ..Snapshot::default()
            },
        ] {
            assert!(rows(&snapshot)
                .iter()
                .all(|(_, value)| value.starts_with("Unavailable — ")));
            let ctx = egui::Context::default();
            let text = painted_blob(&ctx, |ui| draw(ui, &snapshot));
            assert!(text.contains("OS process count was not reported"));
            assert!(!text.contains("0.0%"));
            assert!(!text.contains("Disabled"));
        }
    }

    #[test]
    fn unavailable_summary_wraps_inside_narrow_windows() {
        for width in [300.0, 400.0, 800.0] {
            let ctx = egui::Context::default();
            let labels = crate::gui::headless::painted_text_rects_sized(
                &ctx,
                egui::vec2(width, 1200.0),
                |ui| draw(ui, &Snapshot::default()),
            );
            assert!(!labels.is_empty());
            for (text, rect) in labels {
                assert!(
                    rect.min.x >= 0.0 && rect.max.x <= width,
                    "Summary text {text:?} leaves the {width}px window: {rect:?}"
                );
            }
        }
    }
}
