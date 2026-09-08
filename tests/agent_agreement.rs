//! What the agent tool surface says, against what the ontology resolves.
//!
//! `AiDataApi::call_tool` is what an LLM driving simon through MCP reads, and it
//! is a *third* collection path over the same hardware — separate from both the
//! ontology resolver and the Prometheus exporters. `tests/ai_tool_surface.rs`
//! covers its shape: no absence words, no fabricated zeros, no empty strings.
//! Nothing has compared its **numbers** to anything.
//!
//! The same design as `tests/prometheus_agreement.rs`, for the same reason: only
//! quantities that cannot move between two reads are compared, because a
//! utilisation sampled twice differs for reasons that are not defects. Installed
//! memory, swap size, core counts and total VRAM do not change, so a difference
//! there is a unit, a scale, or two readers looking at different fields — which
//! is exactly how a 93.6 GiB machine came to report 98 MB of RAM through the
//! Prometheus exporter.
//!
//! **A missing field is a failure here, not a skip.** The first draft of this
//! file looked for `cores.physical` in `get_cpu_status`, which reports
//! `core_count`; the pair silently compared nothing and the test passed on the
//! memory figures alone. A path that has stopped existing is either a renamed
//! field or a dropped one, and both are worth a red test.

use serde_json::Value;
use simonlib::ai_api::AiDataApi;

/// The `data` of one tool call, or `None` where the tool declined.
///
/// A decline is legitimate — a machine with no GPU has no GPU tools — and is
/// distinguished below from a tool that answered without the field.
fn tool(name: &str) -> Option<Value> {
    let mut api = AiDataApi::new().ok()?;
    let result = api.call_tool(name, serde_json::json!({})).ok()?;
    let v = serde_json::to_value(&result).ok()?;
    (v.get("success").and_then(Value::as_bool) == Some(true))
        .then(|| v.get("data").cloned())
        .flatten()
}

fn reading(readings: &[simonlib::ontology::resolve::Reading], id: &str) -> Option<f64> {
    readings
        .iter()
        .find(|r| r.id == id)
        .and_then(|r| r.value.as_ref())
        .and_then(Value::as_f64)
}

/// Follow a dotted path into a JSON object.
fn at<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(v, |acc, key| acc.get(key))
}

/// Capacities and counts, as the tool surface and the ontology each report them.
#[test]
fn the_agent_surface_and_the_ontology_agree_on_what_does_not_move() {
    let readings = simonlib::ontology::resolve::snapshot();
    let mut compared = 0usize;
    let mut problems: Vec<String> = Vec::new();

    // (tool, path in its `data`, entity id, the tool's unit as a multiplier of
    // the ontology's). The megabyte figures are integer divisions of a byte
    // count, so they are compared to within the megabyte they were rounded
    // into; the counts are exact.
    let megabyte = 1024.0 * 1024.0;
    let pairs: [(&str, &str, &str, f64); 3] = [
        (
            "get_memory_status",
            "ram.total_mb",
            "memory.total",
            megabyte,
        ),
        (
            "get_memory_status",
            "swap.total_mb",
            "memory.swap.total",
            megabyte,
        ),
        // `read_cpu_stats` returns one entry per schedulable CPU, so this is
        // the logical count, not the physical one.
        ("get_cpu_status", "core_count", "cpu.cores.logical", 1.0),
    ];

    for (name, path, id, scale) in pairs {
        let Some(data) = tool(name) else {
            // The tool declined on this machine. Nothing to compare, and the
            // decline itself is `ai_tool_surface`'s business.
            continue;
        };
        let Some(tool_value) = at(&data, path).and_then(Value::as_f64) else {
            problems.push(format!(
                "{name} answered but reports no `{path}`; the field was renamed \
                 or dropped, and this comparison silently stopped happening"
            ));
            continue;
        };
        let Some(resolved) = reading(&readings, id) else {
            // The ontology reports this absent here, which is a reading in its
            // own right rather than a disagreement.
            continue;
        };

        compared += 1;
        let scaled = tool_value * scale;
        let ok = if scale > 1.0 {
            // Rounded down to whole units of `scale`: never above the byte
            // count, never a whole unit below it.
            scaled <= resolved && resolved - scaled < scale
        } else {
            scaled == resolved
        };
        if !ok {
            problems.push(format!(
                "{name}.{path} = {tool_value} (x{scale} = {scaled}) but {id} = {resolved}"
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "the agent tool surface and the ontology do not agree about quantities \
         that cannot have changed between two reads:\n  {}",
        problems.join("\n  ")
    );

    // On a machine where neither surface reports any of these, the assertion
    // above is vacuous and this says so rather than passing quietly.
    assert!(
        compared > 0,
        "no stable quantity was reported by both the tool surface and the \
         ontology, so this test asserted nothing"
    );
}
