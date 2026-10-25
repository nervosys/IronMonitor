//! NVMe `Get Features` admin pass-through (Linux only).
//!
//! Issues NVMe admin opcode `0x0A` (Get Features) via `NVME_IOCTL_ADMIN_CMD`
//! against `/dev/nvmeN` character devices. This mirrors what `nvme-cli`'s
//! `nvme get-feature` subcommand does and surfaces the runtime values for:
//!
//! | FID  | Name                                | Notes |
//! |------|-------------------------------------|-------|
//! | 0x02 | Power Management                    | Current power state, workload hint |
//! | 0x06 | Volatile Write Cache                | Enable/disable VWC |
//! | 0x07 | Number of Queues                    | Negotiated I/O queue counts |
//! | 0x0B | Asynchronous Event Configuration    | Event notification mask |
//! | 0x0C | Autonomous Power State Transition   | APST enable + table |
//! | 0x0F | Keep Alive Timer                    | KATO in milliseconds |
//!
//! Requires root (CAP_SYS_ADMIN) — `/dev/nvmeN` is mode 0660 root:disk.
//! When permission is denied or the kernel doesn't support the ioctl,
//! providers degrade silently and emit a note on the surrounding group.
//!
//! Writing features (Set Features, opcode 0x09) is intentionally not
//! implemented in this read-only pass.

#[allow(unused_imports)]
use super::{ProfileGroup, Setting, SettingRisk, SettingValue, Subsystem};

#[cfg(target_os = "linux")]
const NVME_IOCTL_ADMIN_CMD: u64 = 0xC0484E41; // _IOWR('N', 0x41, struct nvme_admin_cmd)
#[cfg(target_os = "linux")]
const NVME_ADMIN_GET_FEATURES: u8 = 0x0A;

/// Append per-controller Get-Features data to existing groups.
#[cfg(target_os = "linux")]
pub fn enrich_groups(groups: &mut Vec<ProfileGroup>) {
    use std::fs;
    let Ok(rd) = fs::read_dir("/dev") else { return };
    let mut devs: Vec<std::path::PathBuf> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_string_lossy().to_string();
            // Match only controller chardevs (nvme0, nvme1...), not namespaces.
            if is_controller_chardev(&name) {
                Some(p)
            } else {
                None
            }
        })
        .collect();
    devs.sort();
    for dev in devs {
        let name = dev.file_name().and_then(|n| n.to_str()).unwrap_or("nvme");
        let mut group = ProfileGroup::new(
            Subsystem::Nvme,
            format!("{} — Get-Features", name),
            "NVMe admin Get-Features (opcode 0x0A)",
            dev.display().to_string(),
        );
        match read_features(&dev) {
            Ok(features) => {
                for f in features {
                    group.push(f);
                }
            }
            Err(e) => {
                group.note(format!(
                    "Get-Features admin command failed: {} (root + open chardev required).",
                    e
                ));
            }
        }
        groups.push(group);
    }
}

#[cfg(not(target_os = "linux"))]
pub fn enrich_groups(_groups: &mut Vec<ProfileGroup>) {}

