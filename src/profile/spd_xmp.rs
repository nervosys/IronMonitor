//! XMP / EXPO profile decoder for DDR4 and DDR5 SPD blobs (Linux only).
//!
//! Sources scanned on Linux:
//! - `/sys/bus/i2c/devices/<bus>-00<addr>/eeprom` — exported by the
//!   `at24` / `ee1004` driver when the SPD-EEPROM kernel module is loaded.
//!   DIMM SPD slaves live at I2C addresses 0x50–0x57.
//! - DDR5 modules expose a 1024-byte SPD; DDR4 modules expose 512 bytes
//!   (or 1024 when extended).
//!
//! Layout reference:
//! - SPD byte 2 (DDR type): 0x0C = DDR4, 0x12 = DDR5.
//! - XMP 2.0 magic `0x0C 0x4A` at offset 384 (DDR4 modules). Byte 386 holds
//!   the profile enable bits (bit 0 = profile 1, bit 1 = profile 2). Profile
//!   data starts at 393; its per-profile layout is not decoded here.
//! - XMP 3.0 (DDR5) keeps the same magic at 384, but the slot count expands
//!   to five and slots are 64 bytes. The enable-bit offset for DDR5 is not
//!   confirmed here, so DDR5 profile flags are reported as unreadable.
//! - AMD EXPO magic `0x45 0x58 0x50 0x4F` ("EXPO") at offset 768 with two
//!   profile slots.
//!
//! Byte 387 (the XMP revision) is not decoded: its layout is not confirmed,
//! so the version is reported as unreadable rather than inferred.
//!
//! Writing to SPD is BIOS territory; this module is read-only.

#[allow(unused_imports)]
use super::{ProfileGroup, Setting, SettingRisk, SettingValue, Subsystem};

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DramType {
    Ddr4,
    Ddr5,
    Other(u8),
}

#[cfg(target_os = "linux")]
pub fn scan_xmp_groups() -> Vec<ProfileGroup> {
    use std::fs;
    let base = std::path::Path::new("/sys/bus/i2c/devices");
    let Ok(rd) = fs::read_dir(base) else {
        return Vec::new();
    };
    let mut groups = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        let eeprom = path.join("eeprom");
        if !eeprom.exists() {
            continue;
        }
        let Ok(bytes) = fs::read(&eeprom) else {
            continue;
        };
        if bytes.len() < 384 {
            continue;
        }
        if let Some(g) = decode_blob(
            &bytes,
            &eeprom.display().to_string(),
            entry.file_name().to_string_lossy().to_string(),
        ) {
            groups.push(g);
        }
    }
    groups
}

