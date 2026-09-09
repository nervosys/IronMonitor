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

use ironmonlib::profile::active::{active_profiles_for_processes, matched_active_profiles};

fn main() {
    let only_matched = std::env::args().nth(1).as_deref() == Some("matched");
    let entries = if only_matched {
        matched_active_profiles()
    } else {
        active_profiles_for_processes()
    };
    let matched_count = entries.iter().filter(|e| e.has_any_profile()).count();
    println!(
        "{} of {} processes have a known NVIDIA driver profile",
        matched_count,
        entries.len()
    );
    println!();
    for e in entries {
        let marker = if e.has_nvidia_drs_profile {
            "✓"
        } else {
            "·"
        };
        println!("  {} {:>6}  {}", marker, e.pid, e.name);
    }
}
