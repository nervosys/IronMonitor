// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 nervosys

//! Guest VM resource detection
//!
//! Every field that a guest cannot read is `None`. Nothing here substitutes a
//! default for an unread value: a socket count of 1 or a balloon size of 0 would
//! look like a reading.

use serde::{Deserialize, Serialize};

/// Virtual CPU topology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtCpuTopology {
    pub vcpus: u32,
    /// Distinct physical package ids across the online CPUs. `None` when no
    /// CPU's topology could be read. Used to report 1 when cpu0's id was unread.
    pub sockets: Option<u32>,
    /// Derived only when both `sockets` and `threads_per_core` were read.
    pub cores_per_socket: Option<u32>,
    pub threads_per_core: Option<u32>,
    /// `None` when cpuinfo has no model line. Used to report "Unknown".
    pub cpu_model: Option<String>,
    /// Host-side pinning is not visible from inside the guest. Always `None`
    /// here. Used to report `false`, which was never read.
    pub is_pinned: Option<bool>,
}

/// Virtual disk bus types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskBus {
    VirtIO,
    Scsi,
    Ide,
    Nvme,
    Xvd,
    HyperV,
}

/// Virtual disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtDisk {
    pub name: String,
    pub bus: DiskBus,
    pub size_bytes: Option<u64>,
    pub driver: String,
}

/// Virtual NIC driver types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NicDriver {
    VirtIO,
    Vmxnet3,
    E1000,
    XenNet,
    HyperVNet,
    Ena,
    Other,
}

/// Virtual NIC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtNic {
    pub name: String,
    pub driver: NicDriver,
    pub mac_address: Option<String>,
    pub mtu: Option<u32>,
}

/// Memory balloon info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalloonInfo {
    /// Balloon sizes are known only to the host. Sysfs in the guest does not
    /// expose them, so these stay `None`.
    pub current_bytes: Option<u64>,
    pub target_bytes: Option<u64>,
    pub driver_present: bool,
}

/// Guest agent status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestAgent {
    pub name: String,
    pub running: bool,
    pub version: Option<String>,
}

/// Complete guest resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestResources {
    pub cpu: Option<VirtCpuTopology>,
    /// `None` when /proc/meminfo is unreadable. Used to report 0.
    pub memory_bytes: Option<u64>,
    pub disks: Vec<VirtDisk>,
    pub nics: Vec<VirtNic>,
    pub balloon: Option<BalloonInfo>,
    pub guest_agent: Option<GuestAgent>,
}

