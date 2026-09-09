//! The exporter and the ontology describe one machine, so they have to agree.
//!
//! These are two independent collection paths over the same hardware:
//! `PrometheusExporter` renders from the library's own collectors, and
//! `ontology::resolve::snapshot()` resolves the entity graph. Nothing has ever
//! compared their *values* — `prometheus_exposition.rs` checks names, headers,
//! label syntax and dashboard coverage, all of which pass while a renderer
//! reports kilobytes under a name ending in `_bytes`.
//!
//! **Only quantities that do not move are compared.** A CPU utilisation
//! sampled twice, seconds apart, differs for reasons that are not defects: this
//! machine read 80.8% through the endpoint and 22.4% through the ontology in the
//! same minute, and both were right. Installed memory, swap size and an
//! adapter's total VRAM do not change between two reads, so a difference there
//! is a unit, a scaling factor, or two readers looking at different fields.
//!
//! That distinction is the whole design of this file. A test that compared
//! everything would fail constantly and be deleted; a test that compares the
//! stable half fails only when something is wrong.

use ironmonlib::prometheus::PrometheusExporter;

/// Every sample in the exposition, as `(name, labels, value)`.
fn samples() -> Vec<(String, String, f64)> {
    let mut exporter = PrometheusExporter::new("ironmon");
    exporter.collect_system_metrics();
    let text = exporter.export();

    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((head, value)) = line.rsplit_once(' ') else {
            continue;
        };
        let Ok(value) = value.parse::<f64>() else {
            continue;
        };
        match head.split_once('{') {
            Some((name, labels)) => out.push((
                name.to_string(),
                labels.trim_end_matches('}').to_string(),
                value,
            )),
            None => out.push((head.to_string(), String::new(), value)),
        }
    }
    out
}

/// The numeric value of one ontology reading, or `None` where it is absent.
fn reading(readings: &[ironmonlib::ontology::resolve::Reading], id: &str) -> Option<f64> {
    readings
        .iter()
        .find(|r| r.id == id)
        .and_then(|r| r.value.as_ref())
        .and_then(serde_json::Value::as_f64)
}

/// The label value for one key, e.g. `gpu="1"` -> `1`.
fn label<'a>(labels: &'a str, key: &str) -> Option<&'a str> {
    labels.split(',').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"'))
    })
}

/// Installed memory, swap size and VRAM, as both surfaces report them.
///
/// Exact equality, deliberately. These are capacities read from the same
/// hardware by two paths; a byte is a byte, and a tolerance here would hide
/// precisely the defects the test exists for — a `/ 1024` on one side, a `* 1000`
/// on the other, a reader that took "available" where the other took "total".
#[test]
fn the_exporter_and_the_ontology_agree_on_what_does_not_move() {
    let readings = ironmonlib::ontology::resolve::snapshot();
    let samples = samples();

    let mut compared = 0usize;
    let mut disagreements = Vec::new();

    let mut check = |name: &str, id: String, exported: f64| {
        let Some(resolved) = reading(&readings, &id) else {
            // The ontology reports this absent on this machine. That is a
            // reading in its own right and not a disagreement: the exporter
            // publishing a figure the ontology declines to is covered by the
            // gap lists in `prometheus_exposition.rs`.
            return;
        };
        compared += 1;
        if (exported - resolved).abs() > f64::EPSILON * resolved.abs().max(1.0) {
            disagreements.push(format!(
                "{name} = {exported} but {id} = {resolved} (difference {})",
                exported - resolved
            ));
        }
    };

    for (name, labels, value) in &samples {
        match name.as_str() {
            "ironmon_memory_total_bytes" => check(name, "memory.total".into(), *value),
            "ironmon_swap_total_bytes" => check(name, "memory.swap.total".into(), *value),
            "ironmon_gpu_memory_total_bytes" => {
                if let Some(index) = label(labels, "gpu") {
                    check(name, format!("gpu.{index}.memory.total"), *value);
                }
            }
            _ => {}
        }
    }

    assert!(
        disagreements.is_empty(),
        "the exporter and the ontology disagree about a quantity that cannot \
         have changed between two reads:\n  {}",
        disagreements.join("\n  ")
    );

    // A machine with no readable memory total would pass the assertion above
    // vacuously, which is the failure mode this crate has been bitten by twice.
    assert!(
        compared > 0,
        "no stable quantity was published by both surfaces, so this test \
         asserted nothing; either the exporter stopped emitting totals or the \
         ontology stopped resolving them"
    );
}
