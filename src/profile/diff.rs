//! Profile snapshot diff — compare two [`ProfileSnapshot`]s and surface
//! added / removed / changed settings.
//!
//! Critical for tracking drift after driver updates, BIOS flashes, or BIOS
//! resets. Works on any platform because it operates purely on the
//! serializable snapshot type.
//!
//! A value that could not be read on one side is not a change. A transition
//! to or from [`SettingValue::Unreadable`] is reported as `unread`, so a
//! transient read failure is not mistaken for drift. Groups are matched on
//! their stable [`ProfileGroup::key`] when one is set, so a device label that
//! gains or loses an "(unreadable)" suffix does not move the group to removed
//! and added.

use super::{ProfileGroup, ProfileSnapshot, Setting, SettingValue, Subsystem};
use serde::{Deserialize, Serialize};

/// Result of comparing two snapshots.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileDiff {
    /// Settings that exist in `current` but not `baseline`.
    pub added: Vec<DiffEntry>,
    /// Settings that exist in `baseline` but not `current`.
    pub removed: Vec<DiffEntry>,
    /// Settings whose value changed between baseline and current. Both sides
    /// were read.
    pub changed: Vec<DiffChange>,
    /// Settings where exactly one side could not be read. These are not
    /// changes: the reading on the other side may be the stale or missing one.
    #[serde(default)]
    pub unread: Vec<DiffChange>,
    /// Devices/groups added in `current`.
    pub added_groups: Vec<DiffGroup>,
    /// Devices/groups removed in `current`.
    pub removed_groups: Vec<DiffGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffEntry {
    pub subsystem: Subsystem,
    pub device: String,
    pub profile: String,
    pub setting_id: String,
    pub display_name: String,
    pub value: SettingValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffChange {
    pub subsystem: Subsystem,
    pub device: String,
    pub profile: String,
    pub setting_id: String,
    pub display_name: String,
    pub before: SettingValue,
    pub after: SettingValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffGroup {
    pub subsystem: Subsystem,
    pub device: String,
    pub profile: String,
    pub setting_count: usize,
}

impl ProfileDiff {
    /// True when no setting or group was added, removed or changed. Settings
    /// in [`ProfileDiff::unread`] are not counted: their values are unknown.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.added_groups.is_empty()
            && self.removed_groups.is_empty()
    }

    pub fn total_changes(&self) -> usize {
        self.added.len()
            + self.removed.len()
            + self.changed.len()
            + self.added_groups.len()
            + self.removed_groups.len()
    }
}

fn is_unread(v: &SettingValue) -> bool {
    matches!(v, SettingValue::Unreadable(_))
}

/// Compare two snapshots. The comparison key for a setting is
/// `(subsystem, group key, profile, setting_id)`. The group key is
/// [`ProfileGroup::key`] when set, otherwise the device name. Keys are derived
/// from hardware identifiers, not timestamps or read results.
pub fn diff_snapshots(baseline: &ProfileSnapshot, current: &ProfileSnapshot) -> ProfileDiff {
    let mut diff = ProfileDiff::default();

    let base_groups = flatten_groups(baseline);
    let cur_groups = flatten_groups(current);

    // Detect added/removed groups by (subsystem, key, profile).
    let base_keys: std::collections::BTreeSet<_> = base_groups.keys().cloned().collect();
    let cur_keys: std::collections::BTreeSet<_> = cur_groups.keys().cloned().collect();

    for key in cur_keys.difference(&base_keys) {
        let group = cur_groups[key];
        let device = group.device.clone();
        diff.added_groups.push(DiffGroup {
            subsystem: key.0,
            device: device.clone(),
            profile: key.2.clone(),
            setting_count: group.settings.len(),
        });
        for s in &group.settings {
            diff.added.push(DiffEntry {
                subsystem: key.0,
                device: device.clone(),
                profile: key.2.clone(),
                setting_id: s.id.clone(),
                display_name: s.display_name.clone(),
                value: s.value.clone(),
            });
        }
    }
    for key in base_keys.difference(&cur_keys) {
        let group = base_groups[key];
        let device = group.device.clone();
        diff.removed_groups.push(DiffGroup {
            subsystem: key.0,
            device: device.clone(),
            profile: key.2.clone(),
            setting_count: group.settings.len(),
        });
        for s in &group.settings {
            diff.removed.push(DiffEntry {
                subsystem: key.0,
                device: device.clone(),
                profile: key.2.clone(),
                setting_id: s.id.clone(),
                display_name: s.display_name.clone(),
                value: s.value.clone(),
            });
        }
    }

    // For groups present in both, diff individual settings. Labels come from
    // the current snapshot, so a renamed device shows its current name.
    for key in base_keys.intersection(&cur_keys) {
        let base = base_groups[key];
        let cur = cur_groups[key];
        let device = cur.device.clone();
        let base_settings: std::collections::HashMap<&str, &Setting> =
            base.settings.iter().map(|s| (s.id.as_str(), s)).collect();
        let cur_settings: std::collections::HashMap<&str, &Setting> =
            cur.settings.iter().map(|s| (s.id.as_str(), s)).collect();

        for (id, s) in &cur_settings {
            let Some(prev) = base_settings.get(id) else {
                diff.added.push(DiffEntry {
                    subsystem: key.0,
                    device: device.clone(),
                    profile: key.2.clone(),
                    setting_id: s.id.clone(),
                    display_name: s.display_name.clone(),
                    value: s.value.clone(),
                });
                continue;
            };
            let change = || DiffChange {
                subsystem: key.0,
                device: device.clone(),
                profile: key.2.clone(),
                setting_id: s.id.clone(),
                display_name: s.display_name.clone(),
                before: prev.value.clone(),
                after: s.value.clone(),
            };
            match (is_unread(&prev.value), is_unread(&s.value)) {
                // Both sides unreadable: the reason text is not a value change.
                (true, true) => {}
                // One side read, the other not: the value is unknown, not changed.
                (true, false) | (false, true) => diff.unread.push(change()),
                (false, false) if prev.value != s.value => diff.changed.push(change()),
                _ => {}
            }
        }
        for (id, s) in &base_settings {
            if !cur_settings.contains_key(id) {
                diff.removed.push(DiffEntry {
                    subsystem: key.0,
                    device: device.clone(),
                    profile: key.2.clone(),
                    setting_id: s.id.clone(),
                    display_name: s.display_name.clone(),
                    value: s.value.clone(),
                });
            }
        }
    }

    diff
}

type GroupKey = (Subsystem, String, String);

fn flatten_groups(snap: &ProfileSnapshot) -> std::collections::BTreeMap<GroupKey, &ProfileGroup> {
    let mut out = std::collections::BTreeMap::new();
    for (sub, groups) in &snap.providers {
        for group in groups {
            let identity = group.key.clone().unwrap_or_else(|| group.device.clone());
            out.insert((*sub, identity, group.display_name.clone()), group);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{ProfileGroup, Setting, SettingValue};
    use super::*;
    use std::collections::BTreeMap;

    fn make_snapshot(values: &[(&str, &str, &str, &str, SettingValue)]) -> ProfileSnapshot {
        let mut providers: BTreeMap<Subsystem, Vec<ProfileGroup>> = BTreeMap::new();
        for (sub, device, profile, id, value) in values {
            let sub = Subsystem::parse(sub).unwrap();
            let entry = providers.entry(sub).or_default();
            let group = entry
                .iter_mut()
                .find(|g| g.device == *device && g.display_name == *profile);
            let group = match group {
                Some(g) => g,
                None => {
                    entry.push(ProfileGroup::new(sub, *device, *profile, "test"));
                    entry.last_mut().unwrap()
                }
            };
            group.push(Setting::info(*id, *id, value.clone()));
        }
        ProfileSnapshot {
            timestamp: 0,
            providers,
            errors: BTreeMap::new(),
        }
    }

    /// One NVMe controller group with a stable key, labelled by `device`.
    fn nvme_snapshot(device: &str, settings: Vec<Setting>) -> ProfileSnapshot {
        let mut g = ProfileGroup::new(Subsystem::Nvme, device, "Controller features", "test")
            .with_key("nvme0");
        g.settings = settings;
        let mut providers = BTreeMap::new();
        providers.insert(Subsystem::Nvme, vec![g]);
        ProfileSnapshot {
            timestamp: 0,
            providers,
            errors: BTreeMap::new(),
        }
    }

    fn unread(id: &str) -> Setting {
        Setting::info(
            id,
            id,
            SettingValue::Unreadable("ioctl: permission denied".into()),
        )
    }

    #[test]
    fn detects_added_removed_changed() {
        let baseline = make_snapshot(&[
            ("gpu", "GPU0", "Global", "ecc", SettingValue::Bool(false)),
            ("gpu", "GPU0", "Global", "power", SettingValue::Uint(350)),
            ("gpu", "GPU0", "Global", "doomed", SettingValue::Bool(true)),
        ]);
        let current = make_snapshot(&[
            ("gpu", "GPU0", "Global", "ecc", SettingValue::Bool(false)), // unchanged
            ("gpu", "GPU0", "Global", "power", SettingValue::Uint(450)), // changed
            ("gpu", "GPU0", "Global", "new_key", SettingValue::Int(42)), // added
                                                                         // doomed removed
        ]);
        let diff = diff_snapshots(&baseline, &current);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].setting_id, "power");
    }

    #[test]
    fn detects_added_removed_group() {
        let baseline = make_snapshot(&[("gpu", "GPU0", "Global", "a", SettingValue::Bool(true))]);
        let current = make_snapshot(&[
            ("gpu", "GPU0", "Global", "a", SettingValue::Bool(true)),
            ("gpu", "GPU1", "Global", "x", SettingValue::Uint(1)),
        ]);
        let diff = diff_snapshots(&baseline, &current);
        assert_eq!(diff.added_groups.len(), 1);
        assert_eq!(diff.added_groups[0].device, "GPU1");
    }

    #[test]
    fn empty_diff_for_identical_snapshots() {
        let snap = make_snapshot(&[
            ("gpu", "GPU0", "Global", "a", SettingValue::Bool(true)),
            ("cpu", "CPU0", "Active", "pl1", SettingValue::Float(125.0)),
        ]);
        let diff = diff_snapshots(&snap, &snap);
        assert!(diff.is_empty());
        assert_eq!(diff.total_changes(), 0);
    }

    #[test]
    fn readable_to_unreadable_is_unread_not_changed() {
        let baseline = nvme_snapshot(
            "NVMe nvme0 — Samsung 990 Pro",
            vec![Setting::info(
                "fw",
                "Firmware",
                SettingValue::Text("4B2QGXA7".into()),
            )],
        );
        let current = nvme_snapshot("NVMe nvme0 — Samsung 990 Pro", vec![unread("fw")]);
        let diff = diff_snapshots(&baseline, &current);
        assert!(diff.changed.is_empty());
        assert_eq!(diff.unread.len(), 1);
        assert_eq!(diff.unread[0].setting_id, "fw");
        assert!(diff.is_empty(), "an unread value is not a change");
    }

    #[test]
    fn unreadable_to_readable_is_unread_not_changed() {
        let baseline = nvme_snapshot("NVMe nvme0", vec![unread("fw")]);
        let current = nvme_snapshot(
            "NVMe nvme0",
            vec![Setting::info(
                "fw",
                "fw",
                SettingValue::Text("4B2QGXA7".into()),
            )],
        );
        let diff = diff_snapshots(&baseline, &current);
        assert!(diff.changed.is_empty());
        assert_eq!(diff.unread.len(), 1);
    }

    #[test]
    fn two_different_unread_reasons_are_not_a_change() {
        let baseline = nvme_snapshot(
            "NVMe nvme0",
            vec![Setting::info(
                "fw",
                "fw",
                SettingValue::Unreadable("ioctl: permission denied".into()),
            )],
        );
        let current = nvme_snapshot(
            "NVMe nvme0",
            vec![Setting::info(
                "fw",
                "fw",
                SettingValue::Unreadable("ioctl: device busy".into()),
            )],
        );
        let diff = diff_snapshots(&baseline, &current);
        assert!(diff.changed.is_empty());
        assert!(diff.unread.is_empty());
    }

    #[test]
    fn label_change_with_same_key_is_not_a_removed_group() {
        // The model read failed on the second run, so the label gained an
        // "(model unreadable)" suffix. The group is the same controller.
        let baseline = nvme_snapshot(
            "NVMe nvme0 — Samsung 990 Pro",
            vec![Setting::info("n", "n", SettingValue::Uint(1))],
        );
        let current = nvme_snapshot(
            "NVMe nvme0 — (model unreadable)",
            vec![Setting::info("n", "n", SettingValue::Uint(2))],
        );
        let diff = diff_snapshots(&baseline, &current);
        assert!(diff.added_groups.is_empty());
        assert!(diff.removed_groups.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.added.is_empty());
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].device, "NVMe nvme0 — (model unreadable)");
    }
}