/// Detect guest resources
pub fn detect_guest_resources() -> Option<GuestResources> {
    #[cfg(target_os = "linux")]
    {
        detect_linux_guest()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn detect_linux_guest() -> Option<GuestResources> {
    use std::fs;

    // Gate on virtualization evidence. A readable /proc/cpuinfo, a physical disk
    // or a physical NIC says nothing about whether this kernel is a guest.
    // Used to return GuestResources on bare metal whenever cpuinfo was readable.
    if !super::detect::hypervisor_indicates_vm() {
        return None;
    }

    let cpu = detect_vcpu_topology();
    let memory_bytes = fs::read_to_string("/proc/meminfo").ok().and_then(|m| {
        m.lines()
            .find(|l| l.starts_with("MemTotal:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .map(|kb| kb * 1024)
    });

    let disks = detect_virt_disks();
    let nics = detect_virt_nics();
    let balloon = detect_balloon();
    let guest_agent = detect_guest_agent();

    Some(GuestResources {
        cpu,
        memory_bytes,
        disks,
        nics,
        balloon,
        guest_agent,
    })
}

/// Count the CPUs in a sysfs CPU list such as `0-1` or `0,4-5`.
#[cfg(any(target_os = "linux", test))]
fn count_cpu_list(list: &str) -> Option<u32> {
    let mut total: u32 = 0;
    for part in list.trim().split(',') {
        let part = part.trim();
        match part.split_once('-') {
            Some((lo, hi)) => {
                let lo: u32 = lo.trim().parse().ok()?;
                let hi: u32 = hi.trim().parse().ok()?;
                if hi < lo {
                    return None;
                }
                total = total.checked_add(hi - lo + 1)?;
            }
            None => {
                part.parse::<u32>().ok()?;
                total = total.checked_add(1)?;
            }
        }
    }
    if total == 0 {
        None
    } else {
        Some(total)
    }
}

/// Count distinct physical package ids across the CPUs whose topology is readable.
#[cfg(target_os = "linux")]
fn count_physical_packages() -> Option<u32> {
    use std::collections::HashSet;
    use std::fs;

    let mut packages = HashSet::new();
    for entry in fs::read_dir("/sys/devices/system/cpu").ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_cpu = name
            .strip_prefix("cpu")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if !is_cpu {
            continue;
        }
        // An unreadable CPU (typically offline) is skipped, not assumed to be package 0.
        let id = fs::read_to_string(entry.path().join("topology/physical_package_id"))
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok());
        if let Some(id) = id {
            packages.insert(id);
        }
    }
    if packages.is_empty() {
        None
    } else {
        Some(packages.len() as u32)
    }
}

#[cfg(target_os = "linux")]
fn detect_vcpu_topology() -> Option<VirtCpuTopology> {
    use std::fs;
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").ok()?;

    let vcpus = cpuinfo.matches("processor").count() as u32;
    if vcpus == 0 {
        return None;
    }

    let cpu_model = cpuinfo
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, m)| m.trim().to_string())
        .filter(|m| !m.is_empty());

    let sockets = count_physical_packages();
    let threads_per_core =
        fs::read_to_string("/sys/devices/system/cpu/cpu0/topology/thread_siblings_list")
            .ok()
            .and_then(|v| count_cpu_list(&v));

    // Derived only from values that were read.
    let cores_per_socket = match (sockets, threads_per_core) {
        (Some(s), Some(t)) => vcpus.checked_div(t).and_then(|c| c.checked_div(s)),
        _ => None,
    };

    Some(VirtCpuTopology {
        vcpus,
        sockets,
        cores_per_socket,
        threads_per_core,
        cpu_model,
        is_pinned: None,
    })
}

