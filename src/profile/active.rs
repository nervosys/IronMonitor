//! Active-profile detection — for each running process, determine whether
//! the NVIDIA DRS database (or, on Linux, any GPU profile source) has a
//! per-application profile that would apply to it.
//!
//! Answers the question "does NVIDIA Profile Inspector have a profile for
//! this process?" — useful for AI agents to suggest enabling DLSS, FPS
//! caps, etc. on apps that lack a tuned profile.

use serde::{Deserialize, Serialize};

/// One entry in an active-profiles report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveAppProfile {
    pub pid: u32,
    /// Process image name (e.g. "cyberpunk2077.exe"), case-preserved when
    /// available.
    pub name: String,
    /// Full executable path when reported by the OS.
    pub exe_path: Option<String>,
    /// Whether this process's image name appears in the NVIDIA DRS database.
    ///
    /// `Some(true)` / `Some(false)` is the answer from a DRS read. `None` means
    /// the DRS database was not checked (non-Windows, or the DRS directory is
    /// missing or unreadable). That is not the same as `Some(false)`: `None`
    /// must render as "not checked", never as "no profile".
    pub has_nvidia_drs_profile: Option<bool>,
}

impl ActiveAppProfile {
    /// True only when the DRS database was read and names this process.
    pub fn has_any_profile(&self) -> bool {
        self.has_nvidia_drs_profile == Some(true)
    }
}

/// Look up active per-application profiles for all running processes.
///
/// Reads the DRS string set once and joins against the process list. The
/// result is sorted: processes with a known profile first (most useful for
/// users), then alphabetically by name. A failed process listing is an error,
/// not an empty list.
pub fn active_profiles_for_processes() -> crate::error::Result<Vec<ActiveAppProfile>> {
    // `None` when DRS was not read. This used to be an empty set on every
    // non-Windows platform and whenever the DRS directory was missing, which
    // made every process look like it had no profile.
    let nvidia_set: Option<std::collections::BTreeSet<String>> = {
        #[cfg(windows)]
        {
            super::nvidia_drs::drs_exe_set()
        }
        #[cfg(not(windows))]
        {
            None
        }
    };

    let processes = crate::process_monitor::ProcessMonitor::new()?.processes()?;

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in processes {
        // Process name may be "foo.exe" already (Windows) or "foo" (Linux).
        // Normalize: lowercase + ensure .exe for comparison key.
        let lower = p.name.to_ascii_lowercase();
        let key = if lower.ends_with(".exe") {
            lower.clone()
        } else {
            format!("{}.exe", lower)
        };
        if !seen.insert((p.pid, key.clone())) {
            continue;
        }
        let has_nvidia_drs_profile = nvidia_set.as_ref().map(|set| set.contains(&key));
        out.push(ActiveAppProfile {
            pid: p.pid,
            name: p.name.clone(),
            exe_path: None, // Filled in by future work using a richer ProcessInfo field
            has_nvidia_drs_profile,
        });
    }
    out.sort_by(|a, b| {
        b.has_any_profile().cmp(&a.has_any_profile()).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });
    Ok(out)
}

/// Filter the active profile list to only entries with a known profile.
pub fn matched_active_profiles() -> crate::error::Result<Vec<ActiveAppProfile>> {
    Ok(active_profiles_for_processes()?
        .into_iter()
        .filter(ActiveAppProfile::has_any_profile)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        // Must not panic on any platform.
        let _ = active_profiles_for_processes();
        let _ = matched_active_profiles();
    }

    #[test]
    fn matched_subset_of_full() {
        let all = active_profiles_for_processes().unwrap();
        let matched = matched_active_profiles().unwrap();
        assert!(matched.len() <= all.len());
        for m in &matched {
            assert!(m.has_any_profile());
        }
    }

    #[test]
    fn unchecked_is_not_no_profile() {
        let unchecked = ActiveAppProfile {
            pid: 1,
            name: "game.exe".into(),
            exe_path: None,
            has_nvidia_drs_profile: None,
        };
        let no_profile = ActiveAppProfile {
            has_nvidia_drs_profile: Some(false),
            ..unchecked.clone()
        };
        assert!(!unchecked.has_any_profile());
        assert!(!no_profile.has_any_profile());
        assert_eq!(unchecked.has_nvidia_drs_profile, None);
        assert_eq!(no_profile.has_nvidia_drs_profile, Some(false));
    }
}
