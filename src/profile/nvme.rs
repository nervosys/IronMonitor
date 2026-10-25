//! NVMe parameters provider — `nvme-cli`-style read-only view.
//!
//! Linux sources:
//! - `/sys/class/nvme/nvmeN/` — controller-level attrs (model, serial,
//!   firmware, queue count, transport, state).
//! - `/sys/class/nvme/nvmeN/nvmeNnM/` — namespace attrs (size, LBA size, NGUID).
//! - `/sys/block/nvmeNnM/queue/` — block-layer policy (write_cache, scheduler).
//!
//! Windows: minimal — surfaces the controllers enumerated by the existing
//! [`crate::storage_controller`] module. Full NVMe admin pass-through via
//! `IOCTL_STORAGE_PROTOCOL_COMMAND` would let us read Get-Features (APST,
//! power state, host memory buffer) but is out of scope for this pass.

#[allow(unused_imports)]
use super::{ProfileGroup, ProfileProvider, Setting, SettingRisk, SettingValue, Subsystem};

pub struct NvmeProfileProvider {
    _private: (),
}

impl NvmeProfileProvider {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for NvmeProfileProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileProvider for NvmeProfileProvider {
    fn subsystem(&self) -> Subsystem {
        Subsystem::Nvme
    }

    fn snapshot(&mut self) -> Vec<ProfileGroup> {
        let mut groups = Vec::new();

        #[cfg(target_os = "linux")]
        groups.extend(linux_groups());

        #[cfg(target_os = "linux")]
        super::nvme_features::enrich_groups(&mut groups);

        #[cfg(windows)]
        groups.extend(windows_groups());

        if groups.is_empty() {
            let mut g = ProfileGroup::new(
                Subsystem::Nvme,
                "(no NVMe devices found)",
                "Default",
                "auto-detect",
            );
            g.note("No NVMe controllers were enumerable on this platform.");
            groups.push(g);
        }
        groups
    }
}

/// Read one sysfs attribute. A read error or an empty file is `Unreadable` with
/// the reason. This used to be `unwrap_or_default()` pushed as an empty `Text`.
#[cfg(target_os = "linux")]
fn sysfs_value(path: &std::path::Path) -> SettingValue {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => SettingValue::Text(s.trim().to_string()),
        Ok(_) => SettingValue::Unreadable(format!("{} is empty", path.display())),
        Err(e) => SettingValue::Unreadable(format!("{}: {}", path.display(), e)),
    }
}

#[cfg(target_os = "linux")]
fn linux_groups() -> Vec<ProfileGroup> {
    use std::fs;
    let base = std::path::Path::new("/sys/class/nvme");
    let mut groups = Vec::new();
    let rd = match fs::read_dir(base) {
        Ok(r) => r,
        Err(_) => return groups,
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let ctrl_dir = entry.path();
        let model = sysfs_value(&ctrl_dir.join("model"));
        let serial = sysfs_value(&ctrl_dir.join("serial"));
        let firmware = sysfs_value(&ctrl_dir.join("firmware_rev"));
        let model_label = match &model {
            SettingValue::Text(t) => t.clone(),
            _ => "(model unreadable)".to_string(),
        };

        let mut g = ProfileGroup::new(
            Subsystem::Nvme,
            format!("{} — {}", name, model_label),
            "Controller features",
            ctrl_dir.display().to_string(),
        );
        // The controller name is stable. The device label carries the model,
        // which can read once and then fail.
        g.key = Some(name.clone());
        let push_str = |g: &mut ProfileGroup, file: &str, id: &str, label: &str| {
            if let Ok(v) = fs::read_to_string(ctrl_dir.join(file)) {
                let v = v.trim().to_string();
                if !v.is_empty() {
                    g.push(
                        Setting::info(id, label, SettingValue::Text(v))
                            .with_source(ctrl_dir.join(file).display().to_string()),
                    );
                }
            }
        };
        g.push(Setting::info("model", "Model", model));
        g.push(Setting::info("serial", "Serial", serial));
        g.push(Setting::info("firmware_rev", "Firmware", firmware));
        push_str(&mut g, "transport", "transport", "Transport");
        push_str(&mut g, "state", "state", "Controller State");
        push_str(&mut g, "subsysnqn", "subsysnqn", "Subsystem NQN");
        push_str(&mut g, "address", "address", "Address");
        push_str(&mut g, "queue_count", "queue_count", "I/O Queue Count");
        push_str(&mut g, "sqsize", "sqsize", "Submission Queue Size");
        push_str(&mut g, "cntrltype", "cntrltype", "Controller Type");

        // Per-namespace block-layer policy.
        if let Ok(rd) = fs::read_dir(&ctrl_dir) {
            for ns in rd.flatten() {
                let ns_name = ns.file_name().to_string_lossy().to_string();
                if !ns_name.starts_with(&format!("{}n", name)) {
                    continue;
                }
                let queue = std::path::PathBuf::from("/sys/block")
                    .join(&ns_name)
                    .join("queue");
                for (file, id, label, risk) in [
                    (
                        "write_cache",
                        "write_cache",
                        "Write Cache",
                        SettingRisk::Moderate,
                    ),
                    (
                        "scheduler",
                        "scheduler",
                        "I/O Scheduler",
                        SettingRisk::Moderate,
                    ),
                    (
                        "nr_requests",
                        "nr_requests",
                        "Request Queue Depth",
                        SettingRisk::Moderate,
                    ),
                    (
                        "read_ahead_kb",
                        "read_ahead_kb",
                        "Read-Ahead",
                        SettingRisk::Moderate,
                    ),
                    (
                        "rotational",
                        "rotational",
                        "Rotational",
                        SettingRisk::Informational,
                    ),
                    (
                        "nomerges",
                        "nomerges",
                        "Disable Merges",
                        SettingRisk::Moderate,
                    ),
                    (
                        "max_sectors_kb",
                        "max_sectors_kb",
                        "Max I/O Size",
                        SettingRisk::Informational,
                    ),
                ] {
                    if let Ok(v) = fs::read_to_string(queue.join(file)) {
                        let v = v.trim().to_string();
                        let value = v
                            .parse::<u64>()
                            .map(SettingValue::Uint)
                            .unwrap_or_else(|_| SettingValue::Text(v));
                        g.push(
                            Setting::info(
                                format!("{}.{}", ns_name, id),
                                format!("{}: {}", ns_name, label),
                                value,
                            )
                            .with_risk(risk)
                            .with_source(queue.join(file).display().to_string()),
                        );
                    }
                }
            }
        }
        groups.push(g);
    }
    groups
}

