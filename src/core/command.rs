//! Running an external command without losing the reason it failed.
//!
//! Sixteen enumerators in this crate spawn a helper program — `powershell`,
//! `system_profiler`, `lspci`, `bluetoothctl` — and every one of them was
//! written in this shape:
//!
//! ```ignore
//! if let Ok(output) = Command::new("powershell").args([..]).output() {
//!     if let Ok(text) = String::from_utf8(output.stdout) {
//!         if let Ok(val) = serde_json::from_str::<Value>(&text) {
//! ```
//!
//! Three `if let Ok` with no `else` and no check of `output.status`. A spawn
//! failure, a non-zero exit, non-UTF-8 output and unparseable JSON all fell
//! through to the same place: an empty device list, returned as success.
//!
//! That matters because the ontology resolver reports an empty list as a fact
//! about the machine — `"no PCI devices enumerated on this machine"`,
//! `"no cameras detected"`. **The absence gets published with a reason, and the
//! reason names the hardware when the truth was about the process.** It was
//! found in `pci_devices` by a conformance test going red once under load, and
//! it is load-dependent by construction: rare on a developer's machine, rare in
//! CI, not rare on a busy host.

use crate::error::IronError;

/// Run `program` with `args` and return its stdout as text.
///
/// Returns `Err` — never an empty string — for a spawn failure, a non-zero
/// exit, or output that is not UTF-8. An `Ok("")` therefore means the program
/// ran, succeeded, and printed nothing, which is the only case a caller may
/// read as "there is nothing there".
pub fn capture(program: &str, args: &[&str]) -> Result<String, IronError> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| IronError::CommandFailed(format!("{program}: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(IronError::CommandFailed(if detail.is_empty() {
            format!("{program} exited {}", output.status)
        } else {
            format!("{program} exited {}: {detail}", output.status)
        }));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| IronError::Parse(format!("{program} output is not UTF-8: {e}")))
}

/// Run `program` and parse its stdout as JSON.
///
/// `Ok(None)` means the program printed nothing — PowerShell's
/// `ConvertTo-Json` prints nothing at all for an empty result set, so this is
/// the shape that genuinely means "no devices". Anything else that fails to
/// parse is an error, not an empty machine.
pub fn capture_json(program: &str, args: &[&str]) -> Result<Option<serde_json::Value>, IronError> {
    let text = capture(program, args)?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| IronError::Parse(format!("{program} output is not JSON: {e}")))
}

/// The items of a JSON document that is either one object or an array of them.
///
/// PowerShell's `ConvertTo-Json` emits a bare object when exactly one row
/// matched and an array when several did, so every caller of [`capture_json`]
/// needs this.
pub fn json_items(value: &serde_json::Value) -> Vec<serde_json::Value> {
    match value {
        serde_json::Value::Array(arr) => arr.clone(),
        obj @ serde_json::Value::Object(_) => vec![obj.clone()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The point of the helper: a program that does not exist is an error, not
    /// an empty answer. The old `if let Ok(output)` shape made these two the
    /// same value.
    #[test]
    fn a_missing_program_is_an_error_not_an_empty_result() {
        let err = capture("ironmon-no-such-program-exists", &[]).unwrap_err();
        assert!(
            err.to_string().contains("ironmon-no-such-program-exists"),
            "the error should name the program that failed: {err}"
        );
    }

    /// A non-zero exit is a failure even when the program printed to stdout
    /// first. A partial listing is not a listing.
    #[test]
    fn a_nonzero_exit_is_an_error() {
        #[cfg(target_os = "windows")]
        let (prog, args) = ("cmd", vec!["/C", "echo partial & exit 3"]);
        #[cfg(not(target_os = "windows"))]
        let (prog, args) = ("sh", vec!["-c", "echo partial; exit 3"]);

        let err = capture(prog, &args).unwrap_err();
        assert!(err.to_string().contains('3'), "{err}");
    }

    #[test]
    fn empty_output_is_no_devices_rather_than_a_parse_error() {
        #[cfg(target_os = "windows")]
        let (prog, args) = ("cmd", vec!["/C", "exit 0"]);
        #[cfg(not(target_os = "windows"))]
        let (prog, args) = ("sh", vec!["-c", "true"]);

        assert_eq!(capture_json(prog, &args).unwrap(), None);
    }

    #[test]
    fn one_object_and_an_array_of_one_both_yield_one_item() {
        let obj = serde_json::json!({"Name": "a"});
        let arr = serde_json::json!([{"Name": "a"}]);
        assert_eq!(json_items(&obj).len(), 1);
        assert_eq!(json_items(&arr).len(), 1);
        assert_eq!(json_items(&serde_json::json!(null)).len(), 0);
    }
}
