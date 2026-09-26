//! What the TUI and GUI actually paint, against what the ontology resolves.
//!
//! The agreement tests beside this one compare the paths that *collect* --
//! the Prometheus exporters, the MCP tool surface, the chat agent's context --
//! against the ontology. The TUI and GUI collect nothing of their own; they
//! render the pipeline snapshot. But each converts units for display, and that
//! conversion is its own chance to be wrong. `tests/plausibility.rs` checks the
//! snapshot for self-consistency (used <= total), which a uniform unit error
//! passes, so nothing compared a rendered number to anything.
//!
//! Both renderers had defects there. The GUI's Memory tab computed "available"
//! as `free + buffers + cached`, which on Windows counts the standby list twice:
//! on the machine it was found on, it reported 96,868 MB available on a 95,890
//! MB machine. Its JSON and CSV exports published KB under `bytes`.
//!
//! These tests read the frame each renderer paints with `--frame` -- the text a
//! person sees -- rather than the library, for the reason `agentic_contract.rs`
//! gives: a library test would pass while the rendering was wrong.
//!
//! Only installed memory is compared, because it cannot move between two
//! reads. Used and available do move, so for those the check is internal: the
//! renderer's own used + available must equal its own total.

use std::process::Command;

fn ironmon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ironmon"))
}

/// Whether this platform has a memory reader. macOS's CPU and memory readers
/// feed the ontology but not yet the pipeline the renderers draw from; see the
/// same gate in `agentic_contract.rs`.
fn platform_has_hardware_readers() -> bool {
    cfg!(any(target_os = "linux", target_os = "windows"))
}

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

/// `memory.total` in bytes, as the ontology resolves it.
fn ontology_memory_total() -> Option<f64> {
    let (stdout, _, code) = run(&["get", "memory.total", "--format", "json"]);
    if code != 0 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&stdout).ok()?;
    v.get("value")?.as_f64()
}

/// The number that follows `label` on `line`, e.g. `Total: 93.64 GB` -> 93.64.
fn number_after(line: &str, label: &str) -> Option<f64> {
    let rest = &line[line.find(label)? + label.len()..];
    let token: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    token.parse().ok()
}

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MIB: f64 = 1024.0 * 1024.0;

/// The TUI's RAM line: `Total: 93.64 GB │ Used: 40.26 GB │ Available: 53.38 GB`.
///
/// "GB" there is GiB -- the figure is bytes / 1024^3 -- and it is printed to two
/// decimals, so it is compared to within 0.005 GiB either side.
#[test]
fn the_tui_memory_panel_agrees_with_the_ontology() {
    if !platform_has_hardware_readers() {
        return;
    }
    let (stdout, stderr, code) = run(&[
        "tui", "--frame", "--tab", "Memory", "--width", "160", "--height", "40",
    ]);
    assert_eq!(code, 0, "rendering the Memory tab failed:\n{stderr}");
    // A slow machine may give up waiting for the first snapshot; the command
    // says so, and that is the contract rather than a failure.
    if stderr.contains("no snapshot arrived") {
        return;
    }
    let Some(total_bytes) = ontology_memory_total() else {
        panic!("the ontology resolved no memory.total on a platform with a memory reader");
    };

    // The RAM line is the one that names "Available"; the swap line names "Free".
    let line = stdout
        .lines()
        .find(|l| l.contains("Total:") && l.contains("Available:"))
        .unwrap_or_else(|| panic!("no RAM line with Total and Available:\n{stdout}"));
    let total = number_after(line, "Total:").expect("Total");
    let used = number_after(line, "Used:").expect("Used");
    let available = number_after(line, "Available:").expect("Available");

    let expected = total_bytes / GIB;
    assert!(
        (total - expected).abs() <= 0.006,
        "the TUI shows {total} GiB total; the ontology says {expected:.3} GiB ({total_bytes} bytes)\n{line}"
    );
    assert!(
        (used + available - total).abs() <= 0.02,
        "the TUI's used + available ({used} + {available}) does not equal its total ({total})\n{line}"
    );
}

/// The GUI's Memory tab paints each figure on its own line, in MiB:
/// `Total` / `95890 MB`, `Used` / `41110 MB`, `Available` / `54780 MB`.
#[cfg(feature = "gui")]
#[test]
fn the_gui_memory_tab_agrees_with_the_ontology() {
    if !platform_has_hardware_readers() {
        return;
    }
    let (stdout, stderr, code) = run(&["gui", "--frame", "--tab", "memory"]);
    assert_eq!(code, 0, "rendering the GUI Memory tab failed:\n{stderr}");
    if stderr.contains("no snapshot arrived") {
        return;
    }
    let Some(total_bytes) = ontology_memory_total() else {
        panic!("the ontology resolved no memory.total on a platform with a memory reader");
    };

    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    // The value painted directly under an exact label. `Swap Total` and the
    // like do not match, because the label must be the whole line.
    let card = |label: &str| -> f64 {
        let i = lines
            .iter()
            .position(|l| *l == label)
            .unwrap_or_else(|| panic!("no `{label}` card on the Memory tab:\n{stdout}"));
        number_after(lines.get(i + 1).copied().unwrap_or(""), "")
            .unwrap_or_else(|| panic!("the `{label}` card has no number:\n{stdout}"))
    };
    let total = card("Total");
    let used = card("Used");
    let available = card("Available");

    // Painted as a whole number of MiB, so within half a MiB either side.
    let expected = total_bytes / MIB;
    assert!(
        (total - expected).abs() <= 0.6,
        "the GUI shows {total} MiB total; the ontology says {expected:.1} MiB ({total_bytes} bytes)"
    );
    // The direct guard on the double count: `free + buffers + cached` made this
    // sum 144% of installed RAM on the machine it was found on.
    assert!(
        (used + available - total).abs() <= 1.5,
        "the GUI's used + available ({used} + {available}) does not equal its total ({total})"
    );
}
