//! EDID 1.x block decoder.
//!
//! EDID (Extended Display Identification Data) is the 128-byte block monitors
//! return over DDC. The header at byte 0 is the fixed magic
//! `00 FF FF FF FF FF FF 00`. Subsequent fields encode manufacturer (PNP
//! code), product code, manufacture date, EDID version, established and
//! standard timings, four detailed timing descriptors, and an extension
//! count.
//!
//! Sources:
//! - Linux: `/sys/class/drm/card*-*/edid` (binary blob, 128 or 256+ bytes
//!   depending on extension blocks).
//! - Windows: SetupDi / `MonitorManufacturer`/`EDID` registry value under
//!   `HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\*\*\Device Parameters\EDID`.

use super::{ProfileGroup, Setting, SettingRisk, SettingValue, Subsystem};

const EDID_MAGIC: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

/// Scan platform-specific sources and return one [`ProfileGroup`] per EDID
/// block found.
pub fn scan_edid_groups() -> Vec<ProfileGroup> {
    let mut groups = Vec::new();

    #[cfg(target_os = "linux")]
    linux_scan(&mut groups);

    #[cfg(windows)]
    windows_scan(&mut groups);

    groups
}

#[cfg(target_os = "linux")]
fn linux_scan(groups: &mut Vec<ProfileGroup>) {
    use std::fs;
    let Ok(rd) = fs::read_dir("/sys/class/drm") else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let edid = path.join("edid");
        if !edid.exists() {
            continue;
        }
        let Ok(bytes) = fs::read(&edid) else { continue };
        if let Some(group) = decode_block(
            &bytes,
            edid.display().to_string(),
            entry.file_name().to_string_lossy().to_string(),
        ) {
            groups.push(group);
        }
    }
}

#[cfg(windows)]
fn windows_scan(groups: &mut Vec<ProfileGroup>) {
    use winreg::enums::*;
    use winreg::RegKey;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let Ok(display) = hklm.open_subkey(r"SYSTEM\CurrentControlSet\Enum\DISPLAY") else {
        return;
    };
    for mfg in display.enum_keys().flatten() {
        let Ok(mfg_key) = display.open_subkey(&mfg) else {
            continue;
        };
        for inst in mfg_key.enum_keys().flatten() {
            let Ok(dev) = mfg_key.open_subkey(format!("{}\\Device Parameters", inst)) else {
                continue;
            };
            let edid: Result<Vec<u8>, _> = dev.get_raw_value("EDID").map(|v| v.bytes);
            if let Ok(bytes) = edid {
                let label = format!("{}/{}", mfg, inst);
                if let Some(group) = decode_block(
                    &bytes,
                    format!(r"HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\{}", label),
                    label,
                ) {
                    groups.push(group);
                }
            }
        }
    }
}

