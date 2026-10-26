// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Parsers for the NVMe SMART/Health log page and Identify Controller structure.
//!
//! Field offsets follow the NVM Express Base Specification (1.4, §5.14.1.2 for the
//! health log; §5.15.2.2 and Figure 251 for Identify Controller). They are byte
//! offsets into the raw structure, independent of how it was obtained — Windows
//! `DeviceIoControl` today, a Linux ioctl if that is ever added.
//!
//! These are separated from the platform code so they can be tested without a
//! drive. Every parser returns `None` for a short buffer rather than indexing past
//! the end, and reports absent fields as `None` rather than zero: NVMe uses zero as
//! a real reading for temperature, wear and error counts alike, so a zero that came
//! from a truncated or unread structure cannot be told apart from a measurement
//! unless it is kept distinct here.

/// NVMe SMART/Health Information log page, log identifier 0x02.
#[derive(Debug, Clone, PartialEq)]
pub struct HealthLog {
    /// Critical warning bit flags. `Some(0)` means the drive reported no warnings.
    pub critical_warning: u8,
    /// Composite temperature in Kelvin. `None` when the drive reports 0, which the
    /// spec defines as "no temperature data available" rather than absolute zero.
    pub temperature_kelvin: Option<u16>,
    /// Remaining spare capacity, percent.
    pub available_spare_percent: u8,
    /// Threshold below which available spare triggers a warning, percent.
    pub available_spare_threshold_percent: u8,
    /// Share of rated endurance consumed, percent. May exceed 100.
    pub percentage_used: u8,
    /// Data units read, in 1000×512-byte units.
    pub data_units_read: u128,
    /// Data units written, in 1000×512-byte units.
    pub data_units_written: u128,
    /// Host read commands issued.
    pub host_read_commands: u128,
    /// Host write commands issued.
    pub host_write_commands: u128,
    /// Controller busy time, in minutes.
    ///
    /// Not previously parsed, and its absence is what shifted every field below
    /// it by sixteen bytes. It sits between the host command counters and the
    /// power cycles in the spec's layout.
    pub controller_busy_time: u128,
    /// Power cycles.
    pub power_cycles: u128,
    /// Power-on hours.
    pub power_on_hours: u128,
    /// Unsafe shutdowns.
    pub unsafe_shutdowns: u128,
    /// Media and data integrity errors.
    pub media_errors: u128,
}

/// One NVMe data unit: the spec counts host data in thousands of 512-byte blocks.
/// A reader that holds bytes and needs data units divides by this, not by 512.
pub const BYTES_PER_DATA_UNIT: u128 = 1000 * 512;

/// The health log page is 512 bytes; every field this parses lives below 192.
const HEALTH_LOG_MIN_LEN: usize = 192;

fn u16_at(d: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([d[off], d[off + 1]])
}

