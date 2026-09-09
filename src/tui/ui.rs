//! UI rendering functions
//!
//! Glances-inspired single-screen layout with color-coded thresholds:
//! - Header: Title and system info with quicklook summary
//! - Hardware section: CPU, GPU, RAM, Disk, Network with trend indicators
//! - Process section: Sortable process list with color coding
//! - Footer: Help and controls
//!
//! Color thresholds (Glances-style):
//! - OK (Green): 0-50%
//! - CAREFUL (Cyan): 50-70%
//! - WARNING (Yellow): 70-90%
//! - CRITICAL (Red): 90-100%

#[allow(unused_imports)]
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Row, Sparkline, Table, Tabs},
    Frame,
};

use super::app::App;

// ═══════════════════════════════════════════════════════════════════════════════
// GLANCES-STYLE COLOR SYSTEM
// ═══════════════════════════════════════════════════════════════════════════════

/// Glances-style threshold colors
mod glances_colors {
    use ratatui::style::Color;

    /// OK status - safe level (0-50%)
    pub const OK: Color = Color::Green;
    /// CAREFUL status - watch level (50-70%)
    pub const CAREFUL: Color = Color::Cyan;
    /// WARNING status - attention needed (70-90%)
    pub const WARNING: Color = Color::Yellow;
    /// CRITICAL status - urgent (90-100%)
    pub const CRITICAL: Color = Color::Red;
    /// Title/header color (generic)
    pub const TITLE: Color = Color::Cyan;
    /// Separator/border color
    pub const SEPARATOR: Color = Color::DarkGray;
    /// Inactive/disabled color
    pub const INACTIVE: Color = Color::DarkGray;

    // ── Device-class title colors (bright cyberpunk neon) ────────────────
    /// CPU title color – neon cyan
    pub const CPU_TITLE: Color = Color::Rgb(0, 255, 255);
    /// Accelerator (GPU/NPU) title color – neon green
    pub const ACCEL_TITLE: Color = Color::Rgb(57, 255, 20);
    /// Memory title color – neon magenta
    pub const MEMORY_TITLE: Color = Color::Rgb(255, 0, 255);
    /// Disk title color – neon orange
    pub const DISK_TITLE: Color = Color::Rgb(255, 165, 0);
    /// Network title color – neon blue
    pub const NETWORK_TITLE: Color = Color::Rgb(0, 150, 255);
}

/// Get color based on percentage threshold (Glances-style, generic fallback)
fn threshold_color(percent: f32) -> Color {
    match percent {
        p if p >= 90.0 => glances_colors::CRITICAL,
        p if p >= 70.0 => glances_colors::WARNING,
        p if p >= 50.0 => glances_colors::CAREFUL,
        _ => glances_colors::OK,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// DEVICE-CLASS COLOR PALETTES
// Each device type has a unique color family so bars/sparklines are
// instantly distinguishable at a glance.
// ═══════════════════════════════════════════════════════════════════════════════

/// CPU color – Neon Cyan family
fn cpu_color(percent: f32) -> Color {
    match percent {
        p if p >= 90.0 => Color::Rgb(255, 60, 60), // neon red
        p if p >= 70.0 => Color::Rgb(255, 255, 0), // neon yellow
        p if p >= 50.0 => Color::Rgb(0, 200, 200), // bright cyan
        _ => Color::Rgb(0, 255, 255),              // neon cyan
    }
}

/// GPU / Accelerator color – Neon Green family
fn accel_color(percent: f32) -> Color {
    match percent {
        p if p >= 90.0 => Color::Rgb(255, 60, 60), // neon red
        p if p >= 70.0 => Color::Rgb(255, 255, 0), // neon yellow
        p if p >= 50.0 => Color::Rgb(80, 255, 80), // bright green
        _ => Color::Rgb(57, 255, 20),              // neon green
    }
}

/// Memory color – Neon Magenta family
fn memory_color(percent: f32) -> Color {
    match percent {
        p if p >= 90.0 => Color::Rgb(255, 60, 60),   // neon red
        p if p >= 70.0 => Color::Rgb(255, 255, 0),   // neon yellow
        p if p >= 50.0 => Color::Rgb(255, 100, 255), // bright magenta
        _ => Color::Rgb(255, 0, 255),                // neon magenta
    }
}

/// Disk color – Neon Orange family
fn disk_color(percent: f32) -> Color {
    match percent {
        p if p >= 90.0 => Color::Rgb(255, 60, 60),  // neon red
        p if p >= 70.0 => Color::Rgb(255, 255, 0),  // neon yellow
        p if p >= 50.0 => Color::Rgb(255, 200, 50), // bright amber
        _ => Color::Rgb(255, 165, 0),               // neon orange
    }
}

/// Network color – Neon Blue family
fn network_color(percent: f32) -> Color {
    match percent {
        p if p >= 90.0 => Color::Rgb(255, 60, 60),  // neon red
        p if p >= 70.0 => Color::Rgb(255, 255, 0),  // neon yellow
        p if p >= 50.0 => Color::Rgb(80, 180, 255), // bright blue
        _ => Color::Rgb(0, 150, 255),               // neon blue
    }
}

/// Safely clamp a percentage value to 0-100 range for gauge widgets
/// Handles NaN, infinity, and out-of-range values
fn safe_percent(value: f32) -> u16 {
    if value.is_nan() || value.is_infinite() || value < 0.0 {
        0
    } else if value > 100.0 {
        100
    } else {
        value as u16
    }
}

/// Render a measured clock, or an empty string when nothing was measured.
///
/// The `sep` prefix is included only alongside a real reading so the separator does
/// not dangle. Previously every one of these sites was `unwrap_or(0)`, which printed
/// "0 MHz" — indistinguishable from a genuinely stalled core — for a value the
/// platform had simply not reported.
fn opt_mhz(freq: Option<u64>, sep: &str) -> String {
    match freq {
        Some(mhz) => format!("{sep}{mhz} MHz"),
        None => String::new(),
    }
}

/// Render a measured temperature, or an empty string when nothing was measured.
///
/// Same reasoning as [`opt_mhz`]: "0°C" is a plausible reading, not an obvious
/// placeholder, so an unmeasured sensor must be omitted rather than zeroed.
fn opt_celsius(temp: Option<f32>, sep: &str) -> String {
    match temp {
        Some(c) => format!("{sep}{c:.0}°C"),
        None => String::new(),
    }
}

/// Get trend indicator arrow based on value change
/// Returns (arrow, color) tuple
fn trend_indicator(current: f32, previous: f32) -> (&'static str, Color) {
    let delta = current - previous;
    if delta.abs() < 0.5 {
        ("→", Color::DarkGray)
    } else if delta > 0.0 {
        ("↑", Color::Red)
    } else {
        ("↓", Color::Green)
    }
}

/// Format bytes to human-readable with auto unit (Glances-style)
fn auto_unit(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    match bytes {
        b if b >= TB => format!("{:.1}T", b as f64 / TB as f64),
        b if b >= GB => format!("{:.1}G", b as f64 / GB as f64),
        b if b >= MB => format!("{:.1}M", b as f64 / MB as f64),
        b if b >= KB => format!("{:.1}K", b as f64 / KB as f64),
        _ => format!("{}B", bytes),
    }
}

/// Main drawing function - tabbed layout with dedicated views per data category
pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Min(0),    // Tab content
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    draw_tab_bar(f, app, chunks[0]);

    match app.selected_tab {
        0 => draw_overview_tab(f, app, chunks[1]),
        1 => draw_processes_tab(f, app, chunks[1]),
        2 => draw_cpu_tab(f, app, chunks[1]),
        3 => draw_accelerators_tab(f, app, chunks[1]),
        4 => draw_memory_tab(f, app, chunks[1]),
        5 => draw_system_tab(f, app, chunks[1]),
        6 => draw_peripherals(f, app, chunks[1]),
        7 => draw_profiles_tab(f, app, chunks[1]),
        8 => draw_agent(f, app, chunks[1]),
        _ => {}
    }

    draw_tab_footer(f, app, chunks[2]);

    // Draw overlays based on view mode
    use super::app::ViewMode;
    match app.view_mode {
        ViewMode::Main => {}
        ViewMode::ProcessDetail => draw_process_detail_overlay(f, app),
        ViewMode::ThemeSelection => draw_theme_picker_overlay(f, app),
    }
}

/// Draw the tab bar with system quick-look summary in the title
fn draw_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = app.tabs.iter().map(|t| Line::from(*t)).collect();

    let cpu_pct = app.cpu_info.utilization;
    let mem_pct = if app.memory_info.total > 0 {
        (app.memory_info.used as f64 / app.memory_info.total as f64) * 100.0
    } else {
        0.0
    };

    let title_text = format!(
        "Silicon Monitor │ CPU:{:.0}% MEM:{:.0}% GPU:{} │ {}",
        cpu_pct,
        mem_pct,
        app.accelerators.len(),
        app.system_info.hostname,
    );

    let tabs = Tabs::new(titles)
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                title_text,
                Style::default()
                    .fg(glances_colors::TITLE)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .select(app.selected_tab)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                .fg(glances_colors::TITLE),
        )
        .divider("│");

    f.render_widget(tabs, area);
}

/// Overview tab: compact bars for all hardware categories with sparkline history
fn draw_overview_tab(f: &mut Frame, app: &App, area: Rect) {
    let accel_count = app.accelerators.len();
    let accel_height: u16 = if accel_count > 0 {
        (accel_count * 3) as u16
    } else {
        0
    };

    let mut constraints = vec![Constraint::Length(3)]; // CPU
    if accel_height > 0 {
        constraints.push(Constraint::Length(accel_height));
    }
    constraints.push(Constraint::Length(3)); // RAM
    constraints.push(Constraint::Length(3)); // Disk
    constraints.push(Constraint::Length(3)); // Network
    constraints.push(Constraint::Min(5)); // Sparkline history area

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    draw_cpu_bar(f, app, chunks[idx]);
    idx += 1;
    if accel_height > 0 {
        draw_accelerators(f, app, chunks[idx]);
        idx += 1;
    }
    draw_memory_bar(f, app, chunks[idx]);
    idx += 1;
    draw_disk_bar(f, app, chunks[idx]);
    idx += 1;
    draw_network_bar(f, app, chunks[idx]);
    idx += 1;

    // Sparkline history section
    draw_overview_sparklines(f, app, chunks[idx]);
}

/// Draw sparkline history charts in the overview tab
fn draw_overview_sparklines(f: &mut Frame, app: &App, area: Rect) {
    let spark_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // CPU history sparkline
    let cpu_data: Vec<u64> = app.cpu_history.iter().copied().collect();
    if !cpu_data.is_empty() {
        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("CPU History (60s)"),
            )
            .data(&cpu_data)
            .style(Style::default().fg(cpu_color(app.cpu_info.utilization)));
        f.render_widget(sparkline, spark_chunks[0]);
    } else {
        let empty = Block::default()
            .borders(Borders::ALL)
            .title("CPU History (60s)");
        f.render_widget(empty, spark_chunks[0]);
    }

    // Memory history sparkline
    let mem_data: Vec<u64> = app.memory_history.iter().copied().collect();
    let mem_pct = if app.memory_info.total > 0 {
        (app.memory_info.used as f64 / app.memory_info.total as f64 * 100.0) as f32
    } else {
        0.0
    };
    if !mem_data.is_empty() {
        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Memory History (60s)"),
            )
            .data(&mem_data)
            .style(Style::default().fg(memory_color(mem_pct)));
        f.render_widget(sparkline, spark_chunks[1]);
    } else {
        let empty = Block::default()
            .borders(Borders::ALL)
            .title("Memory History (60s)");
        f.render_widget(empty, spark_chunks[1]);
    }
}

/// Processes tab: full-screen process table
fn draw_processes_tab(f: &mut Frame, app: &App, area: Rect) {
    draw_nvtop_processes(f, app, area);
}