/// True for a controller character device name such as `nvme0`. A namespace
/// block device such as `nvme0n1` is not a controller. The check used to test
/// `!name.contains('n')`, which is always false for any `nvme*` name, so no
/// controller was ever queried.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_controller_chardev(name: &str) -> bool {
    match name.strip_prefix("nvme") {
        Some(digits) => !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

#[cfg(target_os = "linux")]
fn read_features(dev_path: &std::path::Path) -> std::io::Result<Vec<Setting>> {
    use std::os::unix::io::AsRawFd;
    let f = std::fs::File::open(dev_path)?;
    let fd = f.as_raw_fd();
    let mut out = Vec::new();

    for query in feature_queries() {
        let source = format!("NVMe Get-Features FID 0x{:02X}", query.fid);
        match issue_get_features(fd, query.fid) {
            Ok(raw) => {
                for s in decode_feature(query, raw) {
                    out.push(s.with_source(source.clone()));
                }
            }
            Err(e) => {
                out.push(
                    Setting::info(
                        query.primary_id,
                        query.label,
                        SettingValue::Unreadable(format!("ioctl: {}", e)),
                    )
                    .with_source(source),
                );
            }
        }
    }
    Ok(out)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct FeatureQuery {
    fid: u32,
    primary_id: &'static str,
    label: &'static str,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn feature_queries() -> &'static [FeatureQuery] {
    &[
        FeatureQuery {
            fid: 0x02,
            primary_id: "feat.power_management",
            label: "Power Management",
        },
        FeatureQuery {
            fid: 0x06,
            primary_id: "feat.write_cache",
            label: "Volatile Write Cache",
        },
        FeatureQuery {
            fid: 0x07,
            primary_id: "feat.num_queues",
            label: "Number of Queues",
        },
        FeatureQuery {
            fid: 0x0B,
            primary_id: "feat.async_event_config",
            label: "Async Event Config",
        },
        FeatureQuery {
            fid: 0x0C,
            primary_id: "feat.apst",
            label: "Autonomous Power State Transition (APST)",
        },
        FeatureQuery {
            fid: 0x0F,
            primary_id: "feat.keep_alive_timer",
            label: "Keep Alive Timer",
        },
    ]
}

/// Decode the raw u32 result of a Get-Features command into one or more
/// human-readable settings. References: NVMe Base Spec rev 2.0c, §5.27.1.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn decode_feature(q: &FeatureQuery, raw: u32) -> Vec<Setting> {
    match q.fid {
        0x02 => {
            // Power Management (Figure 313): bits[4:0]=PS, bits[7:5]=WH
            let ps = (raw & 0x1F) as u64;
            let wh = ((raw >> 5) & 0x7) as u64;
            let wh_label = match wh {
                0 => "No Workload",
                1 => "Extended Idle Period",
                2 => "Heavy Sustained Workload",
                _ => "Reserved",
            };
            vec![
                Setting::info(
                    "feat.pm.power_state",
                    "Active Power State",
                    SettingValue::Uint(ps),
                )
                .with_description(
                    "Current NVMe power state index (0=highest perf, higher=lower power).",
                ),
                Setting::info(
                    "feat.pm.workload_hint",
                    "Workload Hint",
                    SettingValue::Text(format!("{} ({})", wh, wh_label)),
                )
                .with_description("Hint to the controller about expected workload profile."),
            ]
        }
        0x06 => {
            // Volatile Write Cache (Figure 326): bit 0 = WCE
            let enabled = (raw & 1) == 1;
            vec![
                Setting::info("feat.write_cache.enabled", "Volatile Write Cache", SettingValue::Bool(enabled))
                    .with_description("When enabled, writes complete before persisting to NAND. Faster but risks data loss on power failure unless backed by capacitors.")
                    .with_risk(SettingRisk::Moderate),
            ]
        }
        0x07 => {
            // Number of Queues (Figure 327): bits[15:0]=NSQA, bits[31:16]=NCQA
            // (zero-based: actual count is +1)
            let nsq = (raw & 0xFFFF) as u64 + 1;
            let ncq = ((raw >> 16) & 0xFFFF) as u64 + 1;
            vec![
                Setting::info("feat.num_queues.submission", "I/O Submission Queues", SettingValue::Uint(nsq))
                    .with_description("Negotiated NSQA+1 — number of I/O submission queues the controller will allocate."),
                Setting::info("feat.num_queues.completion", "I/O Completion Queues", SettingValue::Uint(ncq))
                    .with_description("Negotiated NCQA+1 — number of I/O completion queues the controller will allocate."),
            ]
        }
        0x0B => {
            // Async Event Configuration (NVMe base spec, Feature 0Bh). Bits 7:0 are
            // the SMART/Health critical warning enables, bit 8 Namespace Attribute
            // Notices, bit 9 Firmware Activation Notices, bit 10 Telemetry Log
            // Notices, bit 11 Asymmetric Namespace Access Change Notices, bit 12
            // Predictable Latency Event Aggregate Log Change Notices, bit 13 LBA
            // Status Information Notices, bit 14 Endurance Group Event Aggregate
            // Log Change Notices. This used to label bit 11 as telemetry and omit
            // bits 11 to 14, and tested only bit 0 for the SMART bits.
            let smart = (raw & 0xFF) != 0;
            let ns_attr = ((raw >> 8) & 0x1) != 0;
            let fw_act = ((raw >> 9) & 0x1) != 0;
            let telemetry = ((raw >> 10) & 0x1) != 0;
            let asym_ns = ((raw >> 11) & 0x1) != 0;
            let predictable_latency = ((raw >> 12) & 0x1) != 0;
            let lba_status = ((raw >> 13) & 0x1) != 0;
            let endurance_group = ((raw >> 14) & 0x1) != 0;
            vec![
                Setting::info(
                    "feat.aec.raw_mask",
                    "Async Event Mask",
                    SettingValue::Uint(raw as u64),
                )
                .with_description("Raw 32-bit async event enable mask."),
                Setting::info(
                    "feat.aec.smart_critical",
                    "  → SMART/Health Critical Warnings (bits 7:0)",
                    SettingValue::Bool(smart),
                ),
                Setting::info(
                    "feat.aec.namespace_attr",
                    "  → Namespace Attribute Notices",
                    SettingValue::Bool(ns_attr),
                ),
                Setting::info(
                    "feat.aec.firmware_activation",
                    "  → Firmware Activation Notices",
                    SettingValue::Bool(fw_act),
                ),
                Setting::info(
                    "feat.aec.telemetry",
                    "  → Telemetry Log Notices",
                    SettingValue::Bool(telemetry),
                ),
                Setting::info(
                    "feat.aec.asym_ns_access",
                    "  → Asymmetric Namespace Access Change Notices",
                    SettingValue::Bool(asym_ns),
                ),
                Setting::info(
                    "feat.aec.predictable_latency",
                    "  → Predictable Latency Event Aggregate Log Change Notices",
                    SettingValue::Bool(predictable_latency),
                ),
                Setting::info(
                    "feat.aec.lba_status",
                    "  → LBA Status Information Notices",
                    SettingValue::Bool(lba_status),
                ),
                Setting::info(
                    "feat.aec.endurance_group",
                    "  → Endurance Group Event Aggregate Log Change Notices",
                    SettingValue::Bool(endurance_group),
                ),
            ]
        }
        0x0C => {
            // APST (Figure 322): bit 0 = APSTE (enable)
            let enabled = (raw & 1) == 1;
            vec![
                Setting::info("feat.apst.enabled", "APST Enabled", SettingValue::Bool(enabled))
                    .with_description("Autonomous Power State Transition — controller autonomously moves between non-operational power states based on idle time. Major laptop battery-life feature.")
                    .with_risk(SettingRisk::Moderate),
            ]
        }
        0x0F => {
            // Keep Alive Timer (FID 0Fh): KATO in milliseconds, 0 disables it.
            // This used to decode FID 0Fh as a Software Progress Marker count,
            // which is FID 80h. That label and its description were wrong.
            vec![Setting::info(
                "feat.keep_alive_timer",
                "Keep Alive Timer",
                SettingValue::Uint(raw as u64),
            )
            .with_unit("ms")
            .with_description(
                "Keep Alive Timeout in milliseconds; 0 means keep-alive is disabled.",
            )]
        }
        _ => vec![Setting::info(
            q.primary_id,
            q.label,
            SettingValue::Uint(raw as u64),
        )],
    }
}

#[cfg(target_os = "linux")]
#[repr(C, packed)]
struct NvmeAdminCmd {
    opcode: u8,
    flags: u8,
    rsvd1: u16,
    nsid: u32,
    cdw2: u32,
    cdw3: u32,
    metadata: u64,
    addr: u64,
    metadata_len: u32,
    data_len: u32,
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
    cdw13: u32,
    cdw14: u32,
    cdw15: u32,
    timeout_ms: u32,
    result: u32,
}

#[cfg(target_os = "linux")]
fn issue_get_features(fd: i32, fid: u32) -> std::io::Result<u32> {
    let mut cmd = NvmeAdminCmd {
        opcode: NVME_ADMIN_GET_FEATURES,
        flags: 0,
        rsvd1: 0,
        nsid: 0,
        cdw2: 0,
        cdw3: 0,
        metadata: 0,
        addr: 0,
        metadata_len: 0,
        data_len: 0,
        cdw10: fid & 0xFF, // SEL=0 (current), FID in lower 8 bits
        cdw11: 0,
        cdw12: 0,
        cdw13: 0,
        cdw14: 0,
        cdw15: 0,
        timeout_ms: 5_000,
        result: 0,
    };
    let rc = unsafe {
        libc::ioctl(
            fd,
            NVME_IOCTL_ADMIN_CMD as libc::c_ulong,
            &mut cmd as *mut _ as *mut libc::c_void,
        )
    };
    if rc < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if rc != 0 {
        // NVMe returns the Status Field as the ioctl return when non-zero.
        return Err(std::io::Error::other(format!("NVMe status 0x{rc:04x}")));
    }
    Ok(cmd.result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrich_smoke() {
        // Never panics, even with no NVMe devices or wrong platform.
        let mut groups = Vec::new();
        enrich_groups(&mut groups);
    }

    #[test]
    fn decode_power_management() {
        let q = FeatureQuery {
            fid: 0x02,
            primary_id: "x",
            label: "y",
        };
        // PS=3, WH=2 (heavy sustained)
        let raw = 3 | (2 << 5);
        let settings = decode_feature(&q, raw);
        assert_eq!(settings.len(), 2);
        assert!(matches!(settings[0].value, SettingValue::Uint(3)));
        if let SettingValue::Text(ref s) = settings[1].value {
            assert!(s.contains("Heavy Sustained"));
        } else {
            panic!("expected Text workload hint");
        }
    }

    #[test]
    fn decode_async_event_bits() {
        let q = FeatureQuery {
            fid: 0x0B,
            primary_id: "x",
            label: "y",
        };
        let bool_at = |settings: &[Setting], id: &str| -> bool {
            let s = settings
                .iter()
                .find(|s| s.id == id)
                .unwrap_or_else(|| panic!("missing {id}"));
            match s.value {
                SettingValue::Bool(b) => b,
                _ => panic!("{id} is not a Bool"),
            }
        };
        // Bit 0 alone: SMART warnings only.
        let s = decode_feature(&q, 1);
        assert!(bool_at(&s, "feat.aec.smart_critical"));
        assert!(!bool_at(&s, "feat.aec.telemetry"));
        // Bit 10 is telemetry, bit 11 is asymmetric namespace access.
        let s = decode_feature(&q, 1 << 10);
        assert!(bool_at(&s, "feat.aec.telemetry"));
        assert!(!bool_at(&s, "feat.aec.asym_ns_access"));
        let s = decode_feature(&q, 1 << 11);
        assert!(!bool_at(&s, "feat.aec.telemetry"));
        assert!(bool_at(&s, "feat.aec.asym_ns_access"));
        // Bits 12 to 14 map to their own notices.
        let s = decode_feature(&q, (1 << 12) | (1 << 13) | (1 << 14));
        assert!(bool_at(&s, "feat.aec.predictable_latency"));
        assert!(bool_at(&s, "feat.aec.lba_status"));
        assert!(bool_at(&s, "feat.aec.endurance_group"));
        // SMART bits are 7:0, so bit 7 counts.
        assert!(bool_at(
            &decode_feature(&q, 1 << 7),
            "feat.aec.smart_critical"
        ));
    }

    #[test]
    fn decode_write_cache() {
        let q = FeatureQuery {
            fid: 0x06,
            primary_id: "x",
            label: "y",
        };
        assert!(matches!(
            decode_feature(&q, 1)[0].value,
            SettingValue::Bool(true)
        ));
        assert!(matches!(
            decode_feature(&q, 0)[0].value,
            SettingValue::Bool(false)
        ));
    }

    #[test]
    fn decode_num_queues() {
        let q = FeatureQuery {
            fid: 0x07,
            primary_id: "x",
            label: "y",
        };
        // NSQA=7 (=> 8 submission), NCQA=7 (=> 8 completion)
        let raw = 7 | (7 << 16);
        let settings = decode_feature(&q, raw);
        assert!(matches!(settings[0].value, SettingValue::Uint(8)));
        assert!(matches!(settings[1].value, SettingValue::Uint(8)));
    }

    #[test]
    fn decode_async_event_config() {
        let q = FeatureQuery {
            fid: 0x0B,
            primary_id: "x",
            label: "y",
        };
        let raw = 0b0000_1010_0000_0001; // SMART critical + firmware activation
        let settings = decode_feature(&q, raw);
        assert!(matches!(settings[1].value, SettingValue::Bool(true))); // smart
        assert!(matches!(settings[2].value, SettingValue::Bool(false))); // ns_attr
        assert!(matches!(settings[3].value, SettingValue::Bool(true))); // fw_act
    }

    #[test]
    fn decode_apst() {
        let q = FeatureQuery {
            fid: 0x0C,
            primary_id: "x",
            label: "y",
        };
        assert!(matches!(
            decode_feature(&q, 1)[0].value,
            SettingValue::Bool(true)
        ));
    }

    #[test]
    fn decode_keep_alive_timer_fid_0f() {
        let q = FeatureQuery {
            fid: 0x0F,
            primary_id: "feat.keep_alive_timer",
            label: "Keep Alive Timer",
        };
        // 10_000 ms is the common 10 s KATO.
        let s = decode_feature(&q, 10_000);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, "feat.keep_alive_timer");
        assert_eq!(s[0].value, SettingValue::Uint(10_000));
        assert_eq!(s[0].unit.as_deref(), Some("ms"));
        assert!(!s.iter().any(|x| x.display_name.contains("Progress")));
        assert!(!s.iter().any(|x| x.id == "feat.spm.count"));
    }

    #[test]
    fn controller_chardev_filter() {
        assert!(is_controller_chardev("nvme0"));
        assert!(is_controller_chardev("nvme12"));
        assert!(!is_controller_chardev("nvme0n1"));
        assert!(!is_controller_chardev("nvme0n1p2"));
        assert!(!is_controller_chardev("nvme"));
        assert!(!is_controller_chardev("nvme_x"));
        assert!(!is_controller_chardev("sda"));
    }
}
