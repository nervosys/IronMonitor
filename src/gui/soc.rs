//! SoC parts the GPU list does not cover: NPUs, and on Apple silicon the CPU's
//! core types and the unified memory.
//!
//! The GUI listed accelerators from the GPU collection alone, so an Apple
//! silicon Mac showed its GPU and nothing of the Neural Engine, the split
//! between performance and efficiency cores, or the memory all of them share.
//! These are read once, off the UI thread: none of them change while the app
//! runs, and `system_profiler` takes long enough to stall a frame.

use super::theme::{self, CyberColors, DeviceTitleColors};
use crate::silicon::NpuInfo;
use eframe::egui::{self, RichText};
use std::sync::{Arc, OnceLock};

/// What [`spawn_read`] found.
#[derive(Debug, Default)]
pub(super) struct SiliconDetails {
    /// The NPUs the platform lists, or why none could be looked up.
    pub npus: Option<Result<Vec<NpuInfo>, String>>,
    /// The Apple silicon SoC, where this is one.
    #[cfg(all(target_os = "macos", feature = "apple"))]
    pub apple: Option<crate::silicon::apple::AppleSocInventory>,
}

/// Start reading [`SiliconDetails`] on a background thread. The lock is empty
/// until the read finishes.
pub(super) fn spawn_read() -> Arc<OnceLock<SiliconDetails>> {
    let slot = Arc::new(OnceLock::new());
    let writer = Arc::clone(&slot);
    let spawned = std::thread::Builder::new()
        .name("ironmon-soc".into())
        .spawn(move || {
            let _ = writer.set(read());
        });
    if spawned.is_err() {
        // No thread, no details: the tabs say they were not read.
        let _ = slot.set(SiliconDetails::default());
    }
    slot
}

fn read() -> SiliconDetails {
    SiliconDetails {
        npus: Some(crate::silicon::query_npus().map_err(|e| e.to_string())),
        #[cfg(all(target_os = "macos", feature = "apple"))]
        apple: Some(crate::silicon::apple::read_soc_inventory()),
    }
}

/// Bytes as the binary gigabytes Apple labels "GB".
#[cfg(feature = "apple")]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn gigabytes(bytes: u64) -> String {
    let gib = bytes as f64 / (1u64 << 30) as f64;
    if (gib - gib.round()).abs() < 0.05 {
        format!("{gib:.0} GB")
    } else {
        format!("{gib:.1} GB")
    }
}

/// Bytes as megabytes, for caches.
#[cfg(feature = "apple")]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn megabytes(bytes: u64) -> String {
    let mib = bytes as f64 / (1u64 << 20) as f64;
    if (mib - mib.round()).abs() < 0.05 {
        format!("{mib:.0} MB")
    } else {
        format!("{mib:.1} MB")
    }
}

/// One line per CPU performance level, e.g.
/// `("Performance cores", "8 · L2 16 MB per 4 cores")`.
#[cfg(feature = "apple")]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(super) fn perf_level_rows(
    levels: &[crate::silicon::apple::PerfLevel],
) -> Vec<(String, String)> {
    levels
        .iter()
        .map(|level| {
            let label = match &level.name {
                Some(name) => format!("{name} cores"),
                None => format!("Performance level {} cores", level.index),
            };
            let mut value = level
                .physical_cores
                .map_or_else(|| "-".to_string(), |n| n.to_string());
            if level.logical_cores.is_some() && level.logical_cores != level.physical_cores {
                if let Some(threads) = level.logical_cores {
                    value.push_str(&format!(" ({threads} threads)"));
                }
            }
            match (level.l2_cache_bytes, level.cores_per_l2) {
                (Some(l2), Some(per)) => {
                    value.push_str(&format!(" · L2 {} per {per} cores", megabytes(l2)))
                }
                (Some(l2), None) => value.push_str(&format!(" · L2 {}", megabytes(l2))),
                _ => {}
            }
            (label, value)
        })
        .collect()
}

/// The unified memory as one line, e.g. `"16 GB LPDDR5 · Hynix"`.
#[cfg(feature = "apple")]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(super) fn unified_memory_line(memory: &crate::silicon::apple::UnifiedMemory) -> String {
    let mut parts = vec![memory
        .total_bytes
        .map_or_else(|| "size not read".to_string(), gigabytes)];
    if let Some(kind) = &memory.memory_type {
        parts[0].push(' ');
        parts[0].push_str(kind);
    }
    if let Some(maker) = &memory.manufacturer {
        parts.push(maker.clone());
    }
    parts.join(" · ")
}

fn row(ui: &mut egui::Ui, label: &str, value: &str, value_color: egui::Color32) {
    let ctx = ui.ctx().clone();
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!("{label}:"))
                .color(theme::color(&ctx, CyberColors::TEXT_SECONDARY)),
        );
        ui.label(RichText::new(value).color(theme::color(&ctx, value_color)));
    });
}

fn note(ui: &mut egui::Ui, text: &str) {
    let ctx = ui.ctx().clone();
    ui.label(
        RichText::new(text)
            .small()
            .color(theme::color(&ctx, CyberColors::TEXT_MUTED)),
    );
}

