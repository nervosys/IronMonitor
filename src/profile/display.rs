//! Display profile provider — per-display refresh/HDR/color/scale.
//!
//! Builds on the existing [`crate::display::DisplayMonitor`]. Each connected
//! display is exposed as its own profile group with the actively-applied
//! mode + HDR/scale settings.

use super::{ProfileGroup, ProfileProvider, Setting, SettingRisk, SettingValue, Subsystem};

pub struct DisplayProfileProvider {
    _private: (),
}

impl DisplayProfileProvider {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for DisplayProfileProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileProvider for DisplayProfileProvider {
    fn subsystem(&self) -> Subsystem {
        Subsystem::Display
    }

    fn snapshot(&mut self) -> Vec<ProfileGroup> {
        let mut groups = super::edid::scan_edid_groups();
        let monitor = match crate::display::DisplayMonitor::new() {
            Ok(m) => m,
            Err(_) => return groups,
        };
        for d in monitor.displays() {
            let label = d.name.clone().unwrap_or_else(|| d.id.clone());
            let mut g = ProfileGroup::new(
                Subsystem::Display,
                &label,
                if d.is_primary == Some(true) {
                    "Primary display"
                } else {
                    "Display"
                },
                "DisplayMonitor",
            );
            if let Some(mfg) = &d.manufacturer {
                g.push(Setting::info(
                    "manufacturer",
                    "Manufacturer",
                    SettingValue::Text(mfg.clone()),
                ));
            }
            // Two of the three displays here report a connection the reader
            // cannot classify, and `{:?}` printed the literal word "Unknown" as
            // the value. The ontology already turns that into an absence with a
            // reason --- `push_str_as` refuses the word --- and this surface was
            // publishing it as though it were a kind of connector. Dropped
            // instead, the way every other row here behaves when it has nothing.
            let connection = format!("{:?}", d.connection);
            if !connection.eq_ignore_ascii_case("unknown") {
                g.push(Setting::info(
                    "connection",
                    "Connection",
                    SettingValue::Text(connection),
                ));
            }
            // "0x0" was published here as a resolution. A mode that was not
            // read gets no row, the same as the aspect ratio below.
            if let (Some(w), Some(h)) = (d.width, d.height) {
                g.push(Setting::info(
                    "resolution",
                    "Resolution",
                    SettingValue::Text(format!("{w}x{h}")),
                ));
            }
            if let Some(ratio) = d.aspect_ratio() {
                g.push(Setting::info(
                    "aspect_ratio",
                    "Aspect Ratio",
                    SettingValue::Text(ratio),
                ));
            }
            // The Linux and macOS readers do not parse a rate at all, so this
            // row said "0 Hz" on both platforms for every display.
            if let Some(hz) = d.refresh_rate {
                g.push(
                    Setting::info(
                        "refresh_rate_hz",
                        "Refresh Rate",
                        SettingValue::Float(hz as f64),
                    )
                    .with_unit("Hz")
                    .with_risk(SettingRisk::Safe)
                    .with_source("DisplayMonitor"),
                );
            }
            // Windows reports whether HDR is supported and on, but not which
            // format is in use, so those are their own rows and the mode stays
            // unreadable rather than being inferred from "on".
            for (key, label, value) in [
                ("hdr_supported", "HDR Supported", d.hdr_supported),
                ("hdr_enabled", "HDR Enabled", d.hdr_enabled),
            ] {
                g.push(
                    Setting::info(
                        key,
                        label,
                        match value {
                            Some(on) => SettingValue::Bool(on),
                            None => SettingValue::Unreadable(
                                "HDR state is not read on this platform".into(),
                            ),
                        },
                    )
                    .with_risk(SettingRisk::Safe),
                );
            }
            g.push(
                Setting::info(
                    "hdr_mode",
                    "HDR Mode",
                    // `None` is "not read", shown as unreadable rather than as a mode.
                    match d.hdr {
                        Some(mode) => SettingValue::Text(format!("{mode:?}")),
                        None => SettingValue::Unreadable(
                            "the HDR format in use (HDR10, Dolby Vision) is not reported".into(),
                        ),
                    },
                )
                .with_risk(SettingRisk::Safe),
            );
            if let Some(b) = d.brightness {
                g.push(
                    Setting::info("brightness", "Brightness", SettingValue::Float(b as f64))
                        .with_unit("%")
                        .with_risk(SettingRisk::Safe),
                );
            }
            if let Some(s) = d.scale_factor {
                g.push(
                    Setting::info("scale_factor", "DPI Scale", SettingValue::Float(s))
                        .with_risk(SettingRisk::Safe),
                );
            }
            if let Some(bpp) = d.bits_per_pixel {
                g.push(Setting::info(
                    "bpp",
                    "Bits Per Pixel",
                    SettingValue::Uint(bpp as u64),
                ));
            }
            if let (Some(w), Some(h)) = (d.physical_width_mm, d.physical_height_mm) {
                g.push(Setting::info(
                    "physical_mm",
                    "Physical Size",
                    SettingValue::Text(format!("{}×{} mm", w, h)),
                ));
            }
            // Only a read answer becomes a setting. An unread flag is left out
            // rather than shown as "false".
            if let Some(primary) = d.is_primary {
                g.push(Setting::info(
                    "is_primary",
                    "Primary",
                    SettingValue::Bool(primary),
                ));
            }
            groups.push(g);
        }
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smoke() {
        let mut p = DisplayProfileProvider::new();
        let _ = p.snapshot();
    }
}
