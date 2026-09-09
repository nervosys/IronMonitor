//! What the `ai_api` tool surface actually returns.
//!
//! `tests/agentic_contract.rs` covers the argv surface — `ironmon describe`, `get`,
//! `snapshot` — and `tests/ontology_conformance.rs` covers readings. Neither
//! covers `AiDataApi::call_tool`, which is the surface an LLM driving ironmon
//! through MCP touches, and nothing else did either. Three fabrications lived
//! there undisturbed as a result: `uptime_seconds: 0` marked "would need
//! platform-specific impl" next to a reader that had one, a macOS CPU arm
//! publishing load average as `usage_percent`, and `unwrap_or_default()` turning
//! unread strings into `""`.
//!
//! These tests assert about *shape*, not about particular hardware, so they say
//! the same thing on a developer's desktop and on a CI runner with no GPU.

use ironmonlib::ai_api::AiDataApi;
use serde_json::Value;

fn api() -> AiDataApi {
    AiDataApi::new().expect("the tool surface must be constructible")
}

/// Walk a JSON value, calling `f` on every string with its path.
fn walk_strings(path: &str, v: &Value, f: &mut impl FnMut(&str, &str)) {
    match v {
        Value::String(s) => f(path, s),
        Value::Object(m) => {
            for (k, val) in m {
                walk_strings(&format!("{path}.{k}"), val, f);
            }
        }
        Value::Array(a) => {
            for (i, val) in a.iter().enumerate() {
                walk_strings(&format!("{path}[{i}]"), val, f);
            }
        }
        _ => {}
    }
}

/// Every tool that succeeds, with the data it returned.
fn successful_results() -> Vec<(String, Value)> {
    let mut api = api();
    let names: Vec<String> = api.list_tools().into_iter().map(|t| t.name).collect();
    assert!(
        !names.is_empty(),
        "the tool catalogue is empty, so nothing below tests anything"
    );

    names
        .into_iter()
        .filter_map(|name| {
            // A tool that needs parameters is allowed to refuse an empty call.
            // This is about what a *successful* result contains.
            let result = api.call_tool(&name, serde_json::json!({})).ok()?;
            let v = serde_json::to_value(&result).ok()?;
            if v.get("success").and_then(Value::as_bool) != Some(true) {
                return None;
            }
            Some((name, v.get("data").cloned().unwrap_or(Value::Null)))
        })
        .collect()
}

/// The absence-word guard, on the surface that lacked one.
///
/// `push_str_as` has rejected these strings in readings since the DMI "n/a"
/// finding, and `unknown_is_never_dressed_as_a_measurement` enforces it there.
/// The tool surface had no equivalent, and on the machine this was written on it
/// returned seventy of them: `get_usb_devices` reported `"speed": "Unknown"` for
/// thirty-odd devices and `get_bluetooth_devices` reported `"type": "Unknown"`
/// for nine, plus one empty `"address"`.
///
/// An agent cannot distinguish `"Unknown"` from a real value by looking at it —
/// that is the entire problem. `null` can be distinguished by anything.
#[test]
fn no_tool_returns_a_string_that_names_an_absence() {
    let mut bad: Vec<String> = Vec::new();

    for (name, data) in successful_results() {
        walk_strings(&name, &data, &mut |path, s| {
            if s.trim().is_empty() || ironmonlib::ontology::resolve::names_an_absence(s) {
                bad.push(format!("{path} = {s:?}"));
            }
        });
    }

    assert!(
        bad.is_empty(),
        "the tool surface returned {} value(s) naming an absence — these reach an \
         agent looking exactly like measurements. `call_tool` nulls these out at \
         the boundary, so a value arriving here means something bypassed it:\n{bad:#?}",
        bad.len()
    );
}

/// A tool that reports success has to have something to report.
///
/// `ToolResult::success` with `data: null` is a contradiction: the call did not
/// fail, so an agent will read the payload, and there is no payload. Either the
/// reading worked or it did not.
#[test]
fn a_successful_tool_call_carries_data() {
    let empty: Vec<String> = successful_results()
        .into_iter()
        .filter(|(_, data)| data.is_null())
        .map(|(name, _)| name)
        .collect();

    assert!(
        empty.is_empty(),
        "these tools reported success with no data: {empty:#?}"
    );
}

