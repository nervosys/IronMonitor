// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Parser for the ATA device SMART data structure — the 512-byte reply to SMART
//! READ DATA.
//!
//! Offsets follow ACS-4, table "Device SMART data structure": a two-byte revision,
//! thirty 12-byte attribute entries, then status and capability bytes, then a
//! checksum in the final byte. They are offsets into the raw structure and do not
//! depend on how it was obtained — Windows `IOCTL_STORAGE_PREDICT_FAILURE` today,
//! a Linux `HDIO_DRIVE_CMD` if that is ever added.
//!
//! Not target-gated, for the reason [`super::nvme_log`] gives: the arithmetic is
//! identical on every platform, and gating it to Windows would mean these tests
//! only ever ran on one of the three platforms CI covers. That matters more here
//! than it did there, because unlike the NVMe path this one has never been run
//! against a drive — see the note on verification in `docs/DISK_MONITORING.md`.
//!
//! # Why the checksum is enforced
//!
//! [`AtaSmartData::parse`] rejects a structure whose checksum does not validate.
//! ATA defines the final byte so that all 512 bytes sum to zero modulo 256,
//! precisely so a reader can tell a real structure from a buffer that was never
//! filled. smartmontools warns and continues on a bad checksum; this declines,
//! because the caller has an elevated fallback that is known to be correct and
//! there is no way to tell a wrong attribute from a right one once it is returned.
//! A refusal costs a fallback; a bad parse invents a health verdict.

/// One entry from the attribute table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtaSmartAttribute {
    /// Attribute identifier. Never 0 — that value marks an unused entry, which is
    /// skipped rather than returned.
    pub id: u8,
    /// Status flags word. Bit 0 is pre-failure/advisory, bit 1 on-line collection.
    pub flags: u16,
    /// Current normalised value, 1–253. Higher is better; vendor-scaled.
    pub value: u8,
    /// Worst normalised value recorded over the drive's life.
    pub worst: u8,
    /// The 48-bit vendor-specific raw value, zero-extended.
    pub raw: u64,
}

impl AtaSmartAttribute {
    /// Whether the drive treats this attribute as predicting failure rather than
    /// describing wear.
    pub fn pre_fail(&self) -> bool {
        self.flags & 0x0001 != 0
    }

    /// The conventional name for this attribute id, or a generated one.
    ///
    /// Ids above 0xC0 are vendor-assigned and the names below are the common
    /// interpretation rather than a standard; a vendor is free to mean something
    /// else by them. The raw value is reported regardless of whether the name is
    /// recognised, so an unnamed attribute is still a reading.
    pub fn name(&self) -> String {
        match self.id {
            1 => "Raw_Read_Error_Rate",
            2 => "Throughput_Performance",
            3 => "Spin_Up_Time",
            4 => "Start_Stop_Count",
            5 => "Reallocated_Sector_Ct",
            7 => "Seek_Error_Rate",
            8 => "Seek_Time_Performance",
            9 => "Power_On_Hours",
            10 => "Spin_Retry_Count",
            11 => "Calibration_Retry_Count",
            12 => "Power_Cycle_Count",
            173 => "Wear_Leveling_Count",
            177 => "Wear_Leveling_Count",
            179 => "Used_Rsvd_Blk_Cnt_Tot",
            181 => "Program_Fail_Cnt_Total",
            182 => "Erase_Fail_Count_Total",
            183 => "Runtime_Bad_Block",
            184 => "End-to-End_Error",
            187 => "Reported_Uncorrect",
            188 => "Command_Timeout",
            190 => "Airflow_Temperature_Cel",
            191 => "G-Sense_Error_Rate",
            192 => "Power-Off_Retract_Count",
            193 => "Load_Cycle_Count",
            194 => "Temperature_Celsius",
            195 => "Hardware_ECC_Recovered",
            196 => "Reallocated_Event_Count",
            197 => "Current_Pending_Sector",
            198 => "Offline_Uncorrectable",
            199 => "UDMA_CRC_Error_Count",
            200 => "Multi_Zone_Error_Rate",
            233 => "Media_Wearout_Indicator",
            235 => "Good_Block_Count",
            241 => "Total_LBAs_Written",
            242 => "Total_LBAs_Read",
            _ => return format!("Unknown_Attribute_{}", self.id),
        }
        .to_string()
    }
}

