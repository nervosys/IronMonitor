//! Configuration Management Example
//!
//! Demonstrates loading, modifying, and saving configuration with TOML.
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --features full --example config_demo
//! ```
//!
//! # Features Demonstrated
//!
//! - Loading configuration from default path or creating defaults
//! - Modifying configuration programmatically
//! - Saving configuration to disk with TOML serialization
//! - Custom configuration paths
//! - Configuration validation

use ironmonlib::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("⚙️  IronMonitor - Configuration Management Demo\n");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Load existing config or create default
    println!("📂 Loading configuration...");
    let mut config = Config::load().unwrap_or_else(|_| {
        println!("   No existing config found, using defaults");
        Config::default()
    });

    let config_path = Config::default_path()?;
    println!("   Config path: {}", config_path.display());
    println!();

    // Display current configuration
    println!("[MB] Current Configuration:");
    println!("─────────────────────────────────────────────────────────────\n");

    println!("General Settings:");
    println!("  Update Interval: {}ms", config.general.update_interval_ms);
    println!(
        "  Temperature Unit: {}",
        if config.general.use_fahrenheit {
            "Fahrenheit"
        } else {
            "Celsius"
        }
    );
    println!("  Color Scheme: {}", config.general.color_scheme);
    println!(
        "  Encoder/Decoder Hide Timer: {}s",
        config.general.encode_decode_hiding_timer
    );
    println!();

    println!("GPU Settings:");
    if config.gpu.monitored_gpus.is_empty() {
        println!("  Monitored GPUs: All (none specified)");
    } else {
        println!("  Monitored GPUs: {:?}", config.gpu.monitored_gpus);
    }
    println!("  Show Details: {}", config.gpu.show_details);
    println!("  Show Processes: {}", config.gpu.show_processes);
    println!("  Reverse Plot: {}", config.gpu.reverse_plot);
    println!();

    println!("Process Settings:");
    println!("  Visible Columns: {:?}", config.process.visible_columns);
    println!("  Sort Column: {}", config.process.sort_column);
    println!("  Sort Ascending: {}", config.process.sort_ascending);
    println!("  Hide Self: {}", config.process.hide_self);
    println!();

    println!("Chart Settings:");
    println!("  Metrics: {:?}", config.chart.metrics);
    println!("  History Length: {}s", config.chart.history_length);
    println!();

    // Demonstrate configuration modification
    println!("═══════════════════════════════════════════════════════════════\n");
    println!("[BIOS] Modifying Configuration...\n");

    // Modify some settings
    config.general.update_interval_ms = 2000;
    println!("  ✓ Changed update interval to 2000ms");

    config.general.use_fahrenheit = true;
    println!("  ✓ Changed temperature unit to Fahrenheit");

    config.general.encode_decode_hiding_timer = 60;
    println!("  ✓ Changed encoder/decoder hide timer to 60s");

    config.gpu.show_details = true;
    println!("  ✓ Enabled GPU details");

    config.process.sort_column = "cpu".to_string();
    config.process.sort_ascending = true;
    println!("  ✓ Changed process sorting to CPU (ascending)");

    config.chart.history_length = 120;
    println!("  ✓ Increased chart history to 120s");

    println!();

    // Save configuration
    println!("═══════════════════════════════════════════════════════════════\n");
    println!("💾 Saving Configuration...\n");

    match config.save() {
        Ok(()) => {
            println!("  ✓ Configuration saved successfully to:");
            println!("    {}", config_path.display());
            println!();
            println!("  You can now:");
            println!("    - Edit the config file manually");
            println!("    - Run the TUI and press F12 to save settings");
            println!("    - Use these settings across all ironmon applications");
        }
        Err(e) => {
            eprintln!("  ✗ Failed to save configuration: {}", e);
            return Err(Box::new(e));
        }
    }

    println!();

    // Demonstrate custom path save/load
    println!("═══════════════════════════════════════════════════════════════\n");
    println!("📝 Custom Path Operations...\n");

    let custom_path = std::env::current_dir()?.join("ironmon_custom.toml");
    println!("  Saving to custom path: {}", custom_path.display());

    config.save_to(&custom_path)?;
    println!("  ✓ Saved successfully");

    println!("  Loading from custom path...");
    let loaded_config = Config::load_from(&custom_path)?;
    println!("  ✓ Loaded successfully");
    println!(
        "  Loaded update interval: {}ms",
        loaded_config.general.update_interval_ms
    );

    println!();
    println!("═══════════════════════════════════════════════════════════════\n");
    println!("[OK] Configuration Demo Complete!");
    println!();

    // Display example TOML content
    println!("Example TOML content:");
    println!("─────────────────────────────────────────────────────────────");
    let toml_str = toml::to_string_pretty(&config)?;
    println!("{}", toml_str);

    Ok(())
}
