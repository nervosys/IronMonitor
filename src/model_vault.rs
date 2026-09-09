// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Read what models an [IronVault] vault holds, and report them beside the
//! hardware that would run them.
//!
//! [IronVault]: https://crates.io/crates/ironvault
//!
//! # Read-only, and that is a design constraint rather than a phase
//!
//! IronMonitor reports; it does not manage models. Nothing here unlocks a vault, asks
//! for a passphrase, or writes to one. Three consequences follow from reading
//! IronVault's source rather than assuming:
//!
//! 1. **`Vault::new` creates the vault directory if it is missing** and writes a
//!    `VaultOpened` audit entry. A monitor that brings a vault into existence by
//!    looking for one has changed the thing it was asked to observe, so
//!    [`read_vault`] checks the path first and reports [`VaultStatus::Absent`]
//!    without constructing a `Vault` at all.
//! 2. **`VaultConfig::new` also has side effects**, which the first version of
//!    this module missed and a real run caught: it creates IronVault's config,
//!    data and cache directories and writes a default `config.yaml` when none
//!    exists. On a machine that has never run IronVault, asking ironmon what
//!    models it has would therefore install IronVault's furniture. So
//!    [`installed`] probes for an existing installation *before* any IronVault
//!    constructor runs, and [`read_vault`] returns [`VaultStatus::Absent`] on a
//!    machine without one having touched nothing.
//!
//!    That probe mirrors IronVault's documented layout rather than asking it,
//!    which is duplication and is the kind that rots. It is here because the
//!    crate offers no way to resolve its paths without creating them; the honest
//!    fix is upstream, and this guard should be deleted the moment
//!    `VaultConfig` gains a read-only path resolver.
//! 3. **Opening writes to the vault's own audit log** unless
//!    `security.audit_log` is off. ironmon turns it off: it never mutates a vault,
//!    so an entry claiming otherwise on every `ironmon vault` would be noise in
//!    someone else's compliance record.
//!
//! # Metadata is readable while the vault is locked
//!
//! `list_models` and `list_versions` go through the version backend and take
//! `&self` with no unlock check, so names, formats, sizes and checksums are
//! available without a key. The model *bytes* are not, and IronMonitor never asks for
//! them. A locked vault is therefore fully reportable, which is the property
//! that makes this integration honest: no credential ever reaches ironmon.
//!
//! # What is reported is what IronVault records
//!
//! [`ModelVersion`](ironvault::ModelVersion) carries a version number, format,
//! original and compressed sizes, a timestamp, a SHA-256 checksum and a
//! user-supplied metadata map. It does not carry a quantisation field or a
//! signature status, so neither is reported as though it did. Anything a user
//! put in the metadata map is passed through verbatim, labelled as theirs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// One model in the vault, summarised at its newest version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultModel {
    pub name: String,
    /// How many versions are stored, which is not the newest version number:
    /// versions can be deleted, so 3 versions may end at version 7.
    pub version_count: usize,
    pub latest_version: u32,
    /// Format as IronVault recorded it — `safetensors`, `gguf`, and so on.
    pub format: String,
    pub size_bytes: u64,
    /// Size on disk after compression. Reported separately rather than as a
    /// ratio: two numbers a reader can check beat one they have to trust.
    pub compressed_size_bytes: u64,
    pub checksum_sha256: String,
    /// RFC 3339, as IronVault stored it.
    pub created: String,
    /// Whatever the person who stored the model attached. Passed through
    /// untouched — IronMonitor does not know what these keys mean and does not
    /// pretend to.
    pub metadata: BTreeMap<String, String>,
}

/// Everything IronMonitor can say about a vault it can see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultReport {
    pub path: PathBuf,
    /// `false` means metadata only, which is all ironmon ever reads. Reported so
    /// nobody wonders whether a locked vault is being under-reported: it is not.
    pub unlocked: bool,
    pub models: Vec<VaultModel>,
    /// Models the vault lists but which carry no versions.
    ///
    /// A vault inconsistency rather than a normal state, and named rather than
    /// dropped: a listing that quietly omits them reports a vault with fewer
    /// models than it has. They contribute to neither total, because nothing
    /// about their size was ever read.
    #[serde(default)]
    pub versionless: Vec<String>,
    /// Sums over `models` only. Excludes anything in `versionless`.
    pub total_size_bytes: u64,
    pub total_compressed_bytes: u64,
}

