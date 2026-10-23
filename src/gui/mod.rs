//! IronMonitor GUI - Cyber-themed hardware monitoring dashboard
//!
//! A modern, visually appealing GUI inspired by GNOME System Monitor and Glances,
//! with a cyberpunk aesthetic featuring neon colors, dark backgrounds, and
//! real-time hardware metrics visualization.

use eframe::egui;

mod agent_tools;
pub mod app;
// Renders the GUI's widget tree without a window and reads back the text that was
// painted. Originally test-only, now also the backing for `ironmon gui --frame`: the
// TUI became inspectable headlessly and leaving the GUI window-only made it the one
// surface an agent still could not see.
pub mod headless;
mod profile_tab;
mod soc;
mod system_summary;
mod theme;
pub mod widgets;

pub use app::IronMonitorApp;

/// Where a window can go on the primary monitor, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ScreenRoom {
    /// The work area: the monitor less the taskbar and any docked app bars.
    work_area: egui::Rect,
    /// What the window frame adds around the client area — title bar and
    /// resize borders — so a client size can be turned into an outer size.
    frame: egui::Vec2,
}

impl ScreenRoom {
    /// The largest client area whose window fits inside the work area.
    fn max_inner_size(&self) -> egui::Vec2 {
        (self.work_area.size() - self.frame).max(egui::Vec2::ZERO)
    }
}

/// The largest client area a window can have and still fit on screen, once
/// `gui::run` has read it. The Overview autofit grows the window toward this
/// rather than toward a fraction of the monitor.
pub(crate) static MAX_INNER_SIZE: std::sync::OnceLock<egui::Vec2> = std::sync::OnceLock::new();

/// The primary display's physical pixels per inch, where the OS reports the
/// panel's size. The default UI scale is derived from it.
pub(crate) static DISPLAY_PPI: std::sync::OnceLock<f32> = std::sync::OnceLock::new();

/// A pixel density outside this range is a panel size the display misreported
/// (EDID fields of 0 or 1 cm are common on projectors and capture cards), not a
/// screen anyone reads.
#[cfg(any(windows, target_os = "macos"))]
const PLAUSIBLE_PPI: std::ops::RangeInclusive<f32> = 50.0..=600.0;

/// The primary display's physical pixels per inch, from its reported width.
#[cfg(windows)]
fn display_ppi() -> Option<f32> {
    const HORZSIZE: i32 = 4;
    const DESKTOPHORZRES: i32 = 118;
    unsafe extern "system" {
        fn GetDC(hwnd: isize) -> isize;
        fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn GetDeviceCaps(hdc: isize, index: i32) -> i32;
    }
    // SAFETY: the screen DC is acquired, read and released here; both
    // `GetDeviceCaps` indices are plain integer queries.
    let (width_mm, width_px) = unsafe {
        let dc = GetDC(0);
        if dc == 0 {
            return None;
        }
        let read = (
            GetDeviceCaps(dc, HORZSIZE),
            GetDeviceCaps(dc, DESKTOPHORZRES),
        );
        ReleaseDC(0, dc);
        read
    };
    if width_mm <= 0 || width_px <= 0 {
        return None;
    }
    Some(width_px as f32 / (width_mm as f32 / 25.4)).filter(|ppi| PLAUSIBLE_PPI.contains(ppi))
}

