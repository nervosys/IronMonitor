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

/// The primary monitor's size in logical pixels, if it can be read.
///
/// `None` rather than a guess: a fallback resolution would be a number nobody
/// measured, and the caller's response to not knowing is simply to use its
/// preferred size — which is what it did before this existed.
#[cfg(windows)]
fn screen_size() -> Option<egui::Vec2> {
    // SAFETY: `GetSystemMetrics` reads a process-wide constant and cannot fail;
    // it returns 0 for an unknown index, which the check below rejects.
    unsafe {
        const SM_CXSCREEN: i32 = 0;
        const SM_CYSCREEN: i32 = 1;
        unsafe extern "system" {
            fn GetSystemMetrics(index: i32) -> i32;
        }
        let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        (w > 0 && h > 0).then(|| egui::vec2(w as f32, h as f32))
    }
}

#[cfg(not(windows))]
fn screen_size() -> Option<egui::Vec2> {
    // winit reports this only once a window exists, which is after the size has
    // to be chosen. Left unimplemented rather than guessed.
    None
}

/// Run the IronMonitor GUI application
pub fn run() -> Result<(), eframe::Error> {
    // The preferred size, clamped so it cannot exceed the screen it opens on.
    //
    // `[1400, 900]` alone is wider and taller than a 1366x768 laptop panel, so
    // the window opened with its edges off-screen on exactly the machines least
    // able to spare the room — and `with_min_inner_size` does not help, because a
    // minimum is a floor rather than a ceiling.
    //
    // Width is a choice; height is fitted, later, elsewhere.
    //
    // The layout is elastic — every widget sizes to `available_width()` — so the
    // Overview paints out to 1382 px at a 1400 px canvas and 1082 px at 1100 px.
    // There is no width it wants; it takes what it is given, and 1400 is the
    // number this has always opened at.
    //
    // Its height *is* a real property of the tab, and the window is fitted to it —
    // but not here. See `IronMonitorApp::autofit_to_overview_once`, which does it
    // once the Overview has finished loading.
    //
    // **The fitted height cannot be a constant, because it is not a property of
    // the program.** The Overview grows a panel per accelerator card, so it paints
    // 917.5 px tall on this three-GPU desktop and less on a machine with none. A
    // number baked in here would be right on exactly one machine.
    //
    // It cannot be measured here either, which is the reason the runtime fit lives
    // where it does: constructing the app to lay the tab out initialises COM as
    // multi-threaded on the main thread, and winit then fails `OleInitialize` with
    // `RPC_E_CHANGED_MODE`, so no window opens at all.
    //
    // So these two are the size the window *opens* at, before the first real
    // snapshot lands. They are a starting point, not a fit.
    const PREFERRED_WIDTH: f32 = 1400.0;
    const PREFERRED_HEIGHT: f32 = 900.0;

    let preferred = egui::vec2(PREFERRED_WIDTH, PREFERRED_HEIGHT);
    let size = match screen_size() {
        // 92%, leaving room for a title bar and a taskbar rather than filling the
        // panel exactly and tucking the bottom edge under one.
        //
        // `[1400, 900]` unclamped is wider and taller than a 1366x768 laptop
        // panel, so the window opened with its edges off-screen on exactly the
        // machines least able to spare the room. `with_min_inner_size` does not
        // help: a minimum is a floor, not a ceiling.
        Some(screen) => preferred.min(screen * 0.92),
        None => preferred,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([size.x, size.y])
            .with_min_inner_size([800.0_f32.min(size.x), 600.0_f32.min(size.y)])
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