/// Every tool in the catalogue is reachable through `call_tool`.
///
/// The catalogue and the dispatch `match` are two hand-maintained lists of the
/// same names. A tool advertised but not wired answers "Unknown tool", which an
/// agent reads as its own mistake rather than as ironmon's.
#[test]
fn every_advertised_tool_is_dispatchable() {
    let mut api = api();
    let names: Vec<String> = api.list_tools().into_iter().map(|t| t.name).collect();

    let unreachable: Vec<String> = names
        .into_iter()
        .filter(|name| {
            let Ok(result) = api.call_tool(name, serde_json::json!({})) else {
                return false;
            };
            serde_json::to_value(&result)
                .ok()
                .and_then(|v| v.get("error").cloned())
                .and_then(|e| e.as_str().map(|s| s.contains("Unknown tool")))
                .unwrap_or(false)
        })
        .collect();

    assert!(
        unreachable.is_empty(),
        "advertised in the catalogue but not wired into `call_tool`: {unreachable:#?}"
    );
}

/// An identifier a listing hands out must work in the matching details tool.
///
/// This is the contract an agent relies on without being told: call the list,
/// take an id from a row, ask for detail on it. Nothing enforced it, and it was
/// silently false for USB after the device id scheme moved to the platform
/// device path — `get_usb_device_details` still looked devices up by
/// `bus_number` and `port_number`, which the Windows reader had stopped
/// filling. Thirty-nine devices collapsed to **one** addressable pair, so every
/// device but the first became unreachable through the tool while the listing
/// went on advertising them all.
///
/// A pairing whose listing is empty on this machine is skipped rather than
/// failed: no USB devices is a legitimate state for a container, and this test
/// is about self-consistency, not about the hardware present.
#[test]
fn an_id_from_a_listing_resolves_in_its_details_tool() {
    // (listing tool, path to the array, field holding the id, details tool,
    //  parameter name the details tool expects)
    let pairs: [(&str, &[&str], &str, &str, &str); 5] = [
        (
            "get_usb_devices",
            &[],
            "address",
            "get_usb_device_details",
            "address",
        ),
        (
            "get_display_list",
            &["displays"],
            "id",
            "get_display_details",
            "display_id",
        ),
        (
            "get_disk_list",
            &[],
            "name",
            "get_disk_details",
            "disk_name",
        ),
        (
            "get_network_interfaces",
            &[],
            "name",
            "get_interface_details",
            "interface_name",
        ),
        // The listing calls it `index`; the details tool wants `gpu_index`.
        ("get_gpu_list", &[], "index", "get_gpu_details", "gpu_index"),
    ];

    let mut api = api();
    let mut broken: Vec<String> = Vec::new();

    for (list_tool, path, id_field, detail_tool, param) in pairs {
        let Ok(listed) = api.call_tool(list_tool, serde_json::json!({})) else {
            continue;
        };
        let Ok(v) = serde_json::to_value(&listed) else {
            continue;
        };
        let mut node = match v.get("data") {
            Some(d) => d.clone(),
            None => continue,
        };
        for step in path {
            node = node.get(step).cloned().unwrap_or(Value::Null);
        }
        let Some(rows) = node.as_array() else {
            continue;
        };

        for row in rows {
            // An id is a string for some listings and a number for others --
            // `get_gpu_list` numbers its rows -- and both must pass through
            // unchanged, because the details tool parses what it is given.
            let Some(id) = row.get(id_field).filter(|v| v.is_string() || v.is_number()) else {
                broken.push(format!(
                    "{list_tool} returned a row with no usable `{id_field}`, so \
                     nothing can be asked about it: {row}"
                ));
                continue;
            };
            let Ok(detail) = api.call_tool(detail_tool, serde_json::json!({ param: id })) else {
                broken.push(format!("{detail_tool}({param}={id}) could not be called"));
                continue;
            };
            let ok = serde_json::to_value(&detail)
                .ok()
                .and_then(|d| d.get("success").and_then(Value::as_bool))
                .unwrap_or(false);
            if !ok {
                broken.push(format!(
                    "{list_tool} advertised {id_field}={id}, and \
                     {detail_tool} cannot resolve it"
                ));
            }
        }
    }

    assert!(
        broken.is_empty(),
        "a listing handed out identifiers its own details tool rejects. An agent \
         following the catalogue has no other way to name these things: {broken:#?}"
    );
}

/// The process listing is excluded from the pairing table above on purpose.
///
/// A pid is the one identifier that can stop being valid between the call that
/// hands it out and the call that uses it, so a table-driven check over every
/// row would fail whenever a listed process exited -- and a flaky test teaches
/// people to ignore it. This asserts the same contract against the one pid
/// guaranteed to still exist: the test's own.
#[test]
fn the_process_details_tool_resolves_a_live_pid() {
    let mut api = api();
    let pid = std::process::id();

    let result = api
        .call_tool("get_process_details", serde_json::json!({ "pid": pid }))
        .expect("get_process_details is dispatchable");
    let v = serde_json::to_value(&result).expect("a tool result serialises");

    assert_eq!(
        v.get("success").and_then(Value::as_bool),
        Some(true),
        "get_process_details could not resolve the running test's own pid, \
         which is the one process it can be certain exists: {v}"
    );
}
