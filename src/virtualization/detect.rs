// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 nervosys

//! Hypervisor and virtualization platform detection

use serde::{Deserialize, Serialize};

/// Read a CPUID leaf.
///
/// Selects the module matching the target. Every caller here is gated on
/// `any(x86, x86_64)` but the previous version named `core::arch::x86_64`
/// unconditionally — including in the return type — so a 32-bit x86 build did
/// not compile at all. The two architectures get separate definitions rather
/// than one function with a `cfg` block inside it, because the return type has
/// to vary too.
///
/// **The `unsafe` block is deliberate even though `__cpuid` is safe to call on
/// current Rust.** It was removed once as a no-op, and that silently raised the
/// crate's minimum compiler to a version newer than any this project claims to
/// support: on Rust 1.90 and earlier the call is still `unsafe` and the crate
/// did not build. `#[allow(unused_unsafe)]` keeps newer compilers quiet without
/// giving up the older ones. Do not "clean this up" again.
#[cfg(target_arch = "x86_64")]
#[allow(unused_unsafe)]
fn cpuid(leaf: u32) -> core::arch::x86_64::CpuidResult {
    unsafe { core::arch::x86_64::__cpuid(leaf) }
}

#[cfg(target_arch = "x86")]
#[allow(unused_unsafe)]
fn cpuid(leaf: u32) -> core::arch::x86::CpuidResult {
    unsafe { core::arch::x86::__cpuid(leaf) }
}

/// Which side of a Hyper-V partition boundary this code is running on.
///
/// Matters because Windows 11 enables virtualization-based security by default,
/// which puts the *host* under a thin hypervisor: a bare-metal desktop reports
/// the "Microsoft Hv" CPUID signature exactly as a guest VM does. Vendor string
/// alone therefore cannot answer "am I in a VM", and every caller that assumed
/// it could has been wrong on ordinary Windows 11 hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HyperVPartition {
    /// The root (host) partition — physical hardware that happens to run Hyper-V.
    Root,
    /// A guest partition — an actual virtual machine.
    Guest,
    /// Not Hyper-V, or the question cannot be answered on this architecture.
    Unknown,
}

/// Distinguish a Hyper-V root partition from a guest via CPUID leaf 0x40000003.
///
/// EBX holds the high half of the partition privilege mask. `CreatePartitions`
/// (bit 0) and `CpuManagement` (bit 12) are root-only privileges: the root
/// partition is what creates and schedules guests, so a guest is never granted
/// them. Both are required here rather than either, since a single bit is a
/// thinner reed than the pair and they are set together on a root partition.
///
/// Verified on the root-partition side: this machine, a Windows 11 desktop with
/// VBS on, reports ebx=0x002bb9ff — CreatePartitions, AccessPartitionId and
/// CpuManagement all set. The guest side is *not* verified against a real
/// Hyper-V VM; it follows from the TLFS privilege definitions, and the failure
/// mode if it is wrong is the status quo ante — a guest misreported as a host.
/// See HANDOFF.md open work for what would settle it.
pub fn hyperv_partition() -> HyperVPartition {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if (cpuid(0x1).ecx >> 31) & 1 == 0 {
            return HyperVPartition::Unknown; // no hypervisor present
        }

        let vendor = cpuid(0x4000_0000);
        let mut vendor_str = [0u8; 12];
        vendor_str[0..4].copy_from_slice(&vendor.ebx.to_le_bytes());
        vendor_str[4..8].copy_from_slice(&vendor.ecx.to_le_bytes());
        vendor_str[8..12].copy_from_slice(&vendor.edx.to_le_bytes());
        if String::from_utf8_lossy(&vendor_str).trim_end_matches('\0') != "Microsoft Hv" {
            return HyperVPartition::Unknown;
        }

        // The privilege leaf must actually exist; vendor.eax is the max leaf.
        if vendor.eax < 0x4000_0003 {
            return HyperVPartition::Unknown;
        }

        let privileges = cpuid(0x4000_0003).ebx;
        const CREATE_PARTITIONS: u32 = 1 << 0;
        const CPU_MANAGEMENT: u32 = 1 << 12;
        if privileges & CREATE_PARTITIONS != 0 && privileges & CPU_MANAGEMENT != 0 {
            HyperVPartition::Root
        } else {
            HyperVPartition::Guest
        }
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        HyperVPartition::Unknown
    }
}

/// Whether a detected hypervisor means this code is running *inside* a VM.
///
/// The distinction a bare `detect_hypervisor().is_some()` cannot make: a
/// Hyper-V root partition has a hypervisor beneath it and is still the physical
/// machine. Every "am I virtualized" question routes through here.
pub fn hypervisor_indicates_vm() -> bool {
    if hyperv_partition() == HyperVPartition::Root {
        return false;
    }
    detect_hypervisor().is_some()
}