/// A parsed ATA device SMART data structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtaSmartData {
    /// SMART structure revision, from the first two bytes. Vendor-assigned; kept
    /// for diagnostics rather than used to gate parsing, because the values seen in
    /// the field are not confined to what the standard suggests.
    pub revision: u16,
    /// Every populated attribute entry, in table order.
    pub attributes: Vec<AtaSmartAttribute>,
}

/// Attribute-table geometry, from ACS-4.
const ATTRIBUTE_TABLE_OFFSET: usize = 2;
const ATTRIBUTE_ENTRY_LEN: usize = 12;
const ATTRIBUTE_COUNT: usize = 30;
/// The structure is exactly this long, and the last byte is its checksum.
pub const STRUCTURE_LEN: usize = 512;

impl AtaSmartData {
    /// Parse the 512-byte structure.
    ///
    /// Returns `None` for a buffer of the wrong length, one whose checksum does not
    /// validate, or one carrying no populated attribute. The last of those is what
    /// a zero-filled buffer looks like — and a zero-filled buffer has a valid
    /// checksum, so the checksum alone would let it through as a drive with a clean
    /// bill of health and nothing to report.
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() != STRUCTURE_LEN || !checksum_valid(buf) {
            return None;
        }

        let mut attributes = Vec::new();
        for i in 0..ATTRIBUTE_COUNT {
            let start = ATTRIBUTE_TABLE_OFFSET + i * ATTRIBUTE_ENTRY_LEN;
            let e = &buf[start..start + ATTRIBUTE_ENTRY_LEN];
            // Id 0 marks an unused entry. Drives leave gaps mid-table, so this
            // skips rather than stopping at the first one.
            if e[0] == 0 {
                continue;
            }
            attributes.push(AtaSmartAttribute {
                id: e[0],
                flags: u16::from_le_bytes([e[1], e[2]]),
                value: e[3],
                worst: e[4],
                raw: u64::from(e[5])
                    | u64::from(e[6]) << 8
                    | u64::from(e[7]) << 16
                    | u64::from(e[8]) << 24
                    | u64::from(e[9]) << 32
                    | u64::from(e[10]) << 40,
            });
        }

        if attributes.is_empty() {
            return None;
        }

        Some(Self {
            revision: u16::from_le_bytes([buf[0], buf[1]]),
            attributes,
        })
    }

    /// The raw value of the first attribute with this id, if the drive published it.
    fn raw(&self, id: u8) -> Option<u64> {
        self.attributes.iter().find(|a| a.id == id).map(|a| a.raw)
    }

    /// The normalised value of the first attribute with this id.
    fn normalised(&self, id: u8) -> Option<u8> {
        self.attributes.iter().find(|a| a.id == id).map(|a| a.value)
    }

    /// Current temperature in Celsius.
    ///
    /// Both 194 and 190 carry the temperature in the low byte of the raw value,
    /// with the remaining bytes holding lifetime minimum and maximum on drives that
    /// track them. Taking the whole 48-bit raw would report a number in the
    /// millions on exactly those drives, which is the usual way this is got wrong.
    /// 194 wins when both are present: 190 is airflow temperature, which is the
    /// same sensor on most SSDs but not on a drive that has both.
    pub fn temperature_celsius(&self) -> Option<u32> {
        self.raw(194)
            .or_else(|| self.raw(190))
            .map(|raw| (raw & 0xFF) as u32)
    }

    /// Power-on hours.
    ///
    /// The raw count of attribute 9 (Power_On_Hours), returned unconverted.
    ///
    /// Most drives count hours, but the unit is vendor-specific and a few count
    /// minutes or half-minutes. The structure does not say which, so no conversion
    /// is applied: this is the raw attribute-9 count, not a verified hour figure.
    pub fn power_on_hours(&self) -> Option<u64> {
        self.raw(9)
    }

    /// Power cycle count.
    pub fn power_cycle_count(&self) -> Option<u64> {
        self.raw(12)
    }

    /// Reallocated sector count — sectors the drive has already remapped.
    pub fn reallocated_sectors(&self) -> Option<u64> {
        self.raw(5)
    }

    /// Current pending sector count — sectors awaiting remap.
    pub fn pending_sectors(&self) -> Option<u64> {
        self.raw(197)
    }

    /// Offline uncorrectable sector count.
    pub fn uncorrectable_sectors(&self) -> Option<u64> {
        self.raw(198)
    }

    /// Lifetime bytes written, from Total_LBAs_Written.
    ///
    /// The attribute counts LBAs, so this assumes 512-byte logical sectors. That is
    /// what the attribute means on every drive that publishes it; a 4Kn drive
    /// reporting it would be under-counted by eight, which is why the count is
    /// exposed rather than any derived endurance figure.
    pub fn total_bytes_written(&self) -> Option<u64> {
        self.raw(241).map(|lbas| lbas.saturating_mul(512))
    }

    /// Lifetime bytes read, from Total_LBAs_Read.
    pub fn total_bytes_read(&self) -> Option<u64> {
        self.raw(242).map(|lbas| lbas.saturating_mul(512))
    }

    /// Share of rated write endurance consumed, percent.
    ///
    /// Derived from the *normalised* value of the wear attribute, which counts down
    /// from 100 as the drive ages — the raw value of 177 and 233 is a vendor
    /// erase-cycle count with no fixed scale. 173 is excluded for that reason: it
    /// carries the same name on some vendors but a raw cycle count with a
    /// normalised value that does not track wear.
    pub fn wear_percent_used(&self) -> Option<f32> {
        self.normalised(177)
            .or_else(|| self.normalised(233))
            .map(|value| 100.0 - f32::from(value).min(100.0))
    }
}