/// CPU tab: detailed CPU info with per-core breakdown and history
fn draw_cpu_tab(f: &mut Frame, app: &App, area: Rect) {
    let core_count = app.cpu_info.per_core_usage.len();
    let core_display_height = (core_count.min(32) + 3) as u16; // +3 for header line + border

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // CPU bar gauge
            Constraint::Length(core_display_height.min(area.height.saturating_sub(11))), // Per-core
            Constraint::Min(5),    // CPU history sparkline
        ])
        .split(area);

    draw_cpu_bar(f, app, chunks[0]);

    // Detailed CPU info with per-core bars
    let mut info_lines = vec![Line::from(vec![
        Span::styled("Name: ", Style::default().fg(glances_colors::CPU_TITLE)),
        Span::raw(&app.cpu_info.name),
        Span::raw(format!(
            " │ {} cores/{} threads{}{}",
            app.cpu_info.cores,
            app.cpu_info.threads,
            opt_mhz(app.cpu_info.frequency, " │ "),
            opt_celsius(app.cpu_info.temperature, " │ "),
        )),
    ])];

    let per_core_lines: Vec<Line> = app
        .cpu_info
        .per_core_usage
        .iter()
        .enumerate()
        .take(32)
        .map(|(i, &usage)| {
            // `None` is a core that reported no idle time. It gets a row with
            // no bar and no figure rather than an empty gauge, which would read
            // as an idle core.
            let Some(usage) = usage else {
                return Line::from(vec![
                    Span::styled(format!("C{i:<2} "), Style::default().fg(Color::DarkGray)),
                    Span::styled("unread".to_string(), Style::default().fg(Color::DarkGray)),
                ]);
            };
            let bar_width: usize = 30;
            let filled = (usage / 100.0 * bar_width as f32) as usize;
            let bar: String = "█".repeat(filled) + &"░".repeat(bar_width.saturating_sub(filled));
            Line::from(vec![
                Span::styled(
                    format!("Core {:>2}: ", i),
                    Style::default().fg(Color::White),
                ),
                Span::styled(bar, Style::default().fg(cpu_color(usage))),
                Span::styled(
                    format!(" {:>5.1}%", usage),
                    Style::default()
                        .fg(cpu_color(usage))
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        })
        .collect();
    info_lines.extend(per_core_lines);

    let info = Paragraph::new(info_lines)
        .block(Block::default().borders(Borders::ALL).title("CPU Details"))
        .style(Style::default().fg(Color::White));
    f.render_widget(info, chunks[1]);

    // CPU history sparkline
    let cpu_data: Vec<u64> = app.cpu_history.iter().copied().collect();
    if !cpu_data.is_empty() {
        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("CPU Utilization History (60s)"),
            )
            .data(&cpu_data)
            .style(Style::default().fg(cpu_color(app.cpu_info.utilization)));
        f.render_widget(sparkline, chunks[2]);
    }
}

/// Accelerators tab: detailed GPU/NPU info with history sparklines
fn draw_accelerators_tab(f: &mut Frame, app: &App, area: Rect) {
    if app.accelerators.is_empty() {
        let msg = if app.is_loading() {
            "Detecting accelerators..."
        } else {
            "No accelerators detected"
        };
        let para = Paragraph::new(msg)
            .block(Block::default().borders(Borders::ALL).title("Accelerators"))
            .alignment(Alignment::Center);
        f.render_widget(para, area);
        return;
    }

    let accel_count = app.accelerators.len();
    let info_height = (accel_count * 3) as u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(info_height.min(area.height.saturating_sub(8))),
            Constraint::Min(5),
        ])
        .split(area);

    // Draw individual accelerator bars
    draw_accelerators(f, app, chunks[0]);

    // Draw history sparklines for each accelerator (side by side)
    if !app.accelerator_histories.is_empty() {
        let spark_constraints: Vec<Constraint> = app
            .accelerators
            .iter()
            .map(|_| Constraint::Ratio(1, accel_count as u32))
            .collect();
        let spark_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(spark_constraints)
            .split(chunks[1]);

        for (idx, accel) in app.accelerators.iter().enumerate() {
            if let Some(history) = app.accelerator_histories.get(idx) {
                let data: Vec<u64> = history.iter().copied().collect();
                if !data.is_empty() {
                    let title = format!("{} {} History", accel.accel_type, idx);
                    let sparkline = Sparkline::default()
                        .block(Block::default().borders(Borders::ALL).title(title))
                        .data(&data)
                        .style(Style::default().fg(accel_color(accel.utilization.unwrap_or(0.0))));
                    f.render_widget(sparkline, spark_chunks[idx]);
                }
            }
        }
    }
}

/// Memory tab: detailed memory and swap info with history
fn draw_memory_tab(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Memory bar gauge
            Constraint::Length(10), // Detailed memory info
            Constraint::Min(5),     // Memory history sparkline
        ])
        .split(area);

    draw_memory_bar(f, app, chunks[0]);

    // Detailed memory info with visual bars
    let used_gb = app.memory_info.used as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_gb = app.memory_info.total as f64 / (1024.0 * 1024.0 * 1024.0);
    let avail_gb = app.memory_info.available as f64 / (1024.0 * 1024.0 * 1024.0);
    let swap_used_gb = app
        .memory_info
        .swap_used
        .map(|v| v as f64 / (1024.0 * 1024.0 * 1024.0));
    let swap_total_gb = app
        .memory_info
        .swap_total
        .map(|v| v as f64 / (1024.0 * 1024.0 * 1024.0));
    let mem_pct = if total_gb > 0.0 {
        (used_gb / total_gb) * 100.0
    } else {
        0.0
    };
    // Absent when either figure is, rather than 0% for an unread pagefile.
    let swap_pct = match (swap_used_gb, swap_total_gb) {
        (Some(used), Some(total)) if total > 0.0 => Some((used / total) * 100.0),
        _ => None,
    };

    let bar_width: usize = 40;
    let ram_filled = (mem_pct / 100.0 * bar_width as f64) as usize;
    let ram_bar: String =
        "█".repeat(ram_filled) + &"░".repeat(bar_width.saturating_sub(ram_filled));
    // An empty bar where nothing was read, which the "-" beside it explains.
    let swap_filled = (swap_pct.unwrap_or(0.0) / 100.0 * bar_width as f64) as usize;
    let swap_bar: String =
        "█".repeat(swap_filled) + &"░".repeat(bar_width.saturating_sub(swap_filled));

    let info_lines = vec![
        Line::from(vec![
            Span::styled(
                "RAM:  ",
                Style::default()
                    .fg(glances_colors::MEMORY_TITLE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(ram_bar, Style::default().fg(memory_color(mem_pct as f32))),
            Span::styled(
                format!(" {:.1}%", mem_pct),
                Style::default()
                    .fg(memory_color(mem_pct as f32))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(format!(
            "      Total: {:.2} GB │ Used: {:.2} GB │ Available: {:.2} GB",
            total_gb, used_gb, avail_gb
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "SWAP: ",
                Style::default()
                    .fg(glances_colors::MEMORY_TITLE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                swap_bar,
                Style::default().fg(memory_color(swap_pct.unwrap_or(0.0) as f32)),
            ),
            Span::styled(
                match swap_pct {
                    Some(p) => format!(" {p:.1}%"),
                    None => " -".to_string(),
                },
                Style::default()
                    .fg(memory_color(swap_pct.unwrap_or(0.0) as f32))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(match (swap_total_gb, swap_used_gb) {
            (Some(total), Some(used)) => format!(
                "      Total: {:.2} GB │ Used: {:.2} GB │ Free: {:.2} GB",
                total,
                used,
                total - used
            ),
            _ => "      not reported by this platform".to_string(),
        }),
    ];

    let info = Paragraph::new(info_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Memory Details"),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(info, chunks[1]);

    // Memory history sparkline
    let mem_data: Vec<u64> = app.memory_history.iter().copied().collect();
    if !mem_data.is_empty() {
        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Memory Usage History (60s)"),
            )
            .data(&mem_data)
            .style(Style::default().fg(memory_color(mem_pct as f32)));
        f.render_widget(sparkline, chunks[2]);
    }
}

/// System tab: system info, disk details, and network details
fn draw_system_tab(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // System info
            Constraint::Length(3), // Disk bar
            Constraint::Min(4),    // Disk details
            Constraint::Length(3), // Network bar
            Constraint::Min(4),    // Network details
        ])
        .split(area);

    // System Information
    let uptime_secs = app.system_info.uptime.as_secs();
    let days = uptime_secs / 86400;
    let hours = (uptime_secs % 86400) / 3600;
    let minutes = (uptime_secs % 3600) / 60;

    let mut sys_lines = vec![
        Line::from(vec![
            Span::styled("Hostname: ", Style::default().fg(glances_colors::TITLE)),
            Span::raw(&app.system_info.hostname),
        ]),
        Line::from(vec![
            Span::styled("OS: ", Style::default().fg(glances_colors::TITLE)),
            Span::raw(&app.system_info.os),
        ]),
        Line::from(vec![
            Span::styled("Kernel: ", Style::default().fg(glances_colors::TITLE)),
            Span::raw(&app.system_info.kernel),
        ]),
        Line::from(vec![
            Span::styled("Uptime: ", Style::default().fg(glances_colors::TITLE)),
            Span::raw(format!("{}d {}h {}m", days, hours, minutes)),
        ]),
    ];
    if let Some(ref manufacturer) = app.system_info.manufacturer {
        sys_lines.push(Line::from(vec![
            Span::styled("Manufacturer: ", Style::default().fg(glances_colors::TITLE)),
            Span::raw(manufacturer),
        ]));
    }
    if let Some(ref model) = app.system_info.model {
        sys_lines.push(Line::from(vec![
            Span::styled("Model: ", Style::default().fg(glances_colors::TITLE)),
            Span::raw(model),
        ]));
    }

    let sys_info = Paragraph::new(sys_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("System Information"),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(sys_info, chunks[0]);

    // Disk bar
    draw_disk_bar(f, app, chunks[1]);

    // Detailed disk list
    let disk_items: Vec<ListItem> = app
        .disk_info
        .iter()
        .map(|disk| {
            let used_gb = disk.used as f64 / (1024.0 * 1024.0 * 1024.0);
            let total_gb = disk.total as f64 / (1024.0 * 1024.0 * 1024.0);
            let percent = if total_gb > 0.0 {
                (used_gb / total_gb) * 100.0
            } else {
                0.0
            };
            // Shown only when a rate was established and is non-zero. The
            // absent case and the genuinely-idle case both render nothing here,
            // which is the same choice the column made before -- the difference
            // is that an unestablished rate no longer arrives as `0.0` and so
            // no longer claims the disk was idle.
            let io_str = match (disk.read_rate, disk.write_rate) {
                (Some(r), Some(w)) if r > 0.0 || w > 0.0 => {
                    format!(" │ R:{}/s W:{}/s", auto_unit(r as u64), auto_unit(w as u64))
                }
                _ => String::new(),
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{}: ", disk.name),
                    Style::default().fg(glances_colors::DISK_TITLE),
                ),
                Span::styled(
                    format!("{:.1} GB / {:.1} GB ({:.0}%)", used_gb, total_gb, percent),
                    Style::default().fg(disk_color(percent as f32)),
                ),
                Span::raw(format!(
                    " │ {} │ {}{}",
                    disk.filesystem, disk.mount_point, io_str
                )),
            ]))
        })
        .collect();

    let disks = List::new(disk_items)
        .block(Block::default().borders(Borders::ALL).title("Disks"))
        .style(Style::default().fg(Color::White));
    f.render_widget(disks, chunks[2]);

    // Network bar
    draw_network_bar(f, app, chunks[3]);

    // Detailed network interfaces
    let net_items: Vec<ListItem> = app
        .network_info
        .interfaces
        .iter()
        .map(|iface| {
            let speed = iface
                .speed_mbps
                .map(|s| format!(" {}Mbps", s))
                .unwrap_or_default();
            let status = if iface.is_up { "UP" } else { "DOWN" };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{}: ", iface.name),
                    Style::default().fg(glances_colors::NETWORK_TITLE),
                ),
                Span::styled(
                    status,
                    Style::default().fg(if iface.is_up {
                        glances_colors::OK
                    } else {
                        glances_colors::CRITICAL
                    }),
                ),
                Span::raw(format!(
                    "{} │ ↓{} ↑{} │ Total: ↓{} ↑{}",
                    speed,
                    format_bandwidth(iface.rx_rate),
                    format_bandwidth(iface.tx_rate),
                    auto_unit(iface.rx_bytes),
                    auto_unit(iface.tx_bytes),
                )),
            ]))
        })
        .collect();

    let net_list = List::new(net_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Network Interfaces"),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(net_list, chunks[4]);
}