/// Classify a block device by the driver or controller model that names it.
///
/// A physical NVMe disk is not a virtual disk, so `nvme` counts only when the
/// controller model names QEMU. Device names such as `vd*` or `hd*` are not used
/// as evidence.
#[cfg(any(target_os = "linux", test))]
fn classify_disk_bus(driver: &str, model: &str) -> Option<DiskBus> {
    if driver.contains("virtio") {
        Some(DiskBus::VirtIO)
    } else if driver == "vmw_pvscsi" {
        Some(DiskBus::Scsi)
    } else if driver == "hv_storvsc" {
        Some(DiskBus::HyperV)
    } else if driver.contains("blkfront") {
        Some(DiskBus::Xvd)
    } else if driver == "nvme" && model.contains("QEMU") {
        Some(DiskBus::Nvme)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn detect_virt_disks() -> Vec<VirtDisk> {
    use std::fs;
    let mut disks = Vec::new();

    let Ok(entries) = fs::read_dir("/sys/block") else {
        return disks;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let driver_link = format!("/sys/block/{}/device/driver", name);
        let driver = fs::read_link(&driver_link)
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
            .unwrap_or_default();
        let model = fs::read_to_string(format!("/sys/block/{}/device/model", name))
            .map(|m| m.trim().to_string())
            .unwrap_or_default();

        // Skip any disk whose driver or model does not name a virtual device.
        let Some(bus) = classify_disk_bus(&driver, &model) else {
            continue;
        };

        let size_bytes = fs::read_to_string(format!("/sys/block/{}/size", name))
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|s| s * 512);

        disks.push(VirtDisk {
            name,
            bus,
            size_bytes,
            driver,
        });
    }
    disks
}

/// Map a NIC driver name to a virtual NIC type. Any other driver is skipped,
/// since it is a physical NIC. `e1000e` is omitted because it also drives
/// physical Intel NICs and the driver name cannot tell them apart.
#[cfg(any(target_os = "linux", test))]
fn classify_nic_driver(driver: &str) -> Option<NicDriver> {
    Some(match driver {
        "virtio_net" | "virtio-pci" => NicDriver::VirtIO,
        "vmxnet3" => NicDriver::Vmxnet3,
        "e1000" => NicDriver::E1000,
        "xen_netfront" => NicDriver::XenNet,
        "hv_netvsc" => NicDriver::HyperVNet,
        "ena" => NicDriver::Ena,
        _ => return None,
    })
}

#[cfg(target_os = "linux")]
fn detect_virt_nics() -> Vec<VirtNic> {
    use std::fs;
    let mut nics = Vec::new();

    let Ok(entries) = fs::read_dir("/sys/class/net") else {
        return nics;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "lo" {
            continue;
        }

        let driver_link = format!("/sys/class/net/{}/device/driver", name);
        let driver = fs::read_link(&driver_link)
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
            .unwrap_or_default();

        let Some(nic_driver) = classify_nic_driver(&driver) else {
            continue;
        };

        let mac = fs::read_to_string(format!("/sys/class/net/{}/address", name))
            .ok()
            .map(|v| v.trim().to_string());
        let mtu = fs::read_to_string(format!("/sys/class/net/{}/mtu", name))
            .ok()
            .and_then(|v| v.trim().parse().ok());

        nics.push(VirtNic {
            name,
            driver: nic_driver,
            mac_address: mac,
            mtu,
        });
    }
    nics
}

#[cfg(target_os = "linux")]
fn detect_balloon() -> Option<BalloonInfo> {
    use std::path::Path;
    // virtio-balloon
    if Path::new("/sys/devices/virtio-pci/virtio0/balloon").exists()
        || Path::new("/sys/bus/virtio/drivers/virtio_balloon").exists()
    {
        return Some(BalloonInfo {
            current_bytes: None,
            target_bytes: None,
            driver_present: true,
        });
    }
    None
}

#[cfg(target_os = "linux")]
fn detect_guest_agent() -> Option<GuestAgent> {
    use std::process::Command;
    // Check for QEMU guest agent
    let status = Command::new("systemctl")
        .args(["is-active", "qemu-guest-agent"])
        .output()
        .ok()?;
    if status.status.success() {
        return Some(GuestAgent {
            name: "qemu-guest-agent".into(),
            running: true,
            version: None,
        });
    }
    // Check for VMware tools
    let status = Command::new("systemctl")
        .args(["is-active", "vmtoolsd"])
        .output()
        .ok()?;
    if status.status.success() {
        return Some(GuestAgent {
            name: "open-vm-tools".into(),
            running: true,
            version: None,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_list_counts_singletons_and_ranges() {
        assert_eq!(count_cpu_list("0\n"), Some(1));
        assert_eq!(count_cpu_list("0-1\n"), Some(2));
        assert_eq!(count_cpu_list("0,4"), Some(2));
        assert_eq!(count_cpu_list("0,4-5"), Some(3));
    }

    #[test]
    fn malformed_cpu_list_is_unknown() {
        assert_eq!(count_cpu_list(""), None);
        assert_eq!(count_cpu_list("3-1"), None);
        assert_eq!(count_cpu_list("a"), None);
    }

    #[test]
    fn only_virtual_drivers_are_disks() {
        assert_eq!(classify_disk_bus("virtio_blk", ""), Some(DiskBus::VirtIO));
        assert_eq!(classify_disk_bus("vmw_pvscsi", ""), Some(DiskBus::Scsi));
        assert_eq!(classify_disk_bus("hv_storvsc", ""), Some(DiskBus::HyperV));
        assert_eq!(classify_disk_bus("xen-blkfront", ""), Some(DiskBus::Xvd));
        assert_eq!(classify_disk_bus("nvme", "Samsung SSD 990 PRO"), None);
        assert_eq!(
            classify_disk_bus("nvme", "QEMU NVMe Ctrl"),
            Some(DiskBus::Nvme)
        );
        assert_eq!(classify_disk_bus("", ""), None);
    }

    #[test]
    fn only_virtual_drivers_are_nics() {
        assert_eq!(classify_nic_driver("virtio_net"), Some(NicDriver::VirtIO));
        assert_eq!(classify_nic_driver("vmxnet3"), Some(NicDriver::Vmxnet3));
        assert_eq!(classify_nic_driver("iwlwifi"), None);
        assert_eq!(classify_nic_driver("r8169"), None);
        assert_eq!(classify_nic_driver(""), None);
    }
}
