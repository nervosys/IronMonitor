//! The join key for fleet rows, and why it is not a serial number.
//!
//! Fleet analytics needs a stable value to group a machine's rows by. The
//! obvious candidates are already on hand — a board UUID, a disk serial, a NIC
//! MAC — and this crate deliberately refuses every one of them. The ontology
//! draws the line at *describes* versus *identifies*: a GPU's model describes
//! the hardware and is published, while its UUID names one unit and is not.
//! Examples in this repository mask exactly these values, and a documentation
//! test fails if one reaches the docs.
//!
//! **A hardware identifier used as a fleet key would undo that in the one place
//! it matters most**, because the rows are the thing that leaves the machine.
//! So the key here is generated locally, at random, on first use: it identifies
//! a *participant in a fleet* rather than a piece of silicon. Two consequences
//! follow, and both are features.
//!
//! - **It is not derivable.** Nothing about the machine can be recovered from
//!   it, so a leaked table of host ids discloses no inventory.
//! - **It is rotatable.** [`HostId::rotate`] issues a new one and the operator
//!   gets a host that is genuinely new to the table. A serial cannot be
//!   rotated, which is precisely what makes it an identifier.
//!
//! The cost is honest and worth stating: reimaging a host loses its history
//! unless the file is preserved, and one disk image cloned to many machines
//! gives them all the same key. The second is the dangerous one, so
//! [`HostId::load_or_create`] records the hostname it was created under and
//! [`HostId::looks_cloned`] reports the mismatch rather than silently merging
//! two machines' metrics into one series.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A locally generated, rotatable identifier for one participant in a fleet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostId {
    /// 32 lowercase hex characters. Opaque by construction.
    id: String,
    /// The hostname this id was created under, kept only to detect a cloned
    /// disk image. Not used as the key and not a substitute for it: hostnames
    /// collide and change.
    created_as: Option<String>,
    /// Unix seconds, so a rotation shows up in the table as a new id appearing
    /// rather than an old one quietly changing meaning.
    created_at: u64,
}

impl HostId {
    /// The identifier itself.
    pub fn as_str(&self) -> &str {
        &self.id
    }

    /// When this identifier was issued, in Unix seconds.
    pub fn created_at(&self) -> u64 {
        self.created_at
    }

    /// Whether this host looks like a clone of the one that created the file.
    ///
    /// A disk image restored onto several machines carries the same id to all
    /// of them, and their metrics would then merge into one host's history with
    /// nothing looking wrong. Comparing the current hostname against the one
    /// recorded at creation catches the common case. It is a heuristic and says
    /// so: a legitimately renamed machine trips it too, which is why this
    /// reports rather than rotates.
    pub fn looks_cloned(&self, current_hostname: Option<&str>) -> bool {
        match (&self.created_as, current_hostname) {
            (Some(created), Some(current)) => created != current,
            // Nothing recorded, or nothing to compare against. No claim either
            // way, which is not the same as "no, this is fine".
            _ => false,
        }
    }

    /// Issue a fresh identifier, discarding this one.
    ///
    /// Older rows keep the old id. That is deliberate: rewriting them to point
    /// at the new one would assert the operator meant to continue the same
    /// series, and rotating is how an operator says the opposite.
    pub fn rotate(hostname: Option<&str>) -> Self {
        Self::generate(hostname)
    }

    fn generate(hostname: Option<&str>) -> Self {
        Self {
            id: random_hex_32(),
            created_as: hostname.map(str::to_owned),
            created_at: now_unix_seconds(),
        }
    }

    /// Read the identifier, creating and persisting one if there is none.
    ///
    /// Returns the identifier and whether it was newly created, because "this
    /// host appeared in the table for the first time" and "this host has been
    /// reporting for a year" are different facts, and a caller may want to log
    /// the first.
    pub fn load_or_create(path: &Path, hostname: Option<&str>) -> std::io::Result<(Self, bool)> {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(existing) = toml::from_str::<HostId>(&text) {
                return Ok((existing, false));
            }
            // A file that exists but does not parse is not overwritten. It may
            // be the only copy of an id that rows already reference, and losing
            // it splits one machine's history in two without saying so.
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "{} exists but is not a readable host id; move it aside to issue a new one",
                    path.display()
                ),
            ));
        }

        let fresh = Self::generate(hostname);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(&fresh)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, text)?;
        Ok((fresh, true))
    }
}