pub fn decode_block(bytes: &[u8], source: String, locator: String) -> Option<ProfileGroup> {
    if bytes.len() < 128 {
        return None;
    }
    if bytes[..8] != EDID_MAGIC {
        return None;
    }

    let mfg = decode_pnp(&bytes[8..10]);
    let product_code = u16::from_le_bytes([bytes[10], bytes[11]]);
    let serial_id = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    let week = bytes[16];
    let year = bytes[17] as u16 + 1990;
    let edid_major = bytes[18];
    let edid_minor = bytes[19];
    let video_input = bytes[20];
    let digital = (video_input & 0x80) != 0;
    let max_h_cm = bytes[21];
    let max_v_cm = bytes[22];
    let gamma_raw = bytes[23];
    let gamma = (gamma_raw as f64 + 100.0) / 100.0;
    let features = bytes[24];
    let dpms_standby = (features & 0x80) != 0;
    let dpms_suspend = (features & 0x40) != 0;
    let dpms_active_off = (features & 0x20) != 0;
    let display_type = (features >> 3) & 0x3;
    let display_type_str = if digital {
        match display_type {
            0 => "RGB 4:4:4",
            1 => "RGB 4:4:4 + YCrCb 4:4:4",
            2 => "RGB 4:4:4 + YCrCb 4:2:2",
            _ => "RGB 4:4:4 + YCrCb 4:4:4 + YCrCb 4:2:2",
        }
    } else {
        match display_type {
            0 => "Monochrome",
            1 => "RGB color",
            2 => "Non-RGB color",
            _ => "Undefined",
        }
    };
    let monitor_name = read_monitor_name_descriptor(bytes);
    let preferred = read_preferred_timing(bytes);

    let label = monitor_name
        .clone()
        .unwrap_or_else(|| format!("{} {:04X}", mfg, product_code));
    let mut group = ProfileGroup::new(
        Subsystem::Display,
        format!("EDID: {} — {}", locator, label),
        format!("EDID {}.{}", edid_major, edid_minor),
        source,
    );

    group.push(Setting::info(
        "edid.manufacturer",
        "Manufacturer (PNP)",
        SettingValue::Text(mfg),
    ));
    group.push(Setting::info(
        "edid.product_code",
        "Product Code",
        SettingValue::Uint(product_code as u64),
    ));
    if serial_id != 0 {
        group.push(Setting::info(
            "edid.serial_id",
            "Serial ID",
            SettingValue::Uint(serial_id as u64),
        ));
    }
    if let Some(name) = monitor_name {
        group.push(Setting::info(
            "edid.monitor_name",
            "Monitor Name",
            SettingValue::Text(name),
        ));
    }
    if let Some(text) = manufactured_text(week, year) {
        group.push(Setting::info(
            "edid.manufactured",
            "Manufactured",
            SettingValue::Text(text),
        ));
    }
    group.push(Setting::info(
        "edid.signal",
        "Video Signal",
        SettingValue::Text(if digital {
            "Digital".into()
        } else {
            "Analog".into()
        }),
    ));
    group.push(Setting::info(
        "edid.display_type",
        "Display Type",
        SettingValue::Text(display_type_str.into()),
    ));
    if max_h_cm > 0 && max_v_cm > 0 {
        group.push(Setting::info(
            "edid.max_image_size",
            "Max Image Size",
            SettingValue::Text(format!("{} × {} cm", max_h_cm, max_v_cm)),
        ));
    }
    if gamma_raw != 0xFF {
        group.push(
            Setting::info("edid.gamma", "Gamma", SettingValue::Float(gamma))
                .with_description("Display transfer characteristic. 2.2 is sRGB."),
        );
    }
    group.push(Setting::info(
        "edid.dpms_standby",
        "DPMS Standby Supported",
        SettingValue::Bool(dpms_standby),
    ));
    group.push(Setting::info(
        "edid.dpms_suspend",
        "DPMS Suspend Supported",
        SettingValue::Bool(dpms_suspend),
    ));
    group.push(Setting::info(
        "edid.dpms_active_off",
        "DPMS Active-Off Supported",
        SettingValue::Bool(dpms_active_off),
    ));
    if let Some((h, v, refresh_hz, interlaced)) = preferred {
        group.push(
            Setting::info(
                "edid.preferred_timing",
                "Preferred Native Timing",
                SettingValue::Text(format!(
                    "{}×{}{} @ {:.2} Hz",
                    h,
                    v,
                    if interlaced { "i" } else { "" },
                    refresh_hz
                )),
            )
            .with_description(concat!(
                "First Detailed Timing Descriptor — manufacturer's ",
                "recommended mode. An `i` marks an interlaced mode, whose ",
                "line count the descriptor stores per field and which is ",
                "doubled here to whole frames."
            ))
            .with_risk(SettingRisk::Safe),
        );
    }
    let ext_count = bytes[126];
    group.push(Setting::info(
        "edid.extension_blocks",
        "Extension Blocks",
        SettingValue::Uint(ext_count as u64),
    ));
    Some(group)
}

/// Decode the 3-letter PNP manufacturer code from two big-endian bytes.
/// Each letter is a 5-bit value with 'A' = 1.
fn decode_pnp(b: &[u8]) -> String {
    if b.len() < 2 {
        return "???".into();
    }
    let word = ((b[0] as u16) << 8) | b[1] as u16;
    let a = ((word >> 10) & 0x1F) as u8;
    let b2 = ((word >> 5) & 0x1F) as u8;
    let c = (word & 0x1F) as u8;
    fn ch(v: u8) -> char {
        if v == 0 {
            '?'
        } else {
            (b'A' + v - 1) as char
        }
    }
    format!("{}{}{}", ch(a), ch(b2), ch(c))
}

