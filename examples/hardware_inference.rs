//! Hardware inference report
//!
//! Runs `HardwareInferenceEngine::full_analysis()` and prints what it concluded
//! about this machine, with the evidence each conclusion carries.
//!
//! **This exists because the handoff asks for it.** Item 4 of `HANDOFF.md` says
//! the engine was audited on one desktop and wants its report read on a real
//! laptop before the classifier weights are trusted — and until now that meant
//! writing this file first. If you are on a laptop and the classification is not
//! `Laptop`, the weights are wrong in the other direction now.
//!
//! Run with: `cargo run --example hardware_inference --features cli`

use simonlib::HardwareInferenceEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Hardware inference report ===\n");
    println!("Building the engine (it reads the machine, which takes a few seconds)...\n");

    let engine = HardwareInferenceEngine::new()?;
    let report = engine.full_analysis();

    println!("--- Classification ---");
    println!(
        "  class:            {:?} (confidence {:.2})",
        report.classification, report.classification_confidence
    );
    println!(
        "  performance tier: {:?} (score {})",
        report.performance_tier, report.performance_score
    );
    println!("  fingerprint:      {}", report.hardware_fingerprint);

    println!("\n--- Age ---");
    let age = &report.hardware_age;
    println!("  CPU year:   {:?}", age.cpu_year);
    println!("  GPU year:   {:?}", age.gpu_year);
    println!(
        "  age:        {:.1} years (confidence {:.2})",
        age.estimated_age_years, age.confidence
    );
    println!("  reasoning:  {}", age.reasoning);

    println!("\n--- Thermal envelope ---");
    let t = &report.thermal_envelope;
    println!(
        "  CPU TDP:        {:.0} W (from the model-name table)",
        t.cpu_tdp_watts
    );
    println!(
        "  GPU TDP:        {:.0} W over {} adapter(s) ({})",
        t.gpu_tdp_watts,
        t.gpu_adapters_counted,
        if t.gpu_tdp_measured {
            "the caps the drivers enforce"
        } else {
            "the model-name table; no driver reported a cap"
        }
    );
    println!("  total:          {:.0} W", t.estimated_total_tdp_watts);
    println!("  headroom:       {:?}", t.headroom);
    println!("  cooling score:  {}", t.cooling_score);
    for r in &t.recommendations {
        println!("    - {r}");
    }

    println!("\n--- Bottlenecks ---");
    if report.bottlenecks.is_empty() {
        println!("  none identified");
    }
    for b in &report.bottlenecks {
        println!(
            "  {} ({:?}, severity {}, confidence {:.2})",
            b.component, b.bottleneck_type, b.severity, b.confidence
        );
        println!("      {}", b.reason);
    }

    println!("\n--- Workload fit (top five) ---");
    for w in report.workload_scores.iter().take(5) {
        println!(
            "  {:?}: {} (confidence {:.2})",
            w.workload, w.score, w.confidence
        );
        for l in &w.limiting_factors {
            println!("      limited by: {l}");
        }
    }

    println!("\n--- Anomalies ---");
    if report.anomalies.is_empty() {
        println!("  none");
    }
    for a in &report.anomalies {
        println!("  {:?}: {}", a.severity, a.description);
        println!("      {}", a.explanation);
    }

    Ok(())
}