fn u32_at(d: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

fn u128_at(d: &[u8], off: usize) -> u128 {
    let mut b = [0u8; 16];
    b.copy_from_slice(&d[off..off + 16]);
    u128::from_le_bytes(b)
}

/// Trim an NVMe ASCII field, which is space-padded rather than NUL-terminated.
///
/// Some controllers pad with NULs anyway, so both are stripped. Non-ASCII bytes are
/// replaced rather than rejected — a garbled model string is still more use than an
/// error, and it makes the difference visible.
fn ascii_field(d: &[u8], off: usize, len: usize) -> String {
    String::from_utf8_lossy(&d[off..off + len])
        .trim_matches(|c: char| c == ' ' || c == '\0')
        .to_string()
}

impl HealthLog {
    /// Parse the SMART/Health log page. Returns `None` if the buffer is short.
    pub fn parse(d: &[u8]) -> Option<Self> {
        if d.len() < HEALTH_LOG_MIN_LEN {
            return None;
        }
        let kelvin = u16_at(d, 1);
        Some(Self {
            critical_warning: d[0],
            // 0 K is the spec's "not reported", not a reading.
            temperature_kelvin: (kelvin != 0).then_some(kelvin),
            available_spare_percent: d[3],
            available_spare_threshold_percent: d[4],
            percentage_used: d[5],
            data_units_read: u128_at(d, 32),
            data_units_written: u128_at(d, 48),
            host_read_commands: u128_at(d, 64),
            host_write_commands: u128_at(d, 80),
            // Offsets from the NVMe Base Specification, SMART / Health
            // Information log page (LID 02h). Controller Busy Time occupies
            // 111:96, and omitting it shifted everything from here down by
            // sixteen bytes: `power_cycles` was reading Power On Hours,
            // `power_on_hours` was reading Unsafe Shutdowns, `unsafe_shutdowns`
            // was reading Media and Data Integrity Errors, and `media_errors`
            // was reading the error-log entry count.
            //
            // On the machine that found it, a drive reported 2189 "power cycles"
            // and 43 "power-on hours" — a power cycle every seventy seconds,
            // which is what made it visible. The true reading is 2189 hours and
            // 43 unsafe shutdowns. `media_errors` is the field a person uses to
            // decide whether a drive is failing, and it was reading a different
            // counter entirely.
            controller_busy_time: u128_at(d, 96),
            power_cycles: u128_at(d, 112),
            power_on_hours: u128_at(d, 128),
            unsafe_shutdowns: u128_at(d, 144),
            media_errors: u128_at(d, 160),
        })
    }

    /// Composite temperature in Celsius, or `None` if the drive did not report one.
    pub fn temperature_celsius(&self) -> Option<f32> {
        self.temperature_kelvin.map(|k| k as f32 - 273.15)
    }

    /// Bytes read, derived from data units of 1000×512 bytes.
    pub fn bytes_read(&self) -> u128 {
        self.data_units_read * BYTES_PER_DATA_UNIT
    }

    /// Bytes written, derived from data units of 1000×512 bytes.
    pub fn bytes_written(&self) -> u128 {
        self.data_units_written * BYTES_PER_DATA_UNIT
    }
}

/// One entry of the Identify Controller power state descriptor table.
#[derive(Debug, Clone, PartialEq)]
pub struct PowerState {
    /// Power state number.
    pub state: u8,
    /// Maximum power draw, watts.
    pub max_power_watts: f32,
    /// Entry latency, microseconds.
    pub entry_latency_us: u32,
    /// Exit latency, microseconds.
    pub exit_latency_us: u32,
}

/// Identify Controller data structure, CNS 0x01.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentifyController {
    /// Model number.
    pub model: String,
    /// Serial number.
    pub serial: String,
    /// Firmware revision.
    pub firmware: String,
    /// Controller identifier. 0 is a valid controller id.
    pub controller_id: u16,
    /// NVMe specification version, as "major.minor.tertiary". `None` when the VER
    /// field is 0, which controllers predating NVMe 1.2 report.
    pub version: Option<String>,
    /// Number of namespaces.
    pub num_namespaces: u32,
    /// Total NVM capacity in bytes. `None` when unreported, which is permitted for
    /// controllers without namespace management.
    pub total_capacity: Option<u128>,
    /// Unallocated NVM capacity in bytes. `None` when unreported.
    pub unallocated_capacity: Option<u128>,
    /// Power state descriptors, in state order.
    pub power_states: Vec<PowerState>,
}

/// NN sits at 516; the power state descriptors start at 2048.
const IDENTIFY_MIN_LEN: usize = 520;
const POWER_STATE_TABLE_OFFSET: usize = 2048;
const POWER_STATE_ENTRY_LEN: usize = 32;