/// Draw all accelerators (GPUs, NPUs, FPGAs, etc.) with detailed metrics
fn draw_accelerators(f: &mut Frame, app: &App, area: Rect) {
    if app.accelerators.is_empty() {
        let message = if app.is_loading() {
            "Detecting accelerators..."
        } else {
            "No accelerators detected"
        };
        let no_accel = Paragraph::new(message)
            .block(Block::default().borders(Borders::ALL).title("Accelerators"))
            .alignment(Alignment::Center);
        f.render_widget(no_accel, area);
        return;
    }

    // Split area for each accelerator
    let accel_count = app.accelerators.len();
    let constraints: Vec<Constraint> =
        std::iter::repeat_n(Constraint::Ratio(1, accel_count as u32), accel_count).collect();

    let accel_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (idx, accel) in app.accelerators.iter().enumerate() {
        draw_single_accelerator(f, accel, idx, accel_chunks[idx]);
    }
}

/// Draw a single accelerator with all its metrics (Glances-style compact format)
fn draw_single_accelerator(
    f: &mut Frame,
    accel: &super::app::AcceleratorInfo,
    idx: usize,
    area: Rect,
) {
    let type_str = format!("{}", accel.accel_type);

    // Build PCIe info for title
    let pcie_str = match (accel.pcie_gen, accel.pcie_width) {
        (Some(gen), Some(width)) => format!(" │ PCIe Gen{}x{}", gen, width),
        _ => String::new(),
    };

    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        format!(
            "{} {} │ {} ({}){}",
            type_str, idx, accel.name, accel.vendor, pcie_str
        ),
        Style::default()
            .fg(glances_colors::ACCEL_TITLE)
            .add_modifier(Modifier::BOLD),
    ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Memory percentage for threshold color, absent when either figure is.
    let mem_percent = match (accel.memory_used, accel.memory_total) {
        (Some(used), Some(total)) if total > 0 => Some((used as f64 / total as f64) * 100.0),
        _ => None,
    };

    // Build fan speed string
    let fan_str = match (accel.fan_speed_rpm, accel.fan_speed_percent) {
        (Some(rpm), _) => format!(" │ FAN:{}RPM", rpm),
        (None, Some(pct)) => format!(" │ FAN:{:.0}%", pct),
        (None, None) => String::new(),
    };

    // Build encoder/decoder string if active
    let enc_dec_str = {
        let enc = accel.encoder_util.filter(|&u| u > 0.5);
        let dec = accel.decoder_util.filter(|&u| u > 0.5);
        match (enc, dec) {
            (Some(e), Some(d)) => format!(" │ ENC:{:.0}% DEC:{:.0}%", e, d),
            (Some(e), None) => format!(" │ ENC:{:.0}%", e),
            (None, Some(d)) => format!(" │ DEC:{:.0}%", d),
            (None, None) => String::new(),
        }
    };

    // Compact: All key metrics with Glances-style formatting
    let accel_util_label = format!(
        "{}: {} @ {} MHz │ MEM: {}/{} ({:.0}%) @ {} MHz │ {:.0}°C │ {:.0}/{:.0}W{}{}",
        type_str,
        // A device with no utilization counter shows a dash, not 0%.
        accel
            .utilization
            .map_or_else(|| "-".to_string(), |u| format!("{u:.0}%")),
        accel.clock_core.unwrap_or(0),
        auto_unit_opt(accel.memory_used),
        auto_unit_opt(accel.memory_total),
        pct_opt(mem_percent),
        accel.clock_memory.unwrap_or(0),
        accel.temperature.unwrap_or(0.0),
        accel.power.unwrap_or(0.0),
        accel.power_limit.unwrap_or(0.0),
        fan_str,
        enc_dec_str
    );

    let accel_clr = accel_color(accel.utilization.unwrap_or(0.0));

    let accel_gauge = Gauge::default()
        .gauge_style(Style::default().fg(accel_clr).add_modifier(Modifier::BOLD))
        .percent(safe_percent(accel.utilization.unwrap_or(0.0)))
        .label(accel_util_label);
    f.render_widget(accel_gauge, inner);
}

/// Draw all GPU bars with detailed metrics (nvtop style) - DEPRECATED, use draw_accelerators
#[allow(dead_code)]
fn draw_nvtop_gpus(f: &mut Frame, app: &App, area: Rect) {
    if app.gpu_info.is_empty() {
        let no_gpu = Paragraph::new("No GPUs detected")
            .block(Block::default().borders(Borders::ALL).title("GPUs"))
            .alignment(Alignment::Center);
        f.render_widget(no_gpu, area);
        return;
    }

    // Split area for each GPU
    let gpu_count = app.gpu_info.len();
    let constraints: Vec<Constraint> =
        std::iter::repeat_n(Constraint::Ratio(1, gpu_count as u32), gpu_count).collect();

    let gpu_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (idx, gpu) in app.gpu_info.iter().enumerate() {
        draw_single_gpu(f, gpu, idx, gpu_chunks[idx]);
    }
}

/// Draw a single GPU with all its metrics (Glances-style compact format) - DEPRECATED, use draw_single_accelerator
#[allow(dead_code)]
fn draw_single_gpu(f: &mut Frame, gpu: &super::app::GpuInfo, idx: usize, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        format!("GPU {} │ {} ({})", idx, gpu.name, gpu.vendor),
        Style::default()
            .fg(glances_colors::ACCEL_TITLE)
            .add_modifier(Modifier::BOLD),
    ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Memory percentage for threshold color, absent when either figure is.
    let mem_percent = match (gpu.memory_used, gpu.memory_total) {
        (Some(used), Some(total)) if total > 0 => Some((used as f64 / total as f64) * 100.0),
        _ => None,
    };

    // Compact: All key metrics with Glances-style formatting
    let gpu_util_label = format!(
        "GPU: {} @ {} MHz │ MEM: {}/{} ({:.0}%) @ {} MHz │ {:.0}°C │ {:.0}/{:.0}W",
        pct_opt(gpu.utilization.map(f64::from)),
        gpu.clock_graphics.unwrap_or(0),
        auto_unit_opt(gpu.memory_used),
        auto_unit_opt(gpu.memory_total),
        pct_opt(mem_percent),
        gpu.clock_memory.unwrap_or(0),
        gpu.temperature.unwrap_or(0.0),
        gpu.power.unwrap_or(0.0),
        gpu.power_limit.unwrap_or(0.0)
    );

    // A gauge at 0% and a gauge with no reading look identical, so the
    // label carries the dash and the bar simply sits empty.
    let gpu_clr = accel_color(gpu.utilization.unwrap_or(0.0));

    let gpu_gauge = Gauge::default()
        .gauge_style(Style::default().fg(gpu_clr).add_modifier(Modifier::BOLD))
        .percent(safe_percent(gpu.utilization.unwrap_or(0.0)))
        .label(gpu_util_label);
    f.render_widget(gpu_gauge, inner);
}

/// Draw system monitoring graphs (DEPRECATED - bars now drawn individually in order)
#[allow(dead_code)]
fn draw_system_graphs(f: &mut Frame, app: &App, area: Rect) {
    let graph_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // CPU bar
            Constraint::Length(3), // RAM bar
            Constraint::Length(3), // Disk bar
            Constraint::Length(3), // Network bar
        ])
        .split(area);

    draw_cpu_bar(f, app, graph_chunks[0]);
    draw_memory_bar(f, app, graph_chunks[1]);
    draw_disk_bar(f, app, graph_chunks[2]);
    draw_network_bar(f, app, graph_chunks[3]);
}

/// Draw CPU utilization bar gauge with Glances-style formatting and per-core mini-bars
fn draw_cpu_bar(f: &mut Frame, app: &App, area: Rect) {
    // Get previous CPU value for trend indicator
    let prev_cpu = app
        .cpu_history
        .iter()
        .rev()
        .nth(1)
        .map(|&v| v as f32)
        .unwrap_or(app.cpu_info.utilization);
    let (trend_arrow, _trend_color) = trend_indicator(app.cpu_info.utilization, prev_cpu);

    // Build per-core mini display (compact, shows as colored blocks)
    let per_core_display: String = if !app.cpu_info.per_core_usage.is_empty() {
        // Show first 16 cores with mini-bars, then ... if more
        let cores_to_show = app.cpu_info.per_core_usage.len().min(16);
        let mini_bars: String = app.cpu_info.per_core_usage[..cores_to_show]
            .iter()
            .map(|&u| {
                // A core that reported nothing gets a distinct glyph rather
                // than the empty block an idle core would get.
                let Some(u) = u else {
                    return '?';
                };
                // Use block characters for mini utilization bars
                match u as u32 {
                    0..=12 => '▁',
                    13..=25 => '▂',
                    26..=37 => '▃',
                    38..=50 => '▄',
                    51..=62 => '▅',
                    63..=75 => '▆',
                    76..=87 => '▇',
                    _ => '█',
                }
            })
            .collect();

        if app.cpu_info.per_core_usage.len() > 16 {
            format!("[{}…]", mini_bars)
        } else {
            format!("[{}]", mini_bars)
        }
    } else {
        String::new()
    };

    let cpu_label = format!(
        "CPU {} {:.0}% │ {} cores{}{} {}",
        trend_arrow,
        app.cpu_info.utilization,
        app.cpu_info.cores,
        opt_mhz(app.cpu_info.frequency, " @ "),
        opt_celsius(app.cpu_info.temperature, " │ "),
        per_core_display
    );

    let cpu_clr = cpu_color(app.cpu_info.utilization);

    let cpu_gauge = Gauge::default()
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "CPU",
                Style::default()
                    .fg(glances_colors::CPU_TITLE)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .gauge_style(Style::default().fg(cpu_clr).add_modifier(Modifier::BOLD))
        .percent(safe_percent(app.cpu_info.utilization))
        .label(cpu_label);

    f.render_widget(cpu_gauge, area);
}

/// Draw memory utilization bar gauge with Glances-style formatting
fn draw_memory_bar(f: &mut Frame, app: &App, area: Rect) {
    let mem_percent = (app.memory_info.used as f64 / app.memory_info.total.max(1) as f64) * 100.0;

    // Get previous memory value for trend indicator
    let prev_mem = app
        .memory_history
        .iter()
        .rev()
        .nth(1)
        .map(|&v| v as f32)
        .unwrap_or(mem_percent as f32);
    let (trend_arrow, _) = trend_indicator(mem_percent as f32, prev_mem);

    let mem_label = format!(
        "MEM {} {:.0}% │ {}/{} │ SWAP: {}",
        trend_arrow,
        mem_percent,
        auto_unit(app.memory_info.used),
        auto_unit(app.memory_info.total),
        auto_unit_opt(app.memory_info.swap_used)
    );

    let mem_clr = memory_color(mem_percent as f32);

    let mem_gauge = Gauge::default()
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "Memory",
                Style::default()
                    .fg(glances_colors::MEMORY_TITLE)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .gauge_style(Style::default().fg(mem_clr).add_modifier(Modifier::BOLD))
        .percent(safe_percent(mem_percent as f32))
        .label(mem_label);

    f.render_widget(mem_gauge, area);
}

