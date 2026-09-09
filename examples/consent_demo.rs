//! Consent Management Example
//!
//! Demonstrates the ethical consent system for data collection.
//! Shows how to properly gate any telemetry or analytics behind
//! explicit user consent, and demonstrates sandbox detection.
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --features full --example consent_demo
//! ```
//!
//! # Features Demonstrated
//!
//! - Sandbox detection (VM, container, debugger)
//! - Automatic data collection prevention in sandboxes
//! - Individual consent scope management
//! - Consent status viewing
//! - Simulated data collection (gated by consent + sandbox check)

use ironmonlib::consent::{ConsentManager, ConsentScope};
use ironmonlib::sandbox::SandboxDetector;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║     IronMonitor - Ethical Consent System Demo            ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    // STEP 1: Check for sandbox/analysis environment FIRST
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║     Sandbox Detection                                          ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    let sandbox_detector = SandboxDetector::new();
    let sandbox_info = sandbox_detector.detect();

    println!("Environment Analysis:");
    println!("  Status: {}", sandbox_info.summary());

    if !sandbox_info.indicators.is_empty() {
        println!("\n  Detected indicators:");
        for indicator in &sandbox_info.indicators {
            println!("    • {}", indicator);
        }
    }

    if sandbox_info.is_sandboxed() {
        println!("\n⚠️  SANDBOX DETECTED");
        println!("╔════════════════════════════════════════════════════════════════╗");
        println!("║  Data collection is DISABLED in sandboxed environments        ║");
        println!("║                                                                ║");
        println!("║  This protects user privacy during:                           ║");
        println!("║    • Security analysis                                         ║");
        println!("║    • Reverse engineering                                       ║");
        println!("║    • Testing/QA                                                ║");
        println!("║    • Virtual machine evaluation                                ║");
        println!("║                                                                ║");
        println!("║  No telemetry, logging, or data transmission will occur.      ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        // Demonstrate that consent checks will return false
        println!("Consent check behavior in sandbox:");
        println!("  Even if user previously granted consent, sandbox detection");
        println!("  automatically returns 'false' for all has_consent() calls.");
        println!("  This ensures no data collection occurs during analysis/testing.");

        println!("\n[OK] Privacy protection active - no data will be collected.\n");
        return Ok(());
    }

    println!("\n[OK] Not sandboxed - normal operation\n");

    // STEP 2: Load consent manager (only if not sandboxed)
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║     Consent System Overview                                    ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    println!("The consent system ensures:");
    println!("  ✓ No data collection without your explicit permission");
    println!("  ✓ Full transparency about what's collected");
    println!("  ✓ Easy opt-out at any time");
    println!("  ✓ Complete audit trail of your decisions");
    println!("  ✓ Automatic prevention in sandboxed environments\n");

    let manager = ConsentManager::load().unwrap_or_else(|_| {
        println!("Note: No existing consent configuration found.");
        println!("      This is a demonstration - consent would be requested on first run.\n");
        // Create a default manager for demonstration
        let path = ConsentManager::default_path().unwrap();
        ConsentManager::load_from(&path).unwrap_or_else(|_| {
            // This will never be reached but satisfies the compiler
            panic!("Failed to create consent manager")
        })
    });

    // Display current consent status
    println!("\n{}", manager.export_consent_status());

    // Demonstrate checking consent before data collection
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║     Demonstrating Consent-Gated Operations                    ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    // Example: Basic telemetry
    if manager.has_consent(ConsentScope::BasicTelemetry) {
        println!("✓ BasicTelemetry: GRANTED");
        println!("  → Would collect: crash reports, error frequencies");
        simulate_telemetry_collection();
    } else {
        println!("✗ BasicTelemetry: DENIED");
        println!("  → No telemetry data will be collected");
    }
    println!();

    // Example: Hardware info
    if manager.has_consent(ConsentScope::HardwareInfo) {
        println!("✓ HardwareInfo: GRANTED");
        println!("  → Would collect: anonymized GPU/CPU types");
        simulate_hardware_info_collection();
    } else {
        println!("✗ HardwareInfo: DENIED");
        println!("  → No hardware info will be collected");
    }
    println!();

    // Example: Analytics
    if manager.has_consent(ConsentScope::Analytics) {
        println!("✓ Analytics: GRANTED");
        println!("  → Would collect: feature usage patterns");
        simulate_analytics_collection();
    } else {
        println!("✗ Analytics: DENIED");
        println!("  → No analytics will be collected");
    }
    println!();

    // Show how to revoke consent
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║     Consent Revocation                                         ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    println!("Consent can be revoked at any time:");
    println!("  • Programmatically: manager.revoke_consent(scope)");
    println!("  • Revoke all: manager.revoke_all()");
    println!("  • Through TUI: F2 setup window");
    println!("  • CLI flags: --revoke-consent");
    println!("\nExample (not executed in this demo):");
    println!("  manager.revoke_consent(ConsentScope::BasicTelemetry)?;");
    println!("  manager.revoke_all()?;");

    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║     Summary                                                    ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    println!("The consent system ensures ethical data collection:");
    println!("  ✓ Explicit opt-in required");
    println!("  ✓ Granular control per data type");
    println!("  ✓ Easy revocation");
    println!("  ✓ Full audit trail");
    println!("  ✓ No hidden collection");
    println!();

    let consent_path = ConsentManager::default_path()?;
    println!("Your consent preferences are stored at:");
    println!("  {}", consent_path.display());
    println!();
    println!("You can review or change them at any time by:");
    println!("  - Running this demo again");
    println!("  - Editing the config file manually");
    println!("  - Using CLI flags: --consent-status, --revoke-consent");
    println!();

    Ok(())
}

/// Simulate telemetry collection (would only run if consent granted)
fn simulate_telemetry_collection() {
    println!("    [SIMULATION] Collecting telemetry data...");
    println!("    [SIMULATION] - Application version: 0.1.0");
    println!("    [SIMULATION] - OS: {}", std::env::consts::OS);
    println!("    [SIMULATION] - No crashes in this session");
}

/// Simulate hardware info collection (would only run if consent granted)
fn simulate_hardware_info_collection() {
    println!("    [SIMULATION] Collecting hardware info...");
    println!(
        "    [SIMULATION] - CPU architecture: {}",
        std::env::consts::ARCH
    );
    println!("    [SIMULATION] - GPU vendor: NVIDIA (anonymized)");
    println!("    [SIMULATION] - Serial numbers: NEVER collected");
}

/// Simulate analytics collection (would only run if consent granted)
fn simulate_analytics_collection() {
    println!("    [SIMULATION] Collecting analytics...");
    println!("    [SIMULATION] - Feature usage: GPU monitoring enabled");
    println!("    [SIMULATION] - Session duration: 5 minutes");
    println!("    [SIMULATION] - No user profiling or tracking");
}
