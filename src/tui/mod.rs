//! Terminal User Interface (TUI) for IronMonitor
//!
//! This module provides an interactive terminal dashboard for real-time hardware monitoring.
//! It displays CPU, GPU, memory, disk, and system information using the ratatui library.

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, prelude::Backend, Terminal};
use std::io;
use std::time::{Duration, Instant};

pub mod app;
// Renders frames into an in-memory buffer, so the TUI is inspectable without a
// terminal — by tests, and by `ironmon tui --frame` for an agent that has no TTY.
pub mod headless;
mod ui;

use app::PeripheralCache;
pub use app::{AcceleratorInfo, AcceleratorType, App};

/// Run the TUI application
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal FIRST for immediate visual feedback
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app with fast initialization (slow components load in background)
    let mut app = App::new_fast()?;

    // Draw initial "Loading..." state immediately
    terminal.draw(|f| ui::draw(f, &app))?;

    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

/// Main application loop with unified tick-based timing
fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    // Unified tick system - base tick is 250ms (4 ticks/sec)
    // Data updates happen on tick multiples; rendering is on-demand (dirty flag).
    const TICK_MS: u64 = 250;
    let tick_duration = Duration::from_millis(TICK_MS);

    // Update frequencies in ticks
    const FAST_UPDATE_TICKS: u64 = 2; // Every 2 ticks (500ms) - CPU, GPU, Memory, Network, Disks
    const PROCESS_TICKS: u64 = 4; // Every 4 ticks (1s)
                                  // Disks no longer need their own tick: the collector re-enumerates them on its
                                  // own slow cadence and publishes them with every snapshot.

    let mut tick_count: u64 = 0;
    let mut last_tick = Instant::now();
    let mut needs_render = true; // Start dirty so initial frame draws

    // Peripheral refresh runs on a background thread to avoid blocking input
    let peripheral_data = std::sync::Arc::new(std::sync::Mutex::new(PeripheralCache::default()));
    {
        let pd = std::sync::Arc::clone(&peripheral_data);
        std::thread::Builder::new()
            .name("peripheral-refresh".into())
            .spawn(move || loop {
                let mut cache = PeripheralCache::default();
                cache.refresh();
                if let Ok(mut shared) = pd.lock() {
                    *shared = cache;
                }
                std::thread::sleep(Duration::from_secs(10));
            })
            .ok();
    }
    let mut last_peripheral_sync = Instant::now();

    // Minimum interval between event-driven renders to avoid overwhelming the terminal
    const MIN_RENDER_INTERVAL_MS: u64 = 16; // ~60 FPS cap
    let mut last_render = Instant::now() - Duration::from_millis(MIN_RENDER_INTERVAL_MS);

    loop {
        // Calculate time until next tick
        let elapsed = last_tick.elapsed();
        let timeout = tick_duration.saturating_sub(elapsed);

        // --- Event phase: drain ALL pending key events before rendering ---
        // Use a short poll for the first check (respects tick timeout),
        // then drain remaining events with zero-timeout polls.
        let mut events_processed = false;
        if crossterm::event::poll(timeout)? {
            loop {
                if let Ok(Event::Key(key)) = event::read() {
                    if key.kind == KeyEventKind::Press {
                        events_processed = true;
                        // Handle agent input mode separately
                        if app.agent_input_mode {
                            match key.code {
                                KeyCode::Char(c) => app.agent_input_char(c),
                                KeyCode::Backspace => app.agent_input_backspace(),
                                KeyCode::Enter => app.submit_agent_query(),
                                KeyCode::Esc => app.toggle_agent_input(),
                                _ => {}
                            }
                        } else {
                            use crate::tui::app::ViewMode;
                            match app.view_mode {
                                ViewMode::Main => {
                                    if app.handle_main_key(key.code, key.modifiers) {
                                        return Ok(());
                                    }
                                }
                                ViewMode::ProcessDetail => match key.code {
                                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                                        app.close_overlay()
                                    }
                                    KeyCode::Up => app.select_process_up(),
                                    KeyCode::Down => app.select_process_down(),
                                    _ => {}
                                },
                                ViewMode::ThemeSelection => match key.code {
                                    KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
                                    KeyCode::Up => app.theme_picker_prev(),
                                    KeyCode::Down => app.theme_picker_next(),
                                    KeyCode::Enter => app.apply_selected_theme(),
                                    _ => {}
                                },
                            }
                        }
                    }
                }
                // Check if more events are queued (non-blocking)
                if !crossterm::event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }

        if events_processed {
            needs_render = true;
        }

        // --- Render phase: only when dirty, rate-limited ---
        if needs_render && last_render.elapsed() >= Duration::from_millis(MIN_RENDER_INTERVAL_MS) {
            terminal.draw(|f| ui::draw(f, app))?;
            last_render = Instant::now();
            needs_render = false;
        }

        // --- Tick phase: data updates ---
        if last_tick.elapsed() >= tick_duration {
            tick_count = tick_count.wrapping_add(1);
            last_tick = Instant::now();

            // Check for background initialization completion. This is a handful of
            // `try_recv` calls, so it runs every tick — which it already did, via a
            // `% 1` that read like a tunable cadence but could never be one.
            app.check_background_init();

            // Fast updates (CPU, GPU, Memory, Network, Disks).
            //
            // These now read a published snapshot rather than calling hardware APIs,
            // so the tick is cheap. Crucially, a redraw is requested only when the
            // collector actually published a new generation — previously every tick
            // forced a repaint whether or not any value had changed, which burned
            // terminal bandwidth redrawing identical frames.
            if tick_count.is_multiple_of(FAST_UPDATE_TICKS) {
                let _ = app.update_fast();
                if app.snapshot_changed_since_render() {
                    needs_render = true;
                }
            }

            // Process updates
            if tick_count.is_multiple_of(PROCESS_TICKS) {
                let _ = app.update_processes_only();
                needs_render = true;
            }

            // Sync peripheral data from background thread (cheap lock check)
            if last_peripheral_sync.elapsed() >= Duration::from_secs(2) {
                if let Ok(data) = peripheral_data.try_lock() {
                    if app.peripheral_cache.audio_info != data.audio_info
                        || app.peripheral_cache.usb_info != data.usb_info
                    {
                        app.peripheral_cache = data.clone();
                        needs_render = true;
                    }
                }
                last_peripheral_sync = Instant::now();
            }

            // Mark render needed on ticks that didn't update data
            // (for sparkline animation, clock updates, etc.) - but less frequently
            if tick_count.is_multiple_of(4) {
                needs_render = true;
            }
        }
    }
}