/// The main display's backing pixels per inch: its width in points times the
/// backing scale, over the panel's physical width.
#[cfg(target_os = "macos")]
fn display_ppi() -> Option<f32> {
    #[repr(C)]
    struct CgSize {
        width: f64,
        height: f64,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayScreenSize(display: u32) -> CgSize;
    }
    let mtm = objc2::MainThreadMarker::new()?;
    let screen = objc2_app_kit::NSScreen::mainScreen(mtm)?;
    let width_px = screen.frame().size.width * screen.backingScaleFactor();
    // SAFETY: both are plain value queries on the main display's id.
    let size = unsafe { CGDisplayScreenSize(CGMainDisplayID()) };
    if size.width <= 0.0 || width_px <= 0.0 {
        return None;
    }
    Some((width_px / (size.width / 25.4)) as f32).filter(|ppi| PLAUSIBLE_PPI.contains(ppi))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn display_ppi() -> Option<f32> {
    None
}

/// The primary monitor's work area and the window frame's thickness, if they
/// can be read.
///
/// The work area rather than the screen: a 1920x1080 panel at 150% is 1280x720
/// logical, the taskbar takes 48 of those, and the title bar another 31. Sizing
/// to 92% of the screen gave a 662 px client area in a 741 px window on a 720 px
/// screen, so the status bar opened under the taskbar on exactly the laptops the
/// clamp was written for.
///
/// Logical pixels because this runs before winit declares the process DPI aware,
/// so Windows reports both in the scaled coordinates egui lays out in.
///
/// `None` rather than a guess: a fallback resolution would be a number nobody
/// measured, and the caller's response to not knowing is simply to use its
/// preferred size — which is what it did before this existed.
#[cfg(windows)]
fn screen_room() -> Option<ScreenRoom> {
    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    const SPI_GETWORKAREA: u32 = 0x0030;
    const SM_CYCAPTION: i32 = 4;
    const SM_CXSIZEFRAME: i32 = 32;
    const SM_CYSIZEFRAME: i32 = 33;
    const SM_CXPADDEDBORDER: i32 = 92;
    unsafe extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
        fn SystemParametersInfoW(action: u32, param: u32, value: *mut Rect, ini: u32) -> i32;
    }

    let mut area = Rect::default();
    // SAFETY: `SPI_GETWORKAREA` writes one `RECT` through the pointer, which
    // points at a live, correctly laid out `Rect`. `GetSystemMetrics` reads a
    // process-wide constant and returns 0 for an unknown index.
    let (ok, caption, frame_x, frame_y, padding) = unsafe {
        (
            SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut area, 0),
            GetSystemMetrics(SM_CYCAPTION),
            GetSystemMetrics(SM_CXSIZEFRAME),
            GetSystemMetrics(SM_CYSIZEFRAME),
            GetSystemMetrics(SM_CXPADDEDBORDER),
        )
    };
    if ok == 0 || area.right <= area.left || area.bottom <= area.top {
        return None;
    }
    Some(ScreenRoom {
        work_area: egui::Rect::from_min_max(
            egui::pos2(area.left as f32, area.top as f32),
            egui::pos2(area.right as f32, area.bottom as f32),
        ),
        frame: egui::vec2(
            (2 * (frame_x + padding)) as f32,
            (caption + 2 * (frame_y + padding)) as f32,
        ),
    })
}