/// The CPU tab's Apple silicon rows: the chip and its core types.
pub(super) fn draw_cpu_section(ui: &mut egui::Ui, details: Option<&SiliconDetails>) {
    #[cfg(all(target_os = "macos", feature = "apple"))]
    {
        let Some(details) = details else {
            note(ui, "Reading the SoC's core types...");
            return;
        };
        let Some(soc) = &details.apple else { return };
        ui.add_space(8.0);
        if let Some(chip) = &soc.chip {
            row(ui, "Chip", chip, DeviceTitleColors::CPU);
        }
        match &soc.perf_levels {
            Ok(levels) => {
                for (label, value) in perf_level_rows(levels) {
                    row(ui, &label, &value, DeviceTitleColors::CPU);
                }
            }
            Err(why) => note(ui, &format!("Core types not read: {why}")),
        }
    }
    #[cfg(not(all(target_os = "macos", feature = "apple")))]
    let _ = (ui, details);
}

/// The Memory tab's unified memory row.
pub(super) fn draw_memory_section(ui: &mut egui::Ui, details: Option<&SiliconDetails>) {
    #[cfg(all(target_os = "macos", feature = "apple"))]
    {
        let Some(soc) = details.and_then(|d| d.apple.as_ref()) else {
            return;
        };
        ui.add_space(8.0);
        row(
            ui,
            "Unified memory",
            &unified_memory_line(&soc.unified_memory),
            DeviceTitleColors::MEMORY,
        );
        note(
            ui,
            "In the SoC package and shared by the CPU, GPU and Neural Engine. \
             The GPU's share is under Accelerators.",
        );
    }
    #[cfg(not(all(target_os = "macos", feature = "apple")))]
    let _ = (ui, details);
}

/// How many NPU panels [`draw_npus`] will draw.
pub(super) fn npu_count(details: Option<&SiliconDetails>) -> usize {
    match details.and_then(|d| d.npus.as_ref()) {
        Some(Ok(npus)) => npus.len(),
        _ => 0,
    }
}

/// One panel per NPU, below the GPUs on the Accelerators tab.
pub(super) fn draw_npus(ui: &mut egui::Ui, details: Option<&SiliconDetails>, width: f32) {
    let ctx = ui.ctx().clone();
    let Some(Ok(npus)) = details.and_then(|d| d.npus.as_ref()) else {
        return;
    };
    for npu in npus {
        ui.add_space(2.0);
        egui::Frame::NONE
            .stroke(egui::Stroke::new(
                1.0_f32,
                theme::color(&ctx, CyberColors::BORDER),
            ))
            .corner_radius(4)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(width - 16.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("⚡")
                            .color(theme::color(&ctx, DeviceTitleColors::ACCEL))
                            .size(20.0),
                    );
                    ui.label(
                        RichText::new(&npu.name)
                            .color(theme::color(&ctx, CyberColors::TEXT_PRIMARY))
                            .strong()
                            .size(18.0),
                    );
                });
                row(ui, "Type", "NPU", DeviceTitleColors::ACCEL);
                row(
                    ui,
                    "Vendor",
                    npu.vendor.as_deref().unwrap_or("unknown"),
                    DeviceTitleColors::ACCEL,
                );
                if let Some(cores) = npu.cores {
                    row(ui, "Cores", &cores.to_string(), DeviceTitleColors::ACCEL);
                }
                if let Some(util) = npu.utilization {
                    row(
                        ui,
                        "Utilization",
                        &format!("{util}%"),
                        DeviceTitleColors::ACCEL,
                    );
                }
                if let Some(power) = npu.power_watts {
                    row(
                        ui,
                        "Power",
                        &format!("{power:.1} W"),
                        CyberColors::NEON_ORANGE,
                    );
                }
                if let Some(mhz) = npu.frequency_mhz {
                    row(ui, "Clock", &format!("{mhz} MHz"), CyberColors::NEON_BLUE);
                }
                if npu.utilization.is_none() && npu.power_watts.is_none() {
                    note(
                        ui,
                        if npu.vendor.as_deref() == Some("Apple") {
                            "No live figures: the Neural Engine's power comes from \
                             powermetrics, which needs root, and macOS reports no \
                             activity figure for it."
                        } else {
                            "No live figures: the driver reports none for this NPU."
                        },
                    );
                }
            });
    }
}

#[cfg(all(test, feature = "apple"))]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_as_apple_labels_them() {
        assert_eq!(gigabytes(16 << 30), "16 GB");
        assert_eq!(gigabytes(24 << 30), "24 GB");
        assert_eq!(gigabytes(3 << 29), "1.5 GB");
        assert_eq!(megabytes(12 << 20), "12 MB");
    }

    #[test]
    fn perf_levels_read_as_named_core_counts() {
        use crate::silicon::apple::PerfLevel;
        let levels = [
            PerfLevel {
                index: 0,
                name: Some("Performance".into()),
                physical_cores: Some(8),
                logical_cores: Some(8),
                l2_cache_bytes: Some(16 << 20),
                cores_per_l2: Some(4),
            },
            PerfLevel {
                index: 1,
                name: None,
                physical_cores: None,
                logical_cores: None,
                l2_cache_bytes: Some(4 << 20),
                cores_per_l2: None,
            },
        ];
        assert_eq!(
            perf_level_rows(&levels),
            vec![
                (
                    "Performance cores".into(),
                    "8 · L2 16 MB per 4 cores".into()
                ),
                ("Performance level 1 cores".into(), "- · L2 4 MB".into()),
            ]
        );
    }

    #[test]
    fn unified_memory_says_what_was_read() {
        use crate::silicon::apple::UnifiedMemory;
        let full = UnifiedMemory {
            total_bytes: Some(16 << 30),
            memory_type: Some("LPDDR5".into()),
            manufacturer: Some("Hynix".into()),
        };
        assert_eq!(unified_memory_line(&full), "16 GB LPDDR5 · Hynix");
        assert_eq!(
            unified_memory_line(&UnifiedMemory::default()),
            "size not read"
        );
    }
}
