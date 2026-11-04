//! `ironmon get <id>` runs only the readers that id needs. It must still answer exactly
//! as the same id in a full `ironmon snapshot` would.
//!
//! The risk this guards is a reading whose presence or provenance depends on a reader
//! outside its own domain: `resolve_inference` matches each engine's GPU against the
//! `gpu.{n}` readings, so a scoped `get` that skipped the GPU readers would report a
//! different answer. A test that only checked `get` against itself would not see that.
//!
//! These tests drive the built binary, because the argv surface is what an agent
//! calls. Volatile values (utilisation, process figures, counters) are not compared.
//! Provenance, unit, whether a value is present, and the note's wording are. Digits in
//! a note are masked, since a note may quote a count such as "443 processes exist".

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Mutex;

fn ironmon() -> Command {
    // No provider key reaches the child: a test must not call a real host with a
    // developer's credentials because it rendered the Inference tab.
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironmon"));
    for key in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "IRONMON_OPENAI_ADMIN_KEY",
        "IRONMON_ANTHROPIC_ADMIN_KEY",
    ] {
        command.env_remove(key);
    }
    command
}

/// Stdout, stderr and exit code of one invocation.
fn run(args: &[&str]) -> (String, String, i32) {
    let out = ironmon()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run `ironmon {}`: {e}", args.join(" ")));
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

/// Digits masked, so a count quoted in a note does not make two notes differ.
fn mask_digits(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect()
}

/// The first dotted segment of an id, which is the domain it belongs to.
fn domain_of(id: &str) -> &str {
    id.split('.').next().unwrap_or(id)
}

/// Every row of a full snapshot, keyed by id.
fn snapshot_rows() -> BTreeMap<String, serde_json::Value> {
    let (stdout, stderr, code) = run(&["snapshot", "--format", "json"]);
    assert_eq!(code, 0, "snapshot failed: {stderr}");
    let doc: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("snapshot did not emit JSON: {e}\n{stdout}"));
    doc["readings"]
        .as_array()
        .expect("snapshot carries a readings array")
        .iter()
        .map(|row| (row["id"].as_str().expect("id").to_string(), row.clone()))
        .collect()
}

/// Whether `get` answered with the same fact the snapshot row states.
fn same_fact(id: &str, got: &serde_json::Value, row: &serde_json::Value) -> Result<(), String> {
    let mut problems = Vec::new();
    if got["provenance"] != row["provenance"] {
        problems.push(format!(
            "provenance {} from get, {} from snapshot",
            got["provenance"], row["provenance"]
        ));
    }
    if got.get("unit") != row.get("unit") {
        problems.push(format!(
            "unit {:?} from get, {:?} from snapshot",
            got.get("unit"),
            row.get("unit")
        ));
    }
    if got.get("value").is_some() != row.get("value").is_some() {
        problems.push(format!(
            "value present={} from get, present={} from snapshot",
            got.get("value").is_some(),
            row.get("value").is_some()
        ));
    }
    let note = |v: &serde_json::Value| v["note"].as_str().map(mask_digits);
    if note(got) != note(row) {
        problems.push(format!(
            "note {:?} from get, {:?} from snapshot",
            note(got),
            note(row)
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{id}: {}", problems.join("; ")))
    }
}

/// One id per domain, plus that domain's diagnostic rows. Diagnostics matter most: they
/// are the rows that say a domain produced nothing or could not be read, and an agent
/// relies on them being readable by name.
fn sample(rows: &BTreeMap<String, serde_json::Value>) -> Vec<String> {
    let mut per_domain: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for id in rows.keys() {
        per_domain.entry(domain_of(id)).or_default().push(id);
    }
    let mut picked = Vec::new();
    for ids in per_domain.values() {
        let concrete = ids.iter().find(|id| !id.contains(['<', '{']));
        let diagnostic = ids.iter().find(|id| id.contains('<'));
        picked.extend(
            concrete
                .into_iter()
                .chain(diagnostic)
                .map(|id| id.to_string()),
        );
    }
    picked
}

/// Every sampled id, and every diagnostic row, read with `get` and compared with the
/// snapshot row for the same id.
#[test]
fn get_answers_each_sampled_id_as_the_snapshot_does() {
    let rows = snapshot_rows();
    assert!(!rows.is_empty(), "snapshot produced no rows");

    let mut ids = sample(&rows);
    // Every diagnostic row must be readable, not only the first per domain.
    let diagnostics: Vec<String> = rows
        .keys()
        .filter(|id| id.contains('<') && !ids.iter().any(|picked| picked == *id))
        .cloned()
        .collect();
    ids.extend(diagnostics);
    ids.sort();
    ids.dedup();

    // Reads run a few at a time. Each one is a process that reads only its domain, so
    // the total is bounded by the slowest domains rather than by a full snapshot each.
    let failures = Mutex::new(Vec::new());
    let queue = Mutex::new(ids.iter().collect::<Vec<_>>());
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| loop {
                let Some(id) = queue.lock().unwrap().pop() else {
                    break;
                };
                let row = &rows[id];
                let (stdout, stderr, code) = run(&["get", id, "--format", "json"]);
                let expected_code = if row["provenance"] == "unavailable" {
                    2
                } else {
                    0
                };
                if code != expected_code {
                    failures.lock().unwrap().push(format!(
                        "{id}: exit {code}, expected {expected_code} for a {} row; stderr: {stderr}",
                        row["provenance"]
                    ));
                    continue;
                }
                let got: serde_json::Value = match serde_json::from_str(&stdout) {
                    Ok(v) => v,
                    Err(e) => {
                        failures
                            .lock()
                            .unwrap()
                            .push(format!("{id}: get did not emit JSON ({e}): {stdout}"));
                        continue;
                    }
                };
                if let Err(problem) = same_fact(id, &got, row) {
                    failures.lock().unwrap().push(problem);
                }
            });
        }
    });

    let failures = failures.into_inner().unwrap();
    assert!(
        failures.is_empty(),
        "get disagrees with snapshot for {} of {} sampled ids:\n{}",
        failures.len(),
        ids.len(),
        failures.join("\n")
    );
}

/// Unknown ids, templates and instances that are not on this machine all exit 1. None
/// of them is a known id with no value, so none may exit 2.
#[test]
fn get_exits_1_for_ids_that_do_not_resolve() {
    let rows = snapshot_rows();

    let (_, stderr, code) = run(&["get", "definitely.not.an.entity"]);
    assert_eq!(code, 1, "an unknown id must exit 1: {stderr}");
    assert!(stderr.contains("Unknown entity"), "{stderr}");

    let (_, _, code) = run(&["get", "gpu.{n}.name"]);
    assert_eq!(code, 1, "a template id must exit 1");

    // An instance index this machine does not have keeps its current behaviour: it is
    // not in the snapshot, so `get` says it does not exist.
    let absent = "gpu.63.name";
    assert!(
        !rows.keys().any(|id| id.starts_with("gpu.63.")),
        "this machine has a gpu.63, so the probe id is not absent"
    );
    let (_, _, code) = run(&["get", absent]);
    assert_eq!(
        code, 1,
        "an absent instance must exit 1, as the snapshot has no such row"
    );
}