/// Known hypervisors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hypervisor {
    VMware,
    HyperV,
    KVM,
    QEMU,
    VirtualBox,
    Xen,
    Parallels,
    Bhyve,
    AmazonNitro,
    GoogleCompute,
    Azure,
    AppleVirt,
    WSL2,
    Firecracker,
    CloudHypervisor,
    Lxc,
    Other,
}

/// Virtualization platform type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VirtPlatform {
    BareMetal,
    VirtualMachine,
    Container,
    WSL,
    Unknown,
}

/// Hypervisor details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypervisorInfo {
    pub hypervisor: Hypervisor,
    pub version: Option<String>,
    pub cloud_provider: Option<String>,
    pub instance_type: Option<String>,
    pub detection_method: String,
}

/// CPU virtualization capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuVirtCapability {
    /// VT-x or AMD-V, from CPUID.
    pub hardware_virt: bool,
    /// Second-level address translation. `None`: Intel reports EPT in a VMX
    /// MSR that needs ring 0, and this does not read AMD's leaf either. These
    /// four were `false` -- "this CPU lacks EPT, an IOMMU, SR-IOV and nested
    /// virtualization" -- on every machine, from no check at all.
    pub ept_npt: Option<bool>,
    /// Whether an IOMMU is present; not read here.
    pub iommu: Option<bool>,
    /// Whether SR-IOV is available; not read here.
    pub sriov: Option<bool>,
    /// Whether nested virtualization is available; not read here.
    pub nested: Option<bool>,
    pub technology: String,
}

/// Detect the overall virtualization platform
pub fn detect_platform() -> VirtPlatform {
    #[cfg(target_os = "linux")]
    {
        if is_wsl() {
            return VirtPlatform::WSL;
        }
        if is_container_env() {
            return VirtPlatform::Container;
        }
        if hypervisor_indicates_vm() {
            return VirtPlatform::VirtualMachine;
        }
    }
    #[cfg(target_os = "windows")]
    {
        if hypervisor_indicates_vm() {
            return VirtPlatform::VirtualMachine;
        }
    }
    #[cfg(target_os = "macos")]
    {
        if hypervisor_indicates_vm() {
            return VirtPlatform::VirtualMachine;
        }
    }
    VirtPlatform::BareMetal
}

/// Detect the hypervisor
pub fn detect_hypervisor() -> Option<HypervisorInfo> {
    // Try CPUID first
    if let Some(info) = detect_cpuid_hypervisor() {
        return Some(info);
    }
    // Try DMI/SMBIOS
    #[cfg(target_os = "linux")]
    if let Some(info) = detect_dmi_hypervisor() {
        return Some(info);
    }
    None
}

fn detect_cpuid_hypervisor() -> Option<HypervisorInfo> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // CPUID leaf 0x40000000 - hypervisor vendor string
        let result = cpuid(0x1);
        let hypervisor_bit = (result.ecx >> 31) & 1;
        if hypervisor_bit == 0 {
            return None;
        }

        let vendor = cpuid(0x40000000);
        let mut vendor_str = [0u8; 12];
        vendor_str[0..4].copy_from_slice(&vendor.ebx.to_le_bytes());
        vendor_str[4..8].copy_from_slice(&vendor.ecx.to_le_bytes());
        vendor_str[8..12].copy_from_slice(&vendor.edx.to_le_bytes());
        let vendor_id = String::from_utf8_lossy(&vendor_str)
            .trim_end_matches('\0')
            .to_string();

        let hypervisor = match vendor_id.as_str() {
            "VMwareVMware" => Hypervisor::VMware,
            "Microsoft Hv" => Hypervisor::HyperV,
            "KVMKVMKVM\0\0\0" | "KVMKVMKVM" => Hypervisor::KVM,
            "TCGTCGTCGTCG" | "TCGTCGTCG" => Hypervisor::QEMU,
            "VBoxVBoxVBox" => Hypervisor::VirtualBox,
            "XenVMMXenVMM" => Hypervisor::Xen,
            "bhyve bhyve " => Hypervisor::Bhyve,
            " lrpepyh  vr" => Hypervisor::Parallels,
            "ACRNACRNACRN" => Hypervisor::Other,
            _ => Hypervisor::Other,
        };

        Some(HypervisorInfo {
            hypervisor,
            version: None,
            cloud_provider: detect_cloud_provider(),
            instance_type: None,
            detection_method: format!("CPUID: {}", vendor_id),
        })
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn detect_dmi_hypervisor() -> Option<HypervisorInfo> {
    let product = std::fs::read_to_string("/sys/class/dmi/id/product_name")
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let vendor = std::fs::read_to_string("/sys/class/dmi/id/sys_vendor")
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let bios = std::fs::read_to_string("/sys/class/dmi/id/bios_vendor")
        .unwrap_or_default()
        .trim()
        .to_lowercase();

    let hypervisor = if product.contains("virtualbox") || bios.contains("virtualbox") {
        Hypervisor::VirtualBox
    } else if vendor.contains("vmware") || product.contains("vmware") {
        Hypervisor::VMware
    } else if vendor.contains("qemu") || product.contains("kvm") {
        Hypervisor::KVM
    } else if vendor.contains("microsoft") && product.contains("virtual") {
        Hypervisor::HyperV
    } else if vendor.contains("xen") || product.contains("hvm domu") {
        Hypervisor::Xen
    } else if vendor.contains("amazon") || product.contains("nitro") {
        Hypervisor::AmazonNitro
    } else if vendor.contains("google") {
        Hypervisor::GoogleCompute
    } else if product.contains("parallels") {
        Hypervisor::Parallels
    } else {
        return None;
    };

    Some(HypervisorInfo {
        hypervisor,
        version: None,
        cloud_provider: detect_cloud_provider(),
        instance_type: None,
        detection_method: format!("DMI: {} {}", vendor, product),
    })
}