/// Draw disk usage bar gauge with Glances-style auto units
fn draw_disk_bar(f: &mut Frame, app: &App, area: Rect) {
    let total_space: u64 = app.disk_info.iter().map(|d| d.total).sum();
    let used_space: u64 = app.disk_info.iter().map(|d| d.used).sum();
    // Summed over the disks that have a rate. Where none has one the total is
    // zero and the gauge below reads empty, which is what "nothing to show"
    // looks like in a bar -- not a claim that the disks were idle.
    let total_read: f64 = app.disk_info.iter().filter_map(|d| d.read_rate).sum();
    let total_write: f64 = app.disk_info.iter().filter_map(|d| d.write_rate).sum();
    let disk_percent = if total_space > 0 {
        (used_space as f64 / total_space as f64) * 100.0
    } else {
        0.0
    };

    // Build disk list string with Glances-style formatting
    let disk_list: Vec<String> = app
        .disk_info
        .iter()
        .take(3)
        .map(|d| {
            let percent = if d.total > 0 {
                (d.used as f64 / d.total as f64) * 100.0
            } else {
                0.0
            };
            format!(
                "{}:{:.0}%",
                d.name.chars().take(20).collect::<String>(),
                percent
            )
        })
        .collect();

    // Format I/O rates
    let io_str = if total_read > 0.0 || total_write > 0.0 {
        format!(
            " │ R:{}/s W:{}/s",
            auto_unit(total_read as u64),
            auto_unit(total_write as u64)
        )
    } else {
        String::new()
    };

    let disk_label = if !disk_list.is_empty() {
        format!(
            "DISK {:.0}% │ {}/{}{}│ {}",
            disk_percent,
            auto_unit(used_space),
            auto_unit(total_space),
            io_str,
            disk_list.join(" ")
        )
    } else {
        format!(
            "DISK {:.0}% │ {}/{}{} │ No disks",
            disk_percent,
            auto_unit(used_space),
            auto_unit(total_space),
            io_str
        )
    };

    let disk_clr = disk_color(disk_percent as f32);

    let disk_gauge = Gauge::default()
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "Disk",
                Style::default()
                    .fg(glances_colors::DISK_TITLE)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .gauge_style(Style::default().fg(disk_clr).add_modifier(Modifier::BOLD))
        .percent(safe_percent(disk_percent as f32))
        .label(disk_label);

    f.render_widget(disk_gauge, area);
}

/// Draw network bar gauge with Glances-style formatting
/// Draw network bar gauge with Glances-style formatting and real bandwidth data
fn draw_network_bar(f: &mut Frame, app: &App, area: Rect) {
    let net_info = &app.network_info;

    // Format bandwidth rates
    let rx_rate = format_bandwidth(net_info.total_rx_rate);
    let tx_rate = format_bandwidth(net_info.total_tx_rate);

    // Build interface list (top 3 by activity)
    let mut active_ifaces: Vec<_> = net_info
        .interfaces
        .iter()
        .filter(|i| {
            // An interface with no established rate is not known to be idle,
            // so it is not filtered out for being idle.
            i.is_up && (i.rx_rate.unwrap_or(0.0) > 0.0 || i.tx_rate.unwrap_or(0.0) > 0.0)
        })
        .collect();
    active_ifaces.sort_by(|a, b| {
        let a_total = a.rx_rate.unwrap_or(0.0) + a.tx_rate.unwrap_or(0.0);
        let b_total = b.rx_rate.unwrap_or(0.0) + b.tx_rate.unwrap_or(0.0);
        b_total
            .partial_cmp(&a_total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let iface_list: String = if !active_ifaces.is_empty() {
        active_ifaces
            .iter()
            .take(2)
            .map(|i| {
                let speed = i
                    .speed_mbps
                    .map(|s| format!(" {}Mb", s))
                    .unwrap_or_default();
                format!(
                    "{}{}↓{} ↑{}",
                    i.name.chars().take(8).collect::<String>(),
                    speed,
                    format_bandwidth(i.rx_rate),
                    format_bandwidth(i.tx_rate)
                )
            })
            .collect::<Vec<_>>()
            .join(" │ ")
    } else if !net_info.interfaces.is_empty() {
        // Show first interface even if inactive
        net_info
            .interfaces
            .iter()
            .take(2)
            .map(|i| i.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "No interfaces".to_string()
    };

    let net_label = format!(
        "NET ↓{} ↑{} │ Total: ↓{} ↑{} │ {}",
        rx_rate,
        tx_rate,
        auto_unit(net_info.total_rx_bytes),
        auto_unit(net_info.total_tx_bytes),
        iface_list
    );

    // Calculate a visual gauge based on activity (normalize to something reasonable)
    // Use logarithmic scale for better visualization
    let max_rate =
        (net_info.total_rx_rate.unwrap_or(0.0) + net_info.total_tx_rate.unwrap_or(0.0)).max(1.0);
    let gauge_percent = ((max_rate.log10() + 3.0) * 10.0).clamp(0.0, 100.0) as u16;

    // Color based on activity (network-class colors: cyan/teal family)
    let net_clr = network_color(gauge_percent as f32);

    let net_gauge = Gauge::default()
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                "Network",
                Style::default()
                    .fg(glances_colors::NETWORK_TITLE)
                    .add_modifier(Modifier::BOLD),
            )),
        )
        .gauge_style(Style::default().fg(net_clr).add_modifier(Modifier::BOLD))
        .percent(gauge_percent)
        .label(net_label);

    f.render_widget(net_gauge, area);
}

/// Format bandwidth to human-readable with auto unit (B/s, KB/s, MB/s, GB/s)
/// Render a rate, or an em dash where there is not one yet.
///
/// A rate needs two samples; the first refresh after start-up has one. This
/// used to take a bare `f64` and the absent case arrived as `0.0`, so the
/// interface list opened claiming every link was idle.
fn format_bandwidth(bytes_per_sec: Option<f64>) -> String {
    let Some(bytes_per_sec) = bytes_per_sec else {
        return "—".to_string();
    };
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if bytes_per_sec >= GB {
        format!("{:.1}G/s", bytes_per_sec / GB)
    } else if bytes_per_sec >= MB {
        format!("{:.1}M/s", bytes_per_sec / MB)
    } else if bytes_per_sec >= KB {
        format!("{:.0}K/s", bytes_per_sec / KB)
    } else if bytes_per_sec > 0.0 {
        format!("{:.0}B/s", bytes_per_sec)
    } else {
        "0B/s".to_string()
    }
}

/// Draw CPU utilization graph with sparkline (DEPRECATED - use draw_cpu_bar)
#[allow(dead_code)]
fn draw_cpu_graph(f: &mut Frame, app: &App, area: Rect) {
    let inner_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25), // Info
            Constraint::Min(0),     // Graph
        ])
        .split(area);

    // CPU info
    let mut cpu_text = vec![
        Line::from(format!("CPU: {:.0}%", app.cpu_info.utilization)),
        Line::from(format!("{} cores", app.cpu_info.cores,)),
    ];
    if let Some(mhz) = app.cpu_info.frequency {
        cpu_text.push(Line::from(format!("@ {mhz} MHz")));
    }
    let cpu_info = Paragraph::new(cpu_text)
        .block(Block::default().borders(Borders::ALL).title("CPU"))
        .style(Style::default().fg(Color::White));
    f.render_widget(cpu_info, inner_chunks[0]);

    // CPU history sparkline
    let cpu_data: Vec<u64> = app.cpu_history.iter().copied().collect();
    if !cpu_data.is_empty() {
        let sparkline = Sparkline::default()
            .block(Block::default().borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM))
            .data(&cpu_data)
            .style(Style::default().fg(usage_color(app.cpu_info.utilization)));
        f.render_widget(sparkline, inner_chunks[1]);
    }
}

/// Draw memory utilization graph with sparkline (DEPRECATED - use draw_memory_bar)
#[allow(dead_code)]
fn draw_memory_graph(f: &mut Frame, app: &App, area: Rect) {
    let inner_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25), // Info
            Constraint::Min(0),     // Graph
        ])
        .split(area);

    // Memory info
    let mem_used_gb = app.memory_info.used as f64 / (1024.0 * 1024.0 * 1024.0);
    let mem_total_gb = app.memory_info.total as f64 / (1024.0 * 1024.0 * 1024.0);
    let mem_percent = (mem_used_gb / mem_total_gb) * 100.0;

    let mem_text = vec![
        Line::from(format!("RAM: {:.0}%", mem_percent)),
        Line::from(format!("{:.1} GB", mem_used_gb)),
        Line::from(format!("/ {:.1} GB", mem_total_gb)),
    ];
    let mem_info = Paragraph::new(mem_text)
        .block(Block::default().borders(Borders::ALL).title("Memory"))
        .style(Style::default().fg(Color::White));
    f.render_widget(mem_info, inner_chunks[0]);

    // Memory history sparkline
    let mem_data: Vec<u64> = app.memory_history.iter().copied().collect();
    if !mem_data.is_empty() {
        let sparkline = Sparkline::default()
            .block(Block::default().borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM))
            .data(&mem_data)
            .style(Style::default().fg(usage_color(mem_percent as f32)));
        f.render_widget(sparkline, inner_chunks[1]);
    }
}

/// Draw disk I/O information (DEPRECATED - use draw_disk_bar)
#[allow(dead_code)]
fn draw_disk_graph(f: &mut Frame, app: &App, area: Rect) {
    let inner_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25), // Info
            Constraint::Min(0),     // Stats
        ])
        .split(area);

    // Disk summary
    let _total_disks = app.disk_info.len();
    let total_space: u64 = app.disk_info.iter().map(|d| d.total).sum();
    let used_space: u64 = app.disk_info.iter().map(|d| d.used).sum();
    let total_gb = total_space as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = used_space as f64 / (1024.0 * 1024.0 * 1024.0);
    let disk_percent = if total_space > 0 {
        (used_gb / total_gb) * 100.0
    } else {
        0.0
    };

    let disk_text = vec![
        Line::from(format!("Disk: {:.0}%", disk_percent)),
        Line::from(format!("{:.1} GB", used_gb)),
        Line::from(format!("/ {:.1} GB", total_gb)),
    ];
    let disk_info = Paragraph::new(disk_text)
        .block(Block::default().borders(Borders::ALL).title("Disk"))
        .style(Style::default().fg(Color::White));
    f.render_widget(disk_info, inner_chunks[0]);

    // Disk list spanning full width
    if !app.disk_info.is_empty() {
        let disk_items: Vec<Span> = app
            .disk_info
            .iter()
            .map(|disk| {
                let used = disk.used as f64 / (1024.0 * 1024.0 * 1024.0);
                let total = disk.total as f64 / (1024.0 * 1024.0 * 1024.0);
                let percent = if total > 0.0 {
                    (used / total) * 100.0
                } else {
                    0.0
                };
                Span::styled(
                    format!(" {}: {:.0}% ", disk.name, percent),
                    Style::default().fg(usage_color(percent as f32)),
                )
            })
            .collect();

        let disk_list = Paragraph::new(Line::from(disk_items))
            .block(Block::default().borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM))
            .style(Style::default().fg(Color::White));
        f.render_widget(disk_list, inner_chunks[1]);
    } else {
        let no_disk = Paragraph::new("No disks detected")
            .block(Block::default().borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM))
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(no_disk, inner_chunks[1]);
    }
}

