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
    capture_with_timeout(program, args, DEFAULT_TIMEOUT)
}

/// How long a helper program gets before it is killed and reported as hung.
///
/// **A reader that waits forever is worse than one that fails**, and this crate
/// had sixteen of them. `bluetoothctl devices` blocks indefinitely on a Linux
/// host where the binary is installed but `bluetoothd` is not answering: it
/// waits on D-Bus, and closing its stdin does not release it. One
/// `ironmon snapshot` never returned, and a `cargo test` run left twelve
/// `bluetoothctl` processes alive behind it.
///
/// CI never saw this. GitHub's runners do not install `bluetoothctl`, so the
/// spawn fails immediately and the reader reports an honest absence — the green
/// pipeline was measuring a machine where the bug cannot occur. That is the
/// same instrument this file already warns about twice.
///
/// Ten seconds is chosen against the slowest legitimate caller, which is a cold
/// PowerShell start plus a WMI query, not against the fastest.
const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// [`capture`], with an explicit bound on how long the program may take.
pub fn capture_with_timeout(
    program: &str,
    args: &[&str],
    timeout: std::time::Duration,
) -> Result<String, IronError> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut child = Command::new(program)
        .args(args)
        // `output()` nulls stdin already; this spawn does it explicitly so the
        // guarantee survives the switch away from `output()`. It is not what
        // fixes the hang — `bluetoothctl` waits on D-Bus, and an already-null
        // stdin never released it.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| IronError::CommandFailed(format!("{program}: {e}")))?;

    // Drain both pipes on their own threads. Waiting on the child while its
    // stdout pipe fills is its own deadlock, and it does not need a missing
    // daemon to happen — just a chatty program.
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });

    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    // Kill it and reap it, so the process does not outlive the
                    // reader that gave up on it.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(IronError::CommandFailed(format!(
                        "{program} did not finish within {}s and was killed",
                        timeout.as_secs()
                    )));
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(IronError::CommandFailed(format!(
                    "{program}: could not wait for it: {e}"
                )));
            }
        }
    };

    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    let output = std::process::Output {
        status,
        stdout,
        stderr,
    };

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

    /// **A reader that waits forever is worse than one that fails.** This is the
    /// guardrail for the defect that made `ironmon` hang on Linux:
    /// `bluetoothctl devices` never returns when `bluetoothd` is not answering,
    /// and `capture` had no deadline. Without this test the bound is a comment.
    #[test]
    fn a_program_that_never_exits_is_killed_and_reported() {
        #[cfg(target_os = "windows")]
        let (prog, args) = ("cmd", vec!["/C", "ping -n 30 127.0.0.1 > NUL"]);
        #[cfg(not(target_os = "windows"))]
        let (prog, args) = ("sh", vec!["-c", "sleep 30"]);

        let start = std::time::Instant::now();
        let err = capture_with_timeout(prog, &args, std::time::Duration::from_millis(300))
            .expect_err("a program that outlives its deadline must be an error");
        let waited = start.elapsed();

        assert!(
            err.to_string().contains("did not finish"),
            "the error must say it timed out rather than blaming the machine: {err}"
        );
        // Generous, because CI machines are slow — but far below the 30s the
        // child would have taken, which is the only thing being asserted.
        assert!(
            waited < std::time::Duration::from_secs(10),
            "gave up after {waited:?}, which is not a deadline"
        );
    }

    /// The bound must not fire on a program that answers, or every reader on a
    /// loaded machine becomes an absence.
    #[test]
    fn a_program_that_answers_in_time_is_not_killed() {
        #[cfg(target_os = "windows")]
        let (prog, args) = ("cmd", vec!["/C", "echo ready"]);
        #[cfg(not(target_os = "windows"))]
        let (prog, args) = ("sh", vec!["-c", "echo ready"]);

        let out = capture_with_timeout(prog, &args, std::time::Duration::from_secs(30))
            .expect("a fast program must succeed");
        assert!(out.contains("ready"), "{out:?}");
    }

    /// Output larger than a pipe buffer must not deadlock the wait. This is why
    /// both pipes are drained on their own threads: a child blocked writing to a
    /// full stdout pipe never exits, and the parent polling `try_wait` would
    /// wait for it until the deadline and call a working program hung.
    #[test]
    fn output_larger_than_a_pipe_buffer_is_captured_whole() {
        // 512 KiB, comfortably past the 64 KiB pipe buffer on both platforms.
        #[cfg(target_os = "windows")]
        let (prog, args) = ("powershell", vec!["-NoProfile", "-Command", "'x' * 524288"]);
        #[cfg(not(target_os = "windows"))]
        let (prog, args) = ("sh", vec!["-c", "yes x | head -c 524288"]);

        let out = capture_with_timeout(prog, &args, std::time::Duration::from_secs(60))
            .expect("a chatty program must not deadlock the wait");
        assert!(
            out.len() >= 524_288,
            "captured only {} bytes; the pipe was not drained",
            out.len()
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
