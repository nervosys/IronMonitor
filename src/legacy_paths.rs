//! Adoption of files left behind by the `simon` → IronMonitor rename.
//!
//! The rename moved this crate's state from `<config>/simon/` to
//! `<config>/ironmon/`, which strands three files a user may already have: the
//! configuration, the consent record, and the profile audit log. Nothing read
//! the old location, so a rename that changed no behaviour still lost a user's
//! settings.
//!
//! **This is one module because there are three call sites.** Written per call
//! site it would be the same defect this crate has now recorded four times —
//! four parsers for `vm.swapusage`, three copies of one page size, eight copies
//! of one idle expression. The rule is to count the copies first.
//!
//! Two decisions worth keeping:
//!
//! - **Copy, never move.** The legacy file stays where it is. A user who
//!   downgrades still has it, and a migration that goes wrong has destroyed
//!   nothing. The cost is a duplicate file, which is cheap.
//! - **Consent is not adopted.** [`adopt`] is deliberately not called for
//!   `consent.toml`. Copying it would silently restore a privilege grant the
//!   user made to a program under a different name, and this crate's standing
//!   rule is that absent consent is asked for rather than assumed.
//!   [`legacy_consent_exists`] reports the file so a caller can *tell* the user
//!   it is there, which is the honest half of the same job.

use std::path::{Path, PathBuf};

/// The directory name this crate's state used before the rename.
const LEGACY_DIR: &str = "simon";

/// The directory name it uses now.
const CURRENT_DIR: &str = "ironmon";

/// What [`adopt`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Adoption {
    /// The current file already existed and was left alone. Adoption never
    /// overwrites: a file at the new path is the user's current state, whatever
    /// sits at the old one.
    AlreadyPresent,
    /// The legacy file was copied to the current path, which is returned.
    Adopted(PathBuf),
    /// Neither path holds a file, or the legacy path could not be derived.
    Nothing,
    /// A legacy file was found but could not be copied. Carries the reason.
    /// Adoption is best-effort — a caller that cannot copy still has a usable
    /// default — so this is reported rather than raised.
    Failed(String),
}

/// The path this file would have had before the rename, if it is one of ours.
///
/// Returns `None` when no path component is `ironmon`, so a caller that passes
/// an unrelated path gets nothing rather than a rewritten neighbour.
pub fn legacy_path(current: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut rewrote = false;

    for component in current.components() {
        if component.as_os_str() == CURRENT_DIR {
            out.push(LEGACY_DIR);
            rewrote = true;
        } else {
            out.push(component);
        }
    }

    rewrote.then_some(out)
}

/// Copy the pre-rename file into place if the current one is absent.
///
/// Best-effort and idempotent: calling it on every load is correct, because the
/// second call finds the file it created and reports [`Adoption::AlreadyPresent`].
pub fn adopt(current: &Path) -> Adoption {
    if current.exists() {
        return Adoption::AlreadyPresent;
    }

    let Some(legacy) = legacy_path(current) else {
        return Adoption::Nothing;
    };
    if !legacy.is_file() {
        return Adoption::Nothing;
    }

    if let Some(parent) = current.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Adoption::Failed(format!(
                "could not create {} to adopt {}: {e}",
                parent.display(),
                legacy.display()
            ));
        }
    }

    match std::fs::copy(&legacy, current) {
        Ok(_) => Adoption::Adopted(legacy),
        Err(e) => Adoption::Failed(format!(
            "could not copy {} to {}: {e}",
            legacy.display(),
            current.display()
        )),
    }
}