/// Draw GPU processes table (nvtop style)
fn draw_nvtop_processes(f: &mut Frame, app: &App, area: Rect) {
    let mode_name = app.process_mode_name();
    let has_gpus = !app.accelerators.is_empty();
    let scroll_pos = app.get_scroll_position();
    let total_count = app.filtered_process_count;
    let visible_count = 25;

    // Only fetch the visible slice instead of all processes
    let processes = app.get_visible_processes(scroll_pos, visible_count);

    // Calculate which display row is selected (for highlight bar)
    let selected_display_idx = if app.selected_process_idx >= scroll_pos
        && app.selected_process_idx < scroll_pos + visible_count
    {
        Some(app.selected_process_idx - scroll_pos)
    } else {
        None
    };

    // Selection highlight style (Catppuccin Mocha surface0)
    let highlight_style = Style::default().bg(Color::Rgb(69, 71, 90));

    // Get total GPU memory for computing percentages
    // Summed over the devices that reported a capacity. A device that
    // reported none contributes nothing rather than a zero.
    let total_gpu_memory: u64 = app.accelerators.iter().filter_map(|a| a.memory_total).sum();

    // Determine columns based on mode - Glances-style headers
    let (header, rows, widths) = match app.process_display_mode {
        super::app::ProcessDisplayMode::All | super::app::ProcessDisplayMode::Cpu => {
            // Include GPU columns if GPUs are present (nvtop-style comprehensive view)
            let header = if has_gpus {
                Row::new(vec![
                    Span::styled(
                        "PID",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "USER",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "S",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "COMMAND",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "CPU%",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "MEM",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "DEV",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "GPU%",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "GMEM",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "GM%",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "E/D",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "TY",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                ])
                .bottom_margin(1)
            } else {
                Row::new(vec![
                    Span::styled(
                        "PID",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "USER",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "S",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "COMMAND",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "CPU%",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "MEM",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "THR",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                    Span::styled(
                        "I/O",
                        Style::default()
                            .fg(glances_colors::TITLE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ),
                ])
                .bottom_margin(1)
            };

            let rows: Vec<Row> = processes
                .iter()
                .enumerate()
                .map(|(display_idx, p)| {
                    let is_selected = selected_display_idx == Some(display_idx);
                    // Use device-class colors for process metrics
                    let cpu_clr = cpu_color(p.cpu_percent);
                    let gpu_clr = accel_color(p.gpu_usage_percent.unwrap_or(0.0));
                    let enc_clr = accel_color(p.encoder_usage_percent.unwrap_or(0.0));

                    // Calculate GPU memory percentage from total GPU memory
                    let gpu_mem_percent = match p.total_gpu_memory_bytes {
                        Some(bytes) if total_gpu_memory > 0 && bytes > 0 => {
                            Some((bytes as f64 / total_gpu_memory as f64 * 100.0) as f32)
                        }
                        _ => p.gpu_memory_percentage,
                    };
                    let gpu_mem_pct_clr = accel_color(gpu_mem_percent.unwrap_or(0.0));

                    // Color GPU mem based on usage
                    let gpu_mem_color = match p.total_gpu_memory_bytes {
                        Some(b) if b > 1024 * 1024 * 1024 => glances_colors::WARNING, // > 1GB
                        Some(b) if b > 256 * 1024 * 1024 => glances_colors::CAREFUL,  // > 256MB
                        Some(b) if b > 0 => glances_colors::OK,
                        // Zero reported and nothing reported both read grey
                        // here; the column below prints them differently.
                        _ => Color::DarkGray,
                    };

                    // Process state color
                    let state_color = match p.state {
                        'R' => glances_colors::OK,       // Running
                        'S' => Color::White,             // Sleeping
                        'D' => glances_colors::WARNING,  // Disk sleep
                        'Z' => glances_colors::CRITICAL, // Zombie
                        'T' => glances_colors::CAREFUL,  // Stopped
                        _ => Color::DarkGray,
                    };

                    // GPU process type display
                    let gpu_type_str = match p.gpu_process_type {
                        crate::process_monitor::ProcessGpuType::Graphical => "G",
                        crate::process_monitor::ProcessGpuType::Compute => "C",
                        crate::process_monitor::ProcessGpuType::GraphicalCompute => "M",
                        crate::process_monitor::ProcessGpuType::Unknown => "-",
                    };

                    if has_gpus {
                        // Format GPU indices (e.g., "0", "0,1", or "-" if none)
                        let gpu_idx_str = if p.gpu_indices.is_empty() {
                            "-".to_string()
                        } else {
                            p.gpu_indices
                                .iter()
                                .map(|i| i.to_string())
                                .collect::<Vec<_>>()
                                .join(",")
                        };

                        // Format ENC/DEC combined (show both if available)
                        let enc_dec_str = match (p.encoder_usage_percent, p.decoder_usage_percent) {
                            (Some(e), Some(d)) if e > 0.1 || d > 0.1 => {
                                format!("{:.0}/{:.0}", e, d)
                            }
                            (Some(e), None) if e > 0.1 => format!("{:.0}/-", e),
                            (None, Some(d)) if d > 0.1 => format!("-/{:.0}", d),
                            _ => "-/-".to_string(),
                        };

                        Row::new(vec![
                            Span::styled(
                                format!("{:>7}", p.pid),
                                Style::default().fg(Color::White),
                            ),
                            Span::styled(
                                format!(
                                    "{:<8}",
                                    p.user
                                        .as_deref()
                                        .unwrap_or("?")
                                        .chars()
                                        .take(8)
                                        .collect::<String>()
                                ),
                                Style::default().fg(Color::White),
                            ),
                            Span::styled(format!("{}", p.state), Style::default().fg(state_color)),
                            Span::styled(
                                p.name.chars().take(18).collect::<String>(),
                                Style::default().fg(Color::White),
                            ),
                            Span::styled(
                                format!("{:>5.1}%", p.cpu_percent),
                                Style::default().fg(cpu_clr),
                            ),
                            Span::styled(
                                format!("{:>6}", auto_unit(p.memory_bytes)),
                                Style::default().fg(Color::White),
                            ),
                            Span::styled(
                                gpu_idx_str,
                                Style::default().fg(if p.gpu_indices.is_empty() {
                                    Color::DarkGray
                                } else {
                                    Color::Magenta
                                }),
                            ),
                            Span::styled(
                                p.gpu_usage_percent
                                    .map(|u| format!("{:>4.0}%", u))
                                    .unwrap_or_else(|| "   -".to_string()),
                                Style::default().fg(gpu_clr),
                            ),
                            Span::styled(
                                match p.total_gpu_memory_bytes {
                                    Some(b) if b > 0 => format!("{:>5}", auto_unit(b)),
                                    // This pane already printed a dash rather
                                    // than 0, which the CLI did not.
                                    _ => "    -".to_string(),
                                },
                                Style::default().fg(gpu_mem_color),
                            ),
                            Span::styled(
                                gpu_mem_percent
                                    .map(|u| format!("{:>4.1}%", u))
                                    .unwrap_or_else(|| "   -".to_string()),
                                Style::default().fg(gpu_mem_pct_clr),
                            ),
                            Span::styled(
                                enc_dec_str,
                                Style::default().fg(
                                    if p.encoder_usage_percent.is_some()
                                        || p.decoder_usage_percent.is_some()
                                    {
                                        enc_clr
                                    } else {
                                        Color::DarkGray
                                    },
                                ),
                            ),
                            Span::styled(
                                gpu_type_str.to_string(),
                                Style::default().fg(if p.is_gpu_process() {
                                    glances_colors::CAREFUL
                                } else {
                                    glances_colors::INACTIVE
                                }),
                            ),
                        ])
                        .style(if is_selected {
                            highlight_style
                        } else {
                            Style::default()
                        })
                    } else {
                        // Total I/O (read + write) for display
                        let total_io = p.io_read_bytes + p.io_write_bytes;
                        let io_color = if total_io > 1024 * 1024 * 1024 {
                            glances_colors::WARNING // > 1GB total I/O
                        } else if total_io > 100 * 1024 * 1024 {
                            glances_colors::CAREFUL // > 100MB
                        } else if total_io > 0 {
                            glances_colors::OK
                        } else {
                            Color::DarkGray
                        };

                        // Thread count coloring
                        let thread_color = if p.thread_count > 100 {
                            glances_colors::WARNING
                        } else if p.thread_count > 20 {
                            glances_colors::CAREFUL
                        } else if p.thread_count > 0 {
                            Color::White
                        } else {
                            Color::DarkGray
                        };

                        Row::new(vec![
                            Span::styled(
                                format!("{:>7}", p.pid),
                                Style::default().fg(Color::White),
                            ),
                            Span::styled(
                                format!(
                                    "{:<8}",
                                    p.user
                                        .as_deref()
                                        .unwrap_or("?")
                                        .chars()
                                        .take(8)
                                        .collect::<String>()
                                ),
                                Style::default().fg(Color::White),
                            ),
                            Span::styled(format!("{}", p.state), Style::default().fg(state_color)),
                            Span::styled(
                                p.name.chars().take(25).collect::<String>(),
                                Style::default().fg(Color::White),
                            ),
                            Span::styled(
                                format!("{:>5.1}%", p.cpu_percent),
                                Style::default().fg(cpu_clr),
                            ),
                            Span::styled(
                                format!("{:>6}", auto_unit(p.memory_bytes)),
                                Style::default().fg(Color::White),
                            ),
                            Span::styled(
                                if p.thread_count > 0 {
                                    format!("{:>4}", p.thread_count)
                                } else {
                                    "   -".to_string()
                                },
                                Style::default().fg(thread_color),
                            ),
                            Span::styled(
                                if total_io > 0 {
                                    format!("{:>6}", auto_unit(total_io))
                                } else {
                                    "     -".to_string()
                                },
                                Style::default().fg(io_color),
                            ),
                        ])
                        .style(if is_selected {
                            highlight_style
                        } else {
                            Style::default()
                        })
                    }
                })
                .collect();

            let widths = if has_gpus {
                vec![
                    Constraint::Length(8), // PID
                    Constraint::Length(9), // USER
                    Constraint::Length(2), // S (state)
                    Constraint::Min(10),   // COMMAND
                    Constraint::Length(7), // CPU%
                    Constraint::Length(7), // MEM
                    Constraint::Length(4), // DEV (GPU index)
                    Constraint::Length(6), // GPU%
                    Constraint::Length(6), // GMEM
                    Constraint::Length(6), // GM% (GPU mem percentage)
                    Constraint::Length(6), // E/D (enc/dec combined)
                    Constraint::Length(2), // TY (type)
                ]
            } else {
                vec![
                    Constraint::Length(8), // PID
                    Constraint::Length(9), // USER
                    Constraint::Length(2), // S (state)
                    Constraint::Min(15),   // COMMAND
                    Constraint::Length(7), // CPU%
                    Constraint::Length(7), // MEM
                    Constraint::Length(5), // THR (threads)
                    Constraint::Length(7), // I/O
                ]
            };

            (header, rows, widths)
        }
        super::app::ProcessDisplayMode::Gpu(gpu_idx) => {
            let header = Row::new(vec![
                Span::styled(
                    "PID",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "USER",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "COMMAND",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "GPU MEM",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "GPU%",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "TYPE",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
            ])
            .bottom_margin(1);

            let rows: Vec<Row> = processes
                .iter()
                .enumerate()
                .map(|(display_idx, p)| {
                    let is_selected = selected_display_idx == Some(display_idx);
                    let gpu_mem = p
                        .gpu_memory_per_device
                        .get(&gpu_idx)
                        .map(|&m| auto_unit(m))
                        .unwrap_or_else(|| "0B".to_string());

                    let gpu_usage = p
                        .gpu_usage_percent
                        .map(|u| format!("{:>5.1}%", u))
                        .unwrap_or_else(|| "  N/A".to_string());

                    // Use Glances threshold colors for GPU usage
                    let gpu_clr = accel_color(p.gpu_usage_percent.unwrap_or(0.0));

                    let proc_type = format!("{:?}", p.gpu_process_type);

                    Row::new(vec![
                        Span::styled(format!("{:>7}", p.pid), Style::default().fg(Color::White)),
                        Span::styled(
                            format!(
                                "{:<10}",
                                p.user
                                    .as_deref()
                                    .unwrap_or("?")
                                    .chars()
                                    .take(10)
                                    .collect::<String>()
                            ),
                            Style::default().fg(Color::White),
                        ),
                        Span::styled(p.name.clone(), Style::default().fg(Color::White)),
                        Span::styled(format!("{:>7}", gpu_mem), Style::default().fg(gpu_clr)),
                        Span::styled(gpu_usage, Style::default().fg(gpu_clr)),
                        Span::styled(proc_type, Style::default().fg(glances_colors::INACTIVE)),
                    ])
                    .style(if is_selected {
                        highlight_style
                    } else {
                        Style::default()
                    })
                })
                .collect();

            let widths = vec![
                Constraint::Length(8),  // PID
                Constraint::Length(11), // User
                Constraint::Min(15),    // Name (flexible)
                Constraint::Length(10), // GPU Mem
                Constraint::Length(8),  // GPU%
                Constraint::Length(10), // Type
            ];

            (header, rows, widths)
        }
        super::app::ProcessDisplayMode::Npu(_) => {
            let header = Row::new(vec![Span::styled(
                "No NPU processes available",
                Style::default().fg(glances_colors::INACTIVE),
            )]);
            let widths = vec![Constraint::Percentage(100)];
            (header, vec![], widths)
        }
        super::app::ProcessDisplayMode::Accelerator(accel_idx) => {
            let header = Row::new(vec![
                Span::styled(
                    "PID",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "USER",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "COMMAND",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "ACCEL MEM",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "ACCEL%",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
                Span::styled(
                    "TYPE",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
            ])
            .bottom_margin(1);

            let rows: Vec<Row> = processes
                .iter()
                .enumerate()
                .map(|(display_idx, p)| {
                    let is_selected = selected_display_idx == Some(display_idx);
                    let accel_mem = p
                        .gpu_memory_per_device
                        .get(&accel_idx)
                        .map(|&m| auto_unit(m))
                        .unwrap_or_else(|| "0B".to_string());

                    let accel_usage = p
                        .gpu_usage_percent
                        .map(|u| format!("{:>5.1}%", u))
                        .unwrap_or_else(|| "  N/A".to_string());

                    let accel_clr = accel_color(p.gpu_usage_percent.unwrap_or(0.0));

                    let proc_type = format!("{:?}", p.gpu_process_type);

                    Row::new(vec![
                        Span::styled(format!("{:>7}", p.pid), Style::default().fg(Color::White)),
                        Span::styled(
                            format!(
                                "{:<10}",
                                p.user
                                    .as_deref()
                                    .unwrap_or("?")
                                    .chars()
                                    .take(10)
                                    .collect::<String>()
                            ),
                            Style::default().fg(Color::White),
                        ),
                        Span::styled(p.name.clone(), Style::default().fg(Color::White)),
                        Span::styled(format!("{:>9}", accel_mem), Style::default().fg(accel_clr)),
                        Span::styled(accel_usage, Style::default().fg(accel_clr)),
                        Span::styled(proc_type, Style::default().fg(glances_colors::INACTIVE)),
                    ])
                    .style(if is_selected {
                        highlight_style
                    } else {
                        Style::default()
                    })
                })
                .collect();

            let widths = vec![
                Constraint::Length(8),  // PID
                Constraint::Length(11), // User
                Constraint::Min(15),    // Name (flexible)
                Constraint::Length(10), // Accel Mem
                Constraint::Length(8),  // Accel%
                Constraint::Length(10), // Type
            ];

            (header, rows, widths)
        }
    };

    // Build the title showing scroll position if scrolled
    let title = if scroll_pos > 0 {
        format!(
            "Processes - {} ({}-{} of {})",
            mode_name,
            scroll_pos + 1,
            (scroll_pos + visible_count).min(total_count),
            total_count
        )
    } else {
        format!(
            "Processes - {} ({} of {})",
            mode_name,
            visible_count.min(total_count),
            total_count
        )
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .column_spacing(1);

    f.render_widget(table, area);
}

/// Draw footer with tab-aware controls
fn draw_tab_footer(f: &mut Frame, app: &App, area: Rect) {
    // Show status message if active
    if let Some(status_msg) = app.get_status_message() {
        let status = Paragraph::new(Line::from(vec![Span::styled(
            status_msg,
            Style::default()
                .fg(glances_colors::OK)
                .add_modifier(Modifier::BOLD),
        )]))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
        f.render_widget(status, area);
        return;
    }

    // Show agent input mode if active
    if app.agent_input_mode {
        let input_text = format!("> {}", app.agent_input);
        let input = Paragraph::new(Line::from(vec![
            Span::styled(
                "Agent Query: ",
                Style::default()
                    .fg(glances_colors::TITLE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(&input_text),
            Span::styled(
                "█",
                Style::default()
                    .fg(glances_colors::TITLE)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Left);
        f.render_widget(input, area);
        return;
    }

    // Tab-specific help hints
    let mut help_spans = vec![
        Span::styled(
            "Tab",
            Style::default()
                .fg(glances_colors::TITLE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Navigate  "),
        Span::styled(
            "1-8",
            Style::default()
                .fg(glances_colors::TITLE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Jump  "),
        Span::styled(
            "q",
            Style::default()
                .fg(glances_colors::TITLE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Quit  "),
    ];

    // Add tab-specific hints
    match app.selected_tab {
        1 => {
            // Processes tab
            help_spans.extend([
                Span::styled(
                    "↑↓",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Select  "),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Detail  "),
                Span::styled(
                    "p",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Filter  "),
                Span::styled(
                    "PgUp/Dn",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Page  "),
            ]);
        }
        7 => {
            // Agent tab
            help_spans.extend([
                Span::styled(
                    "a",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Ask  "),
                Span::styled(
                    "c",
                    Style::default()
                        .fg(glances_colors::TITLE)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" Clear  "),
            ]);
        }
        _ => {}
    }

    help_spans.extend([
        Span::styled(
            "t",
            Style::default()
                .fg(glances_colors::TITLE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Theme  "),
        Span::styled("│", Style::default().fg(glances_colors::SEPARATOR)),
        Span::raw(" "),
        Span::styled("OK", Style::default().fg(glances_colors::OK)),
        Span::raw(" "),
        Span::styled("CAREFUL", Style::default().fg(glances_colors::CAREFUL)),
        Span::raw(" "),
        Span::styled("WARNING", Style::default().fg(glances_colors::WARNING)),
        Span::raw(" "),
        Span::styled("CRITICAL", Style::default().fg(glances_colors::CRITICAL)),
    ]);

    let help = Paragraph::new(Line::from(help_spans))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    f.render_widget(help, area);
}

// Keep old functions for potential reuse, but mark as unused for now
#[allow(dead_code)]
fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = app.tabs.iter().map(|t| Line::from(*t)).collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Silicon Monitor"),
        )
        .select(app.selected_tab)
        .style(Style::default().fg(Color::Cyan))
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Cyan)
                .fg(Color::Black),
        );

    f.render_widget(tabs, area);
}

#[allow(dead_code)]
fn draw_content(f: &mut Frame, app: &App, area: Rect) {
    match app.selected_tab {
        0 => draw_overview(f, app, area),
        1 => draw_cpu(f, app, area),
        2 => draw_gpu(f, app, area),
        3 => draw_memory(f, app, area),
        4 => draw_peripherals(f, app, area),
        5 => draw_system(f, app, area),
        6 => draw_agent(f, app, area),
        _ => {}
    }
}

#[allow(dead_code)]
fn draw_overview(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(area);

    // CPU Overview
    let cpu_block = Block::default()
        .borders(Borders::ALL)
        .title(format!("CPU - {}", app.cpu_info.name));

    let cpu_gauge = Gauge::default()
        .block(cpu_block)
        .gauge_style(
            Style::default()
                .fg(cpu_color(app.cpu_info.utilization))
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .percent(safe_percent(app.cpu_info.utilization))
        .label(format!(
            "{:.1}% | {} cores{}",
            app.cpu_info.utilization,
            app.cpu_info.cores,
            opt_celsius(app.cpu_info.temperature, " | ")
        ));

    f.render_widget(cpu_gauge, chunks[0]);

    // Memory Overview
    let mem_percent = (app.memory_info.used as f64 / app.memory_info.total.max(1) as f64) * 100.0;
    let mem_block = Block::default().borders(Borders::ALL).title("Memory");

    let mem_gauge = Gauge::default()
        .block(mem_block)
        .gauge_style(
            Style::default()
                .fg(usage_color(mem_percent as f32))
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .percent(safe_percent(mem_percent as f32))
        .label(format!(
            "{:.1} GB / {:.1} GB ({:.0}%)",
            app.memory_info.used as f64 / (1024.0 * 1024.0 * 1024.0),
            app.memory_info.total as f64 / (1024.0 * 1024.0 * 1024.0),
            mem_percent
        ));

    f.render_widget(mem_gauge, chunks[1]);

    // GPU Overview
    if !app.gpu_info.is_empty() {
        let gpu = &app.gpu_info[0];
        let gpu_block = Block::default()
            .borders(Borders::ALL)
            .title(format!("GPU - {} ({})", gpu.name, gpu.vendor));

        let gpu_gauge = Gauge::default()
            .block(gpu_block)
            .gauge_style(
                Style::default()
                    .fg(usage_color(gpu.utilization.unwrap_or(0.0)))
                    .bg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .percent(safe_percent(gpu.utilization.unwrap_or(0.0)))
            .label(format!(
                "{} | {:.0}°C | {:.0}W / {:.0}W",
                pct_opt(gpu.utilization.map(f64::from)),
                gpu.temperature.unwrap_or(0.0),
                gpu.power.unwrap_or(0.0),
                gpu.power_limit.unwrap_or(0.0)
            ));

        f.render_widget(gpu_gauge, chunks[2]);
    } else {
        let no_gpu = Paragraph::new("No GPUs detected")
            .block(Block::default().borders(Borders::ALL).title("GPU"))
            .alignment(Alignment::Center);
        f.render_widget(no_gpu, chunks[2]);
    }
}

#[allow(dead_code)]
fn draw_cpu(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // CPU Info
    let mut info_text = vec![
        Line::from(format!("Name: {}", app.cpu_info.name)),
        Line::from(format!(
            "Cores: {} ({} threads)",
            app.cpu_info.cores, app.cpu_info.threads
        )),
        Line::from(format!("Utilization: {:.1}%", app.cpu_info.utilization)),
    ];
    // A detail pane is the one place worth naming the absence: "not measured" tells
    // the reader the sensor was queried, where an omitted line alone would not.
    info_text.push(Line::from(match app.cpu_info.temperature {
        Some(c) => format!("Temperature: {c:.1}°C"),
        None => "Temperature: not measured".to_string(),
    }));
    info_text.push(Line::from(match app.cpu_info.frequency {
        Some(mhz) => format!("Frequency: {mhz} MHz"),
        None => "Frequency: not measured".to_string(),
    }));

    let info = Paragraph::new(info_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("CPU Information"),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(info, chunks[0]);

    // CPU History Graph
    let sparkline_data: Vec<u64> = app.cpu_history.iter().copied().collect();
    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("CPU History (60s)"),
        )
        .data(&sparkline_data)
        .style(Style::default().fg(Color::Cyan));

    f.render_widget(sparkline, chunks[1]);
}

#[allow(dead_code)]
fn draw_gpu(f: &mut Frame, app: &App, area: Rect) {
    if app.gpu_info.is_empty() {
        let no_gpu = Paragraph::new("No GPUs detected")
            .block(Block::default().borders(Borders::ALL).title("GPU"))
            .alignment(Alignment::Center);
        f.render_widget(no_gpu, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let gpu = &app.gpu_info[0];

    // GPU Info - safely calculate memory percentage with clamping
    let mem_percent = match (gpu.memory_used, gpu.memory_total) {
        (Some(used), Some(total)) if total > 0 => {
            Some(((used as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as u16)
        }
        _ => None,
    };
    let info_text = vec![
        Line::from(format!("Name: {}", gpu.name)),
        Line::from(format!("Vendor: {}", gpu.vendor)),
        Line::from(format!(
            "Utilization: {}",
            pct_opt(gpu.utilization.map(f64::from))
        )),
        Line::from(format!(
            "Temperature: {:.0}°C",
            gpu.temperature.unwrap_or(0.0)
        )),
        Line::from(format!(
            "Power: {:.0}W / {:.0}W",
            gpu.power.unwrap_or(0.0),
            gpu.power_limit.unwrap_or(0.0)
        )),
        Line::from(match (gpu.memory_used, gpu.memory_total, mem_percent) {
            (Some(used), Some(total), Some(pct)) => format!(
                "Memory: {:.1} GB / {:.1} GB ({}%)",
                used as f64 / (1024.0 * 1024.0 * 1024.0),
                total as f64 / (1024.0 * 1024.0 * 1024.0),
                pct
            ),
            (Some(used), None, _) => format!(
                "Memory: {:.1} GB used, capacity not reported",
                used as f64 / (1024.0 * 1024.0 * 1024.0)
            ),
            _ => "Memory: not reported".to_string(),
        }),
        Line::from(format!(
            "Graphics Clock: {} MHz",
            gpu.clock_graphics
                .map(|c| c.to_string())
                .unwrap_or_else(|| "N/A".to_string())
        )),
        Line::from(format!(
            "Memory Clock: {} MHz",
            gpu.clock_memory
                .map(|c| c.to_string())
                .unwrap_or_else(|| "N/A".to_string())
        )),
    ];

    let info = Paragraph::new(info_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("GPU Information"),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(info, chunks[0]);

    // GPU History
    if !app.gpu_histories.is_empty() {
        let sparkline_data: Vec<u64> = app.gpu_histories[0].iter().copied().collect();
        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("GPU Utilization History (60s)"),
            )
            .data(&sparkline_data)
            .style(Style::default().fg(Color::Green));

        f.render_widget(sparkline, chunks[1]);
    }
}

#[allow(dead_code)]
fn draw_memory(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Memory Info
    let used_gb = app.memory_info.used as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_gb = app.memory_info.total as f64 / (1024.0 * 1024.0 * 1024.0);
    let avail_gb = app.memory_info.available as f64 / (1024.0 * 1024.0 * 1024.0);
    let swap_used_gb = app
        .memory_info
        .swap_used
        .map(|v| v as f64 / (1024.0 * 1024.0 * 1024.0));
    let swap_total_gb = app
        .memory_info
        .swap_total
        .map(|v| v as f64 / (1024.0 * 1024.0 * 1024.0));

    let info_text = vec![
        Line::from(format!("Total: {:.2} GB", total_gb)),
        Line::from(format!("Used: {:.2} GB", used_gb)),
        Line::from(format!("Available: {:.2} GB", avail_gb)),
        Line::from(format!("Usage: {:.1}%", (used_gb / total_gb) * 100.0)),
        Line::from(""),
        Line::from(match swap_total_gb {
            Some(v) => format!("Swap Total: {v:.2} GB"),
            None => "Swap Total: not reported".to_string(),
        }),
        Line::from(match swap_used_gb {
            Some(v) => format!("Swap Used: {v:.2} GB"),
            None => "Swap Used: not reported".to_string(),
        }),
    ];

    let info = Paragraph::new(info_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Memory Information"),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(info, chunks[0]);

    // Memory History
    let sparkline_data: Vec<u64> = app.memory_history.iter().copied().collect();
    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Memory Usage History (60s)"),
        )
        .data(&sparkline_data)
        .style(Style::default().fg(Color::Magenta));

    f.render_widget(sparkline, chunks[1]);
}

#[allow(dead_code)]
fn draw_system(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // System Info
    let uptime_secs = app.system_info.uptime.as_secs();
    let days = uptime_secs / 86400;
    let hours = (uptime_secs % 86400) / 3600;
    let minutes = (uptime_secs % 3600) / 60;

    let mut info_lines = vec![
        Line::from(format!("Hostname: {}", app.system_info.hostname)),
        Line::from(format!("OS: {}", app.system_info.os)),
        Line::from(format!("Kernel: {}", app.system_info.kernel)),
        Line::from(format!("Uptime: {}d {}h {}m", days, hours, minutes)),
    ];

    if let Some(ref manufacturer) = app.system_info.manufacturer {
        info_lines.push(Line::from(format!("Manufacturer: {}", manufacturer)));
    }
    if let Some(ref model) = app.system_info.model {
        info_lines.push(Line::from(format!("Model: {}", model)));
    }

    let info = Paragraph::new(info_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("System Information"),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(info, chunks[0]);

    // Disk Info
    let disk_items: Vec<ListItem> = app
        .disk_info
        .iter()
        .map(|disk| {
            let used_gb = disk.used as f64 / (1024.0 * 1024.0 * 1024.0);
            let total_gb = disk.total as f64 / (1024.0 * 1024.0 * 1024.0);
            let percent = (used_gb / total_gb) * 100.0;

            ListItem::new(format!(
                "{}: {:.1} GB / {:.1} GB ({:.0}%) - {}",
                disk.name, used_gb, total_gb, percent, disk.mount_point
            ))
        })
        .collect();

    let disks = List::new(disk_items)
        .block(Block::default().borders(Borders::ALL).title("Disks"))
        .style(Style::default().fg(Color::White));

    f.render_widget(disks, chunks[1]);
}

#[allow(dead_code)]
fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    // Check if there's a status message to display
    if let Some(status_msg) = app.get_status_message() {
        let status = Paragraph::new(Line::from(vec![Span::styled(
            status_msg,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )]))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
        f.render_widget(status, area);
    } else if app.agent_input_mode {
        // Show agent input mode
        let input_text = format!("> {}", app.agent_input);
        let input = Paragraph::new(Line::from(vec![
            Span::styled(
                "Agent Query: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(&input_text),
            Span::styled(
                "█",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Left);
        f.render_widget(input, area);
    } else {
        let help_text = vec![
            Span::raw("Press "),
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to quit | "),
            Span::styled("</", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to switch tabs | "),
            Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to reset graphs | "),
            Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" for agent | "),
            Span::styled("F12", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" to save config"),
        ];

        let help = Paragraph::new(Line::from(help_text))
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);

        f.render_widget(help, area);
    }
}

fn draw_agent(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Info/help
            Constraint::Min(0),    // Conversation history
        ])
        .split(area);

    // Agent info header
    let info_lines = if let Some(ref _agent) = app.agent {
        let cache_stats = app.agent_cache_stats().unwrap_or_default();
        vec![
            Line::from(vec![Span::styled(
                "[AI Agent Active]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(format!("Model: Medium (500M params) | {}", cache_stats)),
            Line::from(""),
            Line::from(vec![
                Span::raw("Press "),
                Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to ask a question | "),
                Span::styled("c", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to clear history"),
            ]),
        ]
    } else {
        vec![
            Line::from(vec![Span::styled(
                "❌ AI Agent Unavailable",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from("Agent failed to initialize. Check error logs."),
        ]
    };

    let info = Paragraph::new(info_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("AI Agent - Natural Language Queries"),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(info, chunks[0]);

    // Conversation history
    if app.agent_history.is_empty() {
        let help_text = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "No queries yet. Try asking:",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from("  * What's my GPU temperature?"),
            Line::from("  * How much power am I using?"),
            Line::from("  * Show GPU utilization"),
            Line::from("  * Is my system healthy?"),
            Line::from("  * Compare GPU temperatures"),
            Line::from("  * What's my memory usage?"),
            Line::from(""),
            Line::from(vec![
                Span::raw("Press "),
                Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to start asking questions"),
            ]),
        ];

        let help = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title("Examples"))
            .alignment(Alignment::Left);

        f.render_widget(help, chunks[1]);
    } else {
        // Show conversation history (most recent first). Use Paragraph
        // rather than List so long responses wrap to the panel width.
        // Build the Lines vec lazily — the conversation only changes when
        // the agent worker delivers a new response or the user clears
        // history, so we cache by `agent_history_version` and skip the
        // per-frame rebuild during typing / sparkline animation.
        thread_local! {
            static CACHE: std::cell::RefCell<(u64, Vec<Line<'static>>)> =
                const { std::cell::RefCell::new((u64::MAX, Vec::new())) };
        }
        let history_paragraph = CACHE.with(|cell| {
            let mut c = cell.borrow_mut();
            if c.0 != app.agent_history_version {
                c.1 = build_agent_history_lines(app);
                c.0 = app.agent_history_version;
            }
            // Clone the cached Vec<Line<'static>> for this frame's Paragraph.
            // Cloning Line<'static> is cheap (it just clones the inner Cow's).
            c.1.clone()
        });

        let history = Paragraph::new(history_paragraph)
            .block(Block::default().borders(Borders::ALL).title(format!(
                "Conversation History ({} queries)",
                app.agent_history.len()
            )))
            .wrap(ratatui::widgets::Wrap { trim: false });

        f.render_widget(history, chunks[1]);
    }
}

/// Build the cached `Vec<Line>` for the agent conversation history.
/// Called only when `agent_history_version` changes.
fn build_agent_history_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let total = app.agent_history.len();
    for (i, response) in app.agent_history.iter().rev().enumerate() {
        let time_str = format!(
            "[{}ms{}]",
            response.inference_time_ms,
            if response.from_cache { ", cached" } else { "" }
        );
        lines.push(Line::from(vec![
            Span::styled(
                format!("Q{}: ", total - i),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(response.query.clone()),
            Span::styled(
                format!(" {}", time_str),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        // Preserve existing newlines in the body; Paragraph's wrap handles
        // the rest at render time.
        let mut first = true;
        for body_line in response.response.split('\n') {
            if first {
                lines.push(Line::from(vec![
                    Span::styled(
                        "A:  ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(body_line.to_string(), Style::default().fg(Color::White)),
                ]));
                first = false;
            } else {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(body_line.to_string(), Style::default().fg(Color::White)),
                ]));
            }
        }
        lines.push(Line::from(""));
    }
    lines
}

/// Get color based on usage percentage (Glances-style thresholds)
fn usage_color(percent: f32) -> Color {
    threshold_color(percent)
}

fn draw_peripherals(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(area);

    // Audio section (from cache)
    let audio_block = Block::default()
        .borders(Borders::ALL)
        .title("Audio Devices")
        .border_style(Style::default().fg(glances_colors::TITLE));
    let audio_para = Paragraph::new(app.peripheral_cache.audio_info.as_str()).block(audio_block);
    f.render_widget(audio_para, chunks[0]);

    // Display section (from cache)
    let display_block = Block::default()
        .borders(Borders::ALL)
        .title("Displays")
        .border_style(Style::default().fg(glances_colors::TITLE));
    let display_para =
        Paragraph::new(app.peripheral_cache.display_info.as_str()).block(display_block);
    f.render_widget(display_para, chunks[1]);

    // USB section (from cache)
    let usb_block = Block::default()
        .borders(Borders::ALL)
        .title("USB Devices")
        .border_style(Style::default().fg(glances_colors::TITLE));
    let usb_para = Paragraph::new(app.peripheral_cache.usb_info.as_str()).block(usb_block);
    f.render_widget(usb_para, chunks[2]);

    // Bluetooth section (from cache)
    let bt_block = Block::default()
        .borders(Borders::ALL)
        .title("Bluetooth")
        .border_style(Style::default().fg(glances_colors::TITLE));
    let bt_para = Paragraph::new(app.peripheral_cache.bluetooth_info.as_str()).block(bt_block);
    f.render_widget(bt_para, chunks[3]);

    // Battery section (from cache)
    let battery_block = Block::default()
        .borders(Borders::ALL)
        .title("Battery / Power")
        .border_style(Style::default().fg(glances_colors::TITLE));
    let battery_para =
        Paragraph::new(app.peripheral_cache.battery_info.as_str()).block(battery_block);
    f.render_widget(battery_para, chunks[4]);
}
/// Draw process detail overlay
fn draw_process_detail_overlay(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 50, f.area());

    // Semi-transparent background
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(137, 180, 250)))
        .title(" Process Detail (Enter/Esc to close) ")
        .title_style(
            Style::default()
                .fg(Color::Rgb(137, 180, 250))
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Color::Rgb(30, 30, 46)));

    let inner = block.inner(area);
    f.render_widget(ratatui::widgets::Clear, area);
    f.render_widget(block, area);

    if let Some(process) = app.get_selected_process() {
        let lines = vec![
            Line::from(vec![
                Span::styled("PID: ", Style::default().fg(Color::Rgb(166, 227, 161))),
                Span::raw(format!("{}", process.pid)),
            ]),
            Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Rgb(166, 227, 161))),
                Span::raw(&process.name),
            ]),
            Line::from(vec![
                Span::styled("User: ", Style::default().fg(Color::Rgb(166, 227, 161))),
                Span::raw(process.user.as_deref().unwrap_or("unknown")),
            ]),
            Line::from(vec![
                Span::styled("CPU: ", Style::default().fg(Color::Rgb(166, 227, 161))),
                Span::raw(format!("{:.1}%", process.cpu_percent)),
            ]),
            Line::from(vec![
                Span::styled("Memory: ", Style::default().fg(Color::Rgb(166, 227, 161))),
                Span::raw(auto_unit(process.memory_bytes)),
            ]),
            Line::from(vec![
                Span::styled("State: ", Style::default().fg(Color::Rgb(166, 227, 161))),
                Span::raw(format!("{}", process.state)),
            ]),
            Line::from(vec![
                Span::styled("Threads: ", Style::default().fg(Color::Rgb(166, 227, 161))),
                Span::raw(format!("{}", process.thread_count)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "GPU Usage: ",
                    Style::default().fg(Color::Rgb(250, 179, 135)),
                ),
                Span::raw(
                    process
                        .gpu_usage_percent
                        .map(|u| format!("{:.1}%", u))
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "GPU Memory: ",
                    Style::default().fg(Color::Rgb(250, 179, 135)),
                ),
                Span::raw(match process.total_gpu_memory_bytes {
                    Some(b) if b > 0 => auto_unit(b),
                    Some(_) => "0".to_string(),
                    None => "-".to_string(),
                }),
            ]),
            Line::from(vec![
                Span::styled("Encoder: ", Style::default().fg(Color::Rgb(250, 179, 135))),
                Span::raw(
                    process
                        .encoder_usage_percent
                        .map(|u| format!("{:.1}%", u))
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ]),
            Line::from(vec![
                Span::styled("Decoder: ", Style::default().fg(Color::Rgb(250, 179, 135))),
                Span::raw(
                    process
                        .decoder_usage_percent
                        .map(|u| format!("{:.1}%", u))
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ]),
        ];
        let para = Paragraph::new(lines);
        f.render_widget(para, inner);
    }
}

/// Draw theme picker overlay
fn draw_theme_picker_overlay(f: &mut Frame, app: &App) {
    use super::app::ColorTheme;

    let themes = ColorTheme::all();
    let height = (themes.len() + 4) as u16;
    let area = centered_rect(40, height.min(20), f.area());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(137, 180, 250)))
        .title(" Select Theme (Enter to apply, Esc to cancel) ")
        .title_style(
            Style::default()
                .fg(Color::Rgb(137, 180, 250))
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Color::Rgb(30, 30, 46)));

    let inner = block.inner(area);
    f.render_widget(ratatui::widgets::Clear, area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = themes
        .iter()
        .enumerate()
        .map(|(idx, theme)| {
            let is_selected = idx == app.selected_theme_idx;
            let is_current = *theme == app.color_theme;
            let name = if is_current {
                format!("{} (current)", theme.name())
            } else {
                theme.name().to_string()
            };
            let style = if is_selected {
                Style::default()
                    .bg(Color::Rgb(69, 71, 90))
                    .fg(Color::Rgb(205, 214, 244))
            } else {
                Style::default().fg(Color::Rgb(166, 173, 200))
            };
            ListItem::new(name).style(style)
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

/// Helper to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

// ═══════════════════════════════════════════════════════════════════════════════
// PROFILES TAB — NVIDIA Profile Inspector / XTU / Ryzen Master style
// ═══════════════════════════════════════════════════════════════════════════════

fn draw_profiles_tab(f: &mut Frame, app: &App, area: Rect) {
    use crate::profile::Subsystem;

    if app.profile_show_deviations {
        draw_profile_deviations_overlay(f, app, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Subsystem selector strip
    let titles: Vec<&str> = Subsystem::ALL.iter().map(|s| s.as_str()).collect();
    let sub_tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Profile Inspector — [1-5] subsystem · [r] refresh · [d] deviations · [↑↓ PgUp/PgDn] scroll "),
        )
        .select(app.profile_subsystem_idx)
        .highlight_style(
            Style::default()
                .fg(glances_colors::TITLE)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(sub_tabs, chunks[0]);

    // Snapshot may be None on first display.
    let Some(snapshot) = app.profile_snapshot.as_ref() else {
        let msg = Paragraph::new(vec![
            Line::from(Span::styled(
                "Loading hardware profile snapshot…",
                Style::default().fg(glances_colors::TITLE),
            )),
            Line::from(""),
            Line::from("Press 'r' to fetch — initial NVML/registry/SMBIOS reads take a moment."),
        ])
        .block(Block::default().borders(Borders::ALL));
        f.render_widget(msg, chunks[1]);
        return;
    };

    let sub = Subsystem::ALL
        .get(app.profile_subsystem_idx)
        .copied()
        .unwrap_or(Subsystem::Gpu);
    let groups = snapshot.providers.get(&sub).cloned().unwrap_or_default();

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(
            "{} group(s) · {} setting(s) · captured @ ts={}",
            groups.len(),
            groups.iter().map(|g| g.settings.len()).sum::<usize>(),
            snapshot.timestamp
        ),
        Style::default().fg(glances_colors::CAREFUL),
    )));
    lines.push(Line::from(""));

    for group in &groups {
        lines.push(Line::from(vec![
            Span::styled(
                "▸ ",
                Style::default()
                    .fg(glances_colors::TITLE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                group.device.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                group.display_name.clone(),
                Style::default().fg(glances_colors::CAREFUL),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  source: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                group.source.clone(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
        for s in &group.settings {
            let risk_color = match s.risk {
                crate::profile::SettingRisk::Informational => Color::Reset,
                crate::profile::SettingRisk::Safe => glances_colors::OK,
                crate::profile::SettingRisk::Moderate => glances_colors::WARNING,
                crate::profile::SettingRisk::Dangerous => glances_colors::CRITICAL,
            };
            let unit = s
                .unit
                .as_deref()
                .map(|u| format!(" {}", u))
                .unwrap_or_default();
            let value_str = format!("{}{}", s.value, unit);
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("{:<28}", trunc(&s.display_name, 28)),
                    Style::default().fg(risk_color),
                ),
                Span::raw(" = "),
                Span::styled(value_str, Style::default().add_modifier(Modifier::BOLD)),
            ]));
        }
        for n in &group.notes {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("• ", Style::default().fg(Color::DarkGray)),
                Span::styled(n.clone(), Style::default().fg(Color::DarkGray)),
            ]));
        }
        lines.push(Line::from(""));
    }
    if groups.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no profile data for this subsystem on this platform)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(format!(
            " {} — Hardware Profile Inspector ",
            sub.as_str().to_uppercase()
        )))
        .scroll((app.profile_scroll, 0));
    f.render_widget(paragraph, chunks[1]);
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn draw_profile_deviations_overlay(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let mut dev_lines: Vec<Line> = Vec::new();
    dev_lines.push(Line::from(Span::styled(
        "press [d] to return to profile view",
        Style::default().fg(Color::DarkGray),
    )));
    dev_lines.push(Line::from(""));
    if let Some(snapshot) = app.profile_snapshot.as_ref() {
        let devs = crate::profile::deviation::deviations_from_default(snapshot);
        if devs.is_empty() {
            dev_lines.push(Line::from(Span::styled(
                "All settings with a declared default are at their default value.",
                Style::default().fg(glances_colors::OK),
            )));
        } else {
            dev_lines.push(Line::from(Span::styled(
                format!("{} deviation(s):", devs.len()),
                Style::default()
                    .fg(glances_colors::WARNING)
                    .add_modifier(Modifier::BOLD),
            )));
            dev_lines.push(Line::from(""));
            for d in devs {
                let risk_color = match d.risk {
                    crate::profile::SettingRisk::Dangerous => glances_colors::CRITICAL,
                    crate::profile::SettingRisk::Moderate => glances_colors::WARNING,
                    crate::profile::SettingRisk::Safe => glances_colors::OK,
                    crate::profile::SettingRisk::Informational => Color::DarkGray,
                };
                dev_lines.push(Line::from(vec![
                    Span::styled(
                        format!("  [{:?}] ", d.risk),
                        Style::default().fg(risk_color),
                    ),
                    Span::styled(
                        format!("[{}] ", d.subsystem.as_str()),
                        Style::default().fg(glances_colors::TITLE),
                    ),
                    Span::raw(format!("{} :: ", d.device)),
                    Span::styled(
                        d.display_name.clone(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ]));
                dev_lines.push(Line::from(vec![
                    Span::raw("      default: "),
                    Span::styled(d.default.to_string(), Style::default().fg(Color::DarkGray)),
                    Span::raw("   current: "),
                    Span::styled(
                        d.current.to_string(),
                        Style::default().fg(risk_color).add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
        }
    } else {
        dev_lines.push(Line::from(Span::styled(
            "(no snapshot — return to profile view and press [r] to refresh)",
            Style::default().fg(Color::DarkGray),
        )));
    }
    let dev_para = Paragraph::new(dev_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Deviations from default "),
    );
    f.render_widget(dev_para, chunks[0]);

    // Audit log tail
    let audit_path = crate::profile::apply::audit_log_path();
    let content = std::fs::read_to_string(&audit_path).unwrap_or_default();
    let all_lines: Vec<&str> = content.lines().collect();
    let tail_n = 8usize.min(all_lines.len());
    let tail = if all_lines.len() > tail_n {
        &all_lines[all_lines.len() - tail_n..]
    } else {
        &all_lines[..]
    };
    let mut audit_lines: Vec<Line> = Vec::new();
    audit_lines.push(Line::from(Span::styled(
        format!("source: {}", audit_path.display()),
        Style::default().fg(Color::DarkGray),
    )));
    audit_lines.push(Line::from(""));
    if tail.is_empty() {
        audit_lines.push(Line::from(Span::styled(
            "(no audit entries)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for line in tail {
            if let Ok(o) = serde_json::from_str::<crate::profile::apply::ApplyOutcome>(line) {
                let (status_str, status_color) = match o.status {
                    crate::profile::apply::ApplyStatus::Applied => ("applied", glances_colors::OK),
                    crate::profile::apply::ApplyStatus::Refused => {
                        ("refused", glances_colors::CRITICAL)
                    }
                    crate::profile::apply::ApplyStatus::Failed => {
                        ("failed", glances_colors::CRITICAL)
                    }
                    crate::profile::apply::ApplyStatus::NotWritable => {
                        ("not-writable", glances_colors::WARNING)
                    }
                    crate::profile::apply::ApplyStatus::NeedsConfirm => {
                        ("needs-confirm", glances_colors::WARNING)
                    }
                };
                audit_lines.push(Line::from(vec![
                    Span::styled(
                        format!("  [{}] ", o.timestamp),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        format!("{:<13}", status_str),
                        Style::default().fg(status_color),
                    ),
                    Span::raw(" "),
                    Span::styled(o.setting_id, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(format!(" = {}", o.requested)),
                ]));
            }
        }
    }
    let audit_para = Paragraph::new(audit_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Apply audit log (tail) "),
    );
    f.render_widget(audit_para, chunks[1]);
}

/// Render an optional byte count, or a dash where nothing was reported.
///
/// The TUI already shows a dash for a device with no utilization counter. These
/// figures were bare numbers until 6.0.0 and had no way to say the same, so an
/// adapter that reported no memory drew "0B/0B (0%)" -- a full-looking readout
/// of an empty card.
fn auto_unit_opt(bytes: Option<u64>) -> String {
    bytes.map_or_else(|| "-".to_string(), auto_unit)
}

/// Render an optional percentage, or a dash.
fn pct_opt(pct: Option<f64>) -> String {
    pct.map_or_else(|| "-".to_string(), |p| format!("{p:.0}%"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_readings_render_as_nothing_not_as_zero() {
        // A missing sensor and a genuinely idle one must not produce the same text.
        // Before these helpers every site was `unwrap_or(0)`, so a CPU whose
        // frequency the platform never reported was indistinguishable from a core
        // parked at 0 MHz — and on Windows 11 24H2 that was every CPU, because the
        // `wmic` subprocess the reader depended on no longer exists.
        assert_eq!(opt_mhz(None, " @ "), "");
        assert_eq!(opt_celsius(None, " │ "), "");

        assert_eq!(opt_mhz(Some(4400), " @ "), " @ 4400 MHz");
        assert_eq!(opt_celsius(Some(61.4), " │ "), " │ 61°C");

        // A measured zero is still a measurement and must be shown.
        assert_eq!(opt_mhz(Some(0), " @ "), " @ 0 MHz");
        assert_eq!(opt_celsius(Some(0.0), " │ "), " │ 0°C");
    }

    #[test]
    fn separators_do_not_dangle_when_a_reading_is_absent() {
        // The separator belongs to the value, so omitting the value must omit it
        // too — otherwise the header renders "24 cores @ │".
        let label = format!(
            "{} cores{}{}",
            24,
            opt_mhz(None, " @ "),
            opt_celsius(None, " │ ")
        );
        assert_eq!(label, "24 cores");
    }
}
