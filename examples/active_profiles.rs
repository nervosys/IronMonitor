//! Show which running processes have an NVIDIA driver profile in the DRS
//! database — the same question NVIDIA Profile Inspector answers when you
//! pick a process from the dropdown.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release --features full --example active_profiles
//! cargo run --release --features full --example active_profiles -- matched
//! ```

use ironmonitor::profile::active::{active_profiles_for_processes, matched_active_profiles};

fn main() -> ironmonitor::Result<()> {
    let only_matched = std::env::args().nth(1).as_deref() == Some("matched");
    let entries = if only_matched {
        matched_active_profiles()?
    } else {
        active_profiles_for_processes()?
    };
    // `None` means the DRS database was not checked, which is not "no profile".
    let drs_checked = entries.iter().any(|e| e.has_nvidia_drs_profile.is_some());
    if drs_checked {
        let matched_count = entries.iter().filter(|e| e.has_any_profile()).count();
        println!(
            "{} of {} processes have a name referenced in the NVIDIA DRS database",
            matched_count,
            entries.len()
        );
    } else {
        println!(
            "NVIDIA driver profiles not checked (DRS database unavailable here); {} processes listed",
            entries.len()
        );
    }
    println!();
    for e in entries {
        let marker = match e.has_nvidia_drs_profile {
            Some(true) => "✓",
            Some(false) => "·",
            None => "?",
        };
        println!("  {} {:>6}  {}", marker, e.pid, e.name);
    }
    Ok(())
}