/// A controller string the driver did not report. This used to be an empty
/// `Text`, which read as a blank firmware or serial.
#[cfg(windows)]
fn text_or_unreadable(value: &str, what: &str) -> SettingValue {
    if value.trim().is_empty() {
        SettingValue::Unreadable(format!("{} not reported by the controller", what))
    } else {
        SettingValue::Text(value.to_string())
    }
}

#[cfg(windows)]
fn windows_groups() -> Vec<ProfileGroup> {
    use crate::storage_controller::StorageControllerMonitor;
    let Ok(monitor) = StorageControllerMonitor::new() else {
        return Vec::new();
    };
    let controllers = monitor.controllers();
    let mut groups = Vec::new();
    for c in controllers {
        if !format!("{:?}", c.interface).to_lowercase().contains("nvme") {
            continue;
        }
        let mut g = ProfileGroup::new(
            Subsystem::Nvme,
            if c.model.is_empty() {
                c.name.clone()
            } else {
                c.model.clone()
            },
            "Controller info",
            "Windows Storage API",
        );
        if !c.vendor.is_empty() {
            g.push(Setting::info(
                "vendor",
                "Vendor",
                SettingValue::Text(c.vendor.clone()),
            ));
        }
        if let Some(driver) = c.driver.as_deref().filter(|d| !d.is_empty()) {
            g.push(Setting::info(
                "driver",
                "Driver",
                SettingValue::Text(driver.to_string()),
            ));
        }
        if !c.pci_address.is_empty() {
            g.push(Setting::info(
                "pci_address",
                "PCI Address",
                SettingValue::Text(c.pci_address.clone()),
            ));
        }
        if let Some(nvme) = &c.nvme_info {
            g.push(Setting::info(
                "firmware",
                "Firmware",
                text_or_unreadable(&nvme.firmware, "firmware revision"),
            ));
            g.push(Setting::info(
                "serial",
                "Serial",
                text_or_unreadable(&nvme.serial, "serial number"),
            ));
        }
        g.push(Setting::info(
            "interface",
            "Interface",
            SettingValue::Text(format!("{:?}", c.interface)),
        ));
        g.note(
            "Full NVMe Get-Features admin pass-through (APST, power state, host \
            memory buffer) on Windows requires IOCTL_STORAGE_PROTOCOL_COMMAND and \
            is not yet implemented.",
        );
        groups.push(g);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_smoke() {
        let mut p = NvmeProfileProvider::new();
        let _ = p.snapshot();
    }
}