impl IdentifyController {
    /// Parse the Identify Controller structure. Returns `None` if the buffer is
    /// short of the last field read (NN at 516).
    pub fn parse(d: &[u8]) -> Option<Self> {
        if d.len() < IDENTIFY_MIN_LEN {
            return None;
        }

        let ver = u32_at(d, 80);
        let total = u128_at(d, 280);
        let unalloc = u128_at(d, 296);

        Some(Self {
            serial: ascii_field(d, 4, 20),
            model: ascii_field(d, 24, 40),
            firmware: ascii_field(d, 64, 8),
            controller_id: u16_at(d, 78),
            // VER is 0 on controllers older than NVMe 1.2, which is "not reported"
            // rather than version 0.0.0.
            version: (ver != 0).then(|| {
                format!(
                    "{}.{}.{}",
                    (ver >> 16) & 0xFFFF,
                    (ver >> 8) & 0xFF,
                    ver & 0xFF
                )
            }),
            num_namespaces: u32_at(d, 516),
            total_capacity: (total != 0).then_some(total),
            // Unlike total capacity, 0 unallocated is a real answer on a fully
            // provisioned drive — but it is only meaningful if the controller
            // reports capacities at all, which TNVMCAP indicates.
            unallocated_capacity: (total != 0).then_some(unalloc),
            power_states: Self::parse_power_states(d),
        })
    }