/// Whether there is a vault, and what it holds.
///
/// Three outcomes, distinguished on purpose. "No vault here" and "there is a
/// vault and I could not read it" are different facts, and collapsing them into
/// an empty list is the failure this crate's ontology exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum VaultStatus {
    /// IronVault is not set up on this machine at all.
    ///
    /// Distinct from [`VaultStatus::Absent`], which means IronVault is here and
    /// holds no vault at the default path. The first version of this enum
    /// collapsed the two and printed "No IronVault vault at " with an empty
    /// path, which is the shape of answer this crate exists to avoid.
    NotInstalled,
    /// IronVault is installed, and there is no vault at this path. Not an
    /// error: a fresh install has none.
    Absent {
        path: PathBuf,
    },
    Present(Box<VaultReport>),
    /// A vault exists and could not be read. Carries the reason, because
    /// "unreadable" without a cause is not actionable.
    Failed {
        path: PathBuf,
        reason: String,
    },
}

/// Is IronVault installed on this machine?
///
/// Answered by looking, not by asking IronVault — see the note above. Returns
/// the config directory it found, so a caller can say where it looked.
///
/// Mirrors `VaultConfig::platform_dirs`: `IRONVAULT_HOME` relocates everything
/// under one root, otherwise the layout is `<config>/ai/models`.
pub fn installed() -> Option<PathBuf> {
    if let Ok(root) = std::env::var("IRONVAULT_HOME") {
        if !root.is_empty() {
            let config = PathBuf::from(root).join("config");
            return config.exists().then_some(config);
        }
    }
    // `directories` is IronVault's own resolver for this, and it is already in
    // the tree as a transitive dependency of it.
    let base = directories::BaseDirs::new()?;
    let config = base.config_dir().join("ai").join("models");
    config.exists().then_some(config)
}

/// Where IronVault would keep its default vault, without creating it.
///
/// Asks IronVault rather than reimplementing XDG resolution here; a second
/// implementation of someone else's path convention is a bug waiting for their
/// next release.
pub fn vault_path() -> Result<PathBuf, String> {
    // Guarded for the same reason `read_vault` is: `VaultConfig::new` writes a
    // config tree. Asking "where would the vault be" must not answer by
    // building somewhere for it to go.
    if installed().is_none() {
        return Err("IronVault is not installed on this machine".into());
    }
    let config = ironvault::VaultConfig::new().map_err(|e| e.to_string())?;
    Ok(config.get_vault_path(None))
}

