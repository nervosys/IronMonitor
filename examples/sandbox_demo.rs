//! Sandbox Detection Example
//!
//! Demonstrates sandbox and virtualization detection to prevent
//! data collection in analysis environments.
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --features full --example sandbox_demo
//! ```

use ironmonlib::sandbox::SandboxDetector;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║     IronMonitor - Sandbox Detection Demo                  ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    let detector = SandboxDetector::new();
    let info = detector.detect();

    println!("Environment Analysis:\n");
    println!("  Overall Status: {}\n", info.summary());

    println!("Detailed Checks:");
    println!(
        "  Virtual Machine: {}",
        if info.is_vm {
            "✗ DETECTED"
        } else {
            "✓ Not detected"
        }
    );
    println!(
        "  Container: {}",
        if info.is_container {
            "✗ DETECTED"
        } else {
            "✓ Not detected"
        }
    );
    println!(
        "  Windows Sandbox: {}",
        if info.is_windows_sandbox {
            "✗ DETECTED"
        } else {
            "✓ Not detected"
        }
    );
    println!(
        "  macOS Sandbox: {}",
        if info.is_macos_sandbox {
            "✗ DETECTED"
        } else {
            "✓ Not detected"
        }
    );
    println!(
        "  Wine/Compat Layer: {}",
        if info.is_wine {
            "✗ DETECTED"
        } else {
            "✓ Not detected"
        }
    );
    println!(
        "  Debugger: {}",
        if info.is_debugged {
            "✗ DETECTED"
        } else {
            "✓ Not detected"
        }
    );

    if let Some(env) = &info.environment {
        println!("\n  Detected Environment: {}", env);
    }

    if !info.indicators.is_empty() {
        println!("\n  Detection Indicators:");
        for indicator in &info.indicators {
            println!("    • {}", indicator);
        }
    }

    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║     Data Collection Policy                                     ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    if info.is_sandboxed() {
        println!("⚠️  SANDBOXED ENVIRONMENT DETECTED");
        println!("\n  Protection Status: ACTIVE");
        println!("  Data Collection: DISABLED");
        println!("  Telemetry: DISABLED");
        println!("  Logging: LOCAL ONLY");
        println!("  Network Transmission: BLOCKED");
        println!("\n  This protects user privacy during:");
        println!("    • Security analysis and reverse engineering");
        println!("    • Quality assurance testing");
        println!("    • Virtual machine evaluation");
        println!("    • Container-based deployments");
        println!("    • Development and debugging");
    } else {
        println!("[OK] NORMAL ENVIRONMENT");
        println!("\n  Protection Status: Not needed");
        println!("  Data Collection: Subject to user consent");
        println!("  Telemetry: Subject to user consent");
        println!("  Privacy: Full user control via consent system");
    }

    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║     Integration with Consent System                            ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    println!("The consent system automatically checks for sandboxes:");
    println!("  1. User grants consent for telemetry");
    println!("  2. Application checks: has_consent(Telemetry)");
    println!("  3. Consent system runs sandbox detection");
    println!("  4. Returns false if sandboxed, regardless of consent");
    println!("\nResult: NO DATA COLLECTION IN SANDBOXES, EVER.\n");

    Ok(())
}
