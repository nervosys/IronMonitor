//! IronMonitor TUI - Interactive Terminal Dashboard
//!
//! This example demonstrates the full Terminal User Interface for IronMonitor.
//! It provides real-time monitoring of CPU, GPU, memory, disk, and system information.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example tui --features cli
//! ```
//!
//! # Controls
//!
//! - `q` or `Esc` - Quit
//! - `←/→` or `1-5` - Switch tabs
//! - `↑/↓` - Scroll (where applicable)
//! - `r` - Reset graphs

#[cfg(feature = "cli")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    ironmonlib::tui::run()
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("This example requires the 'cli' feature to be enabled.");
    eprintln!("Run with: cargo run --example tui --features cli");
}
