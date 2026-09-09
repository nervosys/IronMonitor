//! IronMonitor GUI - Cyber-themed hardware monitoring dashboard
//!
//! A modern, visually appealing GUI inspired by GNOME System Monitor and Glances,
//! with a cyberpunk aesthetic featuring neon colors, dark backgrounds, and
//! real-time hardware metrics visualization.

use eframe::egui;

pub mod app;
// Renders the GUI's widget tree without a window and reads back the text that was
// painted. Originally test-only, now also the backing for `ironmon gui --frame`: the
// TUI became inspectable headlessly and leaving the GUI window-only made it the one
// surface an agent still could not see.
pub mod headless;
mod profile_tab;
mod theme;
pub mod widgets;

pub use app::IronMonitorApp;

/// Run the IronMonitor GUI application
pub fn run() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("IronMonitor")
            .with_icon(load_icon()),
        // Painting already goes through the GPU — egui tessellates on the CPU and
        // submits triangles via glow/OpenGL. Stated explicitly rather than inherited
        // from a default that can change.
        //
        // `Preferred`, not `Required`: IronMonitor is expected to run on headless servers
        // and over remote sessions where no accelerated context exists, and refusing
        // to open a window there would be worse than falling back to software.
        hardware_acceleration: eframe::HardwareAcceleration::Preferred,
        // Cap presentation at the display's refresh rate. Frames are driven by the
        // collector's publish hook and by input, so this only bounds bursts.
        vsync: true,
        ..Default::default()
    };

    eframe::run_native(
        "IronMonitor",
        options,
        Box::new(|cc| Ok(Box::new(IronMonitorApp::new(cc)))),
    )
}

fn load_icon() -> egui::IconData {
    // Simple default icon - cyan colored "S" pattern
    let size = 32;
    let mut rgba = vec![0u8; size * size * 4];

    // Create a simple "S" shape with cyan color
    for y in 0..size {
        for x in 0..size {
            let idx = (y * size + x) * 4;
            let in_border = x < 2 || x >= size - 2 || y < 2 || y >= size - 2;
            let in_s = (y < size / 3 && (x > size / 4 || y < 4))
                || (y >= size / 3 && y < 2 * size / 3 && x > size / 4 && x < 3 * size / 4)
                || (y >= 2 * size / 3 && (x < 3 * size / 4 || y >= size - 4));

            if in_border || in_s {
                rgba[idx] = 0; // R
                rgba[idx + 1] = 255; // G (cyan)
                rgba[idx + 2] = 255; // B (cyan)
                rgba[idx + 3] = 255; // A
            }
        }
    }

    egui::IconData {
        rgba,
        width: size as u32,
        height: size as u32,
    }
}