#[cfg(not(target_os = "linux"))]
pub fn scan_xmp_groups() -> Vec<ProfileGroup> {
    Vec::new()
}

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn dram_type(bytes: &[u8]) -> DramType {
    match bytes.get(2).copied() {
        Some(0x0C) => DramType::Ddr4,
        Some(0x12) => DramType::Ddr5,
        Some(other) => DramType::Other(other),
        None => DramType::Other(0),
    }
}

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn decode_blob(bytes: &[u8], source: &str, locator: String) -> Option<ProfileGroup> {
    let dram = dram_type(bytes);
    let dram_label = match dram {
        DramType::Ddr4 => "DDR4",
        DramType::Ddr5 => "DDR5",
        DramType::Other(_) => return None,
    };

    let mut group = ProfileGroup::new(
        Subsystem::Memory,
        format!("DIMM @ {} ({})", locator, dram_label),
        "SPD XMP/EXPO profiles",
        source.to_string(),
    );

    let mut any_profile = false;

    // XMP magic at byte 384: 0x0C 0x4A
    if bytes.len() >= 386 && bytes[384] == 0x0C && bytes[385] == 0x4A {
        any_profile = true;
        decode_xmp(&mut group, bytes, dram);
    }
    // EXPO magic at byte 768: "EXPO"
    if bytes.len() >= 772 && &bytes[768..772] == b"EXPO" {
        any_profile = true;
        decode_expo(&mut group, bytes);
    }

    if !any_profile {
        group.note(
            "No XMP or EXPO magic found in this SPD blob — this DIMM ships \
            with JEDEC profiles only.",
        );
    }
    Some(group)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn decode_xmp(group: &mut ProfileGroup, bytes: &[u8], dram: DramType) {
    // The XMP revision byte (387) is not decoded. The version used to be a
    // literal chosen by DRAM type, which reported a version the blob never
    // declared, so it is now unreadable until the layout is confirmed.
    group.push(
        Setting::info(
            "xmp.version",
            "XMP Version",
            SettingValue::Unreadable("revision byte 387 not decoded; layout not confirmed".into()),
        )
        .with_source("SPD offset 387"),
    );

    // DDR5 (XMP 3.0): the enable-bit offset is not confirmed, so no profile
    // count or slot values are reported. Previously byte 385 (the second magic
    // byte) was read as the enable mask and slots were decoded without it.
    let enabled = match dram {
        DramType::Ddr4 => {
            // Byte 386: bit 0 = profile 1, bit 1 = profile 2.
            // Byte 385 is the second magic byte, not a profile mask.
            bytes.get(386).copied().unwrap_or(0) & 0b11
        }
        _ => {
            group.push(
                Setting::info(
                    "xmp.profile_count",
                    "XMP Profiles Present",
                    SettingValue::Unreadable(
                        "XMP 3.0 enable-bit offset not confirmed; profiles not decoded".into(),
                    ),
                )
                .with_source("SPD offset 386 (unconfirmed for DDR5)"),
            );
            return;
        }
    };

    let profile_count = enabled.count_ones() as u64;
    group.push(
        Setting::info(
            "xmp.profile_count",
            "XMP Profiles Present",
            SettingValue::Uint(profile_count),
        )
        .with_description("Number of XMP profiles enabled in this DIMM's SPD.")
        .with_source("SPD offset 386"),
    );
    if profile_count == 0 {
        group.note("XMP magic present but no profiles flagged enabled in byte 386.");
    }

    // Per-profile voltage and speed are not decoded. This used to read Vdd at
    // 393 + 35 * slot as 5 mV steps above 1.0 V and the cycle time as a 16-bit
    // MTB count. Published XMP 2.0 layouts put profile 2 at 440 (47-byte
    // profiles) and store tCKmin as one MTB byte plus a fine correction, so
    // profile 2's figures came from the wrong bytes and the speed decode was
    // wrong for both. A Dangerous-risk voltage needs a confirmed layout.
    if profile_count > 0 {
        group.note(
            "Per-profile XMP voltage and speed are not decoded: the profile layout is not confirmed.",
        );
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn decode_expo(group: &mut ProfileGroup, bytes: &[u8]) {
    let enabled = bytes.get(772).copied().unwrap_or(0);
    let profile_count = enabled.count_ones() as u64;
    group.push(
        Setting::info(
            "expo.version",
            "EXPO Version",
            SettingValue::Text("AMD EXPO".into()),
        )
        .with_source("SPD offset 768"),
    );
    group.push(
        Setting::info(
            "expo.profile_count",
            "EXPO Profiles Present",
            SettingValue::Uint(profile_count),
        )
        .with_description("Number of AMD EXPO profiles enabled in this DIMM's SPD.")
        .with_source("SPD offset 772"),
    );
    // EXPO profile slots begin at offset 776 with 64 bytes per slot. Decode
    // voltage and min cycle time identically to XMP 3.0 layout.
    for slot in 0..2 {
        if (enabled >> slot) & 1 == 0 {
            continue;
        }
        let base = 776 + slot * 64;
        if base + 4 >= bytes.len() {
            break;
        }
        let vdd_raw = bytes[base] as u32;
        // A zero byte is an unset profile, not a 0 V reading.
        if vdd_raw == 0 {
            continue;
        }
        group.push(
            Setting::info(
                format!("expo.slot{}.vdd_raw", slot),
                format!("EXPO Profile {} Vdd (raw)", slot + 1),
                SettingValue::Uint(vdd_raw as u64),
            )
            .with_risk(SettingRisk::Dangerous)
            .with_source(format!("SPD offset {}", base)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DDR4 blob with XMP 2.0 magic and two enabled profiles, with non-zero
    /// bytes in both profile areas.
    fn make_ddr4_blob() -> Vec<u8> {
        let mut b = vec![0u8; 512];
        b[2] = 0x0C; // DDR4
        b[384] = 0x0C; // XMP magic
        b[385] = 0x4A;
        b[386] = 0b0000_0011; // profiles 1 and 2 enabled
        b[393] = 0x28;
        b[394] = 0x05;
        b[440] = 0x2C;
        b[441] = 0x06;
        b
    }

    fn find<'a>(g: &'a ProfileGroup, id: &str) -> Option<&'a Setting> {
        g.settings.iter().find(|s| s.id == id)
    }

    #[test]
    fn detects_ddr4_xmp() {
        let blob = make_ddr4_blob();
        let g = decode_blob(&blob, "test", "0-0050".into()).unwrap();
        assert!(find(&g, "xmp.version").is_some());
        let profile_count = find(&g, "xmp.profile_count").unwrap();
        assert_eq!(profile_count.value, SettingValue::Uint(2));
    }

    #[test]
    fn per_profile_voltage_and_speed_are_not_decoded() {
        // The profile layout is unconfirmed, so no voltage or speed is
        // published from it, however plausible the bytes look.
        let g = decode_blob(&make_ddr4_blob(), "test", "0-0050".into()).unwrap();
        assert!(!g.settings.iter().any(|s| s.id.starts_with("xmp.slot")));
    }

    #[test]
    fn version_is_unreadable_not_inferred() {
        let g = decode_blob(&make_ddr4_blob(), "test", "x".into()).unwrap();
        assert!(matches!(
            find(&g, "xmp.version").unwrap().value,
            SettingValue::Unreadable(_)
        ));
    }

    #[test]
    fn a_single_enable_bit_counts_one_profile() {
        let mut blob = make_ddr4_blob();
        blob[386] = 0b0000_0001;
        let g = decode_blob(&blob, "test", "x".into()).unwrap();
        assert_eq!(
            find(&g, "xmp.profile_count").unwrap().value,
            SettingValue::Uint(1)
        );
    }

    #[test]
    fn second_magic_byte_is_not_a_profile_mask() {
        // Byte 385 is 0x4A. With no enable bits in byte 386 the count must be
        // zero, not the three the old code reported from 0x4A.
        let mut blob = make_ddr4_blob();
        blob[386] = 0;
        let g = decode_blob(&blob, "test", "x".into()).unwrap();
        assert_eq!(
            find(&g, "xmp.profile_count").unwrap().value,
            SettingValue::Uint(0)
        );
    }

    #[test]
    fn ddr5_profile_flags_are_unreadable() {
        let mut blob = vec![0u8; 1024];
        blob[2] = 0x12; // DDR5
        blob[384] = 0x0C;
        blob[385] = 0x4A;
        blob[386] = 0b0000_0001;
        blob[393] = 0x50;
        let g = decode_blob(&blob, "test", "x".into()).unwrap();
        assert!(matches!(
            find(&g, "xmp.profile_count").unwrap().value,
            SettingValue::Unreadable(_)
        ));
        assert!(!g.settings.iter().any(|s| s.id.starts_with("xmp.slot")));
    }

    #[test]
    fn skips_unknown_dram_type() {
        let mut blob = vec![0u8; 512];
        blob[2] = 0xFF;
        assert!(decode_blob(&blob, "test", "x".into()).is_none());
    }

    #[test]
    fn detects_expo_magic() {
        let mut b = vec![0u8; 1024];
        b[2] = 0x12; // DDR5
        b[768..772].copy_from_slice(b"EXPO");
        b[772] = 0b0000_0001;
        b[776] = 0x50;
        let g = decode_blob(&b, "test", "y".into()).unwrap();
        assert!(g.settings.iter().any(|s| s.id == "expo.version"));
        assert!(find(&g, "expo.slot0.vdd_raw").is_some());
    }

    #[test]
    fn expo_zero_slot_is_skipped() {
        let mut b = vec![0u8; 1024];
        b[2] = 0x12;
        b[768..772].copy_from_slice(b"EXPO");
        b[772] = 0b0000_0001; // profile 1 flagged, but its byte is zero
        let g = decode_blob(&b, "test", "y".into()).unwrap();
        assert!(find(&g, "expo.slot0.vdd_raw").is_none());
    }
}