    /// NPSS at offset 263 is zero-based: it holds "number of power states minus 1".
    fn parse_power_states(d: &[u8]) -> Vec<PowerState> {
        if d.len() < 264 {
            return Vec::new();
        }
        let count = d[263] as usize + 1;
        let mut states = Vec::new();
        for i in 0..count {
            let base = POWER_STATE_TABLE_OFFSET + i * POWER_STATE_ENTRY_LEN;
            if base + POWER_STATE_ENTRY_LEN > d.len() {
                break;
            }
            // MP is in 0.01 W units, or 0.0001 W when the MXPS bit is set.
            let mp = u16_at(d, base) as f32;
            let max_power_watts = if d[base + 3] & 0x01 != 0 {
                mp * 0.0001
            } else {
                mp * 0.01
            };
            states.push(PowerState {
                state: i as u8,
                max_power_watts,
                entry_latency_us: u32_at(d, base + 4),
                exit_latency_us: u32_at(d, base + 8),
            });
        }
        states
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A health log page carrying the readings the drives in this machine report,
    /// so the offsets are pinned to observed hardware rather than to a reading of
    /// the specification alone.
    fn sample_health_log() -> Vec<u8> {
        let mut d = vec![0u8; 512];
        d[0] = 0x00; // no critical warnings
        d[1..3].copy_from_slice(&323u16.to_le_bytes()); // 323 K = 49.85 C
        d[3] = 100; // available spare
        d[4] = 10; // threshold
        d[5] = 3; // 3% of endurance used
        d[32..48].copy_from_slice(&205_501_219u128.to_le_bytes());
        d[48..64].copy_from_slice(&353_311_735u128.to_le_bytes());
        d[64..80].copy_from_slice(&1_234_567u128.to_le_bytes());
        d[80..96].copy_from_slice(&7_654_321u128.to_le_bytes());
        // Written at the offsets the NVMe specification gives, not at the ones
        // the parser happened to use. The previous fixture used the parser's
        // offsets, so the test agreed with the code and both were wrong by
        // sixteen bytes for four fields.
        //
        // Every value below is distinct and non-zero so that a shift by one
        // field cannot pass: with the old parser these assertions fail on
        // `power_cycles` and everything after it. That is the property the test
        // name claims, and it did not hold before.
        d[96..112].copy_from_slice(&444u128.to_le_bytes()); // controller busy time
        d[112..128].copy_from_slice(&1892u128.to_le_bytes()); // power cycles
        d[128..144].copy_from_slice(&27u128.to_le_bytes()); // power on hours
        d[144..160].copy_from_slice(&5u128.to_le_bytes()); // unsafe shutdowns
        d[160..176].copy_from_slice(&9u128.to_le_bytes()); // media errors
        d[176..192].copy_from_slice(&77u128.to_le_bytes()); // error log entries
        d
    }

    #[test]
    fn health_log_reads_every_field_at_its_specified_offset() {
        let log = HealthLog::parse(&sample_health_log()).expect("parses");
        assert_eq!(log.critical_warning, 0);
        assert_eq!(log.temperature_kelvin, Some(323));
        assert_eq!(log.available_spare_percent, 100);
        assert_eq!(log.available_spare_threshold_percent, 10);
        assert_eq!(log.percentage_used, 3);
        assert_eq!(log.data_units_read, 205_501_219);
        assert_eq!(log.data_units_written, 353_311_735);
        assert_eq!(log.host_read_commands, 1_234_567);
        assert_eq!(log.host_write_commands, 7_654_321);
        assert_eq!(log.controller_busy_time, 444);
        assert_eq!(log.power_cycles, 1892);
        assert_eq!(log.power_on_hours, 27);
        assert_eq!(log.unsafe_shutdowns, 5);
        assert_eq!(log.media_errors, 9);
    }

    /// A shift by one field must fail, which is what the fixture is for.
    ///
    /// The bug this guards was invisible because the fixture was built from the
    /// parser's own offsets: the test asserted that the code agreed with itself.
    /// Distinct values are what make the assertion mean something, so this
    /// checks they are in fact distinct rather than trusting the author of the
    /// next fixture to remember why.
    #[test]
    fn the_health_log_fixture_can_tell_the_fields_apart() {
        let log = HealthLog::parse(&sample_health_log()).expect("parses");
        let counters = [
            log.data_units_read,
            log.data_units_written,
            log.host_read_commands,
            log.host_write_commands,
            log.controller_busy_time,
            log.power_cycles,
            log.power_on_hours,
            log.unsafe_shutdowns,
            log.media_errors,
        ];
        for (i, a) in counters.iter().enumerate() {
            assert_ne!(
                *a, 0,
                "counter {i} is zero, so a shift onto it is invisible"
            );
            for (j, b) in counters.iter().enumerate().skip(i + 1) {
                assert_ne!(a, b, "counters {i} and {j} share a value");
            }
        }
    }

    #[test]
    fn health_log_converts_kelvin_to_celsius() {
        let log = HealthLog::parse(&sample_health_log()).expect("parses");
        let c = log.temperature_celsius().expect("temperature present");
        assert!((c - 49.85).abs() < 0.01, "got {c}");
    }

    /// The drives here report 0 K when idle in some states. Treating that as
    /// -273 C is how a plausibility check gets tripped by a working drive.
    #[test]
    fn a_zero_kelvin_reading_is_absent_rather_than_absolute_zero() {
        let mut d = sample_health_log();
        d[1..3].copy_from_slice(&0u16.to_le_bytes());
        let log = HealthLog::parse(&d).expect("parses");
        assert_eq!(log.temperature_kelvin, None);
        assert_eq!(log.temperature_celsius(), None);
    }

    #[test]
    fn a_short_health_buffer_is_rejected_rather_than_indexed_past() {
        assert!(HealthLog::parse(&[0u8; 191]).is_none());
        assert!(HealthLog::parse(&[]).is_none());
        assert!(HealthLog::parse(&[0u8; 192]).is_some());
    }

    #[test]
    fn data_units_convert_to_bytes_in_thousand_unit_multiples() {
        let log = HealthLog::parse(&sample_health_log()).expect("parses");
        assert_eq!(log.bytes_read(), 205_501_219u128 * 1000 * 512);
        assert_eq!(log.bytes_written(), 353_311_735u128 * 1000 * 512);
    }

    fn sample_identify() -> Vec<u8> {
        let mut d = vec![0u8; 4096];
        // NVMe pads these with spaces, not NULs.
        d[4..24].copy_from_slice(b"S7YANJ0Y502434K     ");
        d[24..64].copy_from_slice(b"Samsung SSD 9100 PRO 4TB                ");
        d[64..72].copy_from_slice(b"0B2QNXH7");
        d[78..80].copy_from_slice(&1u16.to_le_bytes());
        d[80..84].copy_from_slice(&0x0002_0000u32.to_le_bytes()); // 2.0.0
        d[263] = 4; // NPSS is zero-based: five states
        d[280..296].copy_from_slice(&4_000_787_030_016u128.to_le_bytes());
        d[296..312].copy_from_slice(&0u128.to_le_bytes());
        d[516..520].copy_from_slice(&1u32.to_le_bytes());
        // Power state 0: 8.49 W, 0.01 W units.
        d[2048..2050].copy_from_slice(&849u16.to_le_bytes());
        d[2052..2056].copy_from_slice(&1000u32.to_le_bytes());
        d[2056..2060].copy_from_slice(&2000u32.to_le_bytes());
        d
    }

    #[test]
    fn identify_reads_the_fields_the_drives_here_report() {
        let id = IdentifyController::parse(&sample_identify()).expect("parses");
        assert_eq!(id.serial, "S7YANJ0Y502434K");
        assert_eq!(id.model, "Samsung SSD 9100 PRO 4TB");
        assert_eq!(id.firmware, "0B2QNXH7");
        assert_eq!(id.controller_id, 1);
        assert_eq!(id.version.as_deref(), Some("2.0.0"));
        assert_eq!(id.num_namespaces, 1);
        assert_eq!(id.total_capacity, Some(4_000_787_030_016));
        assert_eq!(id.unallocated_capacity, Some(0));
    }

    /// A controller id of 0 is a real controller, so it must survive parsing as a
    /// reading rather than being folded into "not reported".
    #[test]
    fn controller_id_zero_is_a_reading() {
        let mut d = sample_identify();
        d[78..80].copy_from_slice(&0u16.to_le_bytes());
        let id = IdentifyController::parse(&d).expect("parses");
        assert_eq!(id.controller_id, 0);
    }

    #[test]
    fn an_unreported_version_is_none_rather_than_zero_zero_zero() {
        let mut d = sample_identify();
        d[80..84].copy_from_slice(&0u32.to_le_bytes());
        let id = IdentifyController::parse(&d).expect("parses");
        assert_eq!(id.version, None);
    }

    #[test]
    fn an_unreported_capacity_is_none_rather_than_zero_bytes() {
        let mut d = sample_identify();
        d[280..296].copy_from_slice(&0u128.to_le_bytes());
        let id = IdentifyController::parse(&d).expect("parses");
        assert_eq!(id.total_capacity, None);
        assert_eq!(id.unallocated_capacity, None);
    }

    #[test]
    fn power_states_are_counted_from_a_zero_based_npss() {
        let id = IdentifyController::parse(&sample_identify()).expect("parses");
        assert_eq!(id.power_states.len(), 5, "NPSS of 4 means five states");
        let ps0 = &id.power_states[0];
        assert_eq!(ps0.state, 0);
        assert!((ps0.max_power_watts - 8.49).abs() < 0.001);
        assert_eq!(ps0.entry_latency_us, 1000);
        assert_eq!(ps0.exit_latency_us, 2000);
    }

    #[test]
    fn the_max_power_scale_bit_switches_to_hundred_microwatt_units() {
        let mut d = sample_identify();
        d[2051] |= 0x01; // MXPS
        let id = IdentifyController::parse(&d).expect("parses");
        assert!((id.power_states[0].max_power_watts - 0.0849).abs() < 0.0001);
    }

    #[test]
    fn a_truncated_power_state_table_stops_rather_than_panicking() {
        let mut d = sample_identify();
        d.truncate(2048 + 32); // room for one descriptor, NPSS still claims five
        let id = IdentifyController::parse(&d).expect("parses");
        assert_eq!(id.power_states.len(), 1);
    }

    #[test]
    fn a_short_identify_buffer_is_rejected() {
        assert!(IdentifyController::parse(&[0u8; 519]).is_none());
        assert!(IdentifyController::parse(&[0u8; 520]).is_some());
    }
}