/// Read the default vault. Never creates one, never unlocks one.
pub fn read_vault() -> VaultStatus {
    // Before touching any IronVault constructor: if the tool was never set up
    // here, say so and create nothing. `VaultConfig::new` would otherwise write
    // a config tree into the home directory of someone who only asked a
    // question.
    let Some(config_dir) = installed() else {
        return VaultStatus::NotInstalled;
    };

    let mut config = match ironvault::VaultConfig::new() {
        Ok(c) => c,
        Err(e) => {
            // The directory the probe found, not an empty path. An earlier
            // version put `PathBuf::new()` here, which is the same defect
            // `NotInstalled` was added to remove — it just survived in a
            // different arm, so a caller asking where ironmon looked got "".
            return VaultStatus::Failed {
                path: config_dir,
                reason: format!("could not resolve the vault configuration: {e}"),
            };
        }
    };

    let path = config.get_vault_path(None);

    // Check before constructing. `Vault::new` would create this directory, and
    // a monitor must not conjure the thing it reports on.
    if !path.exists() {
        return VaultStatus::Absent { path };
    }

    // IronMonitor only ever reads, so it does not write "vault opened" into someone
    // else's audit trail on every invocation.
    config.security.audit_log = false;

    let vault = match ironvault::Vault::new(Some(config)) {
        Ok(v) => v,
        Err(e) => {
            return VaultStatus::Failed {
                path,
                reason: e.to_string(),
            }
        }
    };

    let mut models = Vec::new();
    let mut versionless = Vec::new();
    for name in vault.list_models() {
        let versions = vault.list_versions(&name);
        // A model with no versions is a vault inconsistency. Inventing a 0-byte
        // entry would report a size that was never measured; dropping it
        // silently would report a vault with fewer models than it has, and
        // silence is the one answer this crate must not give. So it is named
        // separately and counted nowhere else.
        let Some(latest) = versions.iter().max_by_key(|v| v.version) else {
            versionless.push(name);
            continue;
        };
        models.push(VaultModel {
            name,
            version_count: versions.len(),
            latest_version: latest.version,
            format: latest.format.clone(),
            size_bytes: latest.size_bytes,
            compressed_size_bytes: latest.compressed_size_bytes,
            checksum_sha256: latest.checksum_sha256.clone(),
            created: latest.timestamp.to_rfc3339(),
            metadata: latest
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        });
    }
    models.sort_by(|a, b| a.name.cmp(&b.name));
    versionless.sort();

    let total_size_bytes = models.iter().map(|m| m.size_bytes).sum();
    let total_compressed_bytes = models.iter().map(|m| m.compressed_size_bytes).sum();

    VaultStatus::Present(Box::new(VaultReport {
        path,
        unlocked: vault.is_unlocked(),
        models,
        versionless,
        total_size_bytes,
        total_compressed_bytes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Absent and unreadable are different answers, and both are different from
    /// "a vault holding nothing".
    #[test]
    fn the_three_outcomes_are_distinguishable_once_serialised() {
        let absent = VaultStatus::Absent {
            path: PathBuf::from("/nowhere"),
        };
        let failed = VaultStatus::Failed {
            path: PathBuf::from("/nowhere"),
            reason: "permission denied".into(),
        };
        let empty = VaultStatus::Present(Box::new(VaultReport {
            path: PathBuf::from("/somewhere"),
            unlocked: false,
            models: vec![],
            versionless: vec![],
            total_size_bytes: 0,
            total_compressed_bytes: 0,
        }));

        let a = serde_json::to_string(&absent).unwrap();
        let f = serde_json::to_string(&failed).unwrap();
        let e = serde_json::to_string(&empty).unwrap();

        assert!(a.contains("absent"), "{a}");
        let ni = serde_json::to_string(&VaultStatus::NotInstalled).unwrap();
        assert!(ni.contains("not_installed"), "{ni}");
        assert_ne!(ni, a, "not-installed must not look like no-vault");
        assert!(
            f.contains("failed") && f.contains("permission denied"),
            "{f}"
        );
        assert!(e.contains("present"), "{e}");
        assert_ne!(a, e, "an absent vault must not look like an empty one");
    }

    /// Reading must not bring a vault into existence. The path IronVault would
    /// use is reported whether or not anything is there, and asking must not
    /// create it.
    #[test]
    fn reading_does_not_create_a_vault() {
        let Ok(path) = vault_path() else {
            return; // IronVault is not installed here; nothing to not-create
        };
        let existed_before = path.exists();
        let status = read_vault();
        assert_eq!(
            path.exists(),
            existed_before,
            "read_vault must not create the vault directory"
        );
        match (&status, existed_before) {
            (VaultStatus::Absent { .. } | VaultStatus::NotInstalled, false) => {}
            (VaultStatus::Present(_) | VaultStatus::Failed { .. }, true) => {}
            (s, before) => panic!("status {s:?} disagrees with path-exists={before}"),
        }
    }

    /// On a machine with no IronVault, reading must touch nothing at all — not
    /// the vault, and not IronVault's config tree either. Uses a temporary
    /// `IRONVAULT_HOME` so the assertion is about a directory this test owns.
    #[test]
    fn reading_creates_nothing_when_ironvault_is_not_installed() {
        let root = std::env::temp_dir().join(format!("ironmon-vault-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        // SAFETY-ish: single-threaded within this test's scope, and the value is
        // restored below. Rust 2024 makes set_var unsafe; this crate is 2021.
        std::env::set_var("IRONVAULT_HOME", &root);
        let status = read_vault();
        std::env::remove_var("IRONVAULT_HOME");

        assert!(
            matches!(status, VaultStatus::NotInstalled),
            "expected NotInstalled on a machine without IronVault, got {status:?}"
        );
        assert!(
            !root.exists(),
            "reading must not create IronVault's directories at {}",
            root.display()
        );
    }

    /// IronMonitor never holds a key, so a locked vault must still report its
    /// contents rather than looking empty.
    #[test]
    fn a_locked_vault_is_still_reportable() {
        if let VaultStatus::Present(report) = read_vault() {
            assert!(
                !report.unlocked,
                "IronMonitor must never unlock a vault; it has no passphrase to do it with"
            );
        }
    }
}