/// The main screen's visible frame and the title bar's height, from AppKit.
///
/// `visibleFrame` already leaves out the menu bar and the Dock, which is what
/// the Windows work area does for the taskbar. Without this the window opened at
/// 1400x900 points, taller than a 13-inch MacBook's whole screen.
///
/// AppKit measures from the bottom-left corner of the main screen with y up;
/// egui places windows from the top-left with y down, so the visible frame's top
/// edge is flipped against the screen's height.
#[cfg(target_os = "macos")]
fn screen_room() -> Option<ScreenRoom> {
    use objc2_app_kit::{NSScreen, NSWindow, NSWindowStyleMask};

    // AppKit may only be asked from the main thread, which is where `run` is.
    let mtm = objc2::MainThreadMarker::new()?;
    let screen = NSScreen::mainScreen(mtm)?;
    let full = screen.frame();
    let visible = screen.visibleFrame();
    if visible.size.width <= 0.0 || visible.size.height <= 0.0 {
        return None;
    }

    // The frame a titled, resizable window adds around its content: the
    // difference between a content rect and the window frame AppKit builds
    // around it.
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;
    let outer = NSWindow::frameRectForContentRect_styleMask(full, style, mtm);

    let top = full.size.height - (visible.origin.y + visible.size.height);
    Some(ScreenRoom {
        work_area: egui::Rect::from_min_size(
            egui::pos2(visible.origin.x as f32, top as f32),
            egui::vec2(visible.size.width as f32, visible.size.height as f32),
        ),
        frame: egui::vec2(
            (outer.size.width - full.size.width).max(0.0) as f32,
            (outer.size.height - full.size.height).max(0.0) as f32,
        ),
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
fn screen_room() -> Option<ScreenRoom> {
    // winit reports this only once a window exists, which is after the size has
    // to be chosen. Left unimplemented rather than guessed.
    None
}

/// The client size to open at, and where to put the window if it had to shrink.
///
/// Placement only when the clamp applied: a window that fits wherever the
/// system cascades it is left to the system, while one sized to the whole work
/// area is centred in it, because the cascade offset alone would push its
/// bottom edge back under the taskbar.
fn opening_geometry(
    preferred: egui::Vec2,
    room: Option<ScreenRoom>,
) -> (egui::Vec2, Option<egui::Pos2>) {
    let Some(room) = room else {
        return (preferred, None);
    };
    let size = preferred.min(room.max_inner_size());
    if size == preferred {
        return (size, None);
    }
    let outer = size + room.frame;
    let position =
        room.work_area.min + ((room.work_area.size() - outer) * 0.5).max(egui::Vec2::ZERO);
    (size, Some(position))
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
    // `[1400, 900]` unclamped is wider and taller than a 1366x768 laptop panel,
    // so the window opened with its edges off-screen on exactly the machines
    // least able to spare the room. `with_min_inner_size` does not help: a
    // minimum is a floor, not a ceiling.
    let room = screen_room();
    if let Some(room) = room {
        let _ = MAX_INNER_SIZE.set(room.max_inner_size());
    }
    if let Some(ppi) = display_ppi() {
        let _ = DISPLAY_PPI.set(ppi);
    }
    let (size, position) = opening_geometry(preferred, room);

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([size.x, size.y])
        .with_min_inner_size([800.0_f32.min(size.x), 600.0_f32.min(size.y)])
        .with_title("IronMonitor")
        .with_icon(load_icon());
    if let Some(position) = position {
        viewport = viewport.with_position(position);
    }

    let options = eframe::NativeOptions {
        viewport,
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

#[cfg(test)]
mod opening_geometry_tests {
    use super::*;

    /// A 1920x1080 panel at 150%: 1280x720 logical, a 48 px taskbar at the
    /// bottom, and the frame Windows 11 reports at that scale.
    fn laptop() -> ScreenRoom {
        ScreenRoom {
            work_area: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 672.0)),
            frame: egui::vec2(16.0, 39.0),
        }
    }

    #[test]
    fn the_window_fits_inside_the_work_area_on_a_13_inch_laptop() {
        let (size, position) = opening_geometry(egui::vec2(1400.0, 900.0), Some(laptop()));
        let position = position.expect("a clamped window is placed, not cascaded");
        let outer = egui::Rect::from_min_size(position, size + laptop().frame);
        assert!(
            laptop().work_area.contains_rect(outer),
            "window {outer:?} leaves the work area {:?}",
            laptop().work_area
        );
        // And it uses the room rather than leaving a margin nobody asked for.
        assert_eq!(size, egui::vec2(1264.0, 633.0));
    }

    #[test]
    fn a_window_that_already_fits_is_left_where_the_system_puts_it() {
        let desktop = ScreenRoom {
            work_area: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(2560.0, 1392.0)),
            frame: egui::vec2(16.0, 39.0),
        };
        let preferred = egui::vec2(1400.0, 900.0);
        assert_eq!(
            opening_geometry(preferred, Some(desktop)),
            (preferred, None)
        );
    }

    #[test]
    fn an_unreadable_screen_opens_at_the_preferred_size() {
        let preferred = egui::vec2(1400.0, 900.0);
        assert_eq!(opening_geometry(preferred, None), (preferred, None));
    }
}
