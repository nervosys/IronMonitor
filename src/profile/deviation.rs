//! Deviation report — every [`Setting`] with a declared `default` whose
//! current `value` differs from that default.
//!
//! Catches the "I changed something I forgot about" case after BIOS updates,
//! driver reinstalls, or NVPI tinkering. Pure inspection — no writes.

use super::{ProfileSnapshot, SettingValue, Subsystem};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deviation {
    pub subsystem: Subsystem,
    pub device: String,
    pub profile: String,
    pub setting_id: String,
    pub display_name: String,
    pub current: SettingValue,
    pub default: SettingValue,
    /// Risk classification copied from the underlying setting.
    pub risk: super::SettingRisk,
}

/// How much of a snapshot this report was actually able to look at.
///
/// A setting with no declared `default` cannot be compared to one, so it is
/// skipped. On this desktop **2 of 23,541 settings declare a default** — the
/// GPU provider alone contributes 23,445 of them, almost none with a documented
/// stock value — so a report that found nothing was reporting on 0.0085% of the
/// machine while `ironmon profile deviations` printed "No settings deviate from
/// their declared defaults", which reads as a statement about all of it.
///
/// Carrying the denominator makes the difference between *nothing differs* and
/// *almost nothing could be checked* visible to every caller, including the
/// JSON one.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DeviationCoverage {
    /// Settings in the snapshot.
    pub settings: usize,
    /// Settings that declare a default, and so could be compared.
    pub comparable: usize,
}

impl DeviationCoverage {
    /// Settings that declare no default, and about which nothing is known.
    pub fn unchecked(&self) -> usize {
        self.settings.saturating_sub(self.comparable)
    }
}

/// The deviations, with the coverage they were drawn from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviationReport {
    pub deviations: Vec<Deviation>,
    pub coverage: DeviationCoverage,
}

/// Walk a snapshot, collecting deviations and the coverage behind them.
pub fn deviation_report(snapshot: &ProfileSnapshot) -> DeviationReport {
    let mut settings = 0usize;
    let mut comparable = 0usize;
    for groups in snapshot.providers.values() {
        for group in groups {
            for s in &group.settings {
                settings += 1;
                if s.default.is_some() {
                    comparable += 1;
                }
            }
        }
    }
    DeviationReport {
        deviations: deviations_from_default(snapshot),
        coverage: DeviationCoverage {
            settings,
            comparable,
        },
    }
}

/// Walk a snapshot and collect every setting whose `value` differs from its
/// declared `default`. Settings without a default are skipped.
///
/// Prefer [`deviation_report`], which also says how many settings could be
/// compared at all — an empty result from this function alone cannot be told
/// apart from a snapshot nothing was comparable in.
pub fn deviations_from_default(snapshot: &ProfileSnapshot) -> Vec<Deviation> {
    let mut out = Vec::new();
    for (sub, groups) in &snapshot.providers {
        for group in groups {
            for s in &group.settings {
                let Some(default) = s.default.clone() else {
                    continue;
                };
                if default == s.value {
                    continue;
                }
                out.push(Deviation {
                    subsystem: *sub,
                    device: group.device.clone(),
                    profile: group.display_name.clone(),
                    setting_id: s.id.clone(),
                    display_name: s.display_name.clone(),
                    current: s.value.clone(),
                    default,
                    risk: s.risk,
                });
            }
        }
    }
    // Stable ordering: by risk (Dangerous first), then subsystem, device, id.
    out.sort_by(|a, b| {
        risk_rank(a.risk)
            .cmp(&risk_rank(b.risk))
            .then_with(|| a.subsystem.cmp(&b.subsystem))
            .then_with(|| a.device.cmp(&b.device))
            .then_with(|| a.setting_id.cmp(&b.setting_id))
    });
    out
}