/// Whether a pre-rename consent record exists beside the current path.
///
/// Consent is reported, never adopted — see the module docs.
pub fn legacy_consent_exists(current: &Path) -> Option<PathBuf> {
    if current.exists() {
        return None;
    }
    legacy_path(current).filter(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ironmon-legacy-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_legacy_path_swaps_only_the_project_directory() {
        let current = Path::new("/home/u/.config/ironmon/config.toml");
        assert_eq!(
            legacy_path(current),
            Some(PathBuf::from("/home/u/.config/simon/config.toml"))
        );
    }

    #[test]
    fn a_path_that_is_not_ours_has_no_legacy_form() {
        // Nothing to rewrite, so nothing is claimed. A helper that guessed here
        // would happily point at a stranger's directory.
        assert_eq!(legacy_path(Path::new("/home/u/.config/other/x.toml")), None);
        assert_eq!(legacy_path(Path::new("/etc/passwd")), None);
    }

    #[test]
    fn a_filename_that_merely_contains_the_name_is_not_a_component() {
        // `ironmon_profile_audit.log` is a file name, not the project directory,
        // and rewriting it would invent a path nothing ever wrote.
        assert_eq!(
            legacy_path(Path::new("/var/lib/ironmon_profile_audit.log")),
            None
        );
    }

    #[test]
    fn an_existing_file_is_never_overwritten() {
        let dir = temp();
        let current = dir.join("ironmon").join("config.toml");
        let legacy = dir.join("simon").join("config.toml");
        fs::create_dir_all(current.parent().unwrap()).unwrap();
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&current, "current").unwrap();
        fs::write(&legacy, "legacy").unwrap();

        assert_eq!(adopt(&current), Adoption::AlreadyPresent);
        assert_eq!(fs::read_to_string(&current).unwrap(), "current");
    }

    #[test]
    fn a_stranded_file_is_adopted_and_the_original_is_left_in_place() {
        let dir = temp();
        let current = dir.join("ironmon").join("config.toml");
        let legacy = dir.join("simon").join("config.toml");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, "settings").unwrap();

        assert_eq!(adopt(&current), Adoption::Adopted(legacy.clone()));
        assert_eq!(fs::read_to_string(&current).unwrap(), "settings");
        assert!(
            legacy.is_file(),
            "adoption copies; a move would strand a downgrade"
        );
    }

    #[test]
    fn adoption_is_idempotent() {
        let dir = temp();
        let current = dir.join("ironmon").join("config.toml");
        let legacy = dir.join("simon").join("config.toml");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, "settings").unwrap();

        assert!(matches!(adopt(&current), Adoption::Adopted(_)));
        // The second call must find its own work rather than redo it, because
        // this runs on every load.
        assert_eq!(adopt(&current), Adoption::AlreadyPresent);
    }

    #[test]
    fn nothing_to_adopt_is_not_a_failure() {
        let dir = temp();
        let current = dir.join("ironmon").join("config.toml");
        assert_eq!(adopt(&current), Adoption::Nothing);
        assert!(!current.exists(), "adoption must not create an empty file");
    }

    #[test]
    fn a_legacy_directory_is_not_mistaken_for_a_file() {
        let dir = temp();
        let current = dir.join("ironmon").join("config.toml");
        let legacy = dir.join("simon").join("config.toml");
        // A *directory* at the legacy path is not a config file.
        fs::create_dir_all(&legacy).unwrap();
        assert_eq!(adopt(&current), Adoption::Nothing);
    }

    #[test]
    fn consent_is_reported_and_not_adopted() {
        let dir = temp();
        let current = dir.join("ironmon").join("consent.toml");
        let legacy = dir.join("simon").join("consent.toml");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, "granted = true").unwrap();

        assert_eq!(legacy_consent_exists(&current), Some(legacy));
        assert!(
            !current.exists(),
            "a privilege grant made under the old name must be re-asked, \
             not silently restored"
        );
    }

    #[test]
    fn consent_already_answered_reports_nothing() {
        let dir = temp();
        let current = dir.join("ironmon").join("consent.toml");
        fs::create_dir_all(current.parent().unwrap()).unwrap();
        fs::write(&current, "granted = false").unwrap();
        assert_eq!(legacy_consent_exists(&current), None);
    }
}