fn detect_cloud_provider() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let vendor = std::fs::read_to_string("/sys/class/dmi/id/sys_vendor")
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let product = std::fs::read_to_string("/sys/class/dmi/id/product_name")
            .unwrap_or_default()
            .trim()
            .to_lowercase();

        if vendor.contains("amazon") || product.contains("nitro") {
            return Some("AWS".into());
        } else if vendor.contains("google") {
            return Some("Google Cloud".into());
        } else if vendor.contains("microsoft") && product.contains("virtual") {
            return Some("Azure".into());
        }
    }
    None
}

/// Detect CPU virtualization capabilities
pub fn detect_cpu_virt_caps() -> Option<CpuVirtCapability> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let result = cpuid(0x1);
        let vmx = (result.ecx >> 5) & 1 == 1; // Intel VT-x
        let svm = {
            let ext = cpuid(0x80000001);
            (ext.ecx >> 2) & 1 == 1 // AMD-V
        };
        let hw_virt = vmx || svm;
        let tech = if vmx {
            "Intel VT-x"
        } else if svm {
            "AMD-V"
        } else {
            "None"
        };

        Some(CpuVirtCapability {
            hardware_virt: hw_virt,
            ept_npt: None,
            iommu: None,
            sriov: None,
            nested: None,
            technology: tech.into(),
        })
    }
    // Not read on Arm. This returned EL2 support and stage-2 translation as
    // facts, on the grounds that "ARMv8 always has EL2" -- but EL2 is optional
    // in the architecture, firmware commonly keeps it from the OS, and nothing
    // here asked. EL0 cannot read `ID_AA64PFR0_EL1` without the kernel's help.
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn is_wsl() -> bool {
    std::fs::read_to_string("/proc/version")
        .map(|v| v.to_lowercase().contains("microsoft") || v.to_lowercase().contains("wsl"))
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn is_container_env() -> bool {
    use std::path::Path;
    Path::new("/.dockerenv").exists()
        || Path::new("/run/.containerenv").exists()
        || std::fs::read_to_string("/proc/1/cgroup")
            .map(|c| c.contains("docker") || c.contains("containerd") || c.contains("lxc"))
            .unwrap_or(false)
}

#[cfg(test)]
mod partition_tests {
    use super::*;

    /// The partition answer must agree with the vendor string, on any machine.
    ///
    /// Deliberately not asserting `Root`: this invariant has to hold on the
    /// development desktop (a root partition) and in CI, where the Windows
    /// runners are themselves Hyper-V guests on Azure — which is what exercises
    /// the `Guest` arm that no local hardware here can reach.
    #[test]
    fn partition_agrees_with_hypervisor_vendor() {
        let is_hyperv = matches!(
            detect_hypervisor().map(|h| h.hypervisor),
            Some(Hypervisor::HyperV)
        );
        match hyperv_partition() {
            HyperVPartition::Root | HyperVPartition::Guest => assert!(
                is_hyperv,
                "claimed a Hyper-V partition kind without a Hyper-V vendor string"
            ),
            HyperVPartition::Unknown => assert!(
                !is_hyperv || cfg!(not(any(target_arch = "x86", target_arch = "x86_64"))),
                "Hyper-V is present on an x86 machine but the partition kind was Unknown"
            ),
        }
    }

    /// A root partition is not a virtual machine — the defect this fixes.
    #[test]
    fn root_partition_is_not_reported_as_a_vm() {
        if hyperv_partition() == HyperVPartition::Root {
            assert!(
                !crate::virtualization::VirtMonitor::new()
                    .expect("VirtMonitor::new")
                    .is_virtual_machine(),
                "a Hyper-V root partition is bare metal, not a VM"
            );
        }
    }
}
