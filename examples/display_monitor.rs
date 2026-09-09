//! Display monitoring example
//!
//! Demonstrates how to enumerate displays and get their properties.
//!
//! Run with: cargo run --example display_monitor

use ironmonlib::display::{DisplayConnection, DisplayMonitor};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Display Monitor Example ===\n");

    let monitor = DisplayMonitor::new()?;

    let displays = monitor.displays();
    println!("Found {} display(s):\n", displays.len());

    for display in displays {
        let primary = if display.is_primary { " (Primary)" } else { "" };
        let connection_icon = match display.connection {
            DisplayConnection::Hdmi => "📺",
            DisplayConnection::DisplayPort => "🖥️",
            DisplayConnection::Dvi => "🖥️",
            DisplayConnection::Vga => "🖥️",
            DisplayConnection::Internal | DisplayConnection::Edp => "💻",
            DisplayConnection::Usb | DisplayConnection::UsbC => "🔌",
            DisplayConnection::Wireless | DisplayConnection::Virtual => "📶",
            DisplayConnection::Unknown => "❓",
        };

        let name = display.name.as_deref().unwrap_or("Unknown Display");
        println!("{} {}{}", connection_icon, name, primary);
        println!("  ID: {}", display.id);
        match (display.width, display.height) {
            (Some(w), Some(h)) => println!("  Resolution: {w}x{h}"),
            _ => println!("  Resolution: not readable"),
        }
        match display.refresh_rate {
            Some(hz) => println!("  Refresh Rate: {hz} Hz"),
            None => println!("  Refresh Rate: not read on this platform"),
        }

        if let Some(scale) = display.scale_factor {
            println!("  Scale Factor: {:.0}%", scale * 100.0);
        }

        if let (Some(w), Some(h)) = (display.physical_width_mm, display.physical_height_mm) {
            let diagonal_mm = ((w as f64).powi(2) + (h as f64).powi(2)).sqrt();
            let diagonal_inches = diagonal_mm / 25.4;
            println!(
                "  Physical Size: {}mm x {}mm ({:.1}\" diagonal)",
                w, h, diagonal_inches
            );
        }

        if let Some(bits) = display.bits_per_pixel {
            println!("  Color Depth: {} bits", bits);
        }

        println!("  Connection: {:?}", display.connection);
        println!();
    }

    // Summary info
    if !displays.is_empty() {
        // Displays whose mode was not read contribute nothing rather than
        // zero, and the count below says how many were skipped.
        let measured: Vec<_> = displays
            .iter()
            .filter_map(|d| Some((d.width? as u64, d.height? as u64)))
            .collect();
        let total_pixels: u64 = measured.iter().map(|(w, h)| w * h).sum();
        let primary_count = displays.iter().filter(|d| d.is_primary).count();

        println!("Summary:");
        println!("  Total Displays: {}", displays.len());
        println!("  Primary Displays: {}", primary_count);
        println!(
            "  Total Pixels: {} ({:.2}M)",
            total_pixels,
            total_pixels as f64 / 1_000_000.0
        );
    }

    Ok(())
}