/// 128 bits of randomness, hex encoded.
///
/// Drawn from the operating system rather than a seeded generator, because two
/// machines imaged from one template and powered on together are exactly the
/// population a clock-seeded PRNG collides on — and that population is a fleet.
fn random_hex_32() -> String {
    let mut bytes = [0u8; 16];
    fill_random(&mut bytes);
    let mut out = String::with_capacity(32);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(unix)]
fn fill_random(buf: &mut [u8]) {
    use std::io::Read as _;
    // `/dev/urandom` rather than another dependency for sixteen bytes drawn
    // once per machine per lifetime.
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(buf).is_ok() {
            return;
        }
    }
    fill_random_fallback(buf);
}

#[cfg(windows)]
fn fill_random(buf: &mut [u8]) {
    // SAFETY: `ProcessPrng` fills exactly `buf.len()` bytes and does not retain
    // the pointer. It is documented as always succeeding on supported Windows,
    // and the fallback below covers the case where it somehow does not.
    let ok = unsafe { windows::Win32::Security::Cryptography::ProcessPrng(buf) };
    if ok.as_bool() {
        return;
    }
    fill_random_fallback(buf);
}

#[cfg(not(any(unix, windows)))]
fn fill_random(buf: &mut [u8]) {
    fill_random_fallback(buf);
}

/// Last resort when the operating system's source is unavailable.
///
/// **This is weaker, and a caller cannot tell from the returned id**, which is
/// the uncomfortable part of having it. It mixes the clock with two addresses
/// whose values depend on the allocator and on ASLR. It exists so an id can
/// always be issued: if the OS random source is missing, something is wrong
/// that a hardware monitor is not going to fix by refusing to start.
fn fill_random_fallback(buf: &mut [u8]) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let stack_addr = &nanos as *const u64 as u64;
    let boxed = Box::new(0u8);
    let heap_addr = &*boxed as *const u8 as u64;

    let mut state = nanos ^ stack_addr.rotate_left(17) ^ heap_addr.rotate_left(33);
    for chunk in buf.chunks_mut(8) {
        // SplitMix64.
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        let bytes = z.to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
}

fn now_unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Where the identifier lives, alongside the other state this crate keeps.
pub fn default_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.map(|b| b.join("ironmon").join("fleet_host_id.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("ironmon-hostid-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("temp dir");
        p
    }

    /// The identifier must survive a restart, or every reboot is a new machine.
    #[test]
    fn the_same_host_keeps_the_same_id_across_loads() {
        let dir = temp_dir("stable");
        let path = dir.join("id.toml");

        let (first, created) = HostId::load_or_create(&path, Some("host-a")).expect("create");
        assert!(created, "the first load must create it");

        let (second, created_again) = HostId::load_or_create(&path, Some("host-a")).expect("load");
        assert!(!created_again, "the second load must not mint a new one");
        assert_eq!(first, second);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two machines must not collide. The whole key rests on this.
    #[test]
    fn two_hosts_get_different_ids() {
        let a = HostId::rotate(Some("host-a"));
        let b = HostId::rotate(Some("host-b"));
        assert_ne!(a.as_str(), b.as_str());
        assert_eq!(a.as_str().len(), 32, "{}", a.as_str());
        assert!(a.as_str().chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// The id must carry nothing about the machine, or it is a hardware
    /// identifier with extra steps.
    #[test]
    fn the_id_does_not_contain_the_hostname_that_made_it() {
        let id = HostId::rotate(Some("workstation-01"));
        assert!(
            !id.as_str().contains("workstation"),
            "the identifier must not be derived from anything describing the machine"
        );
    }

    /// A cloned disk image is reported rather than silently merged.
    #[test]
    fn a_different_hostname_reads_as_a_possible_clone() {
        let id = HostId::rotate(Some("golden-image"));
        assert!(id.looks_cloned(Some("host-42")));
        assert!(!id.looks_cloned(Some("golden-image")));
        // Nothing to compare against is not evidence of anything.
        assert!(!id.looks_cloned(None));
    }

    /// Rotation must produce a genuinely new host, not a renamed old one.
    #[test]
    fn rotating_yields_an_unrelated_id() {
        let before = HostId::rotate(Some("host-a"));
        let after = HostId::rotate(Some("host-a"));
        assert_ne!(before.as_str(), after.as_str());
    }

    /// An unreadable file is a lost key, not a reason to mint a second one.
    #[test]
    fn a_corrupt_id_file_is_an_error_rather_than_a_new_identity() {
        let dir = temp_dir("corrupt");
        let path = dir.join("id.toml");
        std::fs::write(&path, "this is not toml = = =").expect("write");

        let err = HostId::load_or_create(&path, Some("host-a")).expect_err("must refuse");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