/// Walk the four 18-byte detailed-timing/monitor-descriptor blocks at
/// offsets 54..72..90..108 and return the monitor name (descriptor type
/// `0xFC`) if present.
fn read_monitor_name_descriptor(bytes: &[u8]) -> Option<String> {
    for off in [54usize, 72, 90, 108] {
        if off + 18 > bytes.len() {
            continue;
        }
        let block = &bytes[off..off + 18];
        // A descriptor (not a detailed timing) has first two bytes = 00 00.
        if block[0] == 0 && block[1] == 0 && block[3] == 0xFC {
            let raw = &block[5..18];
            let mut s = String::new();
            for &b in raw {
                if b == 0x0A {
                    break;
                }
                if (0x20..0x7F).contains(&b) {
                    s.push(b as char);
                }
            }
            let trimmed = s.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

/// Decode the first Detailed Timing Descriptor at offset 54 (the
/// manufacturer-preferred / native mode). Returns (h_active, v_active, refresh_hz).
/// The first Detailed Timing Descriptor: active pixels, refresh, and whether the
/// mode is interlaced.
///
/// The interlace flag is not decoration. For an interlaced timing the spec stores
/// **vertical active as lines per field**, so a 1080i mode carries 540 in the
/// DTD. Ignoring the flag reported an AV receiver on this machine as
/// `1920×540 @ 60.05 Hz` — a resolution no equipment has — where the real mode
/// is 1920×1080 interlaced at a 60 Hz field rate. The frame lines are what a
/// reader means by a resolution, so they are what is returned, with the flag
/// alongside so the caller can say which it is.
fn read_preferred_timing(bytes: &[u8]) -> Option<(u32, u32, f64, bool)> {
    if bytes.len() < 72 {
        return None;
    }
    let dtd = &bytes[54..72];
    // Bytes 0-1: pixel clock in 10 kHz units (little-endian). 0 => monitor descriptor.
    let pixel_clock_khz = (u16::from_le_bytes([dtd[0], dtd[1]]) as u32) * 10;
    if pixel_clock_khz == 0 {
        return None;
    }
    let h_active_lo = dtd[2] as u32;
    let h_blank_lo = dtd[3] as u32;
    let h_active_hi = ((dtd[4] >> 4) & 0xF) as u32;
    let h_blank_hi = (dtd[4] & 0xF) as u32;
    let h_active = (h_active_hi << 8) | h_active_lo;
    let h_blank = (h_blank_hi << 8) | h_blank_lo;

    let v_active_lo = dtd[5] as u32;
    let v_blank_lo = dtd[6] as u32;
    let v_active_hi = ((dtd[7] >> 4) & 0xF) as u32;
    let v_blank_hi = (dtd[7] & 0xF) as u32;
    let v_active = (v_active_hi << 8) | v_active_lo;
    let v_blank = (v_blank_hi << 8) | v_blank_lo;

    let h_total = h_active + h_blank;
    let v_total = v_active + v_blank;
    if h_total == 0 || v_total == 0 {
        return None;
    }
    let refresh = (pixel_clock_khz as f64 * 1000.0) / (h_total as f64 * v_total as f64);

    // Byte 17 bit 7. The refresh computed above is the field rate for an
    // interlaced mode, which is the number equipment is labelled with — 1080i60
    // means sixty fields — so it is left as it is and only the line count is
    // doubled back to whole frames.
    let interlaced = (dtd[17] & 0x80) != 0;
    let v_frame = if interlaced { v_active * 2 } else { v_active };

    Some((h_active, v_frame, refresh, interlaced))
}

/// Text for the EDID manufacture date. Byte 16 is the week of manufacture
/// (1-54), 0 when the week is unspecified, or 0xFF when byte 17 is a model
/// year rather than a manufacture year. This used to print 0xFF as
/// "week 255 of <year>".
fn manufactured_text(week: u8, year: u16) -> Option<String> {
    match week {
        0xFF => Some(format!("model year {}", year)),
        0 if year > 1990 => Some(format!("year {} (week unspecified)", year)),
        0 => None,
        w => Some(format!("week {} of {}", w, year)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manufactured_week_model_year_and_unspecified() {
        assert_eq!(
            manufactured_text(10, 2024).as_deref(),
            Some("week 10 of 2024")
        );
        // 0xFF is the model-year flag, not a week.
        assert_eq!(
            manufactured_text(0xFF, 2024).as_deref(),
            Some("model year 2024")
        );
        // Week 0 means unspecified: the year is still reported, the week is not.
        assert_eq!(
            manufactured_text(0, 2024).as_deref(),
            Some("year 2024 (week unspecified)")
        );
        assert_eq!(manufactured_text(0, 1990), None);
    }

    fn synth_edid(name: &str, h: u32, v: u32, pclk_khz: u32) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..8].copy_from_slice(&EDID_MAGIC);
        // Manufacturer = "ABC": A=1,B=2,C=3 → bits 1<<10 | 2<<5 | 3
        let mfg_word: u16 = (1 << 10) | (2 << 5) | 3;
        b[8] = (mfg_word >> 8) as u8;
        b[9] = (mfg_word & 0xFF) as u8;
        b[10] = 0x42;
        b[11] = 0x00;
        b[16] = 10; // week
        b[17] = 34; // year offset → 2024
        b[18] = 1; // major
        b[19] = 4; // minor
        b[20] = 0x80; // digital input
        b[21] = 60; // 60 cm wide
        b[22] = 34; // 34 cm tall
        b[23] = (2.2 * 100.0 - 100.0) as u8; // gamma 2.2
        b[24] = 0xE0; // all DPMS bits

        // DTD at offset 54
        let pclk_units = (pclk_khz / 10) as u16;
        b[54] = (pclk_units & 0xFF) as u8;
        b[55] = (pclk_units >> 8) as u8;
        b[56] = (h & 0xFF) as u8;
        b[57] = 0x00; // h blank lo (= 0 for simplicity)
        b[58] = (((h >> 8) & 0xF) << 4) as u8;
        b[59] = (v & 0xFF) as u8;
        b[60] = 0x00;
        b[61] = (((v >> 8) & 0xF) << 4) as u8;

        // Monitor name descriptor at offset 72
        b[72] = 0x00;
        b[73] = 0x00;
        b[74] = 0x00;
        b[75] = 0xFC;
        b[76] = 0x00;
        let n = name.as_bytes();
        for (i, &c) in n.iter().take(13).enumerate() {
            b[77 + i] = c;
        }
        if n.len() < 13 {
            b[77 + n.len()] = 0x0A;
        }
        b
    }

    /// An interlaced mode reports whole frames, not fields.
    ///
    /// The bytes below are the real first DTD from a Yamaha RX-A740 read out of
    /// this machine's registry, not a synthesised one — a fixture built by the
    /// same hand as the parser is what let four NVMe SMART fields ship sixteen
    /// bytes out of place. Decoded against the EDID specification it is
    /// 1920 active pixels, 540 lines *per field*, pixel clock 74.25 MHz, flags
    /// byte 0x9E with bit 7 set for interlace: 1080i60.
    ///
    /// Before the flag was read this reported `1920×540`, a resolution no
    /// equipment has.
    #[test]
    fn an_interlaced_mode_reports_frame_lines_not_field_lines() {
        const RX_A740_DTD: [u8; 18] = [
            0x01, 0x1D, 0x80, 0x18, 0x71, 0x1C, 0x16, 0x20, 0x58, 0x2C, 0x25, 0x00, 0xC4, 0x8E,
            0x21, 0x00, 0x00, 0x9E,
        ];

        let mut b = synth_edid("RX-A740", 1920, 1080, 74_250);
        b[54..72].copy_from_slice(&RX_A740_DTD);

        let (h, v, refresh, interlaced) = read_preferred_timing(&b).expect("a timing");
        assert_eq!(h, 1920);
        assert_eq!(
            v, 1080,
            "540 field lines must be reported as 1080 frame lines"
        );
        assert!(interlaced, "flags byte 0x9E has bit 7 set");
        // 74.25 MHz over 2200 × 562 is the field rate, which is what 1080i60 names.
        assert!(
            (refresh - 60.05).abs() < 0.05,
            "field rate was {refresh}, expected ~60.05"
        );

        let g = decode_block(&b, "test".into(), "hdmi".into()).expect("decodes");
        let shown = g
            .settings
            .iter()
            .find(|s| s.id == "edid.preferred_timing")
            .map(|s| s.value.to_string())
            .expect("preferred timing is published");
        assert!(
            shown.contains("1920×1080i"),
            "an interlaced mode must be marked as one, got {shown:?}"
        );
    }

    /// A progressive mode is left alone by the same code path.
    #[test]
    fn a_progressive_mode_is_not_doubled() {
        let b = synth_edid("Test Display", 2560, 1440, 241_500);
        let (h, v, _, interlaced) = read_preferred_timing(&b).expect("a timing");
        assert_eq!((h, v), (2560, 1440));
        assert!(!interlaced);
    }

    #[test]
    fn detects_magic_and_pnp() {
        let b = synth_edid("Test Display", 2560, 1440, 241_500);
        let g = decode_block(&b, "test".into(), "card0".into()).unwrap();
        let mfg = g
            .settings
            .iter()
            .find(|s| s.id == "edid.manufacturer")
            .unwrap();
        assert!(matches!(&mfg.value, SettingValue::Text(t) if t == "ABC"));
    }

    #[test]
    fn extracts_monitor_name() {
        let b = synth_edid("DELL U2723QE", 3840, 2160, 533_250);
        let g = decode_block(&b, "test".into(), "card0".into()).unwrap();
        let name = g
            .settings
            .iter()
            .find(|s| s.id == "edid.monitor_name")
            .unwrap();
        assert!(matches!(&name.value, SettingValue::Text(t) if t.contains("DELL")));
    }

    #[test]
    fn decodes_preferred_timing() {
        // 2560x1440 with pclk such that refresh ≈ 60 Hz when h_total≈h_active, v_total≈v_active
        let b = synth_edid("Mon", 2560, 1440, 220_000);
        let g = decode_block(&b, "test".into(), "card0".into()).unwrap();
        let t = g
            .settings
            .iter()
            .find(|s| s.id == "edid.preferred_timing")
            .unwrap();
        if let SettingValue::Text(s) = &t.value {
            assert!(s.contains("2560×1440"));
        } else {
            panic!("expected text");
        }
    }

    #[test]
    fn rejects_bad_magic() {
        let b = vec![0u8; 128];
        assert!(decode_block(&b, "test".into(), "card0".into()).is_none());
    }
}