/// Whether all 512 bytes sum to zero modulo 256, as ATA requires.
fn checksum_valid(buf: &[u8]) -> bool {
    buf.iter().fold(0u8, |acc, b| acc.wrapping_add(*b)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a well-formed structure from `(id, flags, value, worst, raw)` entries,
    /// with the checksum byte computed the way a drive would.
    fn structure(revision: u16, entries: &[(u8, u16, u8, u8, u64)]) -> Vec<u8> {
        let mut buf = vec![0u8; STRUCTURE_LEN];
        buf[0..2].copy_from_slice(&revision.to_le_bytes());
        for (i, (id, flags, value, worst, raw)) in entries.iter().enumerate() {
            let start = ATTRIBUTE_TABLE_OFFSET + i * ATTRIBUTE_ENTRY_LEN;
            buf[start] = *id;
            buf[start + 1..start + 3].copy_from_slice(&flags.to_le_bytes());
            buf[start + 3] = *value;
            buf[start + 4] = *worst;
            for b in 0..6 {
                buf[start + 5 + b] = ((raw >> (8 * b)) & 0xFF) as u8;
            }
        }
        // Two's complement of the sum of the first 511 bytes.
        let sum = buf[..STRUCTURE_LEN - 1]
            .iter()
            .fold(0u8, |acc, b| acc.wrapping_add(*b));
        buf[STRUCTURE_LEN - 1] = sum.wrapping_neg();
        buf
    }

    #[test]
    fn a_well_formed_structure_yields_its_attributes() {
        let buf = structure(
            0x0010,
            &[(9, 0x0032, 99, 99, 1234), (12, 0x0032, 99, 99, 56)],
        );
        let data = AtaSmartData::parse(&buf).expect("valid structure");

        assert_eq!(data.revision, 0x0010);
        assert_eq!(data.attributes.len(), 2);
        assert_eq!(data.power_on_hours(), Some(1234));
        assert_eq!(data.power_cycle_count(), Some(56));
        assert_eq!(data.attributes[0].name(), "Power_On_Hours");
    }

    #[test]
    fn a_corrupt_checksum_is_refused() {
        let mut buf = structure(0x0010, &[(9, 0x0032, 99, 99, 1234)]);
        buf[STRUCTURE_LEN - 1] = buf[STRUCTURE_LEN - 1].wrapping_add(1);
        assert_eq!(AtaSmartData::parse(&buf), None);
    }

    /// The case the checksum alone does not catch: a buffer nobody filled sums to
    /// zero and would otherwise parse as a drive reporting nothing wrong.
    #[test]
    fn a_zero_filled_buffer_is_not_a_reading() {
        assert_eq!(AtaSmartData::parse(&[0u8; STRUCTURE_LEN]), None);
    }

    #[test]
    fn a_short_buffer_is_refused_rather_than_indexed_past() {
        assert_eq!(AtaSmartData::parse(&[0u8; 64]), None);
        assert_eq!(AtaSmartData::parse(&[]), None);
    }

    /// A gap mid-table is normal; stopping at the first empty entry would drop
    /// every attribute after it.
    #[test]
    fn an_unused_entry_does_not_end_the_table() {
        let buf = structure(
            0x0010,
            &[
                (9, 0x0032, 99, 99, 100),
                (0, 0, 0, 0, 0),
                (12, 0x0032, 99, 99, 7),
            ],
        );
        let data = AtaSmartData::parse(&buf).expect("valid structure");
        assert_eq!(data.attributes.len(), 2);
        assert_eq!(data.power_cycle_count(), Some(7));
    }

    /// Drives pack lifetime minimum and maximum into the upper raw bytes. Reading
    /// the whole 48-bit value reports tens of millions of degrees.
    #[test]
    fn temperature_comes_from_the_low_raw_byte_only() {
        // 34 °C now, with 21 and 52 as the lifetime range above it.
        let raw = 34 | (21 << 16) | (52 << 32);
        let buf = structure(0x0010, &[(194, 0x0022, 66, 48, raw)]);
        let data = AtaSmartData::parse(&buf).expect("valid structure");
        assert_eq!(data.temperature_celsius(), Some(34));
    }

    #[test]
    fn airflow_temperature_is_used_only_when_194_is_absent() {
        let only_190 = structure(0x0010, &[(190, 0x0022, 70, 55, 30)]);
        assert_eq!(
            AtaSmartData::parse(&only_190)
                .unwrap()
                .temperature_celsius(),
            Some(30)
        );

        let both = structure(
            0x0010,
            &[(190, 0x0022, 70, 55, 30), (194, 0x0022, 66, 48, 34)],
        );
        assert_eq!(
            AtaSmartData::parse(&both).unwrap().temperature_celsius(),
            Some(34)
        );
    }

    /// Wear is the normalised countdown, not the raw erase-cycle count.
    #[test]
    fn wear_is_derived_from_the_normalised_value() {
        let buf = structure(0x0010, &[(177, 0x0013, 94, 94, 183)]);
        let data = AtaSmartData::parse(&buf).expect("valid structure");
        assert_eq!(data.wear_percent_used(), Some(6.0));
    }

    /// An attribute the drive never published must stay absent rather than
    /// becoming zero — the distinction the whole `Option` shape exists for.
    #[test]
    fn an_absent_attribute_is_none_rather_than_zero() {
        let buf = structure(0x0010, &[(9, 0x0032, 99, 99, 1234)]);
        let data = AtaSmartData::parse(&buf).expect("valid structure");
        assert_eq!(data.reallocated_sectors(), None);
        assert_eq!(data.pending_sectors(), None);
        assert_eq!(data.temperature_celsius(), None);
        assert_eq!(data.wear_percent_used(), None);
    }

    /// A reallocated-sector count of zero is a reading, and a good one.
    #[test]
    fn a_zero_reallocated_count_is_a_reading() {
        let buf = structure(0x0010, &[(5, 0x0033, 100, 100, 0)]);
        let data = AtaSmartData::parse(&buf).expect("valid structure");
        assert_eq!(data.reallocated_sectors(), Some(0));
    }

    #[test]
    fn pre_fail_comes_from_flag_bit_zero() {
        let buf = structure(
            0x0010,
            &[(5, 0x0033, 100, 100, 0), (194, 0x0022, 66, 48, 34)],
        );
        let data = AtaSmartData::parse(&buf).expect("valid structure");
        assert!(data.attributes[0].pre_fail());
        assert!(!data.attributes[1].pre_fail());
    }

    #[test]
    fn lba_counters_are_scaled_to_bytes() {
        let buf = structure(
            0x0010,
            &[(241, 0x0032, 99, 99, 1000), (242, 0x0032, 99, 99, 2000)],
        );
        let data = AtaSmartData::parse(&buf).expect("valid structure");
        assert_eq!(data.total_bytes_written(), Some(512_000));
        assert_eq!(data.total_bytes_read(), Some(1_024_000));
    }

    /// The full 48 bits are reachable; a 32-bit read would truncate a long-lived
    /// drive's LBA counters.
    #[test]
    fn raw_values_use_all_forty_eight_bits() {
        let buf = structure(0x0010, &[(241, 0x0032, 99, 99, 0xFFFF_FFFF_FFFF)]);
        let data = AtaSmartData::parse(&buf).expect("valid structure");
        assert_eq!(data.attributes[0].raw, 0xFFFF_FFFF_FFFF);
    }

    #[test]
    fn an_unrecognised_id_is_still_reported() {
        let buf = structure(0x0010, &[(250, 0x0032, 99, 99, 5)]);
        let data = AtaSmartData::parse(&buf).expect("valid structure");
        assert_eq!(data.attributes[0].name(), "Unknown_Attribute_250");
        assert_eq!(data.attributes[0].raw, 5);
    }
}