fn risk_rank(r: super::SettingRisk) -> u8 {
    match r {
        super::SettingRisk::Dangerous => 0,
        super::SettingRisk::Moderate => 1,
        super::SettingRisk::Safe => 2,
        super::SettingRisk::Informational => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ProfileGroup, Setting, SettingRisk};
    use super::*;
    use std::collections::BTreeMap;

    fn snap_with(settings: Vec<Setting>) -> ProfileSnapshot {
        let mut group = ProfileGroup::new(Subsystem::Gpu, "GPU0", "Global", "test");
        group.settings = settings;
        let mut providers = BTreeMap::new();
        providers.insert(Subsystem::Gpu, vec![group]);
        ProfileSnapshot {
            timestamp: 0,
            providers,
            errors: BTreeMap::new(),
        }
    }

    /// An empty deviation list must not be mistaken for a machine at defaults.
    ///
    /// `ironmon profile deviations` printed "No settings deviate from their
    /// declared defaults" — a claim about the machine — while comparing **2 of
    /// 23,541 settings** on this desktop, because almost nothing the GPU
    /// provider enumerates declares a stock value. The other 23,539 were not
    /// checked, and the message did not say so.
    #[test]
    fn coverage_separates_nothing_differs_from_nothing_was_checked() {
        let mut compared = Setting::info("a", "A", SettingValue::Uint(1));
        compared.default = Some(SettingValue::Uint(1));

        let snap = snap_with(vec![
            compared,
            Setting::info("b", "B", SettingValue::Bool(true)),
            Setting::info("c", "C", SettingValue::Bool(false)),
        ]);

        let report = deviation_report(&snap);
        assert!(
            report.deviations.is_empty(),
            "the one comparable setting matches its default"
        );
        assert_eq!(report.coverage.settings, 3);
        assert_eq!(
            report.coverage.comparable, 1,
            "only one setting declares a default"
        );
        assert_eq!(
            report.coverage.unchecked(),
            2,
            "an empty report must be able to say how much it did not look at"
        );
    }

    /// A snapshot where nothing declares a default reports zero coverage rather
    /// than a clean bill of health.
    #[test]
    fn a_snapshot_with_no_defaults_is_wholly_unchecked() {
        let snap = snap_with(vec![
            Setting::info("a", "A", SettingValue::Bool(true)),
            Setting::info("b", "B", SettingValue::Bool(false)),
        ]);

        let report = deviation_report(&snap);
        assert!(report.deviations.is_empty());
        assert_eq!(report.coverage.comparable, 0);
        assert_eq!(report.coverage.unchecked(), 2);
    }

    #[test]
    fn no_default_no_deviation() {
        let snap = snap_with(vec![Setting::info("a", "A", SettingValue::Bool(true))]);
        assert!(deviations_from_default(&snap).is_empty());
    }

    #[test]
    fn matching_default_no_deviation() {
        let mut s = Setting::info("a", "A", SettingValue::Uint(450));
        s.default = Some(SettingValue::Uint(450));
        let snap = snap_with(vec![s]);
        assert!(deviations_from_default(&snap).is_empty());
    }

    #[test]
    fn mismatched_default_reports_deviation() {
        let mut s = Setting::info("power_limit", "Power Limit", SettingValue::Uint(350_000));
        s.default = Some(SettingValue::Uint(450_000));
        s.risk = SettingRisk::Moderate;
        let snap = snap_with(vec![s]);
        let devs = deviations_from_default(&snap);
        assert_eq!(devs.len(), 1);
        assert_eq!(devs[0].setting_id, "power_limit");
        assert!(matches!(devs[0].current, SettingValue::Uint(350_000)));
        assert!(matches!(devs[0].default, SettingValue::Uint(450_000)));
    }

    #[test]
    fn sorted_by_risk() {
        let mut s1 = Setting::info("safe_one", "S1", SettingValue::Bool(false));
        s1.default = Some(SettingValue::Bool(true));
        s1.risk = SettingRisk::Safe;
        let mut s2 = Setting::info("dangerous_one", "S2", SettingValue::Bool(false));
        s2.default = Some(SettingValue::Bool(true));
        s2.risk = SettingRisk::Dangerous;
        let snap = snap_with(vec![s1, s2]);
        let devs = deviations_from_default(&snap);
        assert_eq!(devs.len(), 2);
        assert_eq!(devs[0].setting_id, "dangerous_one"); // dangerous first
    }
}
