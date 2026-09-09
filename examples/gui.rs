//! IronMonitor GUI Example
//!
//! Launches the graphical user interface for hardware monitoring.
//!
//! # Usage
//! ```bash
//! cargo run --release --features "gui nvidia" --example gui
//! ```

fn main() -> Result<(), eframe::Error> {
    println!("⚡ Starting IronMonitor GUI...");
    ironmonlib::gui::run()
}
